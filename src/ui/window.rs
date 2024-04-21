use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

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
        type ParentType = gtk::ApplicationWindow;

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

            client.connect_message_received(clone!(@weak obj => move |_, message| {
                let imp = obj.imp();

                imp.label.set_label(&format!("{}\n{}", imp.label.label(), message));
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
                        client.send_message(&text, destination).await;
                    });
                }));

            self.peer_list_box
                .bind_model(Some(client.peer_list()), |peer| {
                    let peer = peer.downcast_ref::<Peer>().unwrap();
                    let row = PeerRow::new(peer);
                    row.upcast()
                });

            self.client.set(client).unwrap();

            obj.set_title(Some(&config::name()));
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow;
}

impl Window {
    pub fn new(application: &Application) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }
}
