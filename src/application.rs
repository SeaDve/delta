use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    gps::Gps,
    led::{Color, Led},
    settings::{AllowedPeers, Settings},
    ui::Window,
    APP_ID,
};

mod imp {
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Default)]
    pub struct Application {
        pub(super) gps: Gps,
        pub(super) settings: Settings,
        pub(super) led: OnceCell<Led>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "DeltaApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            self.parent_activate();

            let obj = self.obj();

            if let Some(window) = obj.windows().first() {
                window.present();
                return;
            }

            let window = Window::new(&obj);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();

            let obj = self.obj();

            obj.setup_actions();
            obj.setup_accels();

            self.settings
                .connect_allowed_peers_notify(clone!(@weak obj => move |_| {
                    obj.update_led_color();
                }));

            obj.update_led_color();
        }

        fn shutdown(&self) {
            if let Err(err) = self.settings.save() {
                tracing::error!("Failed to save settings on shutdown: {:?}", err);
            }

            self.parent_shutdown();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("resource-base-path", "/io/github/seadve/Delta/")
            .property("flags", gio::ApplicationFlags::NON_UNIQUE)
            .build()
    }

    /// Returns the global instance of `Application`.
    ///
    /// # Panics
    ///
    /// Panics if the app is not running or if this is called on a non-main thread.
    pub fn get() -> Self {
        debug_assert!(
            gtk::is_initialized_main_thread(),
            "application must only be accessed in the main thread"
        );

        gio::Application::default().unwrap().downcast().unwrap()
    }

    pub fn gps(&self) -> Gps {
        self.imp().gps.clone()
    }

    pub fn settings(&self) -> &Settings {
        &self.imp().settings
    }

    pub fn led(&self) -> Result<&Led> {
        self.imp().led.get_or_try_init(Led::new)
    }

    fn update_led_color(&self) {
        match self.led() {
            Ok(led) => {
                led.set_color(match self.settings().allowed_peers() {
                    AllowedPeers::Everyone => Some(Color::Green),
                    AllowedPeers::Whitelist => Some(Color::Blue),
                    AllowedPeers::None => None,
                });
            }
            Err(err) => tracing::error!("Failed to get LED: {:?}", err),
        }
    }

    fn setup_actions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(|obj: &Self, _, _| {
                obj.quit();
            })
            .build();
        self.add_action_entries([quit_action]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
    }
}
