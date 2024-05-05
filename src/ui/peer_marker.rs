use gtk::glib::{self, clone, closure_local};
use shumate::{prelude::*, subclass::prelude::*};

use crate::{config, peer::Peer};

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "peer_marker.ui")]
    pub struct PeerMarker {
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) distance_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub(super) call_button: TemplateChild<gtk::Button>,

        pub(super) peer: RefCell<Option<Peer>>,
        pub(super) peer_signals: OnceCell<glib::SignalGroup>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerMarker {
        const NAME: &'static str = "DeltaPeerMarker";
        type Type = super::PeerMarker;
        type ParentType = shumate::Marker;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PeerMarker {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let peer_signals = glib::SignalGroup::new::<Peer>();
            peer_signals.connect_notify_local(
                Some("name"),
                clone!(@weak obj => move |_, _| {
                    obj.update_name_label();
                }),
            );
            peer_signals.connect_notify_local(
                Some("location"),
                clone!(@weak obj => move |_, _| {
                    obj.update_location();
                    obj.update_distance_label();
                }),
            );
            self.peer_signals.set(peer_signals).unwrap();

            let gesture_click = gtk::GestureClick::new();
            gesture_click.connect_released(clone!(@weak obj => move |_, _, _, _| {
                let imp = obj.imp();

                imp.popover.popup();
            }));
            obj.add_controller(gesture_click);

            self.call_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    let imp = obj.imp();

                    imp.popover.popdown();

                    obj.emit_by_name::<()>("called", &[]);
                }));

            obj.update_name_label();
            obj.update_distance_label();
            obj.update_location();
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("called").build()])
        }
    }

    impl WidgetImpl for PeerMarker {}
    impl MarkerImpl for PeerMarker {}
}

glib::wrapper! {
    pub struct PeerMarker(ObjectSubclass<imp::PeerMarker>)
        @extends gtk::Widget, shumate::Marker,
        @implements shumate::Location;
}

impl PeerMarker {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_called<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("called", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn set_peer(&self, peer: Option<Peer>) {
        let imp = self.imp();

        imp.peer_signals.get().unwrap().set_target(peer.as_ref());
        imp.peer.replace(peer);

        self.update_name_label();
        self.update_distance_label();
        self.update_location();
    }

    pub fn peer(&self) -> Option<Peer> {
        self.imp().peer.borrow().clone()
    }

    fn update_name_label(&self) {
        let imp = self.imp();

        let name = imp
            .peer
            .borrow()
            .as_ref()
            .map(|peer| peer.name())
            .unwrap_or_default();
        imp.name_label.set_text(&name);
    }

    fn update_distance_label(&self) {
        let imp = self.imp();

        let distance = imp
            .peer
            .borrow()
            .as_ref()
            .and_then(|peer| peer.location())
            .map(|location| format!("{:.2} m", config::location().distance(&location)))
            .unwrap_or_default();
        imp.distance_label.set_text(&distance);
    }

    fn update_location(&self) {
        let imp = self.imp();

        let location = imp.peer.borrow().as_ref().and_then(|peer| peer.location());
        self.set_location(
            location.as_ref().map(|l| l.latitude).unwrap_or_default(),
            location.as_ref().map(|l| l.longitude).unwrap_or_default(),
        );
    }
}
