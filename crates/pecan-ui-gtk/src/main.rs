//! Read-only migration preview. All GTK objects stay on the main thread.
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use gtk::{glib, prelude::*};
use pecan_core::{
    backend::EventSequencer, graph::Momentopname, inbox::LatestInbox, overview::OverviewState,
};

#[derive(Default)]
struct Options {
    fixture: Option<PathBuf>,
    smoke: bool,
    headless: bool,
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
            "--help" | "-h" => {
                println!("pecan-ui-gtk [--fixture graph.json] [--headless | --smoke]\nWithout a fixture: observe live PipeWire; no routing, meters or recording.");
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
        let status = gtk::Label::builder()
            .label("Disconnected · waiting for PipeWire")
            .xalign(0.0)
            .selectable(true)
            .build();
        let notices = gtk::Label::builder().xalign(0.0).wrap(true).build();
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&rows)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.append(
            &gtk::Label::builder()
                .label("Pecan Mixer · read-only GTK preview")
                .xalign(0.0)
                .build(),
        );
        content.append(&status);
        content.append(&notices);
        content.append(&scroll);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Pecan Mixer — GTK preview")
            .default_width(900)
            .default_height(600)
            .child(&content)
            .build();
        let (inbox, wake) = LatestInbox::new(16);
        let observer = Rc::new(RefCell::new(None));
        if let Some(graph) = fixture.clone() {
            inbox.publish(EventSequencer::default().graph(graph));
        } else {
            match pecan_pipewire::graaf::observe(inbox.clone()) {
                Ok(handle) => *observer.borrow_mut() = Some(handle),
                Err(error) => notices.set_text(&format!("Could not start observer: {error}")),
            }
        }
        let app_weak = app.downgrade();
        let task = glib::MainContext::default().spawn_local(async move {
            let mut state = OverviewState::default();
            let mut first = true;
            while wake.recv().await.is_ok() {
                let batch = inbox.drain();
                if !batch.notices.is_empty() {
                    notices.set_text(&format!("{:?}", batch.notices));
                }
                if state.apply(batch) || first {
                    first = false;
                    let view = state.overview();
                    let rate = view
                        .rate
                        .map_or("unknown".into(), |rate| format!("{rate} Hz"));
                    let quantum = view
                        .quantum
                        .map_or("unknown".into(), |quantum| quantum.to_string());
                    status.set_text(&format!(
                        "{} · rate {rate} · quantum {quantum} · {} nodes",
                        if view.connected {
                            "Connected"
                        } else {
                            "Disconnected"
                        },
                        view.nodes.len()
                    ));
                    while let Some(child) = rows.first_child() {
                        rows.remove(&child);
                    }
                    for node in &view.nodes {
                        let node_label = gtk::Label::builder()
                            .label(format!(
                                "{} [{}] · {}",
                                node.label,
                                node.id,
                                node.class.as_deref().unwrap_or("unknown class")
                            ))
                            .xalign(0.0)
                            .selectable(true)
                            .wrap(true)
                            .build();
                        rows.append(&node_label);
                        for port in &node.ports {
                            let label = gtk::Label::builder()
                                .label(format!(
                                    "    {} {} [{}] · {:?}",
                                    if port.input { "IN" } else { "OUT" },
                                    port.label,
                                    port.id,
                                    port.kind
                                ))
                                .xalign(0.0)
                                .selectable(true)
                                .wrap(true)
                                .build();
                            rows.append(&label);
                        }
                    }
                    if options.smoke {
                        println!(
                            "GTK fixture widgets populated: connected={}, nodes={}, ports={}",
                            view.connected,
                            view.nodes.len(),
                            view.nodes.iter().map(|n| n.ports.len()).sum::<usize>()
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
