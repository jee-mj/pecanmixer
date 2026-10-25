//! Explicit temporary paths only. No native backend, process launch or real tray.
use super::*;
use crate::{
    application::{ApplicationOwner, ApplicationShell, ShellOptions},
    desktop_launch::{
        AutostartFile, FileError, LaunchCommand, Phase, RefreshMode, DESKTOP_FILENAME,
        LEGACY_DESKTOP_FILENAME,
    },
    settings_storage::SettingsFile,
    settings_worker::DesktopContext,
};
use pecan_core::settings::SettingsV1;
use std::{
    fs,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pecan-desktop-adoption-synthetic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self) -> PathBuf {
        self.0.join("autostart").join(DESKTOP_FILENAME)
    }
    fn context(&self) -> DesktopContext {
        DesktopContext::new(
            self.0.clone(),
            LaunchCommand::new("/synthetic/native executable")
                .unwrap()
                .with_paths(None, Some("/synthetic/recordings".into()))
                .unwrap(),
            RefreshMode::Release,
        )
        .unwrap()
    }
    fn worker(&self, prefs: Option<SettingsFile>) -> SettingsWorker {
        SettingsWorker::start_with_desktop(prefs, self.context()).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn event(worker: &SettingsWorker) -> SettingsEvent {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match worker.try_event() {
            Ok(e) => return e,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => panic!("worker ended: {e}"),
        }
    }
}
fn close(worker: SettingsWorker) {
    let (events, thread) = worker.close();
    for _ in events {}
    thread.join().unwrap();
}
fn inspected(worker: &mut SettingsWorker, state: &mut DesktopState) -> SettingsEvent {
    state.pump(worker);
    let e = event(worker);
    state.event(&e);
    e
}
#[test]
fn desktop_memory_capability_does_not_load_or_require_preferences_or_enable_on_launch() {
    let temp = Temp::new();
    let mut worker = temp.worker(None);
    assert!(!worker.supports_preferences());
    assert!(worker.supports_desktop());
    assert!(worker.try_event().is_err());
    assert!(!temp.file().exists());
    assert_eq!(
        worker
            .submit(SettingsOperation::Save(SettingsV1::default()))
            .unwrap_err()
            .reason,
        SubmissionFailure::NotReady
    );
    let mut state = DesktopState::new(true);
    let e = inspected(&mut worker, &mut state);
    let mut prefs = crate::application::preferences::PreferencesState::new(false);
    prefs.event(&e);
    assert!(prefs.editing_allowed());
    assert!(prefs.message().contains("memory"));
    assert!(
        !temp.file().exists(),
        "inspection must not enable autostart"
    );
    let source = state.source;
    state.request(source, Action::Enable, &mut worker).unwrap();
    state.event(&event(&worker));
    assert_eq!(state.menu(true).unwrap().checked, Some(true));
    let command = state.snapshot.as_ref().unwrap().launch_command().unwrap();
    assert!(command.settings().is_none());
    assert_eq!(
        command.recording_root().unwrap(),
        std::path::Path::new("/synthetic/recordings")
    );
    close(worker);
}
#[test]
fn desktop_capability_survives_preference_load_failure() {
    let temp = Temp::new();
    let bad = temp.0.join("not-a-settings-file");
    fs::create_dir(&bad).unwrap();
    let mut worker = temp.worker(Some(SettingsFile::new(bad.clone()).unwrap()));
    assert!(matches!(
        event(&worker),
        SettingsEvent::Failed { request: 0, .. }
    ));
    let mut state = DesktopState::new(true);
    inspected(&mut worker, &mut state);
    state
        .request(state.source, Action::Enable, &mut worker)
        .unwrap();
    state.event(&event(&worker));
    assert_eq!(state.menu(true).unwrap().checked, Some(true));
    assert!(bad.is_dir());
    close(worker);
}
#[test]
fn desktop_pending_stale_callbacks_and_late_results_cannot_change_new_scope() {
    let temp = Temp::new();
    let mut worker = temp.worker(None);
    let mut state = DesktopState::new(true);
    let old = inspected(&mut worker, &mut state);
    let source = state.source;
    state.request(source, Action::Enable, &mut worker).unwrap();
    assert!(!state.menu(true).unwrap().enabled);
    assert_eq!(state.menu(true).unwrap().checked, None);
    assert!(state.request(source, Action::Disable, &mut worker).is_err());
    state.event(&old);
    assert!(state.pending.is_some());
    state.event(&event(&worker));
    let bytes = fs::read(temp.file()).unwrap();
    assert!(state.request(source, Action::Disable, &mut worker).is_err());
    assert_eq!(fs::read(temp.file()).unwrap(), bytes);
    state
        .request(state.source, Action::Disable, &mut worker)
        .unwrap();
    state.event(&event(&worker));
    assert_eq!(state.menu(true).unwrap().checked, Some(false));
    assert!(!temp.file().exists());
    close(worker);
}
#[test]
fn desktop_external_edit_and_other_snapshot_are_preserved_until_explicit_reload() {
    let temp = Temp::new();
    let other = Temp::new();
    let mut worker = temp.worker(None);
    let mut state = DesktopState::new(true);
    inspected(&mut worker, &mut state);
    fs::create_dir(temp.0.join("autostart")).unwrap();
    let custom = b"[Desktop Entry]\nName=Synthetic customized entry\n";
    fs::write(temp.file(), custom).unwrap();
    fs::write(
        temp.0.join("autostart").join(LEGACY_DESKTOP_FILENAME),
        b"synthetic legacy sentinel",
    )
    .unwrap();
    state
        .request(state.source, Action::Enable, &mut worker)
        .unwrap();
    state.event(&event(&worker));
    assert!(!state.menu(true).unwrap().enabled);
    assert!(state.error.is_some());
    assert_eq!(fs::read(temp.file()).unwrap(), custom);
    state
        .request(state.source, Action::Inspect, &mut worker)
        .unwrap();
    state.event(&event(&worker));
    assert_eq!(
        state.snapshot.as_ref().unwrap().state(),
        EntryState::Conflict
    );
    let wrong = AutostartFile::new(other.0.clone()).unwrap().load().unwrap();
    worker
        .submit(SettingsOperation::Desktop(DesktopOperation::Enable(wrong)))
        .unwrap();
    assert!(
        matches!(event(&worker), SettingsEvent::Desktop { result: Err(e), .. } if matches!(*e, FileError::WrongSnapshot))
    );
    assert!(!other.file().exists());
    assert_eq!(fs::read(temp.file()).unwrap(), custom);
    assert_eq!(
        fs::read(temp.0.join("autostart").join(LEGACY_DESKTOP_FILENAME)).unwrap(),
        b"synthetic legacy sentinel"
    );
    close(worker);
}
#[test]
fn desktop_published_error_keeps_state_unknown_until_reload_and_late_success_is_ignored() {
    let temp = Temp::new();
    let mut worker = temp.worker(None);
    let mut state = DesktopState::new(true);
    inspected(&mut worker, &mut state);
    state
        .request(state.source, Action::Enable, &mut worker)
        .unwrap();
    let request = state.pending.unwrap();
    let real = event(&worker);
    assert!(temp.file().exists());
    // State-level fault injection; actual storage sync failures are covered by
    // the reviewed helper's tests. This never claims a real filesystem failure.
    let failure = SettingsEvent::Desktop {
        request,
        result: Err(Arc::new(FileError::Io {
            phase: Phase::SyncDirectory,
            path: temp.file(),
            published: true,
            source: std::io::Error::other("Synthetic post-publication error"),
        })),
    };
    state.event(&failure);
    state.event(&real);
    assert!(state.snapshot.is_none());
    assert!(!state.menu(true).unwrap().enabled);
    assert!(state.message().contains("published=true"));
    state
        .request(state.source, Action::Inspect, &mut worker)
        .unwrap();
    state.event(&event(&worker));
    assert_eq!(state.menu(true).unwrap().checked, Some(true));
    close(worker);
}
fn pump(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < end, "synthetic desktop deadline");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn shell(worker: SettingsWorker) -> (Rc<ApplicationShell>, crate::application::FixtureDriver) {
    let owner = Rc::new(ApplicationOwner::fixture());
    let driver = owner.fixture_driver().unwrap();
    driver.complete_shutdown(Default::default());
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanDesktop.Case{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    (
        ApplicationShell::install(&app, owner, ShellOptions::default(), Some(worker)).unwrap(),
        driver,
    )
}
fn button(shell: &ApplicationShell, name: &str) -> gtk::Button {
    let mut todo = vec![shell.window().clone().upcast::<gtk::Widget>()];
    while let Some(w) = todo.pop() {
        if let Some(b) = w.downcast_ref::<gtk::Button>() {
            if b.label().as_deref() == Some(name) {
                return b.clone();
            }
        }
        let mut child = w.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            todo.push(w);
        }
    }
    panic!("missing synthetic desktop button {name}");
}
#[gtk::test]
fn desktop_shell_memory_preferences_tray_handoff_and_retired_buttons_use_real_results() {
    let temp = Temp::new();
    let (shell, _) = shell(temp.worker(None));
    pump(|| {
        shell.tick();
        button(&shell, "Enable/update native login").is_sensitive()
    });
    assert!(shell.preferences().editing_allowed());
    assert!(shell.preferences().message().contains("memory"));
    let old = button(&shell, "Disable native login");
    let first = shell.settings_summary().desktop.unwrap().0;
    assert!(shell.mailbox().autostart(first, true));
    shell.tick();
    pump(|| {
        shell.tick();
        shell
            .settings_summary()
            .desktop
            .as_ref()
            .is_some_and(|(id, _)| *id > first)
    });
    let enabled = shell.settings_summary().desktop.unwrap();
    assert!(enabled.1.unwrap().snapshot.state() == EntryState::Managed(EntryFormat::Native));
    let bytes = fs::read(temp.file()).unwrap();
    old.set_sensitive(true);
    old.emit_clicked();
    assert!(shell.mailbox().autostart(first, false));
    shell.tick();
    assert_eq!(fs::read(temp.file()).unwrap(), bytes);
    let current = shell.settings_summary().desktop.unwrap().0;
    assert!(shell.mailbox().autostart(current, false));
    shell.tick();
    pump(|| {
        shell.tick();
        !temp.file().exists()
            && shell
                .settings_summary()
                .desktop
                .as_ref()
                .is_some_and(|(id, _)| *id > current)
    });
    shell.request_quit();
    pump(|| {
        shell.tick();
        shell.exit_code().is_some()
    });
    assert_eq!(shell.exit_code(), Some(0));
}
#[gtk::test]
fn desktop_quit_drains_admitted_enable_and_reserves_quit_before_late_toggle() {
    let temp = Temp::new();
    let settings = temp.0.join("preferences.json");
    let (shell, _) = shell(temp.worker(Some(SettingsFile::new(settings.clone()).unwrap())));
    pump(|| {
        shell.tick();
        button(&shell, "Enable/update native login").is_sensitive()
            && shell.preferences().editing_allowed()
    });
    button(&shell, "Enable/update native login").emit_clicked();
    let mut desired = SettingsV1::default().into_preferences();
    desired.destination_width_px = Some(222.);
    shell
        .submit_settings(SettingsOperation::Save(SettingsV1::new(desired).unwrap()))
        .unwrap();
    let previous = shell.settings_summary().desktop.unwrap().0;
    shell.mailbox().quit().unwrap();
    shell.mailbox().autostart(previous, false);
    shell.tick();
    pump(|| {
        shell.tick();
        shell.exit_code().is_some()
    });
    assert_eq!(shell.exit_code(), Some(0));
    assert!(
        temp.file().exists(),
        "already admitted enable drains; same-batch quit rejects another toggle"
    );
    let loaded = SettingsFile::new(settings).unwrap().load().unwrap();
    assert_eq!(
        loaded.settings().preferences().destination_width_px,
        Some(222.)
    );
    assert!(
        shell
            .settings_summary()
            .desktop
            .unwrap()
            .1
            .unwrap()
            .snapshot
            .state()
            == EntryState::Managed(EntryFormat::Native)
    );
}

