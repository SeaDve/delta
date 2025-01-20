use std::time::Duration;

use anyhow::Result;
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
    place_finder::{Place, PlaceFinder, PlaceType},
    ui::{alert_marker::AlertMarker, peer_marker::PeerMarker, place_marker::PlaceMarker},
};

const DEFAULT_ZOOM_LEVEL: f64 = 20.0;
const GO_TO_DURATION: Duration = Duration::from_secs(1);

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
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
        pub(super) places_toolbar: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,
        #[template_child]
        pub(super) compass: TemplateChild<shumate::Compass>,
        #[template_child]
        pub(super) place_control_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub(super) prev_place_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) next_place_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) unshow_places_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) return_button: TemplateChild<gtk::Button>,

        pub(super) location: RefCell<Option<Location>>,

        pub(super) marker_layer: OnceCell<shumate::MarkerLayer>,
        pub(super) our_marker: OnceCell<shumate::Marker>,
        pub(super) peer_markers: RefCell<Vec<(Peer, PeerMarker, AlertMarker)>>,

        pub(super) places_marker_layer: OnceCell<shumate::MarkerLayer>,
        pub(super) place_finder: PlaceFinder,

        pub(super) shown_places: RefCell<Vec<Place>>,
        pub(super) shown_place_index: Cell<Option<usize>>,

        pub(super) initial_zoom_done: Cell<bool>,
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

            let places_marker_layer = shumate::MarkerLayer::new(&viewport);
            self.map.add_layer(&places_marker_layer);
            self.places_marker_layer.set(places_marker_layer).unwrap();

            let obj = self.obj();

            self.return_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    let imp = obj.imp();

                    let our_marker = imp.our_marker.get().unwrap();
                    obj.go_to(&Location {
                        latitude: our_marker.latitude(),
                        longitude: our_marker.longitude(),
                    });
                }
            ));

            self.place_control_revealer
                .connect_child_revealed_notify(|revealer| {
                    if !revealer.reveals_child() && !revealer.is_child_revealed() {
                        revealer.set_visible(false);
                    }
                });
            self.prev_place_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.go_to_prev_place();
                }
            ));
            self.next_place_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.go_to_next_place();
                }
            ));
            self.unshow_places_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.unshow_places();
                }
            ));

            for place_type in PlaceType::all() {
                let button = gtk::Button::builder()
                    .tooltip_text(place_type.to_string())
                    .icon_name(place_type.icon_name())
                    .build();
                self.places_toolbar.append(&button);

                button.connect_clicked(clone!(
                    #[weak]
                    obj,
                    move |_| {
                        glib::spawn_future_local(async move {
                            if let Err(err) = obj.show_places_and_go_to_nearest(*place_type).await {
                                tracing::warn!("Failed to show places: {:?}", err);
                            }
                        });
                    }
                ));
            }

            obj.update_place_control_sensitivity();
            obj.set_location(None);
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("called")
                        .param_types([Peer::static_type()])
                        .build(),
                    Signal::builder("show-place-requested")
                        .param_types([Place::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for MapView {
        fn map(&self) {
            self.parent_map();

            if !self.initial_zoom_done.get() {
                let viewport = self.map.viewport().unwrap();
                viewport.set_zoom_level(DEFAULT_ZOOM_LEVEL);

                self.initial_zoom_done.set(true);
            }
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

    pub fn connect_show_place_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Place) + 'static,
    {
        self.connect_closure(
            "show-place-requested",
            false,
            closure_local!(move |obj: &Self, place: &Place| f(obj, place)),
        )
    }

    pub fn bind_model(&self, model: &PeerList) {
        model.connect_items_changed(clone!(
            #[weak(rename_to = obj)]
            self,
            move |model, position, removed, added| {
                let imp = obj.imp();

                let new_markers = (0..added).map(|i| {
                    let peer = model
                        .item(position + i)
                        .unwrap()
                        .downcast::<Peer>()
                        .unwrap();

                    let peer_marker = PeerMarker::new();
                    peer_marker.set_peer(Some(peer.clone()));

                    peer_marker.connect_called(clone!(
                        #[weak]
                        obj,
                        move |marker| {
                            let peer = marker.peer().unwrap();
                            obj.emit_by_name::<()>("called", &[&peer]);
                        }
                    ));

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
            }
        ));
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

    pub fn go_to(&self, location: &Location) {
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

    pub async fn show_places_and_go_to_nearest(&self, place_type: PlaceType) -> Result<()> {
        let imp = self.imp();

        self.unshow_places();

        let places_marker_layer = imp.places_marker_layer.get().unwrap();

        let places = imp.place_finder.find(place_type).await?;

        for place in places {
            let place_marker = PlaceMarker::new(place);
            place_marker.connect_show_place_requested(clone!(
                #[weak(rename_to = obj)]
                self,
                move |place_marker| {
                    let place = place_marker.place();
                    obj.emit_by_name::<()>("show-place-requested", &[&place]);
                }
            ));
            places_marker_layer.add_marker(&place_marker);
        }

        let mut place_vec = places.to_vec();

        if let Some(location) = self.location() {
            place_vec.sort_by(|a, b| {
                a.location()
                    .distance(&location)
                    .partial_cmp(&b.location().distance(&location))
                    .unwrap()
            });
        }

        if let Some(nearest_place) = place_vec.first() {
            self.go_to(nearest_place.location());

            imp.shown_place_index.set(Some(0));
        }

        imp.shown_places.replace(place_vec);

        imp.place_control_revealer.set_visible(true);
        imp.place_control_revealer.set_reveal_child(true);

        self.update_place_control_sensitivity();

        Ok(())
    }

    pub fn is_showing_places(&self) -> bool {
        self.imp().shown_place_index.get().is_some()
    }

    pub fn unshow_places(&self) {
        let imp = self.imp();

        let places_marker_layer = imp.places_marker_layer.get().unwrap();
        places_marker_layer.remove_all();

        imp.shown_places.replace(Vec::new());
        imp.shown_place_index.set(None);

        imp.place_control_revealer.set_visible(true);
        imp.place_control_revealer.set_reveal_child(false);

        self.update_place_control_sensitivity();
    }

    pub fn go_to_prev_place(&self) {
        let imp = self.imp();

        let shown_places = imp.shown_places.borrow();
        let shown_place_index = imp.shown_place_index.get();

        if let Some(index) = shown_place_index {
            if index > 0 {
                let prev_place_index = index - 1;
                let prev_place = &shown_places[prev_place_index];

                self.go_to(prev_place.location());
                imp.shown_place_index.set(Some(prev_place_index));
            }
        }

        self.update_place_control_sensitivity();
    }

    pub fn go_to_next_place(&self) {
        let imp = self.imp();

        let shown_places = imp.shown_places.borrow();
        let shown_place_index = imp.shown_place_index.get();

        if let Some(index) = shown_place_index {
            if index + 1 < shown_places.len() {
                let next_place_index = index + 1;
                let next_place = &shown_places[next_place_index];

                self.go_to(next_place.location());
                imp.shown_place_index.set(Some(next_place_index));
            }
        }

        self.update_place_control_sensitivity();
    }

    fn update_place_control_sensitivity(&self) {
        let imp = self.imp();

        let shown_place_index = imp.shown_place_index.get();
        let shown_places = imp.shown_places.borrow();

        imp.prev_place_button
            .set_sensitive(shown_place_index.is_some_and(|i| i > 0));
        imp.next_place_button
            .set_sensitive(shown_place_index.is_some_and(|i| i + 1 < shown_places.len()));
    }

    fn update_return_button_sensitivity(&self) {
        let imp = self.imp();

        imp.return_button.set_sensitive(self.location().is_some());
    }
}
