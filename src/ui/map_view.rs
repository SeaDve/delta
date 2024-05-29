use std::time::Duration;

use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use shumate::prelude::*;

use crate::{location::Location, peer::Peer, peer_list::PeerList, ui::peer_marker::PeerMarker};

const DEFAULT_ZOOM_LEVEL: f64 = 16.0;
const DEFAULT_GO_TO_DURATION: Duration = Duration::from_secs(1);

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
        pub(super) overlay: TemplateChild<gtk::Overlay>, // Unused
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,
        #[template_child]
        pub(super) compass: TemplateChild<shumate::Compass>,
        #[template_child]
        pub(super) return_button: TemplateChild<gtk::Button>,

        pub(super) location: RefCell<Option<Location>>,

        pub(super) marker_layer: OnceCell<shumate::MarkerLayer>,
        pub(super) our_marker: OnceCell<shumate::Marker>,
        pub(super) peer_markers: RefCell<Vec<PeerMarker>>,
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
                    obj.go_to(our_marker.latitude(), our_marker.longitude());
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

                    let marker = PeerMarker::new();
                    marker.set_peer(Some(peer.clone()));

                    marker.connect_called(clone!(@weak obj => move |marker| {
                        let peer = marker.peer().unwrap();
                        obj.emit_by_name::<()>("called", &[&peer]);
                    }));

                    imp.marker_layer.get().unwrap().add_marker(&marker);

                    marker
                });
                let removed = imp
                    .peer_markers
                    .borrow_mut()
                    .splice(position as usize..(removed + position) as usize, new_markers)
                    .collect::<Vec<_>>();

                for marker in removed {
                    imp.marker_layer.get().unwrap().remove_marker(&marker);
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
            imp.map.center_on(location.latitude, location.longitude);
        }

        imp.location.replace(location);

        self.update_return_button_sensitivity();
    }

    pub fn location(&self) -> Option<Location> {
        self.imp().location.borrow().clone()
    }

    pub fn go_to(&self, latitude: f64, longitude: f64) {
        let imp = self.imp();

        imp.map.go_to_full_with_duration(
            latitude,
            longitude,
            DEFAULT_ZOOM_LEVEL,
            DEFAULT_GO_TO_DURATION.as_millis() as u32,
        );
    }

    fn update_return_button_sensitivity(&self) {
        let imp = self.imp();

        imp.return_button.set_sensitive(self.location().is_some());
    }
}
