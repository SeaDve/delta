use gtk::{glib, prelude::*, subclass::prelude::*};

pub enum CallPageState {
    Incoming,
    Outgoing,
    Connected,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "call_page.ui")]
    pub struct CallPage {
        #[template_child]
        pub(super) caller_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) decline_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CallPage {
        const NAME: &'static str = "DeltaCallPage";
        type Type = super::CallPage;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CallPage {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for CallPage {}
}

glib::wrapper! {
    pub struct CallPage(ObjectSubclass<imp::CallPage>)
        @extends gtk::Widget;
}

impl CallPage {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
