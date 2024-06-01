use std::time::Duration;

use adxl345_driver2::{i2c::Device, Adxl345Reader, Adxl345Writer};
use anyhow::Result;
use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use rppal::i2c::I2c;

const SCALE_MULTIPLIER: f64 = 0.004;
const EARTH_GRAVITY_MS2: f64 = 9.80665;
const REFRESH_INTERVAL: Duration = Duration::from_millis(100);

// This is temporarily reduced for testing purposes.
const IMPACT_SENSITIVITY: f64 = 20.0;

mod imp {
    use std::{cell::RefCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default)]
    pub struct CrashDetector {
        pub(super) device: RefCell<Option<Device<I2c>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CrashDetector {
        const NAME: &'static str = "DeltaCrashDetector";
        type Type = super::CrashDetector;
    }

    impl ObjectImpl for CrashDetector {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if let Err(err) = obj.init() {
                tracing::error!("Failed to initialize accelerometer: {:?}", err);
            }
        }

        fn dispose(&self) {
            if let Some(mut device) = self.device.take() {
                if let Err(err) = device.set_power_control(0) {
                    tracing::debug!("Failed to turn off measurement mode: {:?}", err);
                }
            }
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("crash-detected").build()])
        }
    }
}

glib::wrapper! {
    pub struct CrashDetector(ObjectSubclass<imp::CrashDetector>);
}

impl CrashDetector {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_crash_detected<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("crash-detected", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn simulate_crash(&self) {
        self.emit_by_name::<()>("crash-detected", &[]);
    }

    fn init(&self) -> Result<()> {
        let imp = self.imp();

        let bus = I2c::new()?;
        let mut device = Device::new(bus)?;

        // Set full scale output and range to 2G.
        device.set_data_format(8)?;

        // Set measurement mode on.
        device.set_power_control(8)?;

        imp.device.replace(Some(device));

        glib::spawn_future_local(clone!(@weak self as obj => async move {
            let imp = obj.imp();

            let mut prev_values: Option<(f64, f64, f64)> = None;

            loop {
                match imp.device.borrow_mut().as_mut().unwrap().acceleration() {
                    Ok((raw_x, raw_y, raw_z)) => {
                        let x = convert_to_ms2(raw_x);
                        let y = convert_to_ms2(raw_y);
                        let z = convert_to_ms2(raw_z);

                        if let Some((prev_x, prev_y, prev_z)) = prev_values {
                            let magnitude = ((x - prev_x).powi(2)
                                + (y - prev_y).powi(2)
                                + (z - prev_z).powi(2))
                            .sqrt();

                            if magnitude > IMPACT_SENSITIVITY {
                                obj.emit_by_name::<()>("crash-detected", &[]);
                            }
                        }

                        prev_values = Some((x, y, z));
                    }
                    Err(err) => {
                        tracing::error!("Failed to read device acceleration values: {:?}", err)
                    }
                }

                glib::timeout_future(REFRESH_INTERVAL).await;
            }
        }));

        Ok(())
    }
}

impl Default for CrashDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn convert_to_ms2(value: i16) -> f64 {
    value as f64 * SCALE_MULTIPLIER * EARTH_GRAVITY_MS2
}
