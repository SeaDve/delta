use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone, closure_local};

use crate::{peer::Peer, ui::toggle_button::ToggleButton, Application};

mod imp {
    use std::{cell::OnceCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PeerRow)]
    #[template(file = "peer_row.ui")]
    pub struct PeerRow {
        #[property(get, set, construct_only)]
        pub(super) peer: OnceCell<Peer>,

        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) wireless_status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) call_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) view_on_map_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) mute_button: TemplateChild<ToggleButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerRow {
        const NAME: &'static str = "DeltaPeerRow";
        type Type = super::PeerRow;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PeerRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.call_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.emit_by_name::<()>("called", &[]);
                }
            ));
            self.view_on_map_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.emit_by_name::<()>("viewed-on-map", &[]);
                }
            ));
            self.mute_button.connect_is_active_notify(clone!(
                #[weak]
                obj,
                move |button| {
                    let settings = Application::get().settings();
                    let peer = obj.peer();

                    if button.is_active() {
                        settings.insert_muted_peer(peer.name());
                    } else {
                        settings.remove_muted_peer(&peer.name());
                    }
                }
            ));

            let peer = obj.peer();
            peer.bind_property("name", &*obj, "title")
                .sync_create()
                .build();
            peer.bind_property("icon-name", &*self.image, "icon-name")
                .sync_create()
                .build();
            peer.connect_name_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_mute_button();
                }
            ));
            peer.connect_location_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_subtitle();
                    obj.update_view_on_map_button_sensitivity();
                }
            ));
            peer.connect_speed_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_subtitle();
                }
            ));
            peer.connect_signal_quality_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_wireless_status_icon();
                }
            ));

            let app = Application::get();

            app.gps().connect_location_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_subtitle();
                }
            ));

            app.settings().connect_muted_peers_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_mute_button();
                }
            ));

            obj.update_subtitle();
            obj.update_view_on_map_button_sensitivity();
            obj.update_mute_button();
            obj.update_wireless_status_icon();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("called").build(),
                    Signal::builder("viewed-on-map").build(),
                ]
            })
        }
    }

    impl WidgetImpl for PeerRow {}
    impl ListBoxRowImpl for PeerRow {}
    impl PreferencesRowImpl for PeerRow {}
    impl ActionRowImpl for PeerRow {}
}

glib::wrapper! {
    pub struct PeerRow(ObjectSubclass<imp::PeerRow>)
        @extends gtk::Widget, gtk::ListBoxRow, adw::PreferencesRow, adw::ActionRow;
}

impl PeerRow {
    pub fn new(peer: &Peer) -> Self {
        glib::Object::builder().property("peer", peer).build()
    }

    pub fn connect_called<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("called", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn connect_viewed_on_map<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("viewed-on-map", false, closure_local!(|obj: &Self| f(obj)))
    }

    fn update_subtitle(&self) {
        let peer = self.peer();

        let distance_str = peer.location().and_then(|location| {
            Application::get()
                .gps()
                .location()
                .map(|l| format!("{:.2} m away", l.distance(&location)))
        });
        let speed_str = format!("{:.2} m/s", peer.speed());

        let subtitle = [distance_str, Some(speed_str)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" • ");
        self.set_subtitle(&subtitle);
    }

    fn update_wireless_status_icon(&self) {
        let imp = self.imp();

        let signal_quality = self.peer().signal_quality();

        imp.wireless_status_icon
            .set_icon_name(Some(signal_quality.icon_name()));

        signal_quality.apply_css_class_to_image(&imp.wireless_status_icon);
    }

    fn update_view_on_map_button_sensitivity(&self) {
        let imp = self.imp();

        let location = self.peer().location();
        imp.view_on_map_button.set_sensitive(location.is_some());
    }

    fn update_mute_button(&self) {
        let imp = self.imp();

        let is_muted = Application::get()
            .settings()
            .muted_peers()
            .contains(&self.peer().name());
        imp.mute_button.set_is_active(is_muted);
    }
}
