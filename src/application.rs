use adw::{prelude::*, subclass::prelude::*};
use anyhow::Result;
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    gps::Gps,
    remote::{LedColor, LedId, Remote},
    settings::{AllowedPeers, Settings},
    ui::Window,
    wireless_info::WirelessInfo,
    APP_ID, GRESOURCE_PREFIX,
};

pub const ALLOWED_PEERS_LED_ID: LedId = LedId::_1;
pub const ALERT_LED_ID: LedId = LedId::_2;

mod imp {
    use once_cell::unsync::OnceCell;

    use super::*;

    #[derive(Default)]
    pub struct Application {
        pub(super) gps: Gps,
        pub(super) settings: Settings,
        pub(super) wireless_info: WirelessInfo,

        pub(super) remote: OnceCell<Remote>,
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

            obj.window().present();
        }

        fn startup(&self) {
            self.parent_startup();

            let obj = self.obj();

            obj.setup_actions();
            obj.setup_accels();

            self.settings.connect_allowed_peers_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_allowed_peers_led_color();
                }
            ));
            self.settings.connect_remote_ip_addr_notify(clone!(
                #[weak]
                obj,
                move |settings| {
                    let ip_addr = settings.remote_ip_addr();
                    obj.remote().set_ip_addr(ip_addr);
                }
            ));

            obj.update_allowed_peers_led_color();

            let remote = Remote::new(self.settings.remote_ip_addr());
            self.remote.set(remote).unwrap();
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
            .property("resource-base-path", GRESOURCE_PREFIX)
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

    pub fn settings(&self) -> Settings {
        self.imp().settings.clone()
    }

    pub fn wireless_info(&self) -> WirelessInfo {
        self.imp().wireless_info.clone()
    }

    pub fn remote(&self) -> Remote {
        self.imp().remote.get().unwrap().clone()
    }

    fn window(&self) -> Window {
        self.active_window()
            .map_or_else(|| Window::new(self), |w| w.downcast().unwrap())
    }

    fn update_allowed_peers_led_color(&self) {
        glib::spawn_future_local(clone!(
            #[weak(rename_to = obj)]
            self,
            async move {
                if let Err(err) = obj.update_allowed_peers_led_color_inner().await {
                    tracing::error!("Failed to update allowed peers LED color: {:?}", err);
                }
            }
        ));
    }

    async fn update_allowed_peers_led_color_inner(&self) -> Result<()> {
        let color = match self.settings().allowed_peers() {
            AllowedPeers::ExceptMuted => Some(LedColor::Blue),
            AllowedPeers::All => Some(LedColor::Green),
            AllowedPeers::None => None,
        };

        self.remote()
            .set_led_color(ALLOWED_PEERS_LED_ID, color)
            .await?;

        Ok(())
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
