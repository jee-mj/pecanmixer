//! Read-only migration preview. All GTK objects stay on the main thread.
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use pecan_core::{
    backend::EventSequencer, graph::Momentopname, inbox::LatestInbox, overview::OverviewState,
};
use pecan_ui_gtk::GraphPresentation;

#[derive(Default)]
struct Options {
    fixture: Option<PathBuf>,
    smoke: bool,
    headless: bool,
    patchbay: bool,
}

fn options() -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                options.fixture = Some(args.next().ok_or("--fixture requires a JSON path")?.into())
            }
            "--smoke" => options.smoke = true,
            "--headless" => options.headless = true,
            "--patchbay" => options.patchbay = true,
            "--help" | "-h" => {
                println!("pecan-ui-gtk [--fixture graph.json] [--headless | --smoke] [--patchbay]\nWithout a fixture: observe live PipeWire; no routing, meters or recording.");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    if (options.headless || options.smoke) && options.fixture.is_none() {
        return Err("--headless and --smoke require --fixture".into());
    }
    Ok(options)
}

fn main() -> glib::ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("pecan-ui-gtk: {error}");
            glib::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<glib::ExitCode, Box<dyn std::error::Error>> {
    let options = options()?;
    let fixture: Option<Momentopname> = options
        .fixture
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        })
        .transpose()?;
    if options.headless {
        let (inbox, _) = LatestInbox::new(16);
        if let Some(graph) = fixture {
            inbox.publish(EventSequencer::default().graph(graph));
        }
        let mut state = OverviewState::default();
        state.apply(inbox.drain());
        println!("{}", serde_json::to_string_pretty(&state.overview())?);
        return Ok(glib::ExitCode::SUCCESS);
    }

    let app = gtk::Application::builder()
        .application_id("nl.jetse.pecanmixer.GtkPreview")
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let view = Rc::new(RefCell::new(GraphPresentation::new()));
        if options.patchbay {
            view.borrow().show_patchbay();
        }
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Pecan Mixer — GTK preview")
            .default_width(900)
            .default_height(600)
            .child(view.borrow().widget())
            .build();
        let (inbox, wake) = LatestInbox::new(16);
        let observer = Rc::new(RefCell::new(None));
        if let Some(graph) = fixture.clone() {
            inbox.publish(EventSequencer::default().graph(graph));
        } else {
            match pecan_pipewire::graaf::observe(inbox.clone()) {
                Ok(handle) => *observer.borrow_mut() = Some(handle),
                Err(error) => view
                    .borrow()
                    .show_message(&format!("Could not start observer: {error}")),
            }
        }
        let app_weak = app.downgrade();
        let task = glib::MainContext::default().spawn_local(async move {
            while wake.recv().await.is_ok() {
                let batch = inbox.drain();
                let accepted = view.borrow_mut().apply(batch);
                if let Some(overview) = accepted {
                    if options.smoke {
                        println!(
                            "GTK fixture widgets populated: connected={}, nodes={}, ports={}",
                            overview.connected,
                            overview.nodes.len(),
                            overview.nodes.iter().map(|n| n.ports.len()).sum::<usize>()
                        );
                        if let Some(app) = app_weak.upgrade() {
                            app.quit();
                        }
                    }
                }
            }
        });
        window.connect_close_request(move |_| {
            task.abort();
            observer.borrow_mut().take();
            glib::Propagation::Proceed
        });
        window.present();
    });
    // Our preview options were already parsed; GApplication gets only its name.
    Ok(app.run_with_args(&["pecan-ui-gtk"]))
}
