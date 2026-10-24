//! Explicit native entrypoint. Paths and fixture data are caller-selected;
//! there is no profile discovery, implicit migration or second backend consumer.
use crate::{
    application::{ApplicationOwner, ApplicationShell, ShellOptions},
    settings_storage::SettingsFile,
    settings_worker::SettingsWorker,
};
use gtk::{gio, glib, prelude::*};
use pecan_core::{graph::Momentopname, overview};
use std::{
    cell::RefCell, collections::BTreeMap, ffi::OsString, path::PathBuf, rc::Rc, time::Duration,
};

pub const HELP: &str = "pecan-ui-gtk [--settings /absolute/settings.json | --memory-defaults] [--recording-root /absolute/directory] [--tray] [--hidden] [--patchbay]\npecan-ui-gtk --fixture graph.json [--headless | --smoke] [--patchbay] [--hidden]\nDefault: one native PipeWire owner; display settings are memory-only unless an explicit path is supplied. Recording requires explicit native configuration and directory selection; active arming and structural migration remain unavailable. Fixture mode starts no PipeWire, real tray or settings I/O; fixture commands have synthetic receipts only. No profile discovery or automatic legacy import.";

#[derive(Clone, Default, Debug)]
pub struct Options {
    pub fixture: Option<PathBuf>,
    pub settings: Option<PathBuf>,
    pub recording_root: Option<PathBuf>,
    pub headless: bool,
    pub smoke: bool,
    pub patchbay: bool,
    pub hidden: bool,
    pub tray: bool,
}
impl Options {
    /// None requests help, before constructing GTK, a backend or an I/O worker.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut options = Self::default();
        let mut memory = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--fixture") if options.fixture.is_none() => {
                    options.fixture =
                        Some(args.next().ok_or("--fixture requires a JSON path")?.into());
                }
                Some("--settings") if options.settings.is_none() => {
                    options.settings = Some(
                        args.next()
                            .ok_or("--settings requires an absolute file path")?
                            .into(),
                    );
                }
                Some("--recording-root") if options.recording_root.is_none() => {
                    options.recording_root = Some(
                        args.next()
                            .ok_or("--recording-root requires an absolute directory")?
                            .into(),
                    );
                }
                Some("--memory-defaults") => memory = true,
                Some("--headless") => options.headless = true,
                Some("--smoke") => options.smoke = true,
                Some("--patchbay") => options.patchbay = true,
                Some("--hidden") => options.hidden = true,
                Some("--tray") => options.tray = true,
                Some("--help" | "-h") => return Ok(None),
                _ => return Err("Unknown or repeated native option; use --help.".into()),
            }
        }
        if memory && options.settings.is_some() {
            return Err("Choose --settings or --memory-defaults.".into());
        }
        options.validate()?;
        Ok(Some(options))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.headless && self.smoke {
            return Err("Choose --headless or --smoke.".into());
        }
        if (self.headless || self.smoke) && self.fixture.is_none() {
            return Err("--headless and --smoke require --fixture.".into());
        }
        if self.fixture.is_some()
            && (self.settings.is_some() || self.tray || self.recording_root.is_some())
        {
            return Err(
                "Fixture mode rejects settings paths, recording roots and real tray services."
                    .into(),
            );
        }
        if self
            .recording_root
            .as_ref()
            .is_some_and(|p| !p.is_absolute())
        {
            return Err("Recording root must be an explicit absolute directory.".into());
        }
        if let Some(path) = &self.settings {
            SettingsFile::new(path.clone()).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
struct Resources {
    owner: Rc<ApplicationOwner>,
    settings: Option<SettingsWorker>,
}
type Factory = Box<dyn FnOnce() -> Result<Resources, String>>;
/// One activation adapter retained for the GApplication lifetime. It consumes
/// its resource factory once; the reviewed shell retains the window/hold/clock.
pub struct EntryPoint {
    app: gtk::Application,
    options: Options,
    factory: RefCell<Option<Factory>>,
    shell: RefCell<Option<Rc<ApplicationShell>>>,
    failure: RefCell<Option<String>>,
}
impl EntryPoint {
    fn attach(app: &gtk::Application, options: Options, factory: Factory) -> Rc<Self> {
        let entry = Rc::new(Self {
            app: app.clone(),
            options,
            factory: RefCell::new(Some(factory)),
            shell: RefCell::default(),
            failure: RefCell::default(),
        });
        let weak = Rc::downgrade(&entry);
        app.connect_activate(move |_| {
            if let Some(entry) = weak.upgrade() {
                entry.activate();
            }
        });
        entry
    }
    fn activate(&self) {
        // ApplicationShell installs its own activation handler. Later events
        // present that same window; do not also create resources or clocks here.
        if self.shell.borrow().is_some() || self.failure.borrow().is_some() {
            return;
        }
        if !self.app.is_registered() || self.app.is_remote() {
            *self.failure.borrow_mut() =
                Some("Activate only in the registered primary application.".into());
            return;
        }
        let Some(factory) = self.factory.borrow_mut().take() else {
            return;
        };
        let result = factory().and_then(|r| {
            ApplicationShell::install(
                &self.app,
                r.owner,
                ShellOptions {
                    hidden: self.options.hidden,
                    tray: self.options.tray,
                },
                r.settings,
            )
            .map_err(|failure| {
                if let Some(worker) = failure.settings {
                    // No edits were admitted before installation. Close and
                    // drain the initial-load worker away from the GTK thread.
                    let (events, thread) = worker.close();
                    let _ = std::thread::Builder::new()
                        .name("pecan-uninstalled-settings".into())
                        .spawn(move || {
                            for _ in events {}
                            let _ = thread.join();
                        });
                }
                failure.reason.to_string()
            })
        });
        match result {
            Ok(shell) => {
                if let Err(error) = shell
                    .bus_owner()
                    .set_recording_root(self.options.recording_root.clone())
                {
                    *self.failure.borrow_mut() = Some(error.to_string());
                    shell.request_quit();
                }
                if self.options.patchbay {
                    shell.show_patchbay();
                }
                shell.tick();
                *self.shell.borrow_mut() = Some(shell.clone());
                if self.options.smoke {
                    if let Some(graph) = shell.session().graph() {
                        let overview = overview::project(&graph.value);
                        println!(
                            "GTK fixture widgets populated: connected={}, nodes={}, ports={}",
                            overview.connected,
                            overview.nodes.len(),
                            overview.nodes.iter().map(|n| n.ports.len()).sum::<usize>()
                        );
                    }
                    glib::timeout_add_local_once(Duration::from_millis(150), move || {
                        shell.request_quit()
                    });
                }
            }
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                self.app.quit();
            }
        }
    }
    pub fn shell(&self) -> Option<Rc<ApplicationShell>> {
        self.shell.borrow().clone()
    }
}

