use std::f64::consts::TAU;

use adw::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone},
    graphene::Rect,
};
use shumate::{prelude::*, subclass::prelude::*};

use crate::peer::Peer;

const ANIMATION_DURATION_MS: u32 = 1000;
const MAX_CIRCLE_SIZE: i32 = 200;

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use super::*;

    #[derive(Default)]
    pub struct AlertMarker {
        pub(super) peer: RefCell<Option<Peer>>,
        pub(super) peer_signals: OnceCell<glib::SignalGroup>,

        pub(super) animation: OnceCell<adw::TimedAnimation>,
        pub(super) circle_color: Cell<Option<gdk::RGBA>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlertMarker {
        const NAME: &'static str = "DeltaAlertMarker";
        type Type = super::AlertMarker;
        type ParentType = shumate::Marker;
    }

    impl ObjectImpl for AlertMarker {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.set_width_request(MAX_CIRCLE_SIZE);
            obj.set_height_request(MAX_CIRCLE_SIZE);

            obj.set_can_target(false);
            obj.set_can_focus(false);

            let peer_signals = glib::SignalGroup::new::<Peer>();
            peer_signals.connect_notify_local(
                Some("location"),
                clone!(@weak obj => move |_, _| {
                    obj.update_location();
                }),
            );
            self.peer_signals.set(peer_signals).unwrap();

            let animation_target =
                adw::CallbackAnimationTarget::new(clone!(@weak obj => move |_| {
                    obj.queue_draw();
                }));
            let animation = adw::TimedAnimation::builder()
                .widget(&*obj)
                .duration(ANIMATION_DURATION_MS)
                .value_to(1.0)
                .target(&animation_target)
                .build();
            self.animation.set(animation).unwrap();

            obj.update_location();
        }
    }

    impl WidgetImpl for AlertMarker {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();

            let width = obj.width();
            let height = obj.height();

            let value = self.animation.get().unwrap().value();
            let radius = MAX_CIRCLE_SIZE as f64 / 2.0 * value;

            let cr = snapshot.append_cairo(&Rect::new(0.0, 0.0, width as f32, height as f32));
            cr.set_source_color(
                &self
                    .circle_color
                    .get()
                    .unwrap_or(gdk::RGBA::BLACK)
                    .with_alpha(0.4 * (1.0 - value as f32)),
            );
            cr.arc(width as f64 / 2.0, height as f64 / 2.0, radius, 0.0, TAU);
            cr.fill().unwrap();

            self.parent_snapshot(snapshot);
        }
    }

    impl MarkerImpl for AlertMarker {}
}

glib::wrapper! {
    pub struct AlertMarker(ObjectSubclass<imp::AlertMarker>)
        @extends gtk::Widget, shumate::Marker,
        @implements shumate::Location;
}

impl AlertMarker {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_peer(&self, peer: Option<Peer>) {
        let imp = self.imp();

        imp.peer_signals.get().unwrap().set_target(peer.as_ref());
        imp.peer.replace(peer);

        self.update_location();
    }

    pub fn play_animation(&self, repeat_count: u32, color: gdk::RGBA) {
        let imp = self.imp();

        let animation = imp.animation.get().unwrap();

        animation.reset();

        animation.set_repeat_count(repeat_count);
        imp.circle_color.set(Some(color));

        animation.play();
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
