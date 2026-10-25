//! GTK-free optional StatusNotifierItem adapter. Construction is explicit and
//! startup is untouched. D-Bus spawn/update/shutdown waits run on one worker.
//! Application actions never wait for tray presentation or D-Bus acknowledgement.
mod mailbox;
use crate::lifetime::{QuitPhase, TrayAvailability};
use ksni::{
    blocking::TrayMethods,
    menu::{CheckmarkItem, StandardItem},
    MenuItem, ToolTip,
};
pub use mailbox::{
    ActionBatch, ActionMailbox, AutostartAction, ConsumerStopped, TrayEvidence, VisibilityAction,
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutostartMenu {
    pub source: u64,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub status: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraySnapshot {
    pub autostart: Option<AutostartMenu>,
    pub connected: bool,
    pub rate: Option<u32>,
    pub quit: QuitPhase,
}
impl Default for TraySnapshot {
    fn default() -> Self {
        Self {
            autostart: None,
            connected: false,
            rate: None,
            quit: QuitPhase::Idle,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayFailure {
    BusUnavailable,
    RegistrationRejected,
    NoHost,
}
struct Control {
    latest: Mutex<TraySnapshot>,
    dirty: AtomicBool,
    stop: AtomicBool,
    revalidate: AtomicU64,
    generation: AtomicU64,
    wake: SyncSender<()>,
}
impl Control {
    fn notify(&self) {
        let _ = self.wake.try_send(());
    }
}
struct Icon {
    snapshot: TraySnapshot,
    mailbox: Arc<ActionMailbox>,
    control: Arc<Control>,
    generation: u64,
}
fn quit_label(phase: &QuitPhase) -> &'static str {
    match phase {
        QuitPhase::Idle => "Quit",
        QuitPhase::Waiting => "Finalizing…",
        QuitPhase::Pending { .. } | QuitPhase::Blocked(_) => "Retry Quit",
        QuitPhase::Failed { .. } => "Exit after finalization error",
        QuitPhase::Exiting { .. } => "Exiting…",
    }
}
fn status(snapshot: &TraySnapshot) -> String {
    match (snapshot.connected, snapshot.rate) {
        (true, Some(rate)) => format!("PipeWire: connected, {:.1} kHz", f64::from(rate) / 1000.0),
        (true, None) => "PipeWire: connected".into(),
        _ => "PipeWire: not connected".into(),
    }
}
impl ksni::Tray for Icon {
    fn id(&self) -> String {
        "pecanmixer-native-preview".into()
    }
    fn title(&self) -> String {
        "Pecan Mixer".into()
    }
    fn icon_name(&self) -> String {
        "audio-card".into()
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: concat!("Pecan Mixer ", env!("CARGO_PKG_VERSION")).into(),
            description: format!(
                "{}\n{}",
                status(&self.snapshot),
                quit_label(&self.snapshot.quit)
            ),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.mailbox.toggle();
    }
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.mailbox
            .availability(self.generation, TrayAvailability::Unavailable);
        true
    }
    fn watcher_online(&self) {
        // ksni invokes this BEFORE re-registration. Keep the window reachable
        // until our serialized worker confirms a fresh registration instead.
        self.mailbox
            .availability(self.generation, TrayAvailability::Unavailable);
        if self.control.generation.load(Ordering::SeqCst) == self.generation {
            self.control
                .revalidate
                .fetch_max(self.generation, Ordering::SeqCst);
        }
        self.control.notify();
    }
    fn menu_about_to_show(&mut self) {}
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let offered = self.snapshot.quit.clone();
        let desktop = self.snapshot.autostart.clone();
        let source = desktop.as_ref().map_or(0, |d| d.source);
        let label = desktop.as_ref().map_or_else(
            || "Native login entry (explicit context required)".into(),
            |d| format!("Native login entry · {}", d.status),
        );
        let login = match desktop.as_ref().and_then(|d| d.checked) {
            Some(checked) => CheckmarkItem {
                label,
                checked,
                enabled: desktop.as_ref().is_some_and(|d| d.enabled)
                    && matches!(offered, QuitPhase::Idle),
                activate: Box::new(move |icon: &mut Self| {
                    let _ = icon.mailbox.autostart(source, !checked);
                }),
                ..Default::default()
            }
            .into(),
            None => StandardItem {
                label,
                enabled: false,
                ..Default::default()
            }
            .into(),
        };
        vec![
            login,
            StandardItem {
                label: status(&self.snapshot),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Show Pecan Mixer".into(),
                activate: Box::new(|icon: &mut Self| {
                    let _ = icon.mailbox.present();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: quit_label(&offered).into(),
                enabled: !matches!(offered, QuitPhase::Waiting | QuitPhase::Exiting { .. }),
                activate: Box::new(move |icon: &mut Self| {
                    // Bind to the item actually shown. A cached normal Quit
                    // must not become acknowledgement of a later error.
                    if let QuitPhase::Failed {
                        acknowledgement, ..
                    } = offered
                    {
                        let _ = icon.mailbox.acknowledge_failure(acknowledgement);
                    } else {
                        let _ = icon.mailbox.quit();
                    }
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
trait Service: Send {
    fn healthy(&mut self) -> bool {
        true
    }
    fn update(&mut self, snapshot: TraySnapshot) -> bool;
    fn stop(&mut self);
}
trait Factory: Send + 'static {
    fn start(&mut self, icon: Icon) -> Result<Box<dyn Service>, TrayFailure>;
}
struct KsniFactory;
struct KsniService {
    handle: ksni::blocking::Handle<Icon>,
    probe: zbus::blocking::Connection,
}
impl KsniService {
    fn host_present(&self) -> bool {
        let Ok(proxy) = zbus::blocking::Proxy::new(
            &self.probe,
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.freedesktop.DBus.Properties",
        ) else {
            return false;
        };
        // Explicit Get bypasses property caching; some watchers remain alive
        // after their last host unregisters. Every method has a finite timeout.
        let value: Result<zbus::zvariant::OwnedValue, _> = proxy.call(
            "Get",
            &(
                "org.kde.StatusNotifierWatcher",
                "IsStatusNotifierHostRegistered",
            ),
        );
        value
            .ok()
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false)
    }
}
impl Factory for KsniFactory {
    fn start(&mut self, icon: Icon) -> Result<Box<dyn Service>, TrayFailure> {
        let probe = zbus::blocking::connection::Builder::session()
            .map_err(|_| TrayFailure::BusUnavailable)?
            .method_timeout(Duration::from_secs(1))
            .build()
            .map_err(|_| TrayFailure::BusUnavailable)?;
        let handle = icon.spawn().map_err(|error| match error {
            ksni::Error::Dbus(_) => TrayFailure::BusUnavailable,
            ksni::Error::WontShow => TrayFailure::NoHost,
            _ => TrayFailure::RegistrationRejected,
        })?;
        let mut service = KsniService { handle, probe };
        if !service.host_present() {
            service.stop();
            return Err(TrayFailure::NoHost);
        }
        Ok(Box::new(service))
    }
}
impl Drop for KsniService {
    fn drop(&mut self) {
        let _ = self.handle.shutdown();
    }
}
impl Service for KsniService {
    fn healthy(&mut self) -> bool {
        self.host_present()
    }
    fn update(&mut self, snapshot: TraySnapshot) -> bool {
        self.handle
            .update(move |icon| icon.snapshot = snapshot)
            .is_some()
    }
    fn stop(&mut self) {
        self.handle.shutdown().wait();
    }
}
/// One adapter per owning application. No GTK object crosses into its worker.
/// A blocked D-Bus operation can delay worker teardown; it cannot authorize
/// audio completion and must never become a prerequisite for safe audio quit.
pub struct TrayAdapter {
    control: Arc<Control>,
    thread: Option<JoinHandle<()>>,
}
impl TrayAdapter {
    pub fn start(mailbox: Arc<ActionMailbox>, initial: TraySnapshot) -> io::Result<Self> {
        Self::start_factory(mailbox, initial, KsniFactory)
    }
    fn start_factory(
        mailbox: Arc<ActionMailbox>,
        initial: TraySnapshot,
        factory: impl Factory,
    ) -> io::Result<Self> {
        let (wake, receiver) = mpsc::sync_channel(1);
        let control = Arc::new(Control {
            latest: Mutex::new(initial),
            dirty: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            revalidate: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            wake,
        });
        let worker = control.clone();
        let failed = mailbox.clone();
        let thread = thread::Builder::new()
            .name("pecanmixer-native-tray".into())
            .spawn(move || run(mailbox, worker, receiver, factory));
        match thread {
            Ok(thread) => Ok(Self {
                control,
                thread: Some(thread),
            }),
            Err(error) => {
                if let Some(generation) = failed.begin_generation() {
                    failed.availability(generation, TrayAvailability::Unavailable);
                }
                Err(error)
            }
        }
    }
    /// Nonblocking admission: busy/closed returns the unconsumed snapshot for
    /// deliberate retry. One latest state replaces older pending presentation.
    pub fn try_update(&self, snapshot: TraySnapshot) -> Result<(), TraySnapshot> {
        if self.control.stop.load(Ordering::SeqCst) {
            return Err(snapshot);
        }
        let Ok(mut latest) = self.control.latest.try_lock() else {
            return Err(snapshot);
        };
        *latest = snapshot;
        self.control.dirty.store(true, Ordering::SeqCst);
        self.control.notify();
        Ok(())
    }
    pub fn request_stop(&self) {
        self.control.stop.store(true, Ordering::SeqCst);
        self.control.notify();
    }
    /// Request teardown and return the worker. Join only off-UI, if desired.
    /// There is no promise of bounded D-Bus shutdown; Drop only requests stop.
    pub fn close(mut self) -> JoinHandle<()> {
        self.request_stop();
        self.thread.take().expect("owned tray worker")
    }
}
impl Drop for TrayAdapter {
    fn drop(&mut self) {
        self.request_stop();
    }
}
struct Guard {
    mailbox: Arc<ActionMailbox>,
    control: Arc<Control>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::SeqCst);
        self.mailbox.availability(
            self.control.generation.load(Ordering::SeqCst),
            TrayAvailability::Unavailable,
        );
    }
}
fn run(
    mailbox: Arc<ActionMailbox>,
    control: Arc<Control>,
    receiver: Receiver<()>,
    mut factory: impl Factory,
) {
    let _guard = Guard {
        mailbox: mailbox.clone(),
        control: control.clone(),
    };
    let mut service: Option<Box<dyn Service>> = None;
    let mut retry = Instant::now();
    let mut health_due = Instant::now();
    while !control.stop.load(Ordering::SeqCst) {
        let revalidate = control.revalidate.swap(0, Ordering::SeqCst);
        if revalidate != 0 && revalidate == control.generation.load(Ordering::SeqCst) {
            mailbox.availability(
                control.generation.load(Ordering::SeqCst),
                TrayAvailability::Unavailable,
            );
            if let Some(mut old) = service.take() {
                old.stop();
            }
            retry = Instant::now();
        }
        if control.stop.load(Ordering::SeqCst) {
            break;
        }
        if service.is_none() && Instant::now() >= retry {
            let Some(generation) = mailbox.begin_generation() else {
                break;
            };
            control.generation.store(generation, Ordering::SeqCst);
            let snapshot = match control.latest.lock() {
                Ok(latest) => latest.clone(),
                Err(_) => break,
            };
            let observed_at = mailbox.now();
            match factory.start(Icon {
                snapshot,
                mailbox: mailbox.clone(),
                control: control.clone(),
                generation,
            }) {
                Ok(handle) => {
                    service = Some(handle);
                    mailbox.registered(generation, observed_at);
                    health_due = Instant::now() + Duration::from_secs(1);
                }
                Err(_) => {
                    mailbox.availability(generation, TrayAvailability::Unavailable);
                    retry = Instant::now() + Duration::from_secs(1);
                }
            }
        }
        if control.stop.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() >= health_due {
            if let Some(handle) = service.as_mut() {
                let generation = control.generation.load(Ordering::SeqCst);
                let observed_at = mailbox.now();
                if handle.healthy() {
                    mailbox.heartbeat(generation, observed_at);
                } else {
                    mailbox.availability(generation, TrayAvailability::Unavailable);
                    if let Some(mut old) = service.take() {
                        old.stop();
                    }
                    retry = Instant::now() + Duration::from_secs(1);
                }
                health_due = Instant::now() + Duration::from_secs(1);
            }
        }
        if control.stop.load(Ordering::SeqCst) {
            break;
        }
        if let Some(handle) = service.as_mut() {
            if control.dirty.swap(false, Ordering::SeqCst) {
                let snapshot = match control.latest.lock() {
                    Ok(latest) => latest.clone(),
                    Err(_) => break,
                };
                if !handle.update(snapshot) {
                    mailbox.availability(
                        control.generation.load(Ordering::SeqCst),
                        TrayAvailability::Unavailable,
                    );
                    if let Some(mut old) = service.take() {
                        old.stop();
                    }
                    retry = Instant::now() + Duration::from_secs(1);
                }
            }
        }
        let _ = receiver.recv_timeout(Duration::from_millis(100));
    }
    if let Some(mut service) = service {
        service.stop();
    }
}

#[cfg(all(test, feature = "tray-bus-tests"))]
mod bus_tests;
#[cfg(test)]
mod tests;
