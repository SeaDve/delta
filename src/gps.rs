use std::{io, process::Command as StdCommand};

use anyhow::{bail, Result};
use async_process::{Child, Command, Stdio};
use futures_util::{
    io::{AsyncBufReadExt, BufReader},
    StreamExt,
};
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use serde::Deserialize;

use crate::{config, location::Location};

const DEVICE_PATH: &str = "/dev/ttyAMA0";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "DeltaFixMode")]
pub enum FixMode {
    #[default]
    None,
    TwoD,
    ThreeD,
}

#[derive(Debug, Deserialize)]
struct RawData {
    device: Option<String>,
    mode: Option<i32>,
    #[serde(rename = "lat")]
    latitude: Option<f64>,
    #[serde(rename = "lon")]
    longitude: Option<f64>,
    speed: Option<f64>,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Gps)]
    pub struct Gps {
        #[property(get, builder(FixMode::default()))]
        pub(super) fix_mode: Cell<FixMode>,
        #[property(get)]
        pub(super) location: RefCell<Option<Location>>,
        /// Speed in meters per second
        #[property(get)]
        pub(super) speed: Cell<f64>,

        pub(super) child: RefCell<Option<Child>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Gps {
        const NAME: &'static str = "DeltaGps";
        type Type = super::Gps;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Gps {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if config::is_gps_enabled() {
                tracing::debug!("GPS is enabled, initializing GPS");

                if let Err(err) = obj.init() {
                    tracing::error!("Failed to initialize GPS: {:?}", err);
                }
            }

            if let Some(location) = config::location() {
                obj.set_location(Some(location));
            }
        }

        fn dispose(&self) {
            if let Some(mut child) = self.child.take() {
                if let Err(err) = child.kill() {
                    tracing::error!("Failed to kill gpspipe: {:?}", err);
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct Gps(ObjectSubclass<imp::Gps>);
}

impl Gps {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn override_location(&self, location: Option<Location>) {
        self.set_location(location);
    }

    fn init(&self) -> Result<()> {
        ensure_gpsd()?;

        let mut child = Command::new("gpspipe")
            .stdout(Stdio::piped())
            .arg("-w")
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        glib::spawn_future_local(clone!(@weak self as obj =>  async move {
            let mut lines = reader.lines();

            while let Some(line) = lines.next().await {
                if let Err(err) = obj.handle_gpspipe_output(line) {
                    tracing::error!("Failed to handle gpspipe output: {:?}", err);
                }
            }
        }));

        Ok(())
    }

    fn handle_gpspipe_output(&self, line: io::Result<String>) -> Result<()> {
        let line = line?;
        let data = serde_json::from_str::<RawData>(&line)?;

        if let Some(device) = &data.device {
            tracing::debug!("Received data from device: {}", device);
        }

        if let Some(mode) = data.mode {
            self.set_fix_mode(match mode {
                1 => FixMode::None,
                2 => FixMode::TwoD,
                3 => FixMode::ThreeD,
                _ => {
                    tracing::warn!("Invalid fix mode: {}", mode);
                    FixMode::None
                }
            });
        }

        match (data.latitude, data.longitude) {
            (Some(latitude), Some(longitude)) => {
                if latitude == 0.0 && longitude == 0.0 {
                    self.set_location(None);
                } else {
                    self.set_location(Some(Location {
                        latitude,
                        longitude,
                    }));
                }
            }
            (None, None) => {}
            _ => {
                tracing::warn!("Invalid GPS data: {:?}", data);
            }
        }

        if let Some(speed) = data.speed {
            self.set_speed(speed);
        }

        Ok(())
    }

    fn set_fix_mode(&self, fix_mode: FixMode) {
        let imp = self.imp();

        if fix_mode == self.fix_mode() {
            return;
        }

        imp.fix_mode.set(fix_mode);
        self.notify_fix_mode();
    }

    fn set_location(&self, location: Option<Location>) {
        let imp = self.imp();

        if location == self.location() {
            return;
        }

        imp.location.replace(location);
        self.notify_location();
    }

    fn set_speed(&self, speed: f64) {
        let imp = self.imp();

        if speed == self.speed() {
            return;
        }

        imp.speed.set(speed);
        self.notify_speed();
    }
}

impl Default for Gps {
    fn default() -> Self {
        Self::new()
    }
}

fn ensure_gpsd() -> Result<()> {
    let status = StdCommand::new("gpsd").arg(DEVICE_PATH).spawn()?.wait()?;

    if !status.success() {
        bail!("Failed to start gpsd: {:?}", status.code());
    }

    Ok(())
}
