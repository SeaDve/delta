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
    #[template(file = "settings_view.ui")]
    pub struct SettingsView {
        #[template_child]
        pub(super) simulate_crash_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SettingsView {
        const NAME: &'static str = "DeltaSettingsView";
        type Type = super::SettingsView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
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

            self.simulate_crash_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("crash-simulated", &[]);
                }));
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("crash-simulated").build()])
        }
    }

    impl WidgetImpl for SettingsView {}
}

glib::wrapper! {
    pub struct SettingsView(ObjectSubclass<imp::SettingsView>)
        @extends gtk::Widget;
}

impl SettingsView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_crash_simulated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "crash-simulated",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }
}
