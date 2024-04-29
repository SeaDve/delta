use gtk::{glib, prelude::*, subclass::prelude::*};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};

const EARTH_RADIUS: f64 = 6_378_137.0;

#[derive(Debug, Clone, Deserialize, Serialize, glib::Boxed)]
#[boxed_type(name = "DeltaLocation", nullable)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    /// Calculate the distance between two locations in meters.
    pub fn distance(&self, other: &Location) -> f64 {
        let lat1 = self.latitude.to_radians();
        let lon1 = self.longitude.to_radians();

        let lat2 = other.latitude.to_radians();
        let lon2 = other.longitude.to_radians();

        (lat1.sin() * lat2.sin() + lat1.cos() * lat2.cos() * (lon2 - lon1).cos()).acos()
            * EARTH_RADIUS
    }
}

mod imp {
    use std::cell::{OnceCell, RefCell};

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Peer)]
    pub struct Peer {
        pub(super) id: OnceCell<PeerId>,

        #[property(get, set)]
        pub(super) name: RefCell<String>,
        #[property(get, set, nullable)]
        pub(super) location: RefCell<Option<Location>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Peer {
        const NAME: &'static str = "DeltaPeer";
        type Type = super::Peer;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Peer {}
}

glib::wrapper! {
    pub struct Peer(ObjectSubclass<imp::Peer>);
}

impl Peer {
    pub fn new(id: PeerId) -> Self {
        let this = glib::Object::new::<Self>();
        this.imp().id.set(id).unwrap();
        this
    }

    pub fn id(&self) -> &PeerId {
        self.imp().id.get().unwrap()
    }
}
