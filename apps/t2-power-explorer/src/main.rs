mod collector;
mod diagnostics;
mod model;
mod ui;

use adw::prelude::*;

const APP_ID: &str = "org.t2powerexplorer.gtk";

fn main() {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(ui::build);
    app.run();
}
