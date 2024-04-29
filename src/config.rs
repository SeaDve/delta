use std::env;

use crate::peer::Location;

pub fn name() -> String {
    env::var("NAME").unwrap_or_else(|_| "Anonymous".to_string())
}

pub fn location() -> Location {
    env::var("LOCATION").map_or_else(
        |_| Location {
            latitude: 0.0,
            longitude: 0.0,
        },
        |str| {
            let mut parts = str.split(',');
            let latitude = parts
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            let longitude = parts
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            Location {
                latitude,
                longitude,
            }
        },
    )
}
