use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use crate::{
    application::Application,
    call::{Call, CallState},
    client::{Client, MessageDestination},
    config,
    peer::Peer,
    ui::{call_page::CallPage, peer_row::PeerRow},
};

const LABEL_PEER_KEY: &str = "delta-label-peer";

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) main_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) call_page: TemplateChild<CallPage>,
        #[template_child]
        pub(super) peer_list_box: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub(super) test_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) test_received_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) test_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub(super) test_peer_list_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub(super) test_unselect_all_button: TemplateChild<gtk::Button>,

        pub(super) client: OnceCell<Client>,
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

            client.connect_active_call_notify(clone!(@weak obj => move |client| {
                let imp = obj.imp();

                if let Some(active_call) = client.active_call() {
                    debug_assert!(matches!(
                        active_call.state(),
                        CallState::Incoming | CallState::Outgoing
                    ));

                    imp.call_page.set_call(Some(active_call.clone()));
                    imp.main_stack.set_visible_child(&*imp.call_page);

                    active_call.connect_state_notify(clone!(@weak obj => move |call| {
                        let imp = obj.imp();

                        match call.state() {
                            CallState::Ended => {
                                imp.call_page.set_call(None::<Call>);
                                imp.main_stack.set_visible_child(&*imp.main_page);
                            }
                            CallState::Connected => {}
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
                clone!(@weak client => @default-panic, move |peer| {
                    let peer = peer.downcast_ref::<Peer>().unwrap();

                    let row = PeerRow::new(peer);
                    row.connect_called(clone!(@weak client => move |row| {
                        let peer_id = *row.peer().id();
                        glib::spawn_future_local(async move {
                            client.call_request(peer_id).await;
                        });
                    }));

                    row.upcast()
                }),
            );

            self.client.set(client.clone()).unwrap();

            if false {
                // Add some dummy peers

                let peers = client.peer_list();

                let a = Peer::new(libp2p::PeerId::random());
                a.set_name("Alpha");
                peers.insert(a);

                let b = Peer::new(libp2p::PeerId::random());
                b.set_name("Bravo");
                peers.insert(b);

                let c = Peer::new(libp2p::PeerId::random());
                c.set_name("Charlie");
                peers.insert(c);
            }

            self.test_name_label.set_label(&config::name());

            let test_placeholder_label = gtk::Label::builder().label("No Nearby Peers").build();
            self.test_peer_list_box
                .set_placeholder(Some(&test_placeholder_label));

            self.test_peer_list_box.bind_model(
                Some(client.peer_list()),
                clone!(@weak client => @default-panic, move |peer| {
                    let peer = peer.downcast_ref::<Peer>().unwrap().clone();

                    let label = gtk::Label::builder()
                        .build();
                    peer.bind_property("name", &label, "label")
                        .sync_create()
                        .build();

                    unsafe {
                        label.set_data(LABEL_PEER_KEY, peer);
                    }

                    label.upcast()
                }),
            );

            client.connect_message_received(clone!(@weak obj => move |client, message_received| {
                let imp = obj.imp();

                let peer_name = client.peer_list().get(&message_received.source).map_or(
                    message_received.source.to_string(),
                    |peer| peer.name().to_string(),
                );
                imp.test_received_label.set_label(&format!(
                    "{}\n{}: {}",
                    imp.test_received_label.label(),
                    peer_name,
                    message_received.message
                ));
            }));

            self.test_unselect_all_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    let imp = obj.imp();
                    imp.test_peer_list_box.unselect_all();
                }));

            self.test_entry
                .connect_activate(clone!(@weak obj, @weak client => move |entry| {
                    let imp = obj.imp();

                    let text = entry.text();
                    entry.set_text("");

                    let selected_peer_ids = imp
                        .test_peer_list_box
                        .selected_rows()
                        .iter()
                        .map(|row| unsafe {
                            *row.child()
                                .unwrap()
                                .downcast_ref::<gtk::Label>()
                                .unwrap()
                                .data::<Peer>(LABEL_PEER_KEY)
                                .unwrap()
                                .as_ref()
                                .id()
                        })
                        .collect::<Vec<_>>();
                    let destination = if selected_peer_ids.is_empty() {
                        MessageDestination::All
                    } else {
                        MessageDestination::Peers(selected_peer_ids)
                    };
                    glib::spawn_future_local(async move {
                        client.publish_message(&text, destination).await;
                    });
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
}
