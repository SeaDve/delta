use std::env;

use crate::location::Location;

pub fn is_stt_enabled() -> bool {
    env::var("STT").is_ok_and(|s| s == "1")
}

pub fn is_gps_enabled() -> bool {
    env::var("GPS").is_ok_and(|s| s == "1")
}

pub fn name() -> String {
    env::var("NAME").unwrap_or_else(|_| "Anonymous".to_string())
}

pub fn location() -> Option<Location> {
    env::var("LOCATION")
        .map(|str| {
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
        })
        .ok()
}
