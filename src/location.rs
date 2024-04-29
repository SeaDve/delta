use gtk::glib;
use serde::{Deserialize, Serialize};

const EARTH_RADIUS: f64 = 6_378_137.0;

#[derive(Debug, Clone, Deserialize, Serialize, glib::Boxed)]
#[boxed_type(name = "DeltaLocation", nullable)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    /// Calculate the distance between two locations in meters.
    pub fn distance(&self, other: &Location) -> f64 {
        let lat1 = self.latitude.to_radians();
        let lon1 = self.longitude.to_radians();

        let lat2 = other.latitude.to_radians();
        let lon2 = other.longitude.to_radians();

        (lat1.sin() * lat2.sin() + lat1.cos() * lat2.cos() * (lon2 - lon1).cos()).acos()
            * EARTH_RADIUS
    }
}
