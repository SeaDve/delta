use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};

mod imp {
    use std::sync::OnceLock;

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "crashed_page.ui")]
    pub struct CrashedPage {
        #[template_child]
        pub(super) status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) send_alert_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) ignore_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CrashedPage {
        const NAME: &'static str = "DeltaCrashedPage";
        type Type = super::CrashedPage;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CrashedPage {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.send_alert_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("send-alert-requested", &[]);
                }));

            self.ignore_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("ignored", &[]);
                }));
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("send-alert-requested").build(),
                    Signal::builder("ignored").build(),
                ]
            })
        }
    }

    impl WidgetImpl for CrashedPage {}
}

glib::wrapper! {
    pub struct CrashedPage(ObjectSubclass<imp::CrashedPage>)
        @extends gtk::Widget;
}

impl CrashedPage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_send_alert_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "send-alert-requested",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    pub fn connect_ignored<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("ignored", false, closure_local!(|obj: &Self| f(obj)))
    }
}
