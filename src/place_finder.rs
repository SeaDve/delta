use std::{collections::HashMap, fmt, fs::File};

use anyhow::{Ok, Result};
use async_lock::OnceCell;
use gtk::{gio, glib};

use crate::location::Location;

const PBF_PATH: &str = "balanga.osm.pbf";

#[derive(Debug, Clone, glib::Boxed)]
#[boxed_type(name = "DeltaPlace")]
pub struct Place {
    place_type: PlaceType,
    location: Location,
    name: Option<String>,
}

impl Place {
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn place_type(&self) -> PlaceType {
        self.place_type
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceType {
    Shop,
    Restaurant,
    Fuel,
    Toilet,
    Medical,
    School,
    Parking,
    Cinema,
    Telephone,
    Bank,
    Church,
    Police,
}

impl fmt::Display for PlaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shop => write!(f, "Shop"),
            Self::Restaurant => write!(f, "Restaurant"),
            Self::Fuel => write!(f, "Fuel"),
            Self::Toilet => write!(f, "Toilet"),
            Self::Medical => write!(f, "Medical"),
            Self::School => write!(f, "School"),
            Self::Parking => write!(f, "Parking"),
            Self::Cinema => write!(f, "Cinema"),
            Self::Telephone => write!(f, "Telephone"),
            Self::Bank => write!(f, "Bank"),
            Self::Church => write!(f, "Church"),
            Self::Police => write!(f, "Police"),
        }
    }
}

impl PlaceType {
    pub fn all() -> &'static [PlaceType] {
        &[
            Self::Shop,
            Self::Restaurant,
            Self::Fuel,
            Self::Toilet,
            Self::Medical,
            Self::School,
            Self::Parking,
            Self::Cinema,
            Self::Telephone,
            Self::Bank,
            Self::Church,
            Self::Police,
        ]
    }

    pub fn icon_name(&self) -> String {
        let prefix = match self {
            Self::Shop => "shop",
            Self::Restaurant => "fast-food",
            Self::Fuel => "fuel",
            Self::Toilet => "toilets",
            Self::Medical => "hospital",
            Self::School => "school",
            Self::Parking => "parking-sign",
            Self::Cinema => "theater",
            Self::Telephone => "phone-oldschool",
            Self::Bank => "bank",
            Self::Church => "non-religious-cemetary",
            Self::Police => "police-badge",
        };

        format!("{}-symbolic", prefix)
    }

    fn from_raw(str: &str) -> Option<Self> {
        Self::all()
            .iter()
            .find(|&place_type| place_type.as_raw().contains(&str))
            .copied()
    }

    fn as_raw(&self) -> &'static [&'static str] {
        match self {
            PlaceType::Restaurant => &["restaurant", "bar", "fast_food", "cafe"],
            PlaceType::School => &["college", "school", "university", "library"],
            PlaceType::Parking => &["parking"],
            PlaceType::Medical => &[
                "hospital",
                "doctors",
                "dentist",
                "veterinary",
                "pharmacy",
                "clinic",
            ],
            PlaceType::Cinema => &["theatre", "cinema", "events_venue"],
            PlaceType::Telephone => &["telephone"],
            PlaceType::Bank => &["bank", "atm", "money_transfer"],
            PlaceType::Shop => &["marketplace"],
            PlaceType::Church => &["place_of_worship"],
            PlaceType::Fuel => &["fuel"],
            PlaceType::Police => &["police"],
            PlaceType::Toilet => &["toilets"],
        }
    }
}

#[derive(Debug, Default)]
pub struct PlaceFinder {
    inner: OnceCell<HashMap<PlaceType, Vec<Place>>>,
}

impl PlaceFinder {
    pub async fn find(&self, needle: PlaceType) -> Result<&[Place]> {
        let inner = self
            .inner
            .get_or_try_init(|| async {
                gio::spawn_blocking(move || {
                    let file = File::open(PBF_PATH)?;
                    let mut reader = osmpbfreader::OsmPbfReader::new(file);

                    let objs = reader.get_objs_and_deps(|obj| {
                        obj.is_node() && obj.tags().contains_key("amenity")
                    })?;

                    let mut inner = HashMap::new();

                    for (_, obj) in objs {
                        let tags = obj.tags();

                        let raw_place_type = tags.get("amenity").unwrap();
                        let node = obj.node().unwrap();

                        let Some(place_type) = PlaceType::from_raw(raw_place_type) else {
                            tracing::trace!("Unknown place type: {:?}", raw_place_type);
                            continue;
                        };

                        inner
                            .entry(place_type)
                            .or_insert_with(Vec::new)
                            .push(Place {
                                place_type,
                                location: Location {
                                    latitude: node.lat(),
                                    longitude: node.lon(),
                                },
                                name: tags.get("name").map(|s| s.to_string()),
                            });
                    }

                    Ok(inner)
                })
                .await
                .unwrap()
            })
            .await?;

        Ok(inner.get(&needle).map_or(&[], |v| v.as_slice()))
    }
}
