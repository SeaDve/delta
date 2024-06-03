use gtk::{glib, prelude::*, subclass::prelude::*};
use libp2p::PeerId;

use crate::location::Location;

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
        #[property(get, set)]
        pub(super) icon_name: RefCell<String>,
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
