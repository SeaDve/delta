use adw::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone, closure_local},
    graphene::Rect,
};
use shumate::{prelude::*, subclass::prelude::*};

use crate::{peer::Peer, Application};

const ALERT_ANIMATION_DURATION_MS: u32 = 1000;
const MAX_ALERT_CIRCLE_RADIUS: f64 = 100.0;

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
        f64::consts::TAU,
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

        pub(super) alert_animation: OnceCell<adw::TimedAnimation>,
        pub(super) alert_color: Cell<Option<gdk::RGBA>>,
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
            obj.set_width_request(MAX_ALERT_CIRCLE_RADIUS as i32 * 2);
            obj.set_height_request(MAX_ALERT_CIRCLE_RADIUS as i32 * 2);

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

            Application::get()
                .gps()
                .connect_location_notify(clone!(@weak obj => move |_| {
                    obj.update_distance_label();
                }));

            let animation_target =
                adw::CallbackAnimationTarget::new(clone!(@weak obj => move |_| {
                    obj.queue_draw();
                }));
            let animation = adw::TimedAnimation::builder()
                .widget(&*obj)
                .duration(ALERT_ANIMATION_DURATION_MS)
                .value_to(1.0)
                .target(&animation_target)
                .build();
            self.alert_animation.set(animation).unwrap();

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

    impl WidgetImpl for PeerMarker {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();

            let width = obj.width();
            let height = obj.height();

            let value = self.alert_animation.get().unwrap().value();
            let radius = MAX_ALERT_CIRCLE_RADIUS * value;

            let cr = snapshot.append_cairo(&Rect::new(0.0, 0.0, width as f32, height as f32));
            cr.set_source_color(
                &self
                    .alert_color
                    .get()
                    .unwrap_or(gdk::RGBA::BLACK)
                    .with_alpha(0.4 * (1.0 - value as f32)),
            );
            cr.arc(
                obj.width() as f64 / 2.0,
                obj.height() as f64 / 2.0,
                radius,
                0.0,
                TAU,
            );
            cr.fill().unwrap();

            self.parent_snapshot(snapshot);
        }
    }

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

    pub fn play_alert_animation(&self, repeat_count: u32, color: gdk::RGBA) {
        let imp = self.imp();

        let animation = imp.alert_animation.get().unwrap();

        animation.reset();

        animation.set_repeat_count(repeat_count);
        imp.alert_color.set(Some(color));

        animation.play();
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

    fn update_location(&self) {
        let imp = self.imp();

        let location = imp.peer.borrow().as_ref().and_then(|peer| peer.location());
        self.set_visible(location.is_some());

        if let Some(location) = location {
            self.set_location(location.latitude, location.longitude);
        }
    }
}
