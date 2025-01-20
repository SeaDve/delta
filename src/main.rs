#![allow(clippy::new_without_default)]
#![warn(rust_2018_idioms, clippy::unused_async, clippy::dbg_macro)]

mod application;
mod audio_device;
mod call;
mod client;
mod colors;
mod config;
mod gps;
mod input_stream;
mod location;
mod output_stream;
mod peer;
mod peer_list;
mod place_finder;
mod remote;
mod settings;
mod stt;
mod tts;
mod ui;
mod utils;
mod wireless_info;

use std::path::Path;

use gtk::{gio, glib, prelude::*};

use self::application::Application;

const APP_ID: &str = "io.github.seadve.Delta";
const GRESOURCE_PREFIX: &str = "/io/github/seadve/Delta/";

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    gst::init().unwrap();

    let data = gvdb::gresource::BundleBuilder::from_directory(
        GRESOURCE_PREFIX,
        Path::new("data/resources/"),
        true,
        true,
    )
    .unwrap()
    .build()
    .unwrap();
    let resource = gio::Resource::from_data(&glib::Bytes::from_owned(data)).unwrap();
    gio::resources_register(&resource);

    let app = Application::new();
    app.run()
}
