//! Native owner/shell adoption candidate. No web runtime is constructed.
use gtk::glib;
use pecan_ui_gtk::startup::{self, Options};
fn main() -> glib::ExitCode {
    match Options::parse(std::env::args_os().skip(1)) {
        Ok(None) => {
            println!("{}", startup::HELP);
            glib::ExitCode::SUCCESS
        }
        Ok(Some(options)) => match startup::run(options) {
            Ok(code) => code,
            Err(error) => {
                eprintln!("pecan-ui-gtk: {error}");
                glib::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("pecan-ui-gtk: {error}");
            glib::ExitCode::FAILURE
        }
    }
}
