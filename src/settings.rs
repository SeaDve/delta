use std::cell::RefCell;

use anyhow::{anyhow, Error, Result};
use gtk::{
    gio,
    glib::{self, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::{config, utils};

static SETTINGS_FILE: Lazy<gio::File> = Lazy::new(|| {
    let mut path = config::user_config_dir();
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

#[derive(Debug, Deserialize, Serialize)]
struct Data {
    allowed_peers: AllowedPeers,
    icon_name: String,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            allowed_peers: AllowedPeers::default(),
            icon_name: "driving-symbolic".into(),
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Settings)]
    pub struct Settings {
        #[property(name = "allowed-peers", get, set, member = allowed_peers, type = AllowedPeers, builder(AllowedPeers::default()))]
        #[property(name = "icon-name", get, set, member = icon_name, type = String)]
        pub(super) data: RefCell<Data>,

        pub(super) etag: RefCell<Option<glib::GString>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Settings {
        const NAME: &'static str = "DeltaSettings";
        type Type = super::Settings;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Settings {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if let Err(err) = obj.load() {
                tracing::error!("Failed to load settings: {:?}", err);
            }
        }

        fn dispose(&self) {
            let obj = self.obj();

            if let Err(err) = obj.save() {
                tracing::error!("Failed to save settings on dipose: {:?}", err);
            }
        }
    }
}

glib::wrapper! {
    pub struct Settings(ObjectSubclass<imp::Settings>);
}

impl Settings {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn save(&self) -> Result<()> {
        let imp = self.imp();

        let bytes = serde_json::to_vec(&*imp.data.borrow())?;

        utils::ensure_file_parents(&SETTINGS_FILE)?;

        let etag = SETTINGS_FILE.replace_contents(
            &bytes,
            imp.etag.borrow().as_deref(),
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            gio::Cancellable::NONE,
        )?;
        imp.etag.replace(etag);

        Ok(())
    }

    fn load(&self) -> Result<()> {
        let imp = self.imp();

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

        imp.data.replace(data);
        imp.etag.replace(etag);

        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
