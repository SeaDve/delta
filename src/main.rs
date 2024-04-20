#![allow(clippy::new_without_default)]

mod application;
mod client;
mod ui;

use gtk::{glib, prelude::*};

use self::application::Application;

const APP_ID: &str = "io.github.seadve.Delta";

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    let app = Application::new();
    app.run()
}
