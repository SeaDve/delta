use gtk::{
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};

use crate::{
    call::{Call, CallState},
    peer::Peer,
    Application,
};

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        sync::OnceLock,
    };

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::CallPage)]
    #[template(file = "call_page.ui")]
    pub struct CallPage {
        #[property(get, set = Self::set_call, explicit_notify, nullable)]
        pub(super) call: RefCell<Option<Call>>,

        #[template_child]
        pub(super) vbox: TemplateChild<gtk::Box>, // Unused
        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) caller_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) caller_distance_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) caller_speed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) wireless_status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) incoming_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) accept_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) decline_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) outgoing_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) cancel_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) ongoing_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) duration_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) end_button: TemplateChild<gtk::Button>,

        pub(super) call_signals: OnceCell<glib::SignalGroup>,
        pub(super) call_bindings: glib::BindingGroup,

        pub(super) peer_signals: OnceCell<glib::SignalGroup>,
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

    #[glib::derived_properties]
    impl ObjectImpl for CallPage {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.accept_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("incoming-accepted", &[]);
                }));
            self.decline_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("incoming-declined", &[]);
                }));

            self.cancel_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("outgoing-cancelled", &[]);
                }));

            self.end_button
                .connect_clicked(clone!(@weak obj => move |_| {
                    obj.emit_by_name::<()>("ongoing-ended", &[]);
                }));

            let call_signals = glib::SignalGroup::new::<Call>();
            call_signals.connect_notify_local(
                Some("state"),
                clone!(@weak obj => move |_, _|  {
                    obj.update_stack();
                }),
            );
            call_signals.connect_notify_local(
                Some("duration-secs"),
                clone!(@weak obj => move |_, _| {
                    obj.update_duration_label();
                }),
            );
            self.call_signals.set(call_signals).unwrap();

            let peer_signals = glib::SignalGroup::new::<Peer>();
            peer_signals.connect_notify_local(
                Some("name"),
                clone!(@weak obj => move |_, _| {
                    obj.update_caller_name_label();
                }),
            );
            peer_signals.connect_notify_local(
                Some("location"),
                clone!(@weak obj => move |_, _| {
                    obj.update_caller_distance_label();
                }),
            );
            peer_signals.connect_notify_local(
                Some("speed"),
                clone!(@weak obj => move |_, _| {
                    obj.update_caller_speed_label();
                }),
            );
            peer_signals.connect_notify_local(
                Some("signal-quality"),
                clone!(@weak obj => move |_, _| {
                    obj.update_wireless_status_icon();
                }),
            );
            peer_signals.connect_notify_local(
                Some("icon-name"),
                clone!(@weak obj => move |_, _| {
                    obj.update_image_icon_name();
                }),
            );
            self.peer_signals.set(peer_signals.clone()).unwrap();

            self.call_bindings
                .bind("peer", &peer_signals, "target")
                .build();

            Application::get()
                .gps()
                .connect_location_notify(clone!(@weak obj => move |_| {
                    obj.update_caller_distance_label();
                }));

            obj.update_stack();
            obj.update_caller_name_label();
            obj.update_caller_distance_label();
            obj.update_caller_speed_label();
            obj.update_duration_label();
            obj.update_wireless_status_icon();
            obj.update_image_icon_name();
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("incoming-accepted").build(),
                    Signal::builder("incoming-declined").build(),
                    Signal::builder("outgoing-cancelled").build(),
                    Signal::builder("ongoing-ended").build(),
                ]
            })
        }
    }

    impl WidgetImpl for CallPage {}

    impl CallPage {
        fn set_call(&self, call: Option<Call>) {
            let obj = self.obj();

            if call == obj.call() {
                return;
            }

            self.call.replace(call.clone());

            self.call_signals.get().unwrap().set_target(call.as_ref());
            self.call_bindings.set_source(call.as_ref());

            obj.update_stack();
            obj.update_caller_name_label();
            obj.update_caller_distance_label();
            obj.update_caller_speed_label();
            obj.update_duration_label();
            obj.update_wireless_status_icon();
            obj.update_image_icon_name();

            obj.notify_call();
        }
    }
}

glib::wrapper! {
    pub struct CallPage(ObjectSubclass<imp::CallPage>)
        @extends gtk::Widget;
}

impl CallPage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_incoming_accepted<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "incoming-accepted",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    pub fn connect_incoming_declined<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "incoming-declined",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    pub fn connect_outgoing_cancelled<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "outgoing-cancelled",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    pub fn connect_ongoing_ended<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("ongoing-ended", false, closure_local!(|obj: &Self| f(obj)))
    }

    fn update_caller_name_label(&self) {
        let imp = self.imp();

        let name = self.call().map(|call| call.peer().name());
        imp.caller_name_label.set_label(&name.unwrap_or_default());
    }

    fn update_caller_distance_label(&self) {
        let imp = self.imp();

        let distance_str = self
            .call()
            .and_then(|call| call.peer().location())
            .and_then(|location| {
                Application::get()
                    .gps()
                    .location()
                    .map(|l| format!("{:.2} m away", l.distance(&location)))
            });
        imp.caller_distance_label
            .set_label(&distance_str.unwrap_or_default());
    }

    fn update_caller_speed_label(&self) {
        let imp = self.imp();

        let speed_str = self
            .call()
            .map(|call| format!("{:.2} m/s", call.peer().speed()));
        imp.caller_speed_label
            .set_label(&speed_str.unwrap_or_default());
    }

    fn update_stack(&self) {
        let imp = self.imp();

        match self.call().map(|call| call.state()) {
            Some(CallState::Incoming) => {
                imp.stack.set_visible_child(&*imp.incoming_page);
            }
            Some(CallState::Outgoing) => {
                imp.stack.set_visible_child(&*imp.outgoing_page);
            }
            Some(CallState::Ongoing) => {
                imp.stack.set_visible_child(&*imp.ongoing_page);
            }
            None | Some(CallState::Init) | Some(CallState::Ended(_)) => {
                // We don't do anything here so we avoid flickering
            }
        }
    }

    fn update_duration_label(&self) {
        let imp = self.imp();

        let duration_secs = self.call().map(|call| call.duration_secs()).unwrap_or(0);
        imp.duration_label.set_label(&format_time(duration_secs));
    }

    fn update_wireless_status_icon(&self) {
        let imp = self.imp();

        let signal_quality = self
            .call()
            .map(|call| call.peer().signal_quality())
            .unwrap_or_default();

        imp.wireless_status_icon
            .set_icon_name(Some(signal_quality.icon_name()));

        signal_quality.apply_css_class_to_image(&imp.wireless_status_icon);
    }

    fn update_image_icon_name(&self) {
        let imp = self.imp();

        let icon_name = self.call().map(|call| call.peer().icon_name());
        imp.image.set_icon_name(icon_name.as_deref());
    }
}

/// Formats time in MM:SS.
///
/// The MM part will be more than 2 digits if the time is >= 100 minutes.
fn format_time(secs: u64) -> String {
    let seconds_display = secs % 60;
    let minutes_display = secs / 60;
    format!("{:02}∶{:02}", minutes_display, seconds_display)
}
