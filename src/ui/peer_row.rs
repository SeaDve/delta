use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::peer::Peer;

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PeerRow)]
    #[template(file = "peer_row.ui")]
    pub struct PeerRow {
        #[property(get, set, construct_only)]
        pub(super) peer: OnceCell<Peer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerRow {
        const NAME: &'static str = "DeltaPeerRow";
        type Type = super::PeerRow;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PeerRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let peer = obj.peer();
            peer.bind_property("name", &*obj, "title")
                .sync_create()
                .build();
        }
    }

    impl WidgetImpl for PeerRow {}
    impl ListBoxRowImpl for PeerRow {}
    impl PreferencesRowImpl for PeerRow {}
    impl ActionRowImpl for PeerRow {}
}

glib::wrapper! {
    pub struct PeerRow(ObjectSubclass<imp::PeerRow>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, adw::ActionRow;
}

impl PeerRow {
    pub fn new(peer: &Peer) -> Self {
        glib::Object::builder().property("peer", peer).build()
    }
}
