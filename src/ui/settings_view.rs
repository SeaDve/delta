use gtk::{
    gdk,
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use shumate::prelude::*;

use crate::{location::Location, Application};

const DEFAULT_MAP_ZOOM_LEVEL: f64 = 16.0;

mod imp {
    use std::{cell::OnceCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "settings_view.ui")]
    pub struct SettingsView {
        #[template_child]
        pub(super) simulate_crash_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,

        pub(super) marker: OnceCell<shumate::Marker>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SettingsView {
        const NAME: &'static str = "DeltaSettingsView";
        type Type = super::SettingsView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SettingsView {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.simulate_crash_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("crash-simulated", &[]);
                }));

            let viewport = self.map.viewport().unwrap();
            let registry = shumate::MapSourceRegistry::with_defaults();
            let source = registry.by_id(shumate::MAP_SOURCE_OSM_MAPNIK).unwrap();
            viewport.set_reference_map_source(Some(&source));

            let map_layer = shumate::MapLayer::new(&source, &viewport);
            self.map.add_layer(&map_layer);

            let marker_layer = shumate::MarkerLayer::new(&viewport);
            self.map.add_layer(&marker_layer);

            let marker = shumate::Marker::new();
            marker_layer.add_marker(&marker);

            let image = gtk::Image::from_icon_name("map-marker-symbolic");
            image.add_css_class("map-marker");
            marker.set_child(Some(&image));
            self.marker.set(marker).unwrap();

            let gps = Application::get().gps();
            gps.connect_location_notify(clone!(@weak obj => move |_| {
                obj.update_marker_location();
            }));

            let gesture_click = gtk::GestureClick::builder()
                .button(gdk::BUTTON_SECONDARY)
                .build();
            gesture_click.connect_pressed(clone!(@weak obj => move |_, _n_press, x, y| {
                let imp = obj.imp();

                let viewport = imp.map.viewport().unwrap();
                let (latitude, longitude) = viewport.widget_coords_to_location(&*imp.map, x, y);

                obj.emit_by_name::<()>(
                    "location-override-requested",
                    &[&Location {
                        latitude,
                        longitude,
                    }],
                );
            }));
            self.map.add_controller(gesture_click);

            obj.update_marker_location()
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("crash-simulated").build(),
                    Signal::builder("location-override-requested")
                        .param_types([Location::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for SettingsView {
        fn map(&self) {
            self.parent_map();

            let viewport = self.map.viewport().unwrap();
            viewport.set_zoom_level(DEFAULT_MAP_ZOOM_LEVEL);
        }
    }
}

glib::wrapper! {
    pub struct SettingsView(ObjectSubclass<imp::SettingsView>)
        @extends gtk::Widget;
}

impl SettingsView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_crash_simulated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "crash-simulated",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    pub fn connect_location_override_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Location) + 'static,
    {
        self.connect_closure(
            "location-override-requested",
            false,
            closure_local!(move |obj: &Self, location: &Location| f(obj, location)),
        )
    }

    fn update_marker_location(&self) {
        let imp = self.imp();

        let location = Application::get().gps().location();

        let marker = imp.marker.get().unwrap();
        marker.set_visible(location.is_some());

        if let Some(location) = location {
            marker.set_location(location.latitude, location.longitude);

            let viewport = imp.map.viewport().unwrap();
            if viewport.latitude() == 0.0 && viewport.longitude() == 0.0 {
                imp.map.center_on(location.latitude, location.longitude);
            }
        }
    }
}
