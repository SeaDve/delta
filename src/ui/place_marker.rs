use gtk::glib::{self, clone};
use shumate::{prelude::*, subclass::prelude::*};

use crate::{place_finder::Place, Application};

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PlaceMarker)]
    #[template(file = "place_marker.ui")]
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

            self.image
                .set_icon_name(Some(&place.place_type().icon_name()));
            self.name_label.set_label(
                &place
                    .name()
                    .map_or_else(|| place.place_type().to_string(), |name| name.to_string()),
            );

            Application::get()
                .gps()
                .connect_location_notify(clone!(@weak obj => move |_| {
                    obj.update_distance_label();
                }));

            obj.update_distance_label();
        }

        fn dispose(&self) {
            self.dispose_template();
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
