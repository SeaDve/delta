use gtk::{glib, prelude::*, subclass::prelude::*};

use std::cell::RefCell;

mod imp {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::ToggleButton)]
    pub struct ToggleButton {
        #[property(get, set = Self::set_is_active, explicit_notify)]
        pub(super) is_active: Cell<bool>,
        #[property(get, set = Self::set_default_icon_name, explicit_notify)]
        pub(super) default_icon_name: RefCell<String>,
        #[property(get, set = Self::set_toggled_icon_name, explicit_notify)]
        pub(super) toggled_icon_name: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToggleButton {
        const NAME: &'static str = "DeltaToggleButton";
        type Type = super::ToggleButton;
        type ParentType = gtk::Button;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ToggleButton {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.update_icon_name();
        }
    }

    impl WidgetImpl for ToggleButton {}

    impl ButtonImpl for ToggleButton {
        fn clicked(&self) {
            let obj = self.obj();

            obj.set_is_active(!obj.is_active());

            self.parent_clicked()
        }
    }

    impl ToggleButton {
        fn set_is_active(&self, is_active: bool) {
            let obj = self.obj();

            if is_active == obj.is_active() {
                return;
            }

            self.is_active.set(is_active);
            obj.update_icon_name();
            obj.notify_is_active();
        }

        fn set_default_icon_name(&self, default_icon_name: String) {
            let obj = self.obj();

            if default_icon_name == obj.default_icon_name().as_str() {
                return;
            }

            self.default_icon_name.replace(default_icon_name);
            obj.update_icon_name();
            obj.notify_default_icon_name();
        }

        fn set_toggled_icon_name(&self, toggled_icon_name: String) {
            let obj = self.obj();

            if toggled_icon_name == obj.toggled_icon_name().as_str() {
                return;
            }

            self.toggled_icon_name.replace(toggled_icon_name);
            obj.update_icon_name();
            obj.notify_toggled_icon_name();
        }
    }
}

glib::wrapper! {
    pub struct ToggleButton(ObjectSubclass<imp::ToggleButton>)
        @extends gtk::Widget, gtk::Button;
}

impl ToggleButton {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn update_icon_name(&self) {
        let icon_name = if self.is_active() {
            self.toggled_icon_name()
        } else {
            self.default_icon_name()
        };
        self.set_icon_name(&icon_name);
    }
}
