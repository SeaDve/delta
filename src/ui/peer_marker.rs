use adw::prelude::*;
use gtk::glib::{self, clone, closure_local};
use shumate::{prelude::*, subclass::prelude::*};

use crate::{peer::Peer, ui::toggle_button::ToggleButton, Application};

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Delta/ui/peer_marker.ui")]
    pub struct PeerMarker {
        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) distance_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) speed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) wireless_status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub(super) call_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) mute_button: TemplateChild<ToggleButton>,

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
                clone!(
                    #[weak]
                    obj,
                    move |_, _| {
                        obj.update_name_label();
                        obj.update_mute_button();
                    }
                ),
            );
            peer_signals.connect_notify_local(
                Some("location"),
                clone!(
                    #[weak]
                    obj,
                    move |_, _| {
                        obj.update_location();
                        obj.update_distance_label();
                    }
                ),
            );
            peer_signals.connect_notify_local(
                Some("speed"),
                clone!(
                    #[weak]
                    obj,
                    move |_, _| {
                        obj.update_speed_label();
                    }
                ),
            );
            peer_signals.connect_notify_local(
                Some("signal-quality"),
                clone!(
                    #[weak]
                    obj,
                    move |_, _| {
                        obj.update_wireless_status_icon();
                    }
                ),
            );
            peer_signals.connect_notify_local(
                Some("icon-name"),
                clone!(
                    #[weak]
                    obj,
                    move |_, _| {
                        obj.update_image_icon_name();
                    }
                ),
            );
            self.peer_signals.set(peer_signals).unwrap();

            let gesture_click = gtk::GestureClick::new();
            gesture_click.connect_released(clone!(
                #[weak]
                obj,
                move |_, _, _, _| {
                    let imp = obj.imp();

                    imp.popover.popup();
                }
            ));
            self.image.add_controller(gesture_click);

            self.call_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    let imp = obj.imp();

                    imp.popover.popdown();

                    obj.emit_by_name::<()>("called", &[]);
                }
            ));
            self.mute_button.connect_is_active_notify(clone!(
                #[weak]
                obj,
                move |button| {
                    let settings = Application::get().settings();

                    let Some(peer) = obj.peer() else {
                        return;
                    };

                    if button.is_active() {
                        settings.insert_muted_peer(peer.name());
                    } else {
                        settings.remove_muted_peer(&peer.name());
                    }
                }
            ));

            let app = Application::get();

            app.gps().connect_location_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_distance_label();
                }
            ));

            app.settings().connect_muted_peers_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_mute_button();
                }
            ));

            obj.update_name_label();
            obj.update_distance_label();
            obj.update_speed_label();
            obj.update_location();
            obj.update_wireless_status_icon();
            obj.update_image_icon_name();
            obj.update_mute_button();
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
        self.update_speed_label();
        self.update_location();
        self.update_wireless_status_icon();
        self.update_image_icon_name();
        self.update_mute_button();
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
        imp.name_label.set_label(&name);
    }

    fn update_distance_label(&self) {
        let imp = self.imp();

        let distance_str = imp
            .peer
            .borrow()
            .as_ref()
            .and_then(|peer| peer.location())
            .and_then(|location| {
                Application::get()
                    .gps()
                    .location()
                    .map(|l| format!("{:.2} m", l.distance(&location)))
            });
        imp.distance_label
            .set_label(&distance_str.unwrap_or_default());
    }

    fn update_speed_label(&self) {
        let imp = self.imp();

        let speed_str = imp
            .peer
            .borrow()
            .as_ref()
            .map(|peer| format!("{:.2} m/s", peer.speed()));
        imp.speed_label.set_label(&speed_str.unwrap_or_default());
    }

    fn update_location(&self) {
        let imp = self.imp();

        let location = imp.peer.borrow().as_ref().and_then(|peer| peer.location());
        self.set_visible(location.is_some());

        if let Some(location) = location {
            self.set_location(location.latitude, location.longitude);
        }
    }

    fn update_wireless_status_icon(&self) {
        let imp = self.imp();

        let signal_quality = self
            .peer()
            .map(|peer| peer.signal_quality())
            .unwrap_or_default();

        imp.wireless_status_icon
            .set_icon_name(Some(signal_quality.icon_name()));

        signal_quality.apply_css_class_to_image(&imp.wireless_status_icon);
    }

    fn update_image_icon_name(&self) {
        let imp = self.imp();

        let icon_name = imp.peer.borrow().as_ref().map(|peer| peer.icon_name());
        imp.image.set_icon_name(icon_name.as_deref());
    }

    fn update_mute_button(&self) {
        let imp = self.imp();

        let is_muted = self.peer().is_some_and(|peer| {
            Application::get()
                .settings()
                .muted_peers()
                .contains(&peer.name())
        });
        imp.mute_button.set_is_active(is_muted);
    }
}
