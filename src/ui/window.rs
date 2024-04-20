use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::{application::Application, client::Client};

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
        pub(super) button: TemplateChild<gtk::Button>,

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
                    let text = entry.text();
                    entry.set_text("");
                    glib::spawn_future_local(async move {
                        client.send_message(&text).await;
                    });
                }));

            self.button
                .connect_clicked(clone!(@weak obj, @weak client => move |_| {
                    glib::spawn_future_local(async move {
                        dbg!(client.list_peers().await);
                    });
                }));

            self.client.set(client).unwrap();
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
