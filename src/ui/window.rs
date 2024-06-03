use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use crate::{
    application::Application,
    call::{Call, CallState},
    client::{AlertType, Client},
    colors,
    crash_detector::CrashDetector,
    gps::FixMode,
    led::Color,
    location::Location,
    peer::Peer,
    stt::Stt,
    tts,
    ui::{
        call_page::CallPage, crashed_page::CrashedPage, listening_overlay::ListeningOverlay,
        map_view::MapView, peer_row::PeerRow, settings_view::SettingsView,
    },
};

const ALERT_LED_BLINK_INTERVAL: Duration = Duration::from_millis(500);

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) page_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) main_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) gps_status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) view_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) map_view: TemplateChild<MapView>,
        #[template_child]
        pub(super) peer_list_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) sos_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) hazard_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) yielding_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) settings_view: TemplateChild<SettingsView>,
        #[template_child]
        pub(super) call_page: TemplateChild<CallPage>,
        #[template_child]
        pub(super) crashed_page: TemplateChild<CrashedPage>,
        #[template_child]
        pub(super) listening_overlay_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub(super) listening_overlay: TemplateChild<ListeningOverlay>,

        pub(super) client: OnceCell<Client>,

        pub(super) stt: Stt,
        pub(super) stt_segments: RefCell<String>,
        pub(super) stt_is_accepting_segments: Cell<bool>,

        pub(super) crash_detector: CrashDetector,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "DeltaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            CallPage::ensure_type();

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let client = Client::new();

            client.connect_alert_received(clone!(@weak obj => move |_, peer, alert_type| {
                let imp = obj.imp();

                let (text, alert_color) = match alert_type {
                    AlertType::Sos => (
                        format!("{} is in a life-threatening situation", peer.name()),
                        colors::RED_3,
                    ),
                    AlertType::Hazard => (
                        format!("{} is in a hazardous situation", peer.name()),
                        colors::YELLOW_4,
                    ),
                    AlertType::Yielding => (format!("{} is yielding", peer.name()), colors::BLUE_3),
                };

                tts::speak(&text);

                if imp
                    .view_stack
                    .visible_child()
                    .is_some_and(|child| &child == imp.map_view.upcast_ref::<gtk::Widget>())
                {
                    imp.map_view
                        .play_alert_animation(peer, alert_type.blink_count(), alert_color);
                }

                let toast = adw::Toast::builder()
                    .title(text)
                    .priority(adw::ToastPriority::High)
                    .build();

                toast.connect_button_clicked(clone!(@weak obj, @weak peer => move |_| {
                    let imp = obj.imp();

                    let location = peer.location().unwrap();
                    imp.map_view.go_to(location);

                    imp.view_stack.set_visible_child(&*imp.map_view);

                    imp.map_view
                        .play_alert_animation(&peer, alert_type.blink_count(), alert_color);
                }));

                let binding = peer
                    .bind_property("location", &toast, "button-label")
                    .transform_to(|_, location: Option<Location>| Some(location.map(|_| "View")))
                    .sync_create()
                    .build();
                toast.connect_dismissed(move |_| {
                    binding.unbind();
                });

                imp.toast_overlay.add_toast(toast);
            }));
            client.connect_active_call_notify(clone!(@weak obj => move |client| {
                let imp = obj.imp();

                if let Some(active_call) = client.active_call() {
                    debug_assert!(matches!(
                        active_call.state(),
                        CallState::Incoming | CallState::Outgoing
                    ));

                    if active_call.state() == CallState::Incoming {
                        tts::speak(
                            format!("Incoming call from {}", active_call.peer().name()),
                        );
                    }

                    imp.call_page.set_call(Some(active_call.clone()));
                    imp.page_stack.set_visible_child(&*imp.call_page);

                    active_call.connect_state_notify(clone!(@weak obj => move |call| {
                        let imp = obj.imp();

                        match call.state() {
                            CallState::Ended => {
                                imp.call_page.set_call(None::<Call>);
                                imp.page_stack.set_visible_child(&*imp.main_page);
                            }
                            CallState::Ongoing => {}
                            CallState::Init | CallState::Incoming | CallState::Outgoing => {
                                unreachable!()
                            }
                        }
                    }));
                } else {
                    imp.call_page.set_call(None::<Call>);
                    imp.page_stack.set_visible_child(&*imp.main_page);
                }
            }));

            self.map_view
                .connect_called(clone!(@weak client => move |_, peer| {
                    let peer_id = *peer.id();
                    glib::spawn_future_local(async move {
                        if let Err(err) = client.call_request(peer_id).await {
                            tracing::error!("Failed to request call: {:?}", err);
                        }
                    });
                }));

            self.sos_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.publish_alert(AlertType::Sos);
                }));
            self.hazard_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.publish_alert(AlertType::Hazard);
                }));
            self.yielding_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.publish_alert(AlertType::Yielding);
                }));

            self.settings_view
                .connect_crash_simulate_requested(clone!(@weak obj => move |_| {
                    let imp = obj.imp();
                    imp.crash_detector.simulate_crash();
                }));
            self.settings_view.connect_location_override_requested(
                clone!(@weak obj => move |_, location| {
                    let gps = Application::get().gps();
                    gps.override_location(Some(location.clone()));
                }),
            );

            self.call_page
                .connect_incoming_accepted(clone!(@weak client => move |_| {
                    client.call_incoming_accept();
                }));
            self.call_page
                .connect_incoming_declined(clone!(@weak client => move |_| {
                    client.call_incoming_decline();
                }));
            self.call_page
                .connect_outgoing_cancelled(clone!(@weak client => move |_| {
                    glib::spawn_future_local(async move {
                        if let Err(err) = client.call_outgoing_cancel().await {
                            tracing::error!("Failed to cancel outgoing call: {:?}", err);
                        }
                    });
                }));
            self.call_page
                .connect_ongoing_ended(clone!(@weak client => move |_| {
                    if let Err(err) = client.call_ongoing_end() {
                        tracing::error!("Failed to end ongoing call: {:?}", err);
                    }
                }));

            self.listening_overlay
                .connect_cancelled(clone!(@weak obj => move |_| {
                    obj.reset_stt_segments();
                }));

            self.crashed_page.connect_send_alert_requested(
                clone!(@weak obj, @weak client => move |_| {
                    let imp = obj.imp();
                    imp.page_stack.set_visible_child(&*imp.main_page);

                    obj.publish_alert(AlertType::Sos);
                }),
            );
            self.crashed_page
                .connect_ignored(clone!(@weak obj => move |_| {
                    let imp = obj.imp();
                    imp.page_stack.set_visible_child(&*imp.main_page);
                }));

            self.map_view.bind_model(client.peer_list());

            let placeholder_label = gtk::Label::builder()
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .label("No Nearby Peers")
                .build();
            self.peer_list_box.set_placeholder(Some(&placeholder_label));

            self.peer_list_box.bind_model(
                Some(client.peer_list()),
                clone!(@weak obj, @weak client => @default-panic, move |peer| {
                    let peer = peer.downcast_ref::<Peer>().unwrap();

                    let row = PeerRow::new(peer);
                    row.connect_called(clone!(@weak client => move |row| {
                        let peer_id = *row.peer().id();
                        glib::spawn_future_local(async move {
                            if let Err(err) = client.call_request(peer_id).await  {
                                tracing::error!("Failed to request call: {:?}", err);
                            }
                        });
                    }));
                    row.connect_viewed_on_map(clone!(@weak obj => move |row| {
                        let imp = obj.imp();

                        let location = row.peer().location().unwrap();
                        imp.map_view.go_to(location);

                        imp.view_stack.set_visible_child(&*imp.map_view);
                    }));

                    row.upcast()
                }),
            );

            self.stt
                .connect_transcripted(clone!(@weak obj => move |_, message| {
                    obj.handle_stt_segment(message);
                }));

            self.client.set(client.clone()).unwrap();

            self.crash_detector
                .connect_crash_detected(clone!(@weak obj => move |_| {
                    let imp = obj.imp();

                    imp.page_stack.set_visible_child(&*imp.crashed_page);
                }));

            let gps = Application::get().gps();
            gps.connect_fix_mode_notify(clone!(@weak obj => move |_| {
                obj.update_gps_status_icon();
            }));
            gps.connect_location_notify(clone!(@weak obj => move |_| {
                obj.update_location();
            }));
            obj.update_gps_status_icon();
            obj.update_location();

            self.listening_overlay_revealer
                .connect_child_revealed_notify(clone!(@weak obj => move |revealer| {
                    if !revealer.reveals_child() && !revealer.is_child_revealed() {
                        revealer.set_visible(false);
                    }
                }));
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow;
}

