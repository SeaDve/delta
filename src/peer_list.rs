use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use libp2p::PeerId;

use crate::peer::Peer;

mod imp {
    use std::cell::RefCell;

    use indexmap::IndexMap;

    use super::*;

    #[derive(Default)]
    pub struct PeerList {
        pub(super) map: RefCell<IndexMap<PeerId, Peer>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerList {
        const NAME: &'static str = "DeltaPeerList";
        type Type = super::PeerList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for PeerList {}

    impl ListModelImpl for PeerList {
        fn item_type(&self) -> glib::Type {
            Peer::static_type()
        }

        fn n_items(&self) -> u32 {
            self.map.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.map
                .borrow()
                .get_index(position as usize)
                .map(|(_, v)| v.upcast_ref::<glib::Object>())
                .cloned()
        }
    }
}

glib::wrapper! {
    pub struct PeerList(ObjectSubclass<imp::PeerList>)
        @implements gio::ListModel;
}

impl PeerList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn get(&self, id: &PeerId) -> Option<Peer> {
        self.imp().map.borrow().get(id).cloned()
    }

    pub fn insert(&self, peer: Peer) -> bool {
        let (position, prev_value) = self.imp().map.borrow_mut().insert_full(*peer.id(), peer);

        if prev_value.is_some() {
            self.items_changed(position as u32, 1, 1);
        } else {
            self.items_changed(position as u32, 0, 1);
        }

        prev_value.is_none()
    }

    pub fn remove(&self, id: &PeerId) -> bool {
        let prev_value = self.imp().map.borrow_mut().shift_remove_full(id);

        if let Some((position, _, _)) = prev_value {
            self.items_changed(position as u32, 1, 0);
        }

        prev_value.is_some()
    }
}

impl Default for PeerList {
    fn default() -> Self {
        Self::new()
    }
}
