use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::{self, clone};

use crate::{
    application::Application,
    client::{Client, MessageDestination},
    config,
    peer::Peer,
    ui::peer_row::PeerRow,
};

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub(super) label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) peer_list_box: TemplateChild<gtk::ListBox>,

        pub(super) client: OnceCell<Client>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "DeltaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
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

            client.connect_message_received(clone!(@weak obj => move |client, message_received| {
                let imp = obj.imp();

                let peer_name = client.peer_list().get(&message_received.source).map_or(
                    message_received.source.to_string(),
                    |peer| peer.name().to_string(),
                );
                imp.label.set_label(&format!(
                    "{}\n{}: {}",
                    imp.label.label(),
                    peer_name,
                    message_received.message
                ));
            }));

            self.entry
                .connect_activate(clone!(@weak obj, @weak client => move |entry| {
                    let imp = obj.imp();

                    let text = entry.text();
                    entry.set_text("");

                    let selected_peer_ids = imp
                        .peer_list_box
                        .selected_rows()
                        .iter()
                        .map(|row| *row.downcast_ref::<PeerRow>().unwrap().peer().id())
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

            let label = gtk::Label::builder()
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(12)
                .margin_end(12)
                .label("No Nearby Peers")
                .build();
            self.peer_list_box.set_placeholder(Some(&label));

            self.peer_list_box.bind_model(
                Some(client.peer_list()),
                clone!(@weak client => @default-panic, move |peer| {
                    let peer = peer.downcast_ref::<Peer>().unwrap();
                    let row = PeerRow::new(peer);
                    row.connect_call_requested(clone!(@weak client => move |row| {
                        let peer_id = *row.peer().id();
                        glib::spawn_future_local(async move {
                            client.open_audio_stream(peer_id).await;
                        });
                    }));
                    row.upcast()
                }),
            );

            self.client.set(client).unwrap();

            obj.set_title(Some(&config::name()));
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
