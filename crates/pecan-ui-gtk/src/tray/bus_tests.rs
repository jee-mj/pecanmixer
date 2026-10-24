//! Opt-in, explicitly marked private D-Bus session. No desktop/audio access.
use super::*;
use crate::lifetime::{Effect, Lifetime, Resources, Visibility};
use pecan_pipewire::shutdown::Shutdown;
use std::sync::atomic::AtomicUsize;

fn require_isolated_bus() {
    assert!(
        std::env::var_os("PECAN_TRAY_TEST_BUS").is_some(),
        "start a fresh dbus-run-session and explicitly mark its address"
    );
    assert_eq!(
        std::env::var("PECAN_TRAY_TEST_BUS").ok(),
        std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()
    );
}
struct Watcher {
    host: Arc<AtomicBool>,
    reject: bool,
    registrations: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, _service: &str) -> zbus::fdo::Result<()> {
        if self.reject {
            return Err(zbus::fdo::Error::Failed(
                "Synthetic registration rejection".into(),
            ));
        }
        self.registrations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        self.host.load(Ordering::SeqCst)
    }
}
fn watcher(
    host: bool,
    reject: bool,
) -> (
    zbus::blocking::Connection,
    Arc<AtomicUsize>,
    Arc<AtomicBool>,
) {
    require_isolated_bus();
    let registrations = Arc::new(AtomicUsize::new(0));
    let host = Arc::new(AtomicBool::new(host));
    let connection = zbus::blocking::connection::Builder::session()
        .unwrap()
        .method_timeout(Duration::from_secs(2))
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at(
            "/StatusNotifierWatcher",
            Watcher {
                host: host.clone(),
                reject,
                registrations: registrations.clone(),
            },
        )
        .unwrap()
        .build()
        .unwrap();
    (connection, registrations, host)
}
fn availability(mailbox: &ActionMailbox, expected: TrayAvailability) -> ActionBatch {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let batch = mailbox.drain();
        if batch.tray.map(|e| e.availability) == Some(expected) {
            return batch;
        }
        assert!(
            Instant::now() < deadline,
            "fixture availability did not become {expected:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn isolated_bus_without_watcher_keeps_hidden_launch_reachable() {
    require_isolated_bus();
    let (mailbox, _wake) = ActionMailbox::new();
    let adapter = TrayAdapter::start(mailbox.clone(), TraySnapshot::default()).unwrap();
    let (mut policy, _) = Lifetime::<Shutdown>::start(
        true,
        Resources {
            backend: false,
            settings: false,
        },
        Duration::ZERO,
    );
    assert_eq!(
        availability(&mailbox, TrayAvailability::Unavailable).apply(&mut policy, mailbox.now()),
        vec![Effect::Present]
    );
    assert_eq!(policy.visibility(), Visibility::Visible);
    adapter.close().join().unwrap();
}
#[test]
fn isolated_bus_registration_host_loss_and_recovery_preserve_visible_fallback() {
    require_isolated_bus();
    let (watcher_connection, registrations, _) = watcher(true, false);
    let (mailbox, _wake) = ActionMailbox::new();
    let adapter = TrayAdapter::start(mailbox.clone(), TraySnapshot::default()).unwrap();
    let (mut policy, _) = Lifetime::<Shutdown>::start(
        true,
        Resources {
            backend: false,
            settings: false,
        },
        Duration::ZERO,
    );
    assert_eq!(
        availability(&mailbox, TrayAvailability::Usable).apply(&mut policy, mailbox.now()),
        vec![Effect::Hide]
    );
    assert!(registrations.load(Ordering::SeqCst) > 0);
    let generation = adapter.control.generation.load(Ordering::SeqCst);
    watcher_connection
        .release_name("org.kde.StatusNotifierWatcher")
        .unwrap();
    drop(watcher_connection);
    assert_eq!(
        availability(&mailbox, TrayAvailability::Unavailable).apply(&mut policy, mailbox.now()),
        vec![Effect::Present]
    );
    let (replacement, _, _) = watcher(true, false);
    let recovered = availability(&mailbox, TrayAvailability::Usable);
    assert!(
        adapter.control.generation.load(Ordering::SeqCst) > generation,
        "online callback alone is not confirmation"
    );
    assert!(recovered.apply(&mut policy, mailbox.now()).is_empty());
    assert_eq!(policy.visibility(), Visibility::Visible);
    adapter.close().join().unwrap();
    replacement
        .release_name("org.kde.StatusNotifierWatcher")
        .unwrap();
}
#[test]
fn isolated_bus_host_false_and_rejected_registration_never_report_usable() {
    require_isolated_bus();
    for (host, reject) in [(false, false), (true, true)] {
        let (watcher, _, _) = watcher(host, reject);
        let (mailbox, _wake) = ActionMailbox::new();
        let adapter = TrayAdapter::start(mailbox.clone(), TraySnapshot::default()).unwrap();
        availability(&mailbox, TrayAvailability::Unavailable);
        assert_ne!(
            mailbox.drain().tray.map(|e| e.availability),
            Some(TrayAvailability::Usable)
        );
        adapter.close().join().unwrap();
        watcher
            .release_name("org.kde.StatusNotifierWatcher")
            .unwrap();
    }
}

#[test]
fn isolated_bus_host_loss_without_watcher_restart_is_rechecked_without_cached_properties() {
    require_isolated_bus();
    let (watcher, _, host) = watcher(true, false);
    let (mailbox, _wake) = ActionMailbox::new();
    let adapter = TrayAdapter::start(mailbox.clone(), TraySnapshot::default()).unwrap();
    availability(&mailbox, TrayAvailability::Usable);
    host.store(false, Ordering::SeqCst);
    availability(&mailbox, TrayAvailability::Unavailable);
    adapter.close().join().unwrap();
    watcher
        .release_name("org.kde.StatusNotifierWatcher")
        .unwrap();
}
