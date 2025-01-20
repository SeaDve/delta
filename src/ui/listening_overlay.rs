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
    #[template(resource = "/io/github/seadve/Delta/ui/listening_overlay.ui")]
    pub struct ListeningOverlay {
        #[template_child]
        pub(super) status_page: TemplateChild<adw::StatusPage>, // Unused
        #[template_child]
        pub(super) command_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) cancel_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ListeningOverlay {
        const NAME: &'static str = "DeltaListeningOverlay";
        type Type = super::ListeningOverlay;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ListeningOverlay {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.cancel_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.emit_by_name::<()>("cancelled", &[]);
                }
            ));
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("cancelled").build()])
        }
    }

    impl WidgetImpl for ListeningOverlay {}
}

glib::wrapper! {
    pub struct ListeningOverlay(ObjectSubclass<imp::ListeningOverlay>)
        @extends gtk::Widget;
}

impl ListeningOverlay {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_command(&self, command: &str) {
        let imp = self.imp();

        imp.command_label.set_label(command);
    }

    pub fn connect_cancelled<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("cancelled", false, closure_local!(|obj: &Self| f(obj)))
    }
}
