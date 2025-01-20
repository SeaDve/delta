use std::{cell::RefCell, collections::HashSet, fmt};

use anyhow::Result;
use gtk::{
    gio,
    glib::{self, translate::TryFromGlib},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::config;

static SETTINGS_FILE: Lazy<gio::File> = Lazy::new(|| {
    let mut path = config::user_config_dir();
    path.push("settings.json");
    gio::File::for_path(path)
});

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, glib::Enum)]
#[enum_type(name = "DeltaAllowedPeers")]
pub enum AllowedPeers {
    #[default]
    ExceptMuted,
    All,
    None,
}

impl fmt::Display for AllowedPeers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllowedPeers::ExceptMuted => write!(f, "Except Muted"),
            AllowedPeers::All => write!(f, "All"),
            AllowedPeers::None => write!(f, "None"),
        }
    }
}

impl TryFrom<i32> for AllowedPeers {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        unsafe { Self::try_from_glib(val) }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, glib::Boxed)]
#[serde(transparent)]
#[boxed_type(name = "DeltaMutedPeers")]
pub struct MutedPeers {
    inner: HashSet<String>,
}

impl MutedPeers {
    pub fn contains(&self, name: &str) -> bool {
        self.inner.contains(name)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.inner.iter()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Data {
    allowed_peers: AllowedPeers,
    muted_peers: MutedPeers,
    icon_name: String,
    remote_ip_addr: String,
    accel_impact_threshold: f32,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            allowed_peers: AllowedPeers::default(),
            muted_peers: MutedPeers::default(),
            icon_name: "driving-symbolic".into(),
            remote_ip_addr: "192.168.100.203".into(),
            accel_impact_threshold: 20.0,
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Settings)]
    pub struct Settings {
        #[property(name = "allowed-peers", get, set, member = allowed_peers, type = AllowedPeers, builder(AllowedPeers::default()))]
        #[property(name = "muted-peers", get, set, member = muted_peers, type = MutedPeers)]
        #[property(name = "icon-name", get, set, member = icon_name, type = String)]
        #[property(name = "remote-ip-addr", get, set, member = remote_ip_addr, type = String)]
        #[property(name = "accel-impact-threshold", get, set, member = accel_impact_threshold, type = f32)]
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
                tracing::error!("Failed to save settings on dispose: {:?}", err);
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
            imp.etag.borrow().as_deref(),
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            gio::Cancellable::NONE,
        )?;
        imp.etag.replace(etag);

        Ok(())
    }

    pub fn insert_muted_peer(&self, peer_name: String) {
        let imp = self.imp();

        if imp.data.borrow_mut().muted_peers.inner.insert(peer_name) {
            self.notify_muted_peers();
        }
    }

    pub fn remove_muted_peer(&self, peer_name: &str) {
        let imp = self.imp();

        if imp.data.borrow_mut().muted_peers.inner.remove(peer_name) {
            self.notify_muted_peers();
        }
    }

    pub fn is_allowed_peer(&self, peer_name: &str) -> bool {
        match self.allowed_peers() {
            AllowedPeers::ExceptMuted => !self.muted_peers().contains(peer_name),
            AllowedPeers::All => true,
            AllowedPeers::None => false,
        }
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

        tracing::debug!(
            "Loaded settings from {}",
            SETTINGS_FILE.path().unwrap().display()
        );

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
