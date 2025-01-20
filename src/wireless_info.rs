use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_std::fs::File;
use futures_util::{
    io::{AsyncBufReadExt, BufReader},
    StreamExt,
};
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use serde::{Deserialize, Serialize};

const INFO_PATH: &str = "/proc/net/wireless";
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, glib::Enum)]
#[enum_type(name = "DeltaSignalQuality")]
pub enum SignalQuality {
    #[default]
    None,
    Weak,
    Ok,
    Good,
    Excellent,
}

impl SignalQuality {
    pub fn icon_name(&self) -> &str {
        match self {
            SignalQuality::Excellent => "network-cellular-signal-excellent-symbolic",
            SignalQuality::Good => "network-cellular-signal-good-symbolic",
            SignalQuality::Ok => "network-cellular-signal-ok-symbolic",
            SignalQuality::Weak => "network-cellular-signal-weak-symbolic",
            SignalQuality::None => "network-cellular-signal-none-symbolic",
        }
    }

    pub fn apply_css_class_to_image(&self, image: &gtk::Image) {
        match self {
            SignalQuality::Excellent | SignalQuality::Good => {
                image.remove_css_class("error");
                image.remove_css_class("warning");

                image.add_css_class("success");
            }
            SignalQuality::Ok => {
                image.remove_css_class("success");
                image.remove_css_class("error");

                image.add_css_class("warning");
            }
            SignalQuality::Weak | SignalQuality::None => {
                image.remove_css_class("success");
                image.remove_css_class("warning");

                image.add_css_class("error");
            }
        }
    }
}

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::WirelessInfo)]
    pub struct WirelessInfo {
        #[property(get, builder(SignalQuality::default()))]
        pub(super) signal_quality: Cell<SignalQuality>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WirelessInfo {
        const NAME: &'static str = "DeltaWirelessInfo";
        type Type = super::WirelessInfo;
    }

    #[glib::derived_properties]
    impl ObjectImpl for WirelessInfo {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            glib::spawn_future_local(clone!(
                #[weak]
                obj,
                async move {
                    loop {
                        let signal_quality = match read_signal_quality().await {
                            Ok(q) if q >= -50.0 => SignalQuality::Excellent,
                            Ok(q) if q >= -67.0 => SignalQuality::Good,
                            Ok(q) if q >= -70.0 => SignalQuality::Ok,
                            Ok(q) if q >= -80.0 => SignalQuality::Weak,
                            err => {
                                tracing::trace!("Got signal quality of none: {:?}", err);
                                SignalQuality::None
                            }
                        };
                        obj.set_signal_quality(signal_quality);

                        glib::timeout_future(REFRESH_INTERVAL).await;
                    }
                }
            ));
        }
    }
}

glib::wrapper! {
    pub struct WirelessInfo(ObjectSubclass<imp::WirelessInfo>);
}

impl WirelessInfo {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn set_signal_quality(&self, quality: SignalQuality) {
        let imp = self.imp();

        if quality == self.signal_quality() {
            return;
        }

        imp.signal_quality.set(quality);
        self.notify_signal_quality();
    }
}

impl Default for WirelessInfo {
    fn default() -> Self {
        Self::new()
    }
}

async fn read_signal_quality() -> Result<f32> {
    let file = File::open(INFO_PATH).await?;
    let reader = BufReader::new(file);

    let mut lines = reader.lines().skip(2);

    while let Some(line) = lines.next().await {
        let line = line?;
        let line = line.trim();

        if line.starts_with("wl") && line.contains(':') {
            let (iface_name, info) = line.split_once(':').context("No colon")?;
            tracing::trace!("Found wireless interface `{}`", iface_name);

            let quality_level = info.split_whitespace().nth(2).context("No quality level")?;
            return Ok(quality_level.parse()?);
        }
    }

    Err(anyhow!("No wireless interface found"))
}
