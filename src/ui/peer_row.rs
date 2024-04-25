use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, closure_local};

use crate::peer::Peer;

mod imp {
    use std::{cell::OnceCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PeerRow)]
    #[template(file = "peer_row.ui")]
    pub struct PeerRow {
        #[property(get, set, construct_only)]
        pub(super) peer: OnceCell<Peer>,

        #[template_child]
        pub(super) call_button: TemplateChild<gtk::Button>,
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

            self.call_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("call-requested", &[]);
                }));

            let peer = obj.peer();
            peer.bind_property("name", &*obj, "title")
                .sync_create()
                .build();
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("call-requested").build()])
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

    pub fn connect_call_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "call-requested",
            false,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }
}
