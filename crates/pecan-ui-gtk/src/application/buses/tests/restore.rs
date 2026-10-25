//! Synthetic initial settings, exact registry frames and injected receipts only.
use super::*;
use crate::application::preferences::PreferencesState;
use crate::{settings_storage::SettingsFile, settings_worker::SettingsEvent};
use pecan_core::settings::SettingsV1;
use std::sync::atomic::AtomicU64;
static NEXT: AtomicU64 = AtomicU64::new(1);
fn setup_restore() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let (owner, fake, driver) = levels_owner::setup_levels();
    fake.restore_enabled.set(true);
    (owner, fake, driver)
}
fn preferences() -> PreferencesState {
    let mut state = PreferencesState::new(false);
    let mut p = SettingsV1::default().into_preferences();
    p.buses = p
        .buses
        .with_db("rec-1-2", -18.)
        .unwrap()
        .with_mute("rec-1-2", true)
        .unwrap();
    p.buses = p.buses.arm("rec-1-2", true).unwrap();
    p.clock.start_stop = true;
    assert!(p.buses.midi);
    state.edit(SettingsV1::new(p).unwrap()).unwrap();
    state
}
fn terminal(fake: &Fake, outcome: LocalConfigOutcome, publish: bool) -> ConfigReport {
    let report = fake.complete_config(0, outcome, false);
    let mut report = report;
    if let ConfigProgress::Completed(c) = &mut report.progress {
        let config = c.snapshot.configuration.as_ref().unwrap();
        let pecan_core::graph::Opdracht::ZetBus { gain, .. } =
            config.buses.backend_config().unwrap()
        else {
            panic!()
        };
        c.snapshot.local.as_mut().unwrap().gains = gain;
        if c.outcome == LocalConfigOutcome::NoOp {
            c.snapshot.revision = report.expected;
        }
        if publish {
            *fake.config.borrow_mut() = c.snapshot.clone();
        }
    }
    *fake.configs.borrow()[0].1.report.borrow_mut() = Some(report.clone());
    report
}
fn settled(owner: &BusOwner) -> bool {
    owner
        .data
        .borrow()
        .operation
        .as_ref()
        .is_some_and(|w| w.terminal && w.restore.as_ref().unwrap().released)
}
#[gtk::test]
fn restore_exact_origin_masked_preferences_once_and_no_writeback() {
    let (owner, fake, driver) = setup_restore();
    let p = preferences();
    let before = p.document().clone();
    owner.bind_preference_edits(Rc::new(|_| panic!("restore is not a user preference edit")));
    owner.restore_preferences(&p);
    assert!(owner.session.busy());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    let original = fake.restore_origins.borrow()[0].graph().clone();
    assert!(Arc::ptr_eq(
        &original.value,
        &owner.session.graph().unwrap().value
    ));
    let intent = fake.configs.borrow()[0].0.clone();
    assert!(!intent.configuration.buses.midi);
    assert!(!intent.configuration.start_stop);
    assert!(intent.configuration.clock.is_none());
    assert_eq!(intent.configuration.buses.state("rec-1-2").db, -18.);
    assert!(intent.configuration.buses.state("rec-1-2").mute);
    assert_eq!(
        intent.configuration.buses.state("rec-1-2").armed,
        p.document().preferences().buses.state("rec-1-2").armed
    );
    for _ in 0..1000 {
        owner.restore_preferences(&p);
    }
    assert_eq!(fake.configs.borrow().len(), 1);
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    assert!(owner.session.busy());
    owner.poll();
    assert!(!owner.session.busy());
    assert!(settled(&owner));
    assert_eq!(p.document(), &before);
    assert!(driver.take_requests().is_empty());
}
#[gtk::test]
fn restore_noop_needs_inspection_after_report_and_busy_read_keeps_lease() {
    let (owner, fake, _) = setup_restore();
    owner.restore_preferences(&preferences());
    let report = terminal(&fake, LocalConfigOutcome::NoOp, true);
    owner.poll();
    assert!(owner.session.busy());
    fake.busy.set(true);
    for _ in 0..5 {
        owner.refresh();
        owner.poll();
        assert!(owner.session.busy());
    }
    fake.busy.set(false);
    owner.poll();
    assert!(settled(&owner));
    assert!(!owner.session.busy());
    assert_eq!(owner.configuration_report(), Some(report));
}
#[gtk::test]
fn restore_old_readback_waits_but_newer_mismatch_becomes_uncertainty_without_replay() {
    let (owner, fake, _) = setup_restore();
    owner.restore_preferences(&preferences());
    let report = terminal(&fake, LocalConfigOutcome::Applied, false);
    for _ in 0..5 {
        owner.poll();
        assert!(owner.session.busy());
    }
    fake.config.borrow_mut().revision.revision += 10;
    owner.poll();
    assert!(settled(&owner));
    assert!(!owner.session.busy());
    assert!(matches!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Unconfirmed(_)
    ));
    assert_eq!(owner.configuration_report(), Some(report.clone()));
    for _ in 0..10 {
        owner.restore_preferences(&preferences());
        owner.refresh();
    }
    assert_eq!(fake.configs.borrow().len(), 1);
    assert_eq!(owner.restore.borrow().reports.back(), Some(&report));
}
#[gtk::test]
fn restore_replacement_cannot_adopt_new_identity_or_replay() {
    let (owner, fake, driver) = setup_restore();
    owner.restore_preferences(&preferences());
    let report = terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    let g = (*owner.session.graph().unwrap().value).clone();
    let mut ids = generations(&g);
    *ids.get_mut(&30).unwrap() += 999;
    driver.publish(g, ids);
    owner.session.poll();
    owner.poll();
    assert!(settled(&owner));
    assert!(matches!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Unconfirmed(_)
    ));
    assert_eq!(owner.configuration_report(), Some(report));
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_after_success_admits_new_epoch_once() {
    let (owner, fake, driver) = setup_restore();
    let p = preferences();
    owner.restore_preferences(&p);
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    let mut g = (*owner.session.graph().unwrap().value).clone();
    g.verbonden = false;
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    owner.poll();
    g.verbonden = true;
    let next = driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    fake.config.borrow_mut().revision.epoch = next.stamp.epoch;
    owner.poll();
    owner.restore_preferences(&p);
    owner.restore_preferences(&p);
    assert_eq!(fake.restore_origins.borrow().len(), 2);
    assert_eq!(fake.configs.borrow()[1].0.expected.epoch, next.stamp.epoch);
}
#[gtk::test]
fn restore_unknown_metadata_same_count_mono_is_explicitly_authorized_not_defaulted() {
    let (owner, fake, _) = setup_restore();
    fake.config.borrow_mut().configuration = None;
    owner.poll();
    let mut p = SettingsV1::default().into_preferences();
    p.buses.widths = vec![1, 1];
    p.buses = p.buses.with_db("rec-1", -12.).unwrap();
    let mut state = PreferencesState::new(false);
    state.edit(SettingsV1::new(p).unwrap()).unwrap();
    owner.restore_preferences(&state);
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    assert_eq!(
        fake.configs.borrow()[0].0.configuration.buses.widths,
        [1, 1]
    );
}
#[gtk::test]
fn restore_missing_prefix_waits_and_unsupported_count_stays_explicit() {
    let (owner, fake, driver) = setup_restore();
    let full = (*owner.session.graph().unwrap().value).clone();
    let mut partial = full.clone();
    partial.poorten.retain(|p| p.naam != "rec_2");
    driver.publish(partial.clone(), generations(&partial));
    owner.session.poll();
    owner.poll();
    owner.restore_preferences(&preferences());
    assert!(fake.restore_origins.borrow().is_empty());
    driver.publish(full.clone(), generations(&full));
    owner.session.poll();
    owner.poll();
    let mut p = SettingsV1::default().into_preferences();
    p.buses.widths = vec![2, 2];
    let mut state = PreferencesState::new(false);
    state.edit(SettingsV1::new(p).unwrap()).unwrap();
    owner.restore_preferences(&state);
    assert!(fake.configs.borrow().is_empty());
    assert!(owner.restore_status.text().contains("migration"));
    owner.restore_preferences(&preferences());
    assert!(fake.configs.borrow().is_empty());
}
#[gtk::test]
fn restore_active_or_prior_user_operation_skips_epoch_and_stop_remains_available() {
    for active in [true, false] {
        let (owner, fake, _) = setup_restore();
        if active {
            fake.config.borrow_mut().recording = true;
            owner.poll();
        } else {
            arm(&owner);
        }
        owner.restore_preferences(&preferences());
        assert!(fake.restore_origins.borrow().is_empty());
        assert!(button(owner.widget(), "Stop all").is_sensitive());
        assert!(owner.restore_status.text().contains("skipped"));
    }
}
#[gtk::test]
fn restore_handoff_and_receipt_reentry_do_not_borrow_or_duplicate() {
    let (owner, fake, _) = setup_restore();
    let weak = Rc::downgrade(&owner);
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        o.restore_preferences(&preferences());
        o.recording_status();
        o.poll();
    }));
    owner.restore_preferences(&preferences());
    let weak = Rc::downgrade(&owner);
    *fake.configs.borrow()[0].1.hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        o.recording_status();
        o.refresh();
    }));
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    assert!(settled(&owner));
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_initial_valid_missing_and_protected_settings_never_use_reload_defaults() {
    for bytes in [
        None,
        Some(serde_json::to_vec(&SettingsV1::default()).unwrap()),
        Some(b"{corrupt".to_vec()),
        Some(b"{\"version\":999}".to_vec()),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "pecan-restore-synthetic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("settings.json");
        if let Some(bytes) = &bytes {
            std::fs::write(&path, bytes).unwrap();
        }
        let snapshot = SettingsFile::new(path.clone()).unwrap().load().unwrap();
        let protected = snapshot.recovery_required();
        let mut p = PreferencesState::new(true);
        assert!(p.restorable_buses().is_none());
        p.event(&SettingsEvent::Loaded {
            request: 0,
            snapshot: snapshot.clone(),
        });
        assert_eq!(p.restorable_buses().is_none(), protected);
        p.event(&SettingsEvent::Loaded {
            request: 10,
            snapshot,
        });
        assert!(
            p.restorable_buses().is_none(),
            "explicit reload has no automatic audio policy"
        );
        assert_eq!(std::fs::read(&path).ok(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[gtk::test]
fn restore_existing_shell_clock_waits_initial_load_and_quit_retains_admitted_receipt() {
    use crate::settings_worker::SettingsWorker;
    let dir = std::env::temp_dir().join(format!(
        "pecan-restore-shell-synthetic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("settings.json");
    let p = preferences();
    let bytes = serde_json::to_vec(p.document()).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let host = Rc::new(ApplicationOwner::fixture());
    let driver = host.fixture_driver().unwrap();
    let session = host.activate();
    let mut g = graph();
    g.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam =
        pecan_core::domain::audio::REC_NODE.into();
    let graph = driver.publish(g.clone(), generations(&g));
    session.poll();
    let fake = Fake::new(graph.stamp.epoch);
    fake.restore_enabled.set(true);
    let owner = BusOwner::new(session, Some(fake.clone()));
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanRestore.Case{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let worker = SettingsWorker::start(SettingsFile::new(path.clone()).unwrap()).unwrap();
    let shell =
        ApplicationShell::install(&app, host, ShellOptions::default(), Some(worker)).unwrap();
    shell.attach_test_buses(owner.clone());
    let end = Instant::now() + Duration::from_secs(5);
    while fake.restore_origins.borrow().is_empty() {
        assert!(Instant::now() < end);
        shell.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(shell.preferences().document(), p.document());
    assert!(!shell.preferences().dirty());
    shell.activate();
    shell.activate();
    shell.tick();
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    shell.request_quit();
    assert!(owner.session.admission_closed());
    shell.tick();
    assert!(
        shell.exit_code().is_none(),
        "common quit retains the original pending work"
    );
    assert!(matches!(
        owner.configuration_report().unwrap().progress,
        ConfigProgress::Queued
    ));
    terminal(&fake, LocalConfigOutcome::Applied, true);
    driver.complete_shutdown(pecan_pipewire::shutdown::ShutdownReport::default());
    while shell.exit_code().is_none() {
        assert!(Instant::now() < end);
        shell.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(shell.exit_code(), Some(0));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    assert!(driver.take_requests().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[gtk::test]
fn restore_same_identity_ordinary_observations_do_not_rebind_original_witness() {
    let (owner, fake, driver) = setup_restore();
    owner.restore_preferences(&preferences());
    let original = fake.restore_origins.borrow()[0].graph().clone();
    let g = (*original.value).clone();
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    owner.poll();
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    assert!(settled(&owner));
    assert!(matches!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::AppliedLocally
    ));
    assert_eq!(
        fake.restore_origins.borrow()[0].graph().stamp,
        original.stamp
    );
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}

fn routed_restore(
    unknown_metadata: bool,
    owned_route: bool,
    change_layout: bool,
) -> (Rc<BusOwner>, Rc<Fake>) {
    let (owner, fake, driver) = setup_restore();
    let mut graph = (*owner.session.graph().unwrap().value).clone();
    let mut foreign = graph.knopen.iter().find(|n| n.id == 30).unwrap().clone();
    foreign.id = 900;
    foreign.serial = Some(900);
    let mut input = graph.poorten.iter().find(|p| p.id == 301).unwrap().clone();
    input.id = 901;
    input.knoop = 900;
    graph.knopen.push(foreign);
    graph.poorten.push(input);
    graph.kabels.push(pecan_core::graph::Kabel {
        id: 902,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: if owned_route { 30 } else { 900 },
        in_poort: if owned_route { 301 } else { 901 },
        client: None,
    });
    driver.publish(graph.clone(), generations(&graph));
    owner.session.poll();
    if unknown_metadata {
        fake.config.borrow_mut().configuration = None;
    }
    owner.poll();
    let mut preferences = SettingsV1::default().into_preferences();
    if change_layout {
        preferences.buses.widths = vec![1, 1];
        preferences.buses = preferences.buses.with_db("rec-1", -12.).unwrap();
    } else {
        preferences.buses = preferences.buses.with_db("rec-1-2", -12.).unwrap();
    }
    let mut state = PreferencesState::new(false);
    state.edit(SettingsV1::new(preferences).unwrap()).unwrap();
    owner.restore_preferences(&state);
    (owner, fake)
}
#[gtk::test]
fn restore_foreign_rec_routes_do_not_block_known_owned_layout() {
    let (owner, fake) = routed_restore(false, false, true);
    assert_eq!(
        fake.restore_origins.borrow().len(),
        1,
        "foreign routing is not owned layout authority"
    );
    assert_eq!(
        fake.configs.borrow()[0].0.configuration.buses.widths,
        [1, 1]
    );
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    assert!(settled(&owner));
}
#[gtk::test]
fn restore_foreign_rec_routes_do_not_block_unknown_owned_layout() {
    let (owner, fake) = routed_restore(true, false, true);
    assert_eq!(
        fake.restore_origins.borrow().len(),
        1,
        "foreign routing must not consume the owned initialization attempt"
    );
    assert_eq!(
        fake.configs.borrow()[0].0.configuration.buses.widths,
        [1, 1]
    );
    terminal(&fake, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    assert!(settled(&owner));
}
#[gtk::test]
fn restore_owned_rec_routes_preserve_layout_gate_but_allow_known_same_layout_gains() {
    for unknown in [false, true] {
        let (owner, fake) = routed_restore(unknown, true, true);
        assert!(fake.restore_origins.borrow().is_empty());
        assert!(fake.configs.borrow().is_empty());
        assert!(!owner.session.busy());
        assert!(owner.restore_status.text().contains("migration"));
    }
    let (_, fake) = routed_restore(false, true, false);
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