#[test]
fn desktop_queue_full_retains_unadmitted_operation_and_never_auto_enables() {
    let temp = Temp::new();
    let snapshot = AutostartFile::new(temp.0.clone()).unwrap().load().unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let mut worker = SettingsWorker::fixture_with_desktop(
        SettingsFile::new(temp.0.join("settings.json")).unwrap(),
        Some((entered_tx, release_rx)),
        None,
        false,
        Some(temp.context()),
    )
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    for _ in 0..crate::settings_worker::SETTINGS_QUEUE_CAPACITY {
        worker
            .submit(SettingsOperation::Desktop(DesktopOperation::Inspect))
            .unwrap();
    }
    let error = worker
        .submit(SettingsOperation::Desktop(DesktopOperation::Enable(
            snapshot,
        )))
        .unwrap_err();
    assert_eq!(error.reason, SubmissionFailure::QueueFull);
    assert!(matches!(
        error.operation,
        SettingsOperation::Desktop(DesktopOperation::Enable(_))
    ));
    assert!(!temp.file().exists());
    release_tx.send(()).unwrap();
    close(worker);
    assert!(
        !temp.file().exists(),
        "an unadmitted mutation must not become a deferred effect"
    );
}
#[gtk::test]
fn desktop_shell_error_result_stays_truthful_through_common_quit() {
    let temp = Temp::new();
    let bad = temp.0.join("preferences-directory");
    fs::create_dir(&bad).unwrap();
    let (shell, _) = shell(temp.worker(Some(SettingsFile::new(bad).unwrap())));
    pump(|| {
        shell.tick();
        button(&shell, "Enable/update native login").is_sensitive()
    });
    assert!(!shell.preferences().editing_allowed());
    button(&shell, "Enable/update native login").emit_clicked();
    pump(|| {
        shell.tick();
        shell
            .settings_summary()
            .desktop
            .as_ref()
            .is_some_and(|(_, r)| {
                r.as_ref()
                    .is_ok_and(|v| v.snapshot.state() == EntryState::Managed(EntryFormat::Native))
            })
    });
    shell.request_quit();
    pump(|| {
        shell.tick();
        matches!(shell.phase(), crate::lifetime::QuitPhase::Failed { .. })
    });
    assert!(temp.file().exists());
    assert!(shell.settings_summary().desktop.unwrap().1.is_ok());
    let crate::lifetime::QuitPhase::Failed {
        acknowledgement, ..
    } = shell.phase()
    else {
        panic!()
    };
    shell
        .mailbox()
        .acknowledge_failure(acknowledgement)
        .unwrap();
    pump(|| {
        shell.tick();
        shell.exit_code().is_some()
    });
    assert_eq!(shell.exit_code(), Some(1));
}