impl Window {
    pub fn new(application: &Application) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn publish_alert(&self, alert_type: AlertType) {
        let imp = self.imp();
        let client = imp.client.get().unwrap();

        glib::spawn_future_local(clone!(@weak client => async move {
            client.publish_alert(alert_type).await;
        }));

        match Application::get().alert_led() {
            Ok(alert_led) => {
                let color = match alert_type {
                    AlertType::Sos => Color::Red,
                    AlertType::Hazard => Color::Yellow,
                    AlertType::Yielding => Color::Blue,
                };
                alert_led.blink(color, alert_type.blink_count(), ALERT_LED_BLINK_INTERVAL);
            }
            Err(err) => {
                tracing::error!("Failed to get alert LED: {:?}", err);
            }
        }
    }

    fn reset_stt_segments(&self) {
        let imp = self.imp();

        imp.stt_segments.borrow_mut().clear();
        imp.stt_is_accepting_segments.set(false);

        imp.listening_overlay.set_command("");

        imp.listening_overlay_revealer.set_visible(true);
        imp.listening_overlay_revealer.set_reveal_child(false);
    }

    fn handle_stt_segment(&self, segment: &str) {
        let imp = self.imp();

        let segment = segment.trim().to_lowercase();
        let words = segment
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| c.is_ascii_punctuation()))
            .collect::<Vec<_>>();

        let is_random_noise = segment.trim().starts_with(|c: char| c == '[' || c == '(');
        if is_random_noise && !imp.stt_segments.borrow().is_empty() {
            self.handle_voice_command(imp.stt_segments.borrow().as_str());
            self.reset_stt_segments();
        }

        if imp.stt_is_accepting_segments.get() {
            imp.stt_segments.borrow_mut().push(' ');
            imp.stt_segments
                .borrow_mut()
                .push_str(words.join(" ").as_str());

            imp.listening_overlay
                .set_command(imp.stt_segments.borrow().as_str());
        }

        if let Some(position) = words.iter().position(|w| *w == "delta") {
            imp.stt_segments
                .borrow_mut()
                .push_str(words[(position + 1)..].join(" ").as_str());
            imp.stt_is_accepting_segments.set(true);

            imp.listening_overlay_revealer.set_visible(true);
            imp.listening_overlay_revealer.set_reveal_child(true);
        }
    }

    fn handle_voice_command(&self, command: &str) {
        let imp = self.imp();

        tracing::debug!("Voice command: {}", command);

        let client = imp.client.get().unwrap();

        let mut iter = command.split_whitespace();
        while let Some(word) = iter.next() {
            if client.active_call().is_some() {
                match word {
                    "accept" => {
                        tts::speak("Accepting call");

                        client.call_incoming_accept();

                        break;
                    }
                    "decline" => {
                        tts::speak("Declining call");

                        client.call_incoming_decline();

                        break;
                    }
                    "cancel" => {
                        tts::speak("Cancelling call");

                        glib::spawn_future_local(clone!(@weak client => async move {
                            if let Err(err) = client.call_outgoing_cancel().await {
                                tracing::error!("Failed to cancel outgoing call: {:?}", err);
                            }
                        }));

                        break;
                    }
                    "end" => {
                        tts::speak("Ending call");

                        if let Err(err) = client.call_ongoing_end() {
                            tracing::error!("Failed to end ongoing call: {:?}", err);
                        }

                        break;
                    }
                    _ => {}
                }
            }

            match word {
                "call" if client.active_call().is_none() => {
                    let Some(peer_name) = iter.next() else {
                        break;
                    };

                    let peer = client.peer_list().iter::<Peer>().find_map(|peer| {
                        let peer = peer.unwrap();

                        (peer_name == peer.name()).then_some(peer)
                    });

                    if let Some(peer) = peer {
                        tts::speak(format!("Calling {}", peer.name()));

                        glib::spawn_future_local(clone!(@weak client => async move {
                            if let Err(err) = client.call_request(*peer.id()).await {
                                tracing::error!("Failed to request call: {:?}", err);
                            }
                        }));
                    } else {
                        tts::speak(format!("Peer {} not found", peer_name));
                    }

                    break;
                }
                "alert" => {
                    let Some(alert_type_str) = iter.next() else {
                        break;
                    };

                    let alert_type = match alert_type_str {
                        "sos" => Some(AlertType::Sos),
                        "hazard" => Some(AlertType::Hazard),
                        "yielding" => Some(AlertType::Yielding),
                        _ => None,
                    };

                    if let Some(alert_type) = alert_type {
                        tts::speak(format!("Publishing {} alert", alert_type_str));

                        self.publish_alert(alert_type);
                    } else {
                        tts::speak("Invalid alert type");
                    }

                    break;
                }
                _ => {}
            }
        }
    }

    fn update_gps_status_icon(&self) {
        let imp = self.imp();

        let gps = Application::get().gps();

        match gps.fix_mode() {
            FixMode::None => {
                imp.gps_status_icon.remove_css_class("success");
                imp.gps_status_icon.remove_css_class("warning");

                imp.gps_status_icon.add_css_class("error");
            }
            FixMode::TwoD => {
                imp.gps_status_icon.remove_css_class("success");
                imp.gps_status_icon.remove_css_class("error");

                imp.gps_status_icon.add_css_class("warning");
            }
            FixMode::ThreeD => {
                imp.gps_status_icon.remove_css_class("warning");
                imp.gps_status_icon.remove_css_class("error");

                imp.gps_status_icon.add_css_class("success");
            }
        }
    }

    fn update_location(&self) {
        let imp = self.imp();

        let gps = Application::get().gps();
        let location = gps.location();

        let client = imp.client.get().unwrap();
        client.set_location(location.clone());

        imp.map_view.set_location(location);
    }
}
