use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use crate::{
    application::Application,
    call::{Call, CallState},
    client::{BroadcastType, Client},
    gps::FixMode,
    peer::Peer,
    stt::Stt,
    tts,
    ui::{
        call_page::CallPage, listening_page::ListeningPage, map_view::MapView, peer_row::PeerRow,
    },
};

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) main_stack: TemplateChild<gtk::Stack>,
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
        pub(super) call_page: TemplateChild<CallPage>,
        #[template_child]
        pub(super) listening_page: TemplateChild<ListeningPage>,

        pub(super) client: OnceCell<Client>,

        pub(super) stt: Stt,
        pub(super) stt_segments: RefCell<String>,
        pub(super) stt_is_accepting_segments: Cell<bool>,
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

            client.connect_broadcast_received(clone!(@weak obj => move |_, peer, broadcast| {
                let imp = obj.imp();

                let text = match broadcast {
                    BroadcastType::Sos => {
                        format!("{} is in a life-threatening situation", peer.name())
                    }
                    BroadcastType::Hazard => {
                        format!("{} is in a hazardous situation", peer.name())
                    }
                    BroadcastType::Yielding => {
                        format!("{} is yielding", peer.name())
                    }
                };

                tts::speak(&text);

                let toast = adw::Toast::new(&text);
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
                    imp.main_stack.set_visible_child(&*imp.call_page);

                    active_call.connect_state_notify(clone!(@weak obj => move |call| {
                        let imp = obj.imp();

                        match call.state() {
                            CallState::Ended => {
                                imp.call_page.set_call(None::<Call>);
                                imp.main_stack.set_visible_child(&*imp.main_page);
                            }
                            CallState::Ongoing => {}
                            CallState::Init | CallState::Incoming | CallState::Outgoing => {
                                unreachable!()
                            }
                        }
                    }));
                } else {
                    imp.call_page.set_call(None::<Call>);
                    imp.main_stack.set_visible_child(&*imp.main_page);
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
                .connect_clicked(clone!(@weak client => move |_| {
                    glib::spawn_future_local(async move {
                         client.broadcast(BroadcastType::Sos).await;
                    });
                }));
            self.hazard_button
                .connect_clicked(clone!(@weak client => move |_| {
                    glib::spawn_future_local(async move {
                        client.broadcast(BroadcastType::Hazard).await;
                    });
                }));
            self.yielding_button
                .connect_clicked(clone!(@weak client => move |_| {
                    glib::spawn_future_local(async move {
                        client.broadcast(BroadcastType::Yielding).await;
                    });
                }));

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
                    glib::spawn_future_local(async move {
                        if let Err(err) = client.call_ongoing_end() {
                            tracing::error!("Failed to end ongoing call: {:?}", err);
                        }
                    });
                }));

            self.listening_page
                .connect_cancelled(clone!(@weak obj => move |_|{
                    obj.reset_stt_segments();
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
                        imp.map_view.go_to(location.latitude, location.longitude);

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

            let gps = Application::get().gps();
            gps.connect_fix_mode_notify(clone!(@weak obj => move |_| {
                obj.update_gps_status_icon();
            }));
            gps.connect_location_notify(clone!(@weak obj => move |_| {
                obj.update_location();
            }));
            obj.update_gps_status_icon();
            obj.update_location();
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

    fn reset_stt_segments(&self) {
        let imp = self.imp();

        imp.stt_segments.borrow_mut().clear();
        imp.stt_is_accepting_segments.set(false);

        imp.listening_page.set_command("");
        imp.main_stack.set_visible_child(&*imp.main_page);
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

            imp.listening_page
                .set_command(imp.stt_segments.borrow().as_str());
        }

        if let Some(position) = words.iter().position(|w| *w == "delta") {
            imp.stt_segments
                .borrow_mut()
                .push_str(words[(position + 1)..].join(" ").as_str());
            imp.stt_is_accepting_segments.set(true);

            imp.main_stack.set_visible_child(&*imp.listening_page);
        }
    }

    fn handle_voice_command(&self, command: &str) {
        // TODO: Implement voice commands

        tracing::debug!("Voice command: {}", command);
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