#[gtk::test]
fn desktop_current_disable_survives_retired_enable_in_both_callback_orders() {
    for current_first in [true, false] {
        let temp = Temp::new();
        let (shell, _) = shell(temp.worker(None));
        pump(|| {
            shell.tick();
            button(&shell, "Enable/update native login").is_sensitive()
        });
        let retired = shell.settings_summary().desktop.unwrap().0;
        button(&shell, "Enable/update native login").emit_clicked();
        pump(|| {
            shell.tick();
            shell
                .settings_summary()
                .desktop
                .as_ref()
                .is_some_and(|(source, result)| {
                    *source > retired
                        && result.as_ref().is_ok_and(|r| {
                            r.snapshot.state() == EntryState::Managed(EntryFormat::Native)
                        })
                })
        });
        let current = shell.settings_summary().desktop.unwrap().0;
        if current_first {
            shell.mailbox().autostart(current, false);
            shell.mailbox().autostart(retired, true);
        } else {
            shell.mailbox().autostart(retired, true);
            shell.mailbox().autostart(current, false);
        }
        shell.tick();
        pump(|| {
            shell.tick();
            shell
                .settings_summary()
                .desktop
                .as_ref()
                .is_some_and(|(source, result)| {
                    *source > current
                        && result
                            .as_ref()
                            .is_ok_and(|r| r.snapshot.state() == EntryState::Missing)
                })
        });
        assert!(!temp.file().exists());
        shell.request_quit();
        pump(|| {
            shell.tick();
            shell.exit_code().is_some()
        });
        assert_eq!(shell.exit_code(), Some(0));
    }
}
