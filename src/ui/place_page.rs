use anyhow::Result;
use gtk::{
    gdk,
    glib::{self, clone, closure_local},
    prelude::*,
    subclass::prelude::*,
};
use qrcode::{render::svg, QrCode};

use crate::{location::Location, place_finder::Place, Application};

mod imp {
    use std::{cell::RefCell, sync::OnceLock};

    use glib::subclass::Signal;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(file = "place_page.ui")]
    pub struct PlacePage {
        #[template_child]
        pub(super) hbox: TemplateChild<gtk::Box>, // Unused
        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) distance_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub(super) done_button: TemplateChild<gtk::Button>,

        pub(super) place: RefCell<Option<Place>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlacePage {
        const NAME: &'static str = "DeltaPlacePage";
        type Type = super::PlacePage;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlacePage {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.done_button.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.emit_by_name::<()>("done", &[]);
                }
            ));

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

            SIGNALS.get_or_init(|| vec![Signal::builder("done").build()])
        }
    }

    impl WidgetImpl for PlacePage {}
}

glib::wrapper! {
    pub struct PlacePage(ObjectSubclass<imp::PlacePage>)
        @extends gtk::Widget;
}

impl PlacePage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_done<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure("done", false, closure_local!(|obj: &Self| f(obj)))
    }

    pub fn set_place(&self, place: Option<&Place>) {
        let imp = self.imp();

        if let Some(place) = place {
            imp.image.set_icon_name(Some(&place.type_().icon_name()));
            imp.name_label.set_label(&place.name());

            match qrcode_texture_for_location(place.location()) {
                Ok(texture) => imp.picture.set_paintable(Some(&texture)),
                Err(err) => {
                    tracing::error!("Failed to generate QR code texture: {:?}", err);
                    imp.picture.set_paintable(gdk::Paintable::NONE);
                }
            }
        } else {
            imp.image.set_icon_name(None);
            imp.name_label.set_label("");

            imp.picture.set_paintable(gdk::Paintable::NONE);
        }

        imp.place.replace(place.cloned());

        self.update_distance_label();
    }

    fn update_distance_label(&self) {
        let imp = self.imp();

        let distance_str = imp.place.borrow().as_ref().and_then(|place| {
            Application::get()
                .gps()
                .location()
                .map(|l| format!("{:.2} m", l.distance(place.location())))
        });
        imp.distance_label
            .set_label(&distance_str.unwrap_or_default());
    }
}

fn qrcode_texture_for_location(location: &Location) -> Result<gdk::Texture> {
    let qrcode_data = format!("geo:0,0?q={},{}", location.latitude, location.longitude);
    let qrcode = QrCode::new(qrcode_data)?;
    let svg_bytes = qrcode.render::<svg::Color<'_>>().build();
    let texture = gdk::Texture::from_bytes(&svg_bytes.as_bytes().into())?;
    Ok(texture)
}