/// Parsing, explicit fixture loading and headless projection happen before GTK
/// construction. Only an actual primary GUI activation invokes the factory.
pub fn run(options: Options) -> Result<glib::ExitCode, Box<dyn std::error::Error>> {
    options.validate()?;
    let fixture: Option<Momentopname> = options
        .fixture
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        })
        .transpose()?;
    if options.headless {
        println!(
            "{}",
            serde_json::to_string_pretty(&overview::project(
                fixture.as_ref().expect("validated fixture mode")
            ))?
        );
        return Ok(glib::ExitCode::SUCCESS);
    }
    let path = options.settings.clone();
    let factory: Factory = if let Some(graph) = fixture {
        Box::new(move || {
            let owner = Rc::new(ApplicationOwner::fixture());
            let driver = owner.fixture_driver().expect("fixture owner");
            // Authored fixture-only generations: never use this construction
            // for production, which obtains registry witnesses from Graaf.
            let ids = graph
                .knopen
                .iter()
                .map(|n| n.id)
                .chain(graph.poorten.iter().map(|p| p.id))
                .chain(graph.kabels.iter().map(|k| k.id));
            let generations: BTreeMap<_, _> = ids
                .enumerate()
                .map(|(index, id)| (id, index as u64 + 1))
                .collect();
            driver.publish(graph, generations);
            // A fixture owns no native worker/writer to join. Tests can inject
            // a delayed driver instead; real shutdown uses the retained Graaf.
            driver.complete_shutdown(Default::default());
            Ok(Resources {
                owner,
                settings: None,
            })
        })
    } else {
        Box::new(move || {
            let settings = path
                .map(|path| {
                    SettingsFile::new(path)
                        .map_err(|e| e.to_string())
                        .and_then(|file| SettingsWorker::start(file).map_err(|e| e.to_string()))
                })
                .transpose()?;
            Ok(Resources {
                owner: Rc::new(ApplicationOwner::native()),
                settings,
            })
        })
    };
    // A fixture must never activate an already-running native application.
    let fixture_mode = options.fixture.is_some();
    let app = gtk::Application::builder()
        .application_id(if fixture_mode {
            "nl.jetse.pecanmixer.GtkFixture"
        } else {
            "nl.jetse.pecanmixer.Native"
        })
        .flags(if fixture_mode {
            gio::ApplicationFlags::NON_UNIQUE
        } else {
            gio::ApplicationFlags::FLAGS_NONE
        })
        .build();
    let entry = EntryPoint::attach(&app, options, factory);
    let result = app.run_with_args(&["pecan-ui-gtk"]);
    if let Some(error) = entry.failure.borrow().clone() {
        return Err(error.into());
    }
    Ok(match entry.shell().and_then(|s| s.exit_code()) {
        Some(0) => glib::ExitCode::SUCCESS,
        Some(_) => glib::ExitCode::FAILURE,
        None => result,
    })
}

#[cfg(test)]
mod tests;
