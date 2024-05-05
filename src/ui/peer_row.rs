use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, closure_local};

use crate::{config, location::Location, peer::Peer};

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
        #[template_child]
        pub(super) view_on_map_button: TemplateChild<gtk::Button>,
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
                    obj.emit_by_name::<()>("called", &[]);
                }));
            self.view_on_map_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("viewed-on-map", &[]);
                }));

            let peer = obj.peer();
            peer.bind_property("name", &*obj, "title")
                .sync_create()
                .build();
            peer.bind_property("location", &*obj, "subtitle")
                .transform_to(|_, location: Option<Location>| {
                    Some(
                        location
                            .map(|location| {
                                format!("{:.2} m away", config::location().distance(&location))
                            })
                            .unwrap_or_default(),
                    )
                })
                .sync_create()
                .build();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("called").build(),
                    Signal::builder("viewed-on-map").build(),
                ]
            })
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

    pub fn connect_called<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("called", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn connect_viewed_on_map<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("viewed-on-map", false, closure_local!(|obj: &Self| f(obj)))
    }
}
