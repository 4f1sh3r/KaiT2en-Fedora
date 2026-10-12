mod actuators;
mod daemon;
mod error;
mod gui;
mod plimit;
mod sensors;
mod sysfs;

use gtk4::glib;

fn main() -> glib::ExitCode {
    if std::env::args().any(|arg| arg == "--daemon") {
        if let Err(error) = daemon::run() {
            eprintln!("{error}");
            return glib::ExitCode::from(1);
        }
        return glib::ExitCode::SUCCESS;
    }
    gui::run()
}
