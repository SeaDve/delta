use gtk::glib::{self, clone, closure_local};
use shumate::{prelude::*, subclass::prelude::*};

use crate::{place_finder::Place, Application};

mod imp {
    use std::{cell::OnceCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PlaceMarker)]
    #[template(resource = "/io/github/seadve/Delta/ui/place_marker.ui")]
    pub struct PlaceMarker {
        #[property(get, set, construct_only)]
        pub(super) place: OnceCell<Place>,

        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) distance_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaceMarker {
        const NAME: &'static str = "DeltaPlaceMarker";
        type Type = super::PlaceMarker;
        type ParentType = shumate::Marker;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PlaceMarker {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let place = obj.place();

            let location = place.location();
            obj.set_location(location.latitude, location.longitude);

            self.image.set_icon_name(Some(&place.type_().icon_name()));
            self.name_label.set_label(&place.name());

            let gesture_click = gtk::GestureClick::new();
            gesture_click.connect_released(clone!(
                #[weak]
                obj,
                move |_, _, _, _| {
                    obj.emit_by_name::<()>("show-place-requested", &[]);
                }
            ));
            self.image.add_controller(gesture_click);

            Application::get().gps().connect_location_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_distance_label();
                }
            ));

            obj.update_distance_label();
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

            SIGNALS.get_or_init(|| vec![Signal::builder("show-place-requested").build()])
        }
    }

    impl WidgetImpl for PlaceMarker {}
    impl MarkerImpl for PlaceMarker {}
}

glib::wrapper! {
    pub struct PlaceMarker(ObjectSubclass<imp::PlaceMarker>)
        @extends gtk::Widget, shumate::Marker,
        @implements shumate::Location;
}

impl PlaceMarker {
    pub fn new(place: &Place) -> Self {
        glib::Object::builder().property("place", place).build()
    }

    pub fn connect_show_place_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "show-place-requested",
            false,
            closure_local!(|obj: &Self| f(obj)),
        )
    }

    fn update_distance_label(&self) {
        let imp = self.imp();

        let distance_str = Application::get()
            .gps()
            .location()
            .map(|l| format!("{:.2} m", l.distance(self.place().location())));
        imp.distance_label
            .set_label(&distance_str.unwrap_or_default());
    }
}
