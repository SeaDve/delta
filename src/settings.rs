use std::cell::RefCell;

use anyhow::{anyhow, Error, Result};
use gtk::{
    gio::{self, prelude::*},
    glib::{self, translate::TryFromGlib},
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::APP_ID;

static SETTINGS_FILE: Lazy<gio::File> = Lazy::new(|| {
    let mut path = glib::user_config_dir();
    path.push(APP_ID);
    path.push("settings.json");
    gio::File::for_path(path)
});

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, glib::Enum)]
#[enum_type(name = "DeltaAllowedPeers")]
pub enum AllowedPeers {
    #[default]
    Everyone,
    Whitelist,
    None,
}

impl TryFrom<i32> for AllowedPeers {
    type Error = Error;

    fn try_from(val: i32) -> Result<Self> {
        unsafe { Self::try_from_glib(val) }.map_err(|_| anyhow!("Invalid value `{}`", val))
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Data {
    allowed_peers: AllowedPeers,
}

pub struct Settings {
    data: RefCell<Data>,
    etag: RefCell<Option<glib::GString>>,
}

impl Default for Settings {
    fn default() -> Self {
        match Self::load() {
            Ok(settings) => settings,
            Err(err) => {
                tracing::error!("Failed to load settings, using default: {:?}", err);

                Self {
                    data: RefCell::new(Data::default()),
                    etag: RefCell::new(None),
                }
            }
        }
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        if let Err(err) = self.save() {
            tracing::error!("Failed to save settings on drop: {:?}", err);
        }
    }
}

impl Settings {
    fn load() -> Result<Self> {
        let (data, etag) = match SETTINGS_FILE.load_contents(gio::Cancellable::NONE) {
            Ok((bytes, etag)) => (serde_json::from_slice::<Data>(&bytes)?, etag),
            Err(err) => {
                if err.matches(gio::IOErrorEnum::NotFound) {
                    (Data::default(), None)
                } else {
                    return Err(err.into());
                }
            }
        };

        Ok(Self {
            data: RefCell::new(data),
            etag: RefCell::new(etag),
        })
    }

    pub fn save(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&*self.data.borrow())?;

        if let Err(err) = SETTINGS_FILE
            .parent()
            .unwrap()
            .make_directory_with_parents(gio::Cancellable::NONE)
        {
            if !err.matches(gio::IOErrorEnum::Exists) {
                return Err(err.into());
            }
        }

        let etag = SETTINGS_FILE.replace_contents(
            &bytes,
            self.etag.borrow().as_deref(),
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            gio::Cancellable::NONE,
        )?;
        self.etag.replace(etag);

        Ok(())
    }

    pub fn set_allowed_peers(&self, peers: AllowedPeers) {
        self.data.borrow_mut().allowed_peers = peers;
    }

    pub fn allowed_peers(&self) -> AllowedPeers {
        self.data.borrow().allowed_peers
    }
}
