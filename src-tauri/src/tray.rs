//! Het icoon in "Status and Notifications", zoals bij CachyMix. Het venster
//! sluiten verbergt de app alleen: routes, meters en lopende opnames blijven
//! doorgaan zolang hij hier staat.
//!
//! Niet via de tray van Tauri: die loopt op Linux over libayatana-appindicator,
//! en daar bestaat alleen een menu. Een klik op het icoon toont dan dat menu in
//! plaats van de app te openen. `ksni` spreekt StatusNotifierItem zelf, dus
//! komt de linkerklik binnen als `activate` en het menu op de rechterklik.

use std::sync::Mutex;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use tauri::image::Image;
use tauri::{AppHandle, Manager};

use crate::autostart;
use crate::graaf::{Graaf, Opdracht};
use crate::quit::{Action, Phase, QuitAdapter};

/// Wat het icoon laat zien. Alleen bij een verandering gaat er iets over D-Bus.
#[derive(Clone, Default, PartialEq)]
pub struct Stand {
    pub verbonden: bool,
    pub rate: Option<u32>,
    pub opname: bool,
    /// De bussen die opnemen; leeg met `opname` = alleen MIDI.
    pub bussen: Vec<String>,
}

pub struct Tray {
    handle: Handle<Icoon>,
    laatst: Mutex<Stand>,
}

pub struct Icoon {
    app: AppHandle,
    stand: Stand,
    autostart: bool,
}

/// PNG naar wat StatusNotifierItem wil: ARGB32 in netwerkvolgorde.
fn pixmap(stand: &Stand) -> Vec<Icon> {
    let bytes: &[u8] = if stand.opname {
        include_bytes!("../icons/tray-opname.png")
    } else if stand.verbonden {
        include_bytes!("../icons/tray-aan.png")
    } else {
        include_bytes!("../icons/tray-uit.png")
    };
    let Ok(beeld) = Image::from_bytes(bytes) else {
        return Vec::new();
    };
    let data = beeld
        .rgba()
        .chunks_exact(4)
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    vec![Icon {
        width: beeld.width() as i32,
        height: beeld.height() as i32,
        data,
    }]
}

fn status(stand: &Stand) -> String {
    match (stand.verbonden, stand.rate) {
        (true, Some(r)) => format!("PipeWire: connected, {:.1} kHz", f64::from(r) / 1000.0),
        (true, None) => "PipeWire: connected".into(),
        (false, _) => "PipeWire: not connected".into(),
    }
}

fn opname(stand: &Stand) -> Option<String> {
    stand.opname.then(|| match stand.bussen.as_slice() {
        [] => "Recording".into(),
        b => format!("Recording: {}", b.join(", ")),
    })
}

pub fn toon_venster(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Een klik op het icoon: open als hij weg is, weg als hij open is — zoals
/// de andere iconen in de tray van KDE het doen.
fn wissel_venster(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(w) if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => toon_venster(app),
    }
}

/// Request full backend shutdown regardless of the cached recording flag:
/// writers can still be finalizing after transport reports "not recording".
/// No completion/D-Bus wait runs in the UI/tray callback. Repeated clicks
/// share one attempt; timeout keeps the app open and retains its handle.
pub fn stop(app: &AppHandle, _neemt_op: bool) {
    let Some(quit) = app.try_state::<QuitAdapter>() else {
        eprintln!("pecanmixer: Quit blocked: coordinator is not ready");
        return;
    };
    let Some(attempt) = quit.begin() else { return };
    let worker_app = app.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("pecanmixer-quit".into())
        .spawn(move || {
            let quit = worker_app.state::<QuitAdapter>();
            let action = quit.run(attempt, || {
                worker_app
                    .try_state::<Graaf>()
                    .map(|graph| graph.request_shutdown())
                    .ok_or_else(|| "Backend is not ready. Choose Retry Quit.".into())
            });
            match action {
                Action::Exit(code) => worker_app.exit(code),
                Action::StayOpen => {
                    eprintln!("pecanmixer: {:?}", quit.phase());
                    // menu_about_to_show refreshes presentation on the next
                    // menu open. Never wait for a D-Bus update acknowledgement.
                }
            }
        })
    {
        quit.could_not_spawn(format!(
            "Could not start quit check: {error}. Choose Retry Quit."
        ));
        eprintln!("pecanmixer: {:?}", quit.phase());
    }
}

