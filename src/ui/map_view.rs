use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use shumate::prelude::*;

use crate::{
    peer::{Location, Peer},
    peer_list::PeerList,
};

mod imp {
    use std::cell::{OnceCell, RefCell};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "map_view.ui")]
    pub struct MapView {
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,

        pub(super) marker_layer: OnceCell<shumate::MarkerLayer>,
        pub(super) our_marker: OnceCell<shumate::Marker>,
        pub(super) peers: RefCell<Vec<(Peer, shumate::Marker, Vec<glib::Binding>)>>,
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

            let map_layer = shumate::MapLayer::new(&source, &viewport);
            self.map.add_layer(&map_layer);

            let marker_layer = shumate::MarkerLayer::new(&viewport);
            self.map.add_layer(&marker_layer);
            self.marker_layer.set(marker_layer).unwrap();

            let image = gtk::Image::from_icon_name("driving-symbolic");
            image.add_css_class("map-marker");

            let marker = shumate::Marker::new();
            marker.set_child(Some(&image));
            self.marker_layer.get().unwrap().add_marker(&marker);

            self.our_marker.set(marker).unwrap();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for MapView {}
}

glib::wrapper! {
    pub struct MapView(ObjectSubclass<imp::MapView>)
        @extends gtk::Widget;
}

impl MapView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn bind_model(&self, model: &PeerList) {
        model.connect_items_changed(
            clone!(@weak self as obj => move |model, position, removed, added| {
                let imp = obj.imp();

                let new_peers = (0..added).map(|i| {
                    let peer = model
                        .item(position + i)
                        .unwrap()
                        .downcast::<Peer>()
                        .unwrap();

                    let hbox = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(6)
                        .build();

                    let image = gtk::Image::builder()
                        .icon_name("driving-symbolic")
                        .halign(gtk::Align::Center)
                        .build();
                    image.add_css_class("map-marker");
                    hbox.append(&image);

                    let label = gtk::Label::builder().build();
                    hbox.append(&label);

                    let marker = shumate::Marker::new();
                    marker.set_child(Some(&hbox));

                    imp.marker_layer.get().unwrap().add_marker(&marker);

                    let peer_bindings = vec![
                        peer.bind_property("name", &label, "label")
                            .sync_create()
                            .build(),
                        peer.bind_property("location", &marker, "latitude")
                            .transform_to(|_, location: Option<Location>| {
                                Some(
                                    location
                                        .map(|location| location.latitude)
                                        .unwrap_or_default(),
                                )
                            })
                            .sync_create()
                            .build(),
                        peer.bind_property("location", &marker, "longitude")
                            .transform_to(|_, location: Option<Location>| {
                                Some(
                                    location
                                        .map(|location| location.longitude)
                                        .unwrap_or_default(),
                                )
                            })
                            .sync_create()
                            .build(),
                    ];

                    (peer, marker, peer_bindings)
                });
                let removed = imp
                    .peers
                    .borrow_mut()
                    .splice(position as usize..(removed + position) as usize, new_peers)
                    .collect::<Vec<_>>();

                for (_, marker, _) in removed {
                    imp.marker_layer.get().unwrap().remove_marker(&marker);
                }
            }),
        );
    }

    pub fn set_location(&self, latitude: f64, longitude: f64) {
        let imp = self.imp();

        imp.our_marker
            .get()
            .unwrap()
            .set_location(latitude, longitude);
        imp.map.center_on(latitude, longitude);
    }
}
