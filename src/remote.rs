use std::time::Duration;

use anyhow::{ensure, Context, Result};
use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use isahc::{config::Configurable, AsyncReadResponseExt, HttpClient};
use once_cell::sync::Lazy;
use url::Url;

use crate::utils;

const PORT: u16 = 8888;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(3);

const ACCEL_IMPACT_SENSITIVITY: f32 = 20.0;
const ACCEL_MAGNITUDE_REQUEST_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum LedId {
    _1,
    _2,
}

#[derive(Debug, Clone, Copy)]
pub enum LedColor {
    Red,
    Green,
    Blue,
    Yellow,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "DeltaRemoteStatus")]
pub enum RemoteStatus {
    #[default]
    Disconnected,
    Connected,
    Error(String),
}

mod imp {
    use std::{cell::RefCell, collections::HashMap, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Remote)]
    pub struct Remote {
        #[property(get)]
        pub(super) status: RefCell<RemoteStatus>,

        pub(super) ip_addr: RefCell<String>,
        pub(super) accel_magnitude_request_handle: RefCell<Option<glib::JoinHandle<()>>>,
        pub(super) led_blink_handle: RefCell<HashMap<LedId, glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Remote {
        const NAME: &'static str = "DeltaRemote";
        type Type = super::Remote;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Remote {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let accel_magnitude_request_handle = utils::spawn_future_local_idle(clone!(
                #[weak]
                obj,
                async move {
                    tracing::trace!("Started accel magnitude request loop");

                    loop {
                        if let Err(err) = obj.handle_accel_magnitude_request().await {
                            tracing::warn!("Failed to handle accel magnitude request: {:?}", err);
                        }

                        glib::timeout_future(ACCEL_MAGNITUDE_REQUEST_INTERVAL).await;
                    }
                }
            ));
            self.accel_magnitude_request_handle
                .replace(Some(accel_magnitude_request_handle));
        }

        fn dispose(&self) {
            if let Some(handle) = self.accel_magnitude_request_handle.take() {
                handle.abort();
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("crash-detected").build()])
        }
    }
}

glib::wrapper! {
    pub struct Remote(ObjectSubclass<imp::Remote>);
}

impl Remote {
    pub fn new(ip_addr: String) -> Self {
        let this = glib::Object::new::<Self>();

        let imp = this.imp();
        imp.ip_addr.replace(ip_addr);

        this
    }

    pub fn connect_crash_detected<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("crash-detected", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn simulate_crashed(&self) {
        self.emit_by_name::<()>("crash-detected", &[]);
    }

    pub fn set_ip_addr(&self, ip_addr: String) {
        let imp = self.imp();

        imp.ip_addr.replace(ip_addr);

        self.set_status(RemoteStatus::Disconnected);
    }

    pub async fn blink_led(
        &self,
        id: LedId,
        color: LedColor,
        repeat_count: u32,
        interval: Duration,
    ) -> Result<()> {
        let imp = self.imp();

        let handle = imp.led_blink_handle.borrow_mut().remove(&id);
        if let Some(handle) = handle {
            self.set_led_color(id, None).await?;
            handle.abort();
        }

        let handle = utils::spawn_future_local_idle(clone!(
            #[weak(rename_to = obj)]
            self,
            async move {
                if let Err(err) = obj.blink_led_inner(id, color, repeat_count, interval).await {
                    tracing::warn!("Failed to blink LED: {:?}", err);
                }
            }
        ));
        imp.led_blink_handle.borrow_mut().insert(id, handle);

        Ok(())
    }

    pub async fn set_led_color(&self, id: LedId, color: Option<LedColor>) -> Result<()> {
        let id = match id {
            LedId::_1 => 1,
            LedId::_2 => 2,
        };

        let (r, g, b) = match color {
            Some(LedColor::Red) => (1, 0, 0),
            Some(LedColor::Green) => (0, 1, 0),
            Some(LedColor::Blue) => (0, 0, 1),
            Some(LedColor::Yellow) => (1, 1, 0),
            None => (0, 0, 0),
        };

        self.http_get(
            "setLedValue",
            Some(&format!("id={}&r={}&g={}&b={}", id, r, g, b)),
        )
        .await?;

        Ok(())
    }

    fn set_status(&self, status: RemoteStatus) {
        let imp = self.imp();

        if status == self.status() {
            return;
        }

        imp.status.replace(status);
        self.notify_status();
    }

    async fn blink_led_inner(
        &self,
        id: LedId,
        color: LedColor,
        repeat_count: u32,
        interval: Duration,
    ) -> Result<()> {
        let imp = self.imp();

        let mut count = repeat_count * 2;

        loop {
            if count % 2 == 0 {
                self.set_led_color(id, Some(color)).await?;
            } else {
                self.set_led_color(id, None).await?;
            }

            count -= 1;

            if count == 0 {
                self.set_led_color(id, None).await?;
                imp.led_blink_handle.borrow_mut().remove(&id);
                break;
            }

            glib::timeout_future(interval).await;
        }

        Ok(())
    }

    async fn http_get(
        &self,
        path: &str,
        query: Option<&str>,
    ) -> Result<isahc::Response<isahc::AsyncBody>> {
        static HTTP_CLIENT: Lazy<HttpClient> = Lazy::new(|| {
            HttpClient::builder()
                .timeout(CLIENT_TIMEOUT)
                .build()
                .unwrap()
        });

        let imp = self.imp();

        let raw_uri = format!("http://{}:{PORT}/{path}", imp.ip_addr.borrow());

        let mut uri = raw_uri
            .parse::<Url>()
            .with_context(|| format!("Failed to parse URI: {}", raw_uri))?;
        uri.set_query(query);

        let res = HTTP_CLIENT.get_async(uri.as_str()).await;

        let status = if let Err(err) = &res {
            RemoteStatus::Error(err.to_string())
        } else {
            RemoteStatus::Connected
        };
        self.set_status(status);

        let response = res?;

        ensure!(
            response.status().is_success(),
            "Failed to send GET request at {}",
            raw_uri
        );

        Ok(response)
    }

    async fn handle_accel_magnitude_request(&self) -> Result<()> {
        let magnitude = self
            .http_get("getAccelMagnitude", None)
            .await?
            .text()
            .await?
            .parse::<f32>()?;

        if magnitude > ACCEL_IMPACT_SENSITIVITY {
            self.emit_by_name::<()>("crash-detected", &[]);
        }

        Ok(())
    }
}
