use adw::prelude::*;
use adw::{gio, glib};
use demo::Window;

const APP_ID: &str = "mobi.phosh.syncbus.demo";

fn on_activate(app: &adw::Application) {
    let window = Window::new(app);
    window.present();
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("resources.gresource").expect("Failed to register resources.");

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(on_activate);
    app.run()
}
