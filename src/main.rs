#![allow(clippy::new_without_default)]

mod application;
mod audio_stream;
mod client;
mod config;
mod input_stream;
mod output_stream;
mod peer;
mod peer_list;
mod ui;

use gtk::{glib, prelude::*};

use self::application::Application;

const APP_ID: &str = "io.github.seadve.Delta";

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    gst::init().unwrap();

    let app = Application::new();
    app.run()
}
