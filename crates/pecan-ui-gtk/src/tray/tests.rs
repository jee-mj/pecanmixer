use super::*;
use crate::lifetime::{Completion, Effect, Event, Lifetime, Resources};
use ksni::Tray;
use pecan_pipewire::shutdown::ShutdownReport;
use std::sync::atomic::AtomicUsize;
fn control() -> Arc<Control> {
    let (wake, _receiver) = mpsc::sync_channel(1);
    Arc::new(Control {
        latest: Mutex::new(TraySnapshot::default()),
        dirty: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        revalidate: AtomicU64::new(0),
        generation: AtomicU64::new(0),
        wake,
    })
}
#[derive(Clone)]
struct Failed;
impl Completion for Failed {
    fn try_complete(&self) -> Option<ShutdownReport> {
        Some(ShutdownReport {
            worker_panicked: true,
            ..Default::default()
        })
    }
}
#[test]
fn menu_callbacks_have_no_gtk_and_cached_quit_cannot_acknowledge_a_later_error() {
    let (mailbox, _receiver) = ActionMailbox::new();
    let control = control();
    let mut icon = Icon {
        snapshot: TraySnapshot::default(),
        mailbox: mailbox.clone(),
        control,
        generation: 0,
    };
    let mut old_menu = icon.menu();
    let (mut policy, _) = Lifetime::<Failed>::start(
        false,
        Resources {
            backend: true,
            settings: false,
        },
        Duration::ZERO,
    );
    let effects = policy.apply(Event::Quit, Duration::ZERO);
    let id = effects
        .iter()
        .find_map(|e| {
            if let Effect::RequestBackendShutdown { request } = e {
                Some(*request)
            } else {
                None
            }
        })
        .unwrap();
    policy.backend_requested(id, Ok(Failed), Duration::ZERO);
    icon.snapshot.quit = policy.phase().clone();
    let MenuItem::Standard(item) = old_menu.pop().unwrap() else {
        panic!("quit item")
    };
    (item.activate)(&mut icon);
    assert!(mailbox
        .drain()
        .apply(&mut policy, Duration::ZERO)
        .is_empty());
    assert!(matches!(policy.phase(), QuitPhase::Failed { .. }));
    let MenuItem::Standard(item) = icon.menu().pop().unwrap() else {
        panic!("explicit error item")
    };
    assert_eq!(item.label, "Exit after finalization error");
    (item.activate)(&mut icon);
    assert_eq!(
        mailbox.drain().apply(&mut policy, Duration::ZERO),
        vec![Effect::Exit { code: 1 }]
    );
}
#[test]
fn watcher_online_never_proves_registration_and_old_callbacks_cannot_revalidate_new_service() {
    let (mailbox, _receiver) = ActionMailbox::new();
    let control = control();
    let first = mailbox.begin_generation().unwrap();
    control.generation.store(first, Ordering::SeqCst);
    let icon = Icon {
        snapshot: TraySnapshot::default(),
        mailbox: mailbox.clone(),
        control: control.clone(),
        generation: first,
    };
    mailbox.registered(first, mailbox.now());
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Usable)
    );
    icon.watcher_offline(ksni::OfflineReason::No);
    icon.watcher_online();
    mailbox.registered(first, mailbox.now());
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Unavailable)
    );
    assert_eq!(control.revalidate.swap(0, Ordering::SeqCst), first);
    let second = mailbox.begin_generation().unwrap();
    control.generation.store(second, Ordering::SeqCst);
    mailbox.registered(second, mailbox.now());
    icon.watcher_online();
    assert_eq!(control.revalidate.load(Ordering::SeqCst), 0);
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Usable)
    );
}
struct GateFactory {
    entered: SyncSender<thread::ThreadId>,
    release: Receiver<()>,
    stops: Arc<AtomicUsize>,
}
struct NoopService(Arc<AtomicUsize>);
impl Service for NoopService {
    fn update(&mut self, _: TraySnapshot) -> bool {
        true
    }
    fn stop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl Factory for GateFactory {
    fn start(&mut self, _: Icon) -> Result<Box<dyn Service>, TrayFailure> {
        self.entered.send(thread::current().id()).unwrap();
        self.release.recv_timeout(Duration::from_secs(3)).unwrap();
        Ok(Box::new(NoopService(self.stops.clone())))
    }
}
#[test]
fn blocked_registration_cannot_block_quit_admission_or_close_and_busy_update_retains_snapshot() {
    let (mailbox, _receiver) = ActionMailbox::new();
    let (tx, entered) = mpsc::sync_channel(1);
    let (release, rx) = mpsc::sync_channel(1);
    let stops = Arc::new(AtomicUsize::new(0));
    let adapter = TrayAdapter::start_factory(
        mailbox.clone(),
        TraySnapshot::default(),
        GateFactory {
            entered: tx,
            release: rx,
            stops: stops.clone(),
        },
    )
    .unwrap();
    assert_ne!(
        entered.recv_timeout(Duration::from_secs(3)).unwrap(),
        thread::current().id()
    );
    let snapshot = TraySnapshot {
        autostart: None,
        connected: true,
        rate: Some(48000),
        quit: QuitPhase::Idle,
    };
    let guard = adapter.control.latest.lock().unwrap();
    let before = Instant::now();
    assert_eq!(adapter.try_update(snapshot.clone()), Err(snapshot));
    assert!(before.elapsed() < Duration::from_millis(200));
    drop(guard);
    for _ in 0..10000 {
        mailbox.toggle().unwrap();
        mailbox.quit().unwrap();
    }
    assert!(mailbox.drain().quit);
    let before = Instant::now();
    let handle = adapter.close();
    assert!(before.elapsed() < Duration::from_millis(200));
    assert!(!handle.is_finished());
    release.send(()).unwrap();
    handle.join().unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Unavailable)
    );
}
struct UpdateFactory {
    calls: SyncSender<(u32, thread::ThreadId)>,
    release: Option<Receiver<()>>,
}
struct UpdateService {
    calls: SyncSender<(u32, thread::ThreadId)>,
    release: Option<Receiver<()>>,
}
impl Factory for UpdateFactory {
    fn start(&mut self, _: Icon) -> Result<Box<dyn Service>, TrayFailure> {
        Ok(Box::new(UpdateService {
            calls: self.calls.clone(),
            release: self.release.take(),
        }))
    }
}
impl Service for UpdateService {
    fn update(&mut self, snapshot: TraySnapshot) -> bool {
        self.calls
            .send((snapshot.rate.unwrap(), thread::current().id()))
            .unwrap();
        if let Some(release) = self.release.take() {
            release.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        true
    }
    fn stop(&mut self) {}
}
#[test]
fn stalled_update_retains_only_latest_pending_state_on_the_same_worker() {
    let (mailbox, _receiver) = ActionMailbox::new();
    let (tx, calls) = mpsc::sync_channel(4);
    let (release, rx) = mpsc::sync_channel(1);
    let adapter = TrayAdapter::start_factory(
        mailbox,
        TraySnapshot::default(),
        UpdateFactory {
            calls: tx,
            release: Some(rx),
        },
    )
    .unwrap();
    adapter
        .try_update(TraySnapshot {
            autostart: None,
            rate: Some(1),
            ..Default::default()
        })
        .unwrap();
    let (first, worker) = calls.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(first, 1);
    assert_ne!(worker, thread::current().id());
    for rate in 2..=50000 {
        adapter
            .try_update(TraySnapshot {
                autostart: None,
                rate: Some(rate),
                ..Default::default()
            })
            .unwrap();
    }
    release.send(()).unwrap();
    let (latest, same) = calls.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(latest, 50000);
    assert_eq!(same, worker);
    adapter.close().join().unwrap();
    assert!(calls.try_recv().is_err());
}

struct HealthFactory {
    entered: SyncSender<()>,
    release: Option<Receiver<()>>,
}
struct HealthService {
    entered: SyncSender<()>,
    release: Option<Receiver<()>>,
}
impl Factory for HealthFactory {
    fn start(&mut self, _: Icon) -> Result<Box<dyn Service>, TrayFailure> {
        Ok(Box::new(HealthService {
            entered: self.entered.clone(),
            release: self.release.take(),
        }))
    }
}
impl Service for HealthService {
    fn update(&mut self, _: TraySnapshot) -> bool {
        true
    }
    fn healthy(&mut self) -> bool {
        self.entered.send(()).unwrap();
        self.release
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        true
    }
    fn stop(&mut self) {}
}
#[test]
fn stalled_host_check_cannot_block_visibility_recovery_quit_or_close() {
    let (mailbox, _wake) = ActionMailbox::new();
    let (tx, entered) = mpsc::sync_channel(1);
    let (release, rx) = mpsc::sync_channel(1);
    let adapter = TrayAdapter::start_factory(
        mailbox.clone(),
        TraySnapshot::default(),
        HealthFactory {
            entered: tx,
            release: Some(rx),
        },
    )
    .unwrap();
    let (mut policy, _) = Lifetime::<Failed>::start(
        true,
        Resources {
            backend: true,
            settings: false,
        },
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let batch = mailbox.drain();
        if batch.tray.map(|e| e.availability) == Some(TrayAvailability::Usable) {
            batch.apply(&mut policy, mailbox.now());
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(policy.visibility(), crate::lifetime::Visibility::Hidden);
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    let now = mailbox.now() + crate::lifetime::TRAY_EVIDENCE_LIMIT;
    assert_eq!(policy.apply(Event::Tick, now), vec![Effect::Present]);
    mailbox.quit().unwrap();
    assert!(mailbox
        .drain()
        .apply(&mut policy, now)
        .iter()
        .any(|e| matches!(e, Effect::RequestBackendShutdown { .. })));
    let before = Instant::now();
    let handle = adapter.close();
    assert!(before.elapsed() < Duration::from_millis(200));
    assert!(!handle.is_finished());
    release.send(()).unwrap();
    handle.join().unwrap();
}

struct PanickingFactory;
impl Factory for PanickingFactory {
    fn start(&mut self, _: Icon) -> Result<Box<dyn Service>, TrayFailure> {
        panic!("Synthetic tray worker failure")
    }
}
#[test]
fn failed_worker_revokes_availability_and_rejects_later_presentation() {
    let (mailbox, _wake) = ActionMailbox::new();
    let adapter =
        TrayAdapter::start_factory(mailbox.clone(), TraySnapshot::default(), PanickingFactory)
            .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !adapter.control.stop.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    let snapshot = TraySnapshot {
        autostart: None,
        connected: true,
        ..Default::default()
    };
    assert_eq!(adapter.try_update(snapshot.clone()), Err(snapshot));
    assert!(adapter.close().join().is_err());
    assert_eq!(
        mailbox.drain().tray.map(|e| e.availability),
        Some(TrayAvailability::Unavailable)
    );
}

#[test]
fn desktop_tray_callback_retains_offered_source_and_unknown_is_not_false_disabled() {
    let (mailbox, _wake) = ActionMailbox::new();
    let mut icon = Icon {
        mailbox: mailbox.clone(),
        snapshot: TraySnapshot::default(),
        generation: 1,
        control: control(),
    };
    assert!(matches!(&icon.menu()[0], MenuItem::Standard(item) if !item.enabled));
    icon.snapshot.autostart = Some(AutostartMenu {
        source: 7,
        checked: Some(false),
        enabled: true,
        status: "Synthetic missing native entry".into(),
    });
    let mut old = icon.menu();
    icon.snapshot.autostart = Some(AutostartMenu {
        source: 9,
        checked: None,
        enabled: false,
        status: "Synthetic pending inspection".into(),
    });
    let MenuItem::Checkmark(item) = old.remove(0) else {
        panic!("known state needs checkmark")
    };
    (item.activate)(&mut icon);
    assert_eq!(
        mailbox.drain().autostart,
        Some(AutostartAction {
            source: 7,
            enabled: true
        })
    );
    assert!(matches!(&icon.menu()[0], MenuItem::Standard(item) if !item.enabled));
}
