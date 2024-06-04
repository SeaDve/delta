use std::time::Duration;

use gtk::{
    gdk,
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use shumate::prelude::*;

use crate::{
    location::Location,
    peer::Peer,
    peer_list::PeerList,
    ui::{alert_marker::AlertMarker, peer_marker::PeerMarker},
};

const DEFAULT_ZOOM_LEVEL: f64 = 20.0;
const GO_TO_DURATION: Duration = Duration::from_secs(1);

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "map_view.ui")]
    pub struct MapView {
        #[template_child]
        pub(super) hbox: TemplateChild<gtk::Box>, // Unused
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,
        #[template_child]
        pub(super) compass: TemplateChild<shumate::Compass>,
        #[template_child]
        pub(super) return_button: TemplateChild<gtk::Button>,

        pub(super) location: RefCell<Option<Location>>,

        pub(super) marker_layer: OnceCell<shumate::MarkerLayer>,
        pub(super) our_marker: OnceCell<shumate::Marker>,
        pub(super) peer_markers: RefCell<Vec<(Peer, PeerMarker, AlertMarker)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MapView {
        const NAME: &'static str = "DeltaMapView";
        type Type = super::MapView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MapView {
        fn constructed(&self) {
            self.parent_constructed();

            let registry = shumate::MapSourceRegistry::with_defaults();
            let source = registry.by_id(shumate::MAP_SOURCE_OSM_MAPNIK).unwrap();

            self.map.set_map_source(&source);

            let viewport = self.map.viewport().unwrap();
            viewport.set_reference_map_source(Some(&source));

            self.compass.set_viewport(Some(&viewport));

            let map_layer = shumate::MapLayer::new(&source, &viewport);
            self.map.add_layer(&map_layer);

            let marker_layer = shumate::MarkerLayer::new(&viewport);
            self.map.add_layer(&marker_layer);
            self.marker_layer.set(marker_layer).unwrap();

            let image = gtk::Image::from_icon_name("map-marker-symbolic");
            image.add_css_class("map-marker");

            let marker = shumate::Marker::new();
            marker.set_child(Some(&image));
            self.marker_layer.get().unwrap().add_marker(&marker);

            self.our_marker.set(marker).unwrap();

            let obj = self.obj();

            self.return_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    let imp = obj.imp();

                    let our_marker = imp.our_marker.get().unwrap();
                    obj.go_to(Location {
                        latitude: our_marker.latitude(),
                        longitude: our_marker.longitude(),
                    });
                }));

            obj.set_location(None);
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![Signal::builder("called")
                    .param_types([Peer::static_type()])
                    .build()]
            })
        }
    }

    impl WidgetImpl for MapView {
        fn map(&self) {
            self.parent_map();

            let viewport = self.map.viewport().unwrap();
            viewport.set_zoom_level(DEFAULT_ZOOM_LEVEL);
        }
    }
}

glib::wrapper! {
    pub struct MapView(ObjectSubclass<imp::MapView>)
        @extends gtk::Widget;
}

impl MapView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_called<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Peer) + 'static,
    {
        self.connect_closure(
            "called",
            false,
            closure_local!(|obj: &Self, peer: &Peer| f(obj, peer)),
        )
    }

    pub fn bind_model(&self, model: &PeerList) {
        model.connect_items_changed(
            clone!(@weak self as obj => move |model, position, removed, added| {
                let imp = obj.imp();

                let new_markers = (0..added).map(|i| {
                    let peer = model
                        .item(position + i)
                        .unwrap()
                        .downcast::<Peer>()
                        .unwrap();

                    let peer_marker = PeerMarker::new();
                    peer_marker.set_peer(Some(peer.clone()));

                    peer_marker.connect_called(clone!(@weak obj => move |marker| {
                        let peer = marker.peer().unwrap();
                        obj.emit_by_name::<()>("called", &[&peer]);
                    }));

                    let alert_marker = AlertMarker::new();
                    alert_marker.set_peer(Some(peer.clone()));

                    let marker_layer = imp.marker_layer.get().unwrap();
                    marker_layer.add_marker(&peer_marker);
                    marker_layer.add_marker(&alert_marker);

                    (peer, peer_marker, alert_marker)
                });
                let removed = imp
                    .peer_markers
                    .borrow_mut()
                    .splice(
                        position as usize..(removed + position) as usize,
                        new_markers,
                    )
                    .collect::<Vec<_>>();

                for (_, peer_marker, alert_marker) in removed {
                    let marker_layer = imp.marker_layer.get().unwrap();
                    marker_layer.remove_marker(&peer_marker);
                    marker_layer.remove_marker(&alert_marker);
                }
            }),
        );
    }

    pub fn set_location(&self, location: Option<Location>) {
        let imp = self.imp();

        let our_marker = imp.our_marker.get().unwrap();
        our_marker.set_visible(location.is_some());

        if let Some(location) = &location {
            our_marker.set_location(location.latitude, location.longitude);

            let viewport = imp.map.viewport().unwrap();
            if viewport.latitude() == 0.0 && viewport.longitude() == 0.0 {
                imp.map.center_on(location.latitude, location.longitude);
            }
        }

        imp.location.replace(location);

        self.update_return_button_sensitivity();
    }

    pub fn location(&self) -> Option<Location> {
        self.imp().location.borrow().clone()
    }

    pub fn go_to(&self, location: Location) {
        let imp = self.imp();

        imp.map.go_to_full_with_duration(
            location.latitude,
            location.longitude,
            DEFAULT_ZOOM_LEVEL,
            GO_TO_DURATION.as_millis() as u32,
        );
    }

    pub fn play_alert_animation(&self, peer: &Peer, repeat_count: u32, color: gdk::RGBA) {
        let imp = self.imp();

        if let Some((_, _, alert_marker)) =
            imp.peer_markers.borrow().iter().find(|(p, _, _)| p == peer)
        {
            alert_marker.play_animation(repeat_count, color);
        } else {
            tracing::warn!("Failed to play alert animation: No marker found for peer");
        }
    }

    fn update_return_button_sensitivity(&self) {
        let imp = self.imp();

        imp.return_button.set_sensitive(self.location().is_some());
    }
}
