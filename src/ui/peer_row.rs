use gtk::{glib, prelude::*, subclass::prelude::*};

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

        #[template_child]
        pub(super) label: TemplateChild<gtk::Label>,

        pub(super) bindings: glib::BindingGroup,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerRow {
        const NAME: &'static str = "DeltaPeerRow";
        type Type = super::PeerRow;
        type ParentType = gtk::ListBoxRow;

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

            self.bindings.bind("name", &*self.label, "label").build();

            let peer = self.peer.get().unwrap();
            self.bindings.set_source(Some(peer));
        }
    }

    impl WidgetImpl for PeerRow {}
    impl ListBoxRowImpl for PeerRow {}
}

glib::wrapper! {
    pub struct PeerRow(ObjectSubclass<imp::PeerRow>)
        @extends gtk::Widget, gtk::ListBoxRow;
}

impl PeerRow {
    pub fn new(peer: &Peer) -> Self {
        glib::Object::builder().property("peer", peer).build()
    }
}
