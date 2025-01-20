use std::process::Command;

use adw::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone, closure, closure_local},
    subclass::prelude::*,
};
use shumate::prelude::*;

use crate::{location::Location, remote::RemoteStatus, settings::AllowedPeers, Application};

const DEFAULT_MAP_ZOOM_LEVEL: f64 = 16.0;
const ICON_LIST: &[&str] = &[
    "driving-symbolic",
    "bus-symbolic",
    "ambulance-symbolic",
    "cycling-symbolic",
];

mod imp {
    use std::{
        cell::{Cell, OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Delta/ui/settings_view.ui")]
    pub struct SettingsView {
        #[template_child]
        pub(super) page: TemplateChild<adw::PreferencesPage>, // Unused
        #[template_child]
        pub(super) icon_flow_box: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub(super) allowed_peers_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) allowed_peers_model: TemplateChild<adw::EnumListModel>,
        #[template_child]
        pub(super) muted_peers_row: TemplateChild<adw::ExpanderRow>,
        #[template_child]
        pub(super) remote_ip_addr_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) remote_status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) simulate_crash_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) quit_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) shutdown_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) map: TemplateChild<shumate::Map>,

        pub(super) marker: OnceCell<shumate::Marker>,

        pub(super) initial_zoom_done: Cell<bool>,

        pub(super) muted_peers_row_items: RefCell<Vec<adw::ActionRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SettingsView {
        const NAME: &'static str = "DeltaSettingsView";
        type Type = super::SettingsView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            AllowedPeers::ensure_type();

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

            let app = Application::get();

            let settings = app.settings();
            settings.connect_muted_peers_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_muted_peers_row_items();
                }
            ));

            let remote = app.remote();
            remote.connect_status_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_remote_status_label();
                }
            ));

            let icon_model = gtk::StringList::new(ICON_LIST);
            self.icon_flow_box.bind_model(Some(&icon_model), |item| {
                let string_obj = item.downcast_ref::<gtk::StringObject>().unwrap();

                let image = gtk::Image::builder()
                    .halign(gtk::Align::Center)
                    .icon_name(string_obj.string())
                    .build();
                image.add_css_class("small-icon");

                image.upcast()
            });

            let icon_name_index = ICON_LIST
                .iter()
                .position(|icon_name| *icon_name == settings.icon_name())
                .unwrap();
            self.icon_flow_box.select_child(
                &self
                    .icon_flow_box
                    .child_at_index(icon_name_index as i32)
                    .unwrap(),
            );
            self.icon_flow_box
                .connect_selected_children_changed(|flow_box| {
                    let selected_children = flow_box.selected_children();
                    debug_assert_eq!(selected_children.len(), 1);

                    let selected_child = selected_children.first().unwrap();
                    let icon_name = selected_child
                        .child()
                        .unwrap()
                        .downcast_ref::<gtk::Image>()
                        .unwrap()
                        .icon_name()
                        .unwrap();

                    let app = Application::get();
                    let settings = app.settings();
                    settings.set_icon_name(icon_name);
                });

            self.simulate_crash_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.emit_by_name::<()>("crash-simulate-requested", &[]);
                }
            ));
            self.remote_ip_addr_row.set_text(&settings.remote_ip_addr());
            self.remote_ip_addr_row.connect_apply(|entry| {
                Application::get()
                    .settings()
                    .set_remote_ip_addr(entry.text().trim());
            });
            self.quit_button.connect_clicked(|_| {
                Application::get().quit();
            });
            self.shutdown_button.connect_clicked(|_| {
                if let Err(err) = Command::new("shutdown").arg("now").spawn() {
                    tracing::error!("Failed to run shutdown command: {:?}", err);
                }
            });

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

            let gps = app.gps();
            gps.connect_location_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_marker_location();
                }
            ));

            let gesture_click = gtk::GestureClick::builder()
                .button(gdk::BUTTON_SECONDARY)
                .build();
            gesture_click.connect_pressed(clone!(
                #[weak]
                obj,
                move |_, _n_press, x, y| {
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
                }
            ));
            self.map.add_controller(gesture_click);

            self.allowed_peers_row.set_selected(
                self.allowed_peers_model
                    .find_position(settings.allowed_peers() as i32),
            );
            self.allowed_peers_row
                .set_expression(Some(&gtk::ClosureExpression::new::<glib::GString>(
                    &[] as &[gtk::Expression],
                    closure!(|list_item: adw::EnumListItem| {
                        AllowedPeers::try_from(list_item.value())
                            .unwrap()
                            .to_string()
                    }),
                )));
            self.allowed_peers_row.connect_selected_notify(|row| {
                let app = Application::get();
                let settings = app.settings();

                if let Some(ref item) = row.selected_item() {
                    let value = item.downcast_ref::<adw::EnumListItem>().unwrap().value();
                    settings.set_allowed_peers(AllowedPeers::try_from(value).unwrap());
                } else {
                    tracing::warn!("Allowed peers row doesn't have a selected item");
                    settings.set_allowed_peers(AllowedPeers::default());
                }
            });

            obj.update_marker_location();
            obj.update_muted_peers_row_items();
            obj.update_remote_status_label();
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("crash-simulate-requested").build(),
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

            if !self.initial_zoom_done.get() {
                let viewport = self.map.viewport().unwrap();
                viewport.set_zoom_level(DEFAULT_MAP_ZOOM_LEVEL);

                self.initial_zoom_done.set(true);
            }
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

    pub fn connect_crash_simulate_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "crash-simulate-requested",
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

    fn update_muted_peers_row_items(&self) {
        let imp = self.imp();

        let settings = Application::get().settings();

        for row in imp.muted_peers_row_items.take() {
            imp.muted_peers_row.remove(&row);
        }

        let muted_peers = settings.muted_peers();

        for peer_name in muted_peers.iter() {
            let row = adw::ActionRow::builder().title(peer_name).build();

            let unmute_button = gtk::Button::builder()
                .icon_name("edit-delete-symbolic")
                .valign(gtk::Align::Center)
                .build();
            unmute_button.add_css_class("flat");
            unmute_button.connect_clicked(clone!(
                #[strong]
                peer_name,
                #[weak]
                settings,
                move |_| {
                    settings.remove_muted_peer(&peer_name);
                }
            ));
            row.add_suffix(&unmute_button);

            imp.muted_peers_row.add_row(&row);

            imp.muted_peers_row_items.borrow_mut().push(row);
        }

        imp.muted_peers_row
            .set_enable_expansion(!muted_peers.is_empty());
    }

    fn update_remote_status_label(&self) {
        let imp = self.imp();

        match Application::get().remote().status() {
            RemoteStatus::Disconnected => {
                imp.remote_status_label.set_text("X");
                imp.remote_status_label.add_css_class("error");
                imp.remote_status_label.remove_css_class("success");
            }
            RemoteStatus::Connected => {
                imp.remote_status_label.set_text("OK");
                imp.remote_status_label.add_css_class("success");
                imp.remote_status_label.remove_css_class("error");
            }
            RemoteStatus::Error(error) => {
                imp.remote_status_label.set_text(&error);
                imp.remote_status_label.add_css_class("error");
                imp.remote_status_label.remove_css_class("success");
            }
        }
    }
}
