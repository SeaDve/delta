use gtk::glib::{self, clone};
use shumate::{prelude::*, subclass::prelude::*};

use crate::peer::Peer;

mod imp {
    use std::cell::{OnceCell, RefCell};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "peer_marker.ui")]
    pub struct PeerMarker {
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,

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
                    obj.update_label();
                }),
            );
            peer_signals.connect_notify_local(
                Some("location"),
                clone!(@weak obj => move |_, _| {
                    obj.update_location();
                }),
            );
            self.peer_signals.set(peer_signals).unwrap();

            obj.update_label();
            obj.update_location();
        }

        fn dispose(&self) {
            self.dispose_template();
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

    pub fn set_peer(&self, peer: Option<Peer>) {
        let imp = self.imp();

        imp.peer_signals.get().unwrap().set_target(peer.as_ref());
        imp.peer.replace(peer);

        self.update_label();
        self.update_location();
    }

    fn update_label(&self) {
        let imp = self.imp();

        let name = imp
            .peer
            .borrow()
            .as_ref()
            .map(|peer| peer.name())
            .unwrap_or_default();
        imp.name_label.set_text(&name);
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
