//! Registered synthetic applications on Xvfb/private test bus. No native audio
//! constructor or real tray is invoked, including when fixture tray=true.
use super::super::FixtureDriver;
use super::*;
use crate::settings_storage::SettingsFile;
use pecan_core::{
    domain::history::Action,
    native_command::{Counts, NativeProgress},
    settings::{Preferences, SettingsV1},
};
use std::{
    collections::BTreeMap,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};
static NEXT: AtomicUsize = AtomicUsize::new(1);
fn app() -> gtk::Application {
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    let app = gtk::Application::builder()
        .application_id(format!("org.example.PecanFixture.Case{id}"))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    app
}
fn setup(hidden: bool, settings: Option<SettingsWorker>) -> (Rc<ApplicationShell>, FixtureDriver) {
    let app = app();
    let owner = Rc::new(ApplicationOwner::fixture());
    let driver = owner.fixture_driver().unwrap();
    let shell =
        ApplicationShell::install(&app, owner, ShellOptions { hidden, tray: true }, settings)
            .unwrap();
    assert!(shell.owner.native_handle().is_none());
    assert!(shell.tray.borrow().is_none());
    (shell, driver)
}
fn finish(shell: &ApplicationShell, driver: &FixtureDriver) {
    driver.complete_shutdown(ShutdownReport::default());
    shell.request_quit();
    shell.tick();
    assert_eq!(shell.exit_code(), Some(0));
    assert!(shell.hold.borrow().is_none());
    assert!(shell.clock.borrow().is_none());
    assert!(shell.keep_alive.borrow().is_none());
}
fn pump_until(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < end, "synthetic completion deadline");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn file() -> (std::path::PathBuf, SettingsWorker) {
    let dir = std::env::temp_dir().join(format!(
        "pecan-shell-synthetic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir(&dir).unwrap();
    let worker =
        SettingsWorker::start(SettingsFile::new(dir.join("settings.json")).unwrap()).unwrap();
    (dir, worker)
}
#[gtk::test]
fn repeated_activation_and_install_reuse_one_window_hold_clock_and_owner() {
    let (shell, driver) = setup(false, None);
    let first = shell.window.clone();
    let session = shell.session();
    for _ in 0..100 {
        shell.app.activate();
        let again = ApplicationShell::install(
            &shell.app,
            shell.owner.clone(),
            ShellOptions {
                hidden: true,
                tray: true,
            },
            None,
        )
        .unwrap();
        assert!(Rc::ptr_eq(&again, &shell));
        assert_eq!(again.window, first);
        assert!(session.same_session(&again.session()));
    }
    assert_eq!(shell.app.windows().len(), 1);
    assert!(shell.hold.borrow().is_some());
    assert!(shell.clock.borrow().is_some());
    assert_eq!(driver.shutdown_requests(), 0);
    finish(&shell, &driver);
    assert!(shell.app.windows().is_empty());
}
#[gtk::test]
fn hidden_no_host_then_host_loss_and_expiry_always_restore_a_window() {
    let (shell, driver) = setup(true, None);
    assert!(!shell.window.is_visible());
    shell.offset.set(Duration::from_secs(2));
    shell.tick();
    assert!(shell.window.is_visible());
    shell.event(Event::Tray(TrayAvailability::Usable));
    assert!(
        shell.window.is_visible(),
        "host recovery does not rehide fallback"
    );
    shell.window.close();
    assert!(!shell.window.is_visible());
    assert_eq!(driver.shutdown_requests(), 0);
    shell.event(Event::Tray(TrayAvailability::Unavailable));
    assert!(shell.window.is_visible());
    shell.event(Event::Tray(TrayAvailability::Usable));
    shell.window.close();
    assert!(!shell.window.is_visible());
    shell.offset.set(Duration::from_secs(8));
    shell.tick();
    assert!(shell.window.is_visible());
    finish(&shell, &driver);
}
#[gtk::test]
fn close_without_tray_requests_shutdown_and_keeps_pending_window_reachable() {
    let (shell, driver) = setup(false, None);
    shell.window.close();
    assert_eq!(shell.phase(), QuitPhase::Waiting);
    assert!(shell.window.is_visible());
    assert_eq!(driver.shutdown_requests(), 1);
    assert!(shell.session.admission_closed());
    for _ in 0..100 {
        shell.window.close();
        shell.request_quit();
    }
    assert_eq!(driver.shutdown_requests(), 1);
    assert!(shell.hold.borrow().is_some());
    driver.complete_shutdown(ShutdownReport::default());
    shell.tick();
    assert_eq!(shell.exit_code(), Some(0));
}
#[gtk::test]
fn timeout_retry_retains_real_request_and_never_acknowledges_pending_work_as_error() {
    let (shell, driver) = setup(false, None);
    shell.request_quit();
    shell.offset.set(Duration::from_secs(6));
    shell.tick();
    assert!(matches!(
        shell.phase(),
        QuitPhase::Pending { backend: true, .. }
    ));
    assert!(shell.retry.is_visible());
    assert_eq!(shell.exit_code(), None);
    assert!(shell.error_box.first_child().is_none());
    shell.retry.emit_clicked();
    assert_eq!(driver.shutdown_requests(), 1);
    assert_eq!(shell.phase(), QuitPhase::Waiting);
    driver.complete_shutdown(ShutdownReport::default());
    shell.tick();
    assert_eq!(shell.exit_code(), Some(0));
}
#[gtk::test]
fn backend_factory_failure_can_retry_without_reopening_mutation_admission() {
    let (shell, driver) = setup(false, None);
    driver.fail_next_shutdown("Synthetic completion-handle failure");
    shell.request_quit();
    assert!(matches!(shell.phase(), QuitPhase::Blocked(_)));
    assert!(shell.window.is_visible());
    assert!(shell.session.admission_closed());
    driver.complete_shutdown(ShutdownReport::default());
    shell.retry.emit_clicked();
    assert_eq!(driver.shutdown_requests(), 2);
    assert_eq!(shell.exit_code(), Some(0));
}
#[gtk::test]
fn completed_failure_requires_separate_explicit_error_exit_button() {
    let (shell, driver) = setup(false, None);
    driver.complete_shutdown(ShutdownReport {
        writers_failed: 1,
        writer_errors: vec!["Synthetic joined writer error".into()],
        ..Default::default()
    });
    shell.request_quit();
    assert!(matches!(shell.phase(), QuitPhase::Failed { .. }));
    assert_eq!(shell.exit_code(), None);
    shell.request_quit();
    assert_eq!(shell.exit_code(), None);
    let button = shell
        .error_box
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    button.emit_clicked();
    assert_eq!(shell.exit_code(), Some(1));
    assert!(shell.hold.borrow().is_none());
    assert!(shell.clock.borrow().is_none());
}
#[gtk::test]
fn hidden_clock_polls_receipts_and_admission_closes_all_retained_clients() {
    let (shell, driver) = setup(false, None);
    let graph: pecan_core::graph::Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap();
    let gens: BTreeMap<_, _> = graph
        .knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(graph.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect();
    let origin = driver.publish(graph, gens.clone());
    shell.tick();
    shell.event(Event::Tray(TrayAvailability::Usable));
    shell.window.close();
    assert!(!shell.window.is_visible());
    let retained = shell.session();
    retained
        .execute(
            &origin,
            "Mute",
            Action::Mute {
                node: 20,
                muted: true,
            },
        )
        .unwrap();
    let request = driver.take_requests().pop().unwrap();
    let mut next = (*origin.value).clone();
    next.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(true);
    let next = driver.publish(next, gens);
    request.reporter.set(NativeProgress::Observed {
        stamp: next.stamp,
        counts: Counts::default(),
    });
    shell.tick();
    assert_eq!(retained.history_lengths(), (1, 0));
    assert!(!shell.window.is_visible());
    shell.request_quit();
    assert!(retained
        .execute(
            &next,
            "Mute",
            Action::Mute {
                node: 20,
                muted: false
            }
        )
        .is_err());
    assert!(retained.undo(&next).is_err());
    assert!(retained.redo(&next).is_err());
    assert!(driver.take_requests().is_empty());
    driver.complete_shutdown(ShutdownReport::default());
    shell.tick();
    assert_eq!(shell.exit_code(), Some(0));
}
#[gtk::test]
fn actual_hidden_main_context_clock_reopens_and_completes_without_manual_tick() {
    let (shell, driver) = setup(true, None);
    shell.offset.set(Duration::from_secs(2));
    pump_until(|| shell.window.is_visible());
    shell.request_quit();
    driver.complete_shutdown(ShutdownReport::default());
    pump_until(|| shell.exit_code() == Some(0));
    assert!(shell.clock.borrow().is_none());
}
#[gtk::test]
fn reserved_quit_with_coalesced_visibility_cannot_present_after_destroy() {
    let (shell, driver) = setup(false, None);
    driver.complete_shutdown(ShutdownReport::default());
    let m = shell.mailbox();
    m.quit().unwrap();
    m.present().unwrap();
    shell.tick();
    assert_eq!(shell.exit_code(), Some(0));
    assert!(shell.app.windows().is_empty());
    assert_eq!(driver.shutdown_requests(), 1);
}
#[gtk::test]
fn settings_drain_handles_all_accepted_results_and_joins_off_ui_before_exit() {
    let (dir, worker) = file();
    let (shell, driver) = setup(false, Some(worker));
    pump_until(|| {
        shell.tick();
        shell.settings_summary().results >= 1
    });
    for width in 240..252 {
        let settings = SettingsV1::new(Preferences {
            destination_width_px: Some(width as f64),
            ..Default::default()
        })
        .unwrap();
        shell
            .submit_settings(SettingsOperation::Save(settings))
            .unwrap();
    }
    driver.complete_shutdown(ShutdownReport::default());
    shell.request_quit();
    assert!(shell.hold.borrow().is_some());
    pump_until(|| {
        shell.tick();
        shell.exit_code().is_some()
    });
    assert_eq!(shell.exit_code(), Some(0));
    assert_eq!(shell.settings_summary().results, 13);
    assert_eq!(shell.settings_summary().last_request, 12);
    assert!(!shell.settings_summary().worker_panicked);
    let saved: SettingsV1 =
        serde_json::from_slice(&fs::read(dir.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved.preferences().destination_width_px, Some(251.0));
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn gated_off_ui_drain_cannot_be_restarted_or_mistaken_for_completion() {
    let (dir, mut worker) = file();
    for _ in 0..16 {
        worker
            .submit(SettingsOperation::Save(SettingsV1::default()))
            .unwrap();
    }
    let (shell, driver) = setup(false, Some(worker));
    driver.complete_shutdown(ShutdownReport::default());
    shell.request_quit();
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, rx) = mpsc::sync_channel(1);
    shell.drain.borrow_mut().as_mut().unwrap().1.gate = Some((entered, rx));
    shell.tick();
    let mut thread_id = None;
    pump_until(|| {
        thread_id = entry.try_recv().ok();
        thread_id.is_some()
    });
    assert_ne!(thread_id.unwrap(), std::thread::current().id());
    shell.offset.set(Duration::from_secs(6));
    shell.tick();
    assert!(matches!(
        shell.phase(),
        QuitPhase::Pending { settings: true, .. }
    ));
    assert_eq!(shell.exit_code(), None);
    assert!(shell.window.is_visible());
    for _ in 0..50 {
        shell.request_quit();
        shell.tick();
    }
    assert_eq!(driver.shutdown_requests(), 1);
    release.send(()).unwrap();
    pump_until(|| {
        shell.tick();
        shell.drain.borrow().is_none()
    });
    if shell.exit_code().is_none() {
        shell.retry.emit_clicked();
    }
    assert_eq!(shell.exit_code(), Some(0));
    assert_eq!(shell.settings_summary().results, 17);
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn failed_drain_spawn_retains_worker_and_retry_is_not_false_completion() {
    let (dir, mut worker) = file();
    for _ in 0..12 {
        worker
            .submit(SettingsOperation::Save(SettingsV1::default()))
            .unwrap();
    }
    let (shell, driver) = setup(false, Some(worker));
    driver.complete_shutdown(ShutdownReport::default());
    shell.request_quit();
    shell.drain.borrow_mut().as_mut().unwrap().1.fail_next_spawn = true;
    shell.tick();
    assert!(shell
        .drain
        .borrow()
        .as_ref()
        .unwrap()
        .1
        .warning
        .as_ref()
        .unwrap()
        .contains("still owned/pending"));
    assert_eq!(shell.exit_code(), None);
    shell.offset.set(Duration::from_secs(2));
    pump_until(|| {
        shell.tick();
        shell.exit_code().is_some()
    });
    assert_eq!(shell.exit_code(), Some(0));
    assert_eq!(shell.settings_summary().results, 13);
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn settings_error_is_reported_after_join_and_requires_explicit_error_exit() {
    let (dir, worker) = file();
    let (shell, driver) = setup(false, Some(worker));
    pump_until(|| {
        shell.tick();
        shell.settings_summary().results >= 1
    });
    fs::write(dir.join("settings.json"), b"synthetic external edit").unwrap();
    shell
        .submit_settings(SettingsOperation::Save(SettingsV1::default()))
        .unwrap();
    driver.complete_shutdown(ShutdownReport::default());
    shell.request_quit();
    pump_until(|| {
        shell.tick();
        matches!(shell.phase(), QuitPhase::Failed { .. })
    });
    assert_eq!(shell.exit_code(), None);
    assert_eq!(shell.settings_summary().failures, 1);
    assert_eq!(
        fs::read(dir.join("settings.json")).unwrap(),
        b"synthetic external edit"
    );
    shell
        .error_box
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(shell.exit_code(), Some(1));
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn rejected_installation_returns_accepted_settings_work_for_safe_owner_cleanup() {
    let (dir, worker) = file();
    let unregistered = gtk::Application::builder()
        .application_id("org.example.PecanFixture.Unregistered")
        .build();
    let owner = Rc::new(ApplicationOwner::fixture());
    let error = ApplicationShell::install(
        &unregistered,
        owner.clone(),
        ShellOptions::default(),
        Some(worker),
    )
    .err()
    .unwrap();
    assert!(Rc::ptr_eq(&owner, &error.owner));
    assert!(owner.active.borrow().is_none());
    let worker = error.settings.unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let (events, worker) = worker.close();
        for _ in events {}
        tx.send(worker.join().is_ok()).unwrap();
    });
    let mut joined = None;
    pump_until(|| {
        joined = rx.try_recv().ok();
        joined.is_some()
    });
    assert_eq!(joined, Some(true));
    fs::remove_dir_all(dir).unwrap();
}

#[gtk::test]
fn a_second_owner_cannot_replace_the_primary_window_or_start_its_own_session() {
    let (shell, driver) = setup(false, None);
    let second = Rc::new(ApplicationOwner::fixture());
    let failure =
        ApplicationShell::install(&shell.app, second.clone(), ShellOptions::default(), None)
            .err()
            .unwrap();
    assert_eq!(failure.reason.kind(), io::ErrorKind::AlreadyExists);
    assert!(second.active.borrow().is_none());
    assert_eq!(shell.app.windows().len(), 1);
    finish(&shell, &driver);
}

#[gtk::test]
fn an_exited_owner_cannot_be_reinstalled_as_a_new_primary() {
    let (shell, driver) = setup(false, None);
    let app = shell.app.clone();
    let owner = shell.owner.clone();
    finish(&shell, &driver);
    drop(shell);
    let error = ApplicationShell::install(&app, owner, ShellOptions::default(), None)
        .err()
        .unwrap();
    assert!(error
        .reason
        .to_string()
        .contains("finalization has started"));
    assert!(app.windows().is_empty());
    assert_eq!(driver.shutdown_requests(), 1);
}