fn quit_phase(app: &AppHandle) -> Phase {
    app.try_state::<QuitAdapter>()
        .map(|q| q.phase())
        .unwrap_or_else(|| {
            Phase::Blocked("Quit coordinator is not ready. Choose Retry Quit.".into())
        })
}

fn exit_after_error(app: &AppHandle) {
    if let Some(quit) = app.try_state::<QuitAdapter>() {
        if let Action::Exit(code) = quit.exit_after_error() {
            app.exit(code);
        }
    }
}

impl ksni::Tray for Icoon {
    fn id(&self) -> String {
        "pecanmixer".into()
    }

    fn title(&self) -> String {
        "Pecan Mixer".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        pixmap(&self.stand)
    }

    fn tool_tip(&self) -> ToolTip {
        let mut regels = vec![status(&self.stand)];
        regels.extend(opname(&self.stand));
        regels.extend(quit_phase(&self.app).status());
        ToolTip {
            title: concat!("Pecan Mixer ", env!("CARGO_PKG_VERSION")).into(),
            description: regels.join("\n"),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        wissel_venster(&self.app);
    }

    // Override ksni's default hook so its AboutToShow handler refreshes
    // properties/menu from the current quit phase. Background completion
    // never blocks on Handle::update (including from inside a tray callback).
    fn menu_about_to_show(&mut self) {}

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let quit = quit_phase(&self.app);
        let mut menu: Vec<MenuItem<Self>> = vec![StandardItem {
            label: status(&self.stand),
            enabled: false,
            ..Default::default()
        }
        .into()];
        if let Some(o) = opname(&self.stand) {
            menu.push(
                StandardItem {
                    label: o,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        if let Some(message) = quit.status() {
            menu.push(
                StandardItem {
                    label: message,
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        menu.push(MenuItem::Separator);
        menu.push(
            StandardItem {
                label: "Show Pecan Mixer".into(),
                activate: Box::new(|t: &mut Self| toon_venster(&t.app)),
                ..Default::default()
            }
            .into(),
        );
        if self.stand.opname {
            menu.push(
                StandardItem {
                    label: "Stop all recordings".into(),
                    icon_name: "media-playback-stop".into(),
                    activate: Box::new(|t: &mut Self| {
                        if let Some(g) = t.app.try_state::<Graaf>() {
                            g.doe(Opdracht::Stop);
                        }
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        menu.push(
            CheckmarkItem {
                label: "Start at login".into(),
                checked: self.autostart,
                activate: Box::new(|t: &mut Self| {
                    let _ = autostart::zet(!t.autostart);
                    t.autostart = autostart::staat_aan();
                }),
                ..Default::default()
            }
            .into(),
        );
        menu.push(MenuItem::Separator);
        // Bind the action to the label the user was shown. A cached normal
        // Quit item must not become an error acknowledgement after rendering.
        let error_exit_offered = matches!(quit, Phase::Failed(_));
        menu.push(
            StandardItem {
                label: quit.label().into(),
                enabled: !matches!(quit, Phase::Waiting | Phase::Exiting),
                icon_name: "application-exit".into(),
                activate: Box::new(move |t: &mut Self| {
                    // Recheck current policy; cached menu items cannot force
                    // an exit while another attempt is still pending.
                    if error_exit_offered {
                        exit_after_error(&t.app);
                    } else {
                        stop(&t.app, t.stand.opname);
                    }
                }),
                ..Default::default()
            }
            .into(),
        );
        menu
    }
}

pub fn maak(app: &AppHandle) {
    let icoon = Icoon {
        app: app.clone(),
        stand: Stand::default(),
        autostart: autostart::staat_aan(),
    };
    // Zonder tray (geen StatusNotifierWatcher) draait de app gewoon door,
    // alleen zonder icoon.
    if let Ok(handle) = icoon.spawn() {
        app.manage(Tray {
            handle,
            laatst: Mutex::new(Stand::default()),
        });
    }
}

/// Past de stand aan; het icoon hoort het alleen als er echt iets verandert.
pub fn werk_bij(app: &AppHandle, f: impl FnOnce(&mut Stand)) {
    let Some(t) = app.try_state::<Tray>() else {
        return;
    };
    let mut laatst = t.laatst.lock().unwrap();
    let mut nieuw = laatst.clone();
    f(&mut nieuw);
    if nieuw != *laatst {
        *laatst = nieuw.clone();
        drop(laatst);
        t.handle.update(move |i| i.stand = nieuw);
    }
}
