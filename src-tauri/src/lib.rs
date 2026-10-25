pub mod autostart;
mod backend_adapter;
mod quit;
pub use pecan_pipewire::graaf;
pub use pecan_pipewire::meter;
pub use pecan_pipewire::midimeter;
pub use pecan_pipewire::opname;
pub mod tray;

use tauri::{Manager, State, WindowEvent};

use graaf::{BusOpname, Graaf, Momentopname, Opdracht, Opties};

#[tauri::command]
fn haal_graaf(g: State<Graaf>) -> Momentopname {
    g.nu()
}

#[tauri::command]
fn verbind(g: State<Graaf>, uit: u32, naar: u32) {
    g.doe(Opdracht::Verbind { uit, naar });
}

#[tauri::command]
fn ontkoppel(g: State<Graaf>, kabels: Vec<u32>) {
    for kabel in kabels {
        g.doe(Opdracht::Ontkoppel { kabel });
    }
}

/// `doel` is een serial, of `null` om het aan WirePlumber terug te geven.
#[tauri::command]
fn zet_doel(g: State<Graaf>, knoop: u32, doel: Option<String>) {
    g.doe(Opdracht::ZetDoel { knoop, doel });
}

#[tauri::command]
fn zet_volume(g: State<Graaf>, knoop: u32, volume: Vec<f32>) {
    let volume = volume.into_iter().map(|v| v.clamp(0.0, 4.0)).collect();
    g.doe(Opdracht::ZetVolume { knoop, volume });
}

#[tauri::command]
fn zet_transport(g: State<Graaf>, opties: Opties) {
    g.doe(Opdracht::ZetTransport(opties));
}

#[tauri::command]
fn zet_bus(g: State<Graaf>, kanalen: usize, gain: Vec<f32>) {
    g.doe(Opdracht::ZetBus { kanalen, gain });
}

#[tauri::command]
fn start_opname(g: State<Graaf>) {
    g.doe(Opdracht::Start);
}

#[tauri::command]
fn stop_opname(g: State<Graaf>) {
    g.doe(Opdracht::Stop);
}

/// Opent de map van een take, of `~/Recordings`.
#[tauri::command]
fn open_map(map: Option<String>) -> Result<(), String> {
    opname::open_map(map.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn start_bus(g: State<Graaf>, bus: BusOpname) {
    g.doe(Opdracht::StartBus(bus));
}

#[tauri::command]
fn stop_bus(g: State<Graaf>, naam: String) {
    g.doe(Opdracht::StopBus(naam));
}

#[tauri::command]
fn zet_mute(g: State<Graaf>, knoop: u32, mute: bool) {
    g.doe(Opdracht::ZetMute { knoop, mute });
}

pub fn run() {
    tauri::Builder::default()
        .manage(quit::QuitAdapter::default())
        // Een tweede start (uit het menu, terwijl hij al in de tray staat)
        // haalt alleen het venster van de eerste tevoorschijn.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::toon_venster(app)
        }))
        .on_window_event(|venster, e| {
            // Sluiten is verbergen: de app blijft in de tray, en routes,
            // meters en opnames lopen door. Stoppen gaat via Quit in de tray.
            if let WindowEvent::CloseRequested { api, .. } = e {
                if venster.app_handle().try_state::<tray::Tray>().is_some() {
                    api.prevent_close();
                    let _ = venster.hide();
                }
            }
        })
        .setup(|app| {
            tray::maak(app.handle());
            autostart::werk_bij();
            // Met --hidden (zo start de autostart hem) blijft het venster dicht.
            if !std::env::args().any(|a| a == "--hidden") {
                tray::toon_venster(app.handle());
            }
            app.manage(backend_adapter::start(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            haal_graaf,
            verbind,
            ontkoppel,
            zet_doel,
            zet_volume,
            zet_mute,
            zet_transport,
            zet_bus,
            start_opname,
            stop_opname,
            start_bus,
            stop_bus,
            open_map,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
