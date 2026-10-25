//! Real shell/preferences worker with synthetic audio-side endpoints only.
use super::*;
use crate::recording_buses::levels::{Change, Event, Reply};
use crate::{settings_storage::SettingsFile, settings_worker::SettingsWorker};
use pecan_core::settings::SettingsV1;
use std::{fs, sync::atomic::AtomicU64};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Case {
    shell: Rc<ApplicationShell>,
    owner: Rc<BusOwner>,
    fake: Rc<Fake>,
    driver: FixtureDriver,
}
fn setup_case(worker: Option<SettingsWorker>) -> Case {
    let host = Rc::new(ApplicationOwner::fixture());
    let driver = host.fixture_driver().unwrap();
    let session = host.activate();
    let mut g = graph();
    g.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam =
        pecan_core::domain::audio::REC_NODE.into();
    let origin = driver.publish(g.clone(), generations(&g));
    session.poll();
    let fake = Fake::new(origin.stamp.epoch);
    fake.levels_enabled.set(true);
    let owner = BusOwner::new(session, Some(fake.clone()));
    owner
        .set_recording_root(Some(
            std::env::temp_dir().join("pecan-rec-preference-synthetic-root"),
        ))
        .unwrap();
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanRecPreferences.Case{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let shell = ApplicationShell::install(&app, host, ShellOptions::default(), worker).unwrap();
    shell.attach_test_buses(owner.clone());
    Case {
        shell,
        owner,
        fake,
        driver,
    }
}
fn pump_until(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < end, "synthetic Rec preferences deadline");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn preference_button(c: &Case, label: &str) -> gtk::Button {
    let mut widgets = vec![c.shell.window().clone().upcast::<gtk::Widget>()];
    while let Some(w) = widgets.pop() {
        if let Some(expander) = w.downcast_ref::<gtk::Expander>() {
            if expander.label().as_deref() == Some("Display preferences") {
                return button(&expander.child().unwrap(), label);
            }
        }
        let mut child = w.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            widgets.push(w);
        }
    }
    panic!("synthetic preferences panel");
}
fn finish(c: &Case) {
    c.driver
        .complete_shutdown(pecan_pipewire::shutdown::ShutdownReport::default());
    c.shell.request_quit();
    pump_until(|| {
        c.shell.tick();
        c.shell.exit_code().is_some()
    });
    assert_eq!(c.shell.exit_code(), Some(0));
}
fn set(c: &Case, change: Change) -> Result<Request, String> {
    let capture = levels_owner::capture(&c.owner, "rec-1-2");
    match c
        .owner
        .level_event(Event::Set(capture.propose(change).unwrap()))?
    {
        Reply::Tracked(r) => Ok(r),
        _ => panic!(),
    }
}
fn rich() -> SettingsV1 {
    let mut p = SettingsV1::default().into_preferences();
    p.buses = p.buses.with_db("rec-1-2", -18.).unwrap();
    p.buses.widths = vec![2, 2];
    p.buses.states.insert(
        "rec-3-4".into(),
        pecan_core::domain::buses::BusState {
            db: -11.,
            mute: true,
            armed: false,
        },
    );
    p.clock.start_stop = true;
    p.destination_width_px = Some(221.);
    SettingsV1::new(p).unwrap()
}
fn file(doc: &SettingsV1) -> (std::path::PathBuf, SettingsFile) {
    let dir = std::env::temp_dir().join(format!(
        "pecan-rec-preferences-synthetic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let f = SettingsFile::new(dir.join("settings.json")).unwrap();
    fs::write(f.path(), serde_json::to_vec(doc).unwrap()).unwrap();
    (dir, f)
}
#[gtk::test]
fn rec_writeback_passive_readback_and_default_initialization_do_not_save_or_restore() {
    let doc = rich();
    let (dir, file) = file(&doc);
    let c = setup_case(Some(SettingsWorker::start(file.clone()).unwrap()));
    pump_until(|| {
        c.shell.tick();
        c.shell.preferences().editing_allowed()
    });
    for _ in 0..40 {
        c.shell.tick();
    }
    assert_eq!(c.shell.preferences().document(), &doc);
    assert_eq!(c.shell.preferences().revision(), 0);
    assert!(!c.shell.preferences().dirty());
    assert_eq!(
        c.fake
            .config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .widths,
        vec![2]
    );
    assert!(
        !c.fake
            .config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .midi
    );
    assert!(
        c.fake.configs.borrow().is_empty()
            && c.fake.gains.borrow().is_empty()
            && c.fake.starts.borrow().is_empty()
    );
    c.fake.config.borrow_mut().configuration = None;
    c.owner.poll();
    c.owner.initialize(&c.owner.origin().unwrap()).unwrap();
    c.fake.complete_config(0, LocalConfigOutcome::Applied, true);
    c.owner.poll();
    for _ in 0..3 {
        c.shell.tick();
    }
    assert_eq!(c.shell.preferences().document(), &doc);
    assert_eq!(file.load().unwrap().settings(), &doc);
    finish(&c);
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn rec_writeback_rejected_audio_then_real_mute_preserves_unsatisfied_saved_db() {
    let c = setup_case(None);
    set(&c, Change::Db(-18.)).unwrap();
    assert_eq!(
        c.shell
            .preferences()
            .document()
            .preferences()
            .buses
            .state("rec-1-2")
            .db,
        -18.
    );
    assert_eq!(
        c.fake
            .config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .state("rec-1-2")
            .db,
        0.
    );
    levels_owner::complete(
        &c.fake,
        0,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
        true,
    );
    c.owner.poll();
    c.owner.poll();
    button(c.owner.widget(), "Mute bus").emit_clicked();
    let saved = c.shell.preferences();
    assert_eq!(
        saved.document().preferences().buses.state("rec-1-2").db,
        -18.
    );
    assert!(saved.document().preferences().buses.state("rec-1-2").mute);
    assert!(saved.document().preferences().buses.midi);
    assert!(!c.fake.gains.borrow()[1].1.configuration.buses.midi);
    levels_owner::complete(&c.fake, 1, LocalConfigOutcome::Applied, true);
    c.owner.poll();
    c.owner.poll();
    c.driver
        .complete_shutdown(pecan_pipewire::shutdown::ShutdownReport::default());
    c.shell.request_quit();
    pump_until(|| {
        c.shell.tick();
        matches!(c.shell.phase(), crate::lifetime::QuitPhase::Failed { .. })
    });
    assert_eq!(
        c.shell.exit_code(),
        None,
        "retained audio rejection is not a successful quit"
    );
    button(c.shell.window(), "Exit after finalization error").emit_clicked();
    assert_eq!(c.shell.exit_code(), Some(1));
}
#[gtk::test]
fn rec_writeback_initial_load_blocks_preference_bearing_input_without_audio_effect() {
    let (dir, file) = file(&rich());
    let (entered, seen) = std::sync::mpsc::sync_channel(1);
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let worker = SettingsWorker::fixture(file.clone(), Some((entered, wait)), None, false).unwrap();
    seen.recv_timeout(Duration::from_secs(2)).unwrap();
    let c = setup_case(Some(worker));
    assert!(set(&c, Change::Db(-6.)).is_err());
    assert!(c.fake.gains.borrow().is_empty());
    assert!(!c.owner.session.busy());
    release.send(()).unwrap();
    pump_until(|| {
        c.shell.tick();
        c.shell.preferences().editing_allowed()
    });
    set(&c, Change::Db(-6.)).unwrap();
    assert_eq!(
        c.shell
            .preferences()
            .document()
            .preferences()
            .buses
            .state("rec-1-2")
            .db,
        -6.
    );
    assert_eq!(
        c.shell.preferences().document().preferences().buses.widths,
        vec![2, 2],
        "a field edit cannot save the native startup layout over saved intent"
    );
    levels_owner::complete(&c.fake, 0, LocalConfigOutcome::Applied, true);
    c.owner.poll();
    c.owner.poll();
    finish(&c);
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn rec_writeback_rapid_final_intent_is_drained_at_quit_without_claiming_audio_flush() {
    let (dir, file) = file(&rich());
    let c = setup_case(Some(SettingsWorker::start(file.clone()).unwrap()));
    pump_until(|| {
        c.shell.tick();
        c.shell.preferences().editing_allowed()
    });
    let capture = levels_owner::capture(&c.owner, "rec-1-2");
    let Reply::Tracked(request) = c
        .owner
        .level_event(Event::Begin {
            capture,
            kind: crate::controls::fader::Kind::Pointer,
        })
        .unwrap()
    else {
        panic!()
    };
    for n in 0..1000 {
        c.owner
            .level_event(Event::Update {
                request: request.clone(),
                db: -1. - (n % 30) as f64,
            })
            .unwrap();
    }
    c.owner
        .level_event(Event::End {
            request,
            db: -23.,
            reason: crate::controls::fader::End::Released,
        })
        .unwrap();
    assert_eq!(c.fake.gains.borrow().len(), 1);
    levels_owner::complete(&c.fake, 0, LocalConfigOutcome::Applied, true);
    let last = c.shell.preferences().document().clone();
    assert_eq!(last.preferences().buses.state("rec-1-2").db, -23.);
    finish(&c);
    assert_eq!(file.load().unwrap().settings(), &last);
    assert_eq!(
        c.fake.gains.borrow().len(),
        1,
        "quit disables new audio samples; preference save is not audio completion"
    );
    assert_eq!(
        c.fake
            .config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .state("rec-1-2")
            .db,
        -1.
    );
    assert!(last.preferences().buses.midi && last.preferences().clock.start_stop);
    fs::remove_dir_all(dir).unwrap();
}
#[gtk::test]
fn rec_writeback_layout_and_arming_emit_user_edits_before_backend_reports() {
    let (owner, fake, _) = levels_owner::setup_levels();
    let doc = Rc::new(RefCell::new(rich()));
    let target = doc.clone();
    owner.bind_preference_edits(Rc::new(move |edit| {
        let next = edit.apply(&target.borrow())?;
        *target.borrow_mut() = next;
        Ok(())
    }));
    let origin = owner.origin().unwrap();
    let before = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    owner
        .configure(
            &origin,
            ui::Action::Split {
                bus: "rec-1-2".into(),
            },
            Some(before.change_coupling(0).unwrap()),
        )
        .unwrap();
    assert_eq!(doc.borrow().preferences().buses.widths, vec![1, 1]);
    assert_eq!(doc.borrow().preferences().buses.state("rec-1").db, -18.);
    assert_eq!(
        fake.config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .widths,
        vec![2]
    );
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    let origin = owner.origin().unwrap();
    let before = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    owner
        .configure(
            &origin,
            ui::Action::Arm {
                bus: "rec-1".into(),
                additive: true,
            },
            Some(before.arm("rec-1", true).unwrap()),
        )
        .unwrap();
    assert!(!doc.borrow().preferences().buses.state("rec-1").armed);
    assert_eq!(doc.borrow().preferences().buses.state("rec-1").db, -18.);
    assert!(doc.borrow().preferences().buses.midi && doc.borrow().preferences().clock.start_stop);
    assert!(fake.gains.borrow().is_empty());
}
#[gtk::test]
fn rec_writeback_reentrant_sink_reads_owner_and_finalization_cannot_admit_audio() {
    let (owner, fake, _) = levels_owner::setup_levels();
    let weak = Rc::downgrade(&owner);
    let edits = Rc::new(Cell::new(0));
    let calls = edits.clone();
    owner.bind_preference_edits(Rc::new(move |_| {
        let owner = weak.upgrade().unwrap();
        owner.recording_status();
        calls.set(calls.get() + 1);
        owner.session.close_admission();
        Ok(())
    }));
    let capture = levels_owner::capture(&owner, "rec-1-2");
    assert!(owner
        .level_event(Event::Set(capture.propose(Change::Db(-6.)).unwrap()))
        .is_err());
    assert_eq!(edits.get(), 1);
    assert!(fake.gains.borrow().is_empty());
    assert!(!owner.session.busy());
}

#[gtk::test]
fn rec_writeback_dynamic_arming_and_routed_remove_use_intent_not_result_readback() {
    let (owner, fake, _) = levels_owner::setup_levels();
    let document = Rc::new(RefCell::new(rich()));
    let saved = document.clone();
    owner.bind_preference_edits(Rc::new(move |edit| {
        let next = edit.apply(&saved.borrow())?;
        *saved.borrow_mut() = next;
        Ok(())
    }));
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 41);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    let origin = owner.origin().unwrap();
    let before = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    owner
        .configure(
            &origin,
            ui::Action::Arm {
                bus: "rec-1-2".into(),
                additive: true,
            },
            Some(before.arm("rec-1-2", true).unwrap()),
        )
        .unwrap();
    assert_eq!(fake.arms.borrow().len(), 1);
    assert!(!document.borrow().preferences().buses.state("rec-1-2").armed);
    assert_eq!(
        document.borrow().preferences().buses.state("rec-1-2").db,
        -18.
    );
    assert!(document.borrow().preferences().buses.midi);

    let (owner, fake, _, _) = migration::routed();
    let document = Rc::new(RefCell::new(rich()));
    let saved = document.clone();
    owner.bind_preference_edits(Rc::new(move |edit| {
        let next = edit.apply(&saved.borrow())?;
        *saved.borrow_mut() = next;
        Ok(())
    }));
    owner
        .configure(
            &owner.origin().unwrap(),
            ui::Action::Remove {
                bus: "rec-1-2".into(),
            },
            None,
        )
        .unwrap();
    assert_eq!(fake.migrations.borrow().len(), 1);
    assert_eq!(document.borrow().preferences().buses.widths, vec![2]);
    assert_eq!(
        document.borrow().preferences().buses.state("rec-1-2").db,
        -11.
    );
    assert!(document.borrow().preferences().buses.state("rec-1-2").mute);
    assert!(
        document.borrow().preferences().buses.midi
            && document.borrow().preferences().clock.start_stop
    );
}

#[gtk::test]
fn rec_writeback_stale_or_invalid_action_is_not_a_preference_event() {
    let (owner, fake, _) = levels_owner::setup_levels();
    let calls = Rc::new(Cell::new(0));
    let log = calls.clone();
    owner.bind_preference_edits(Rc::new(move |_| {
        log.set(log.get() + 1);
        Ok(())
    }));
    let old = owner.origin().unwrap();
    fake.config.borrow_mut().revision.revision += 1;
    owner.poll();
    assert!(owner.configure(&old, ui::Action::AddStereo, None).is_err());
    assert!(owner
        .configure(&owner.origin().unwrap(), ui::Action::AddStereo, None)
        .is_err());
    assert_eq!(calls.get(), 0);
    assert!(fake.configs.borrow().is_empty());
}

#[gtk::test]
fn rec_writeback_post_rename_failure_retains_newer_intent_until_explicit_reload_and_save() {
    let (dir, file) = file(&rich());
    let worker = SettingsWorker::fixture(file.clone(), None, None, true).unwrap();
    let c = setup_case(Some(worker));
    pump_until(|| {
        c.shell.tick();
        c.shell.preferences().editing_allowed()
    });
    set(&c, Change::Db(-8.)).unwrap();
    levels_owner::complete(&c.fake, 0, LocalConfigOutcome::Applied, true);
    c.owner.poll();
    c.owner.poll();
    pump_until(|| {
        c.shell.tick();
        c.shell.settings_summary().failures == 1
    });
    assert!(c.shell.preferences().dirty());
    set(&c, Change::Mute(true)).unwrap();
    levels_owner::complete(&c.fake, 1, LocalConfigOutcome::Applied, true);
    c.owner.poll();
    c.owner.poll();
    let desired = c.shell.preferences().document().clone();
    assert!(desired.preferences().buses.state("rec-1-2").mute);
    assert!(
        !file
            .load()
            .unwrap()
            .settings()
            .preferences()
            .buses
            .state("rec-1-2")
            .mute
    );
    for _ in 0..20 {
        c.shell.tick();
    }
    assert_eq!(c.shell.settings_summary().last_request, 1);
    c.shell
        .submit_settings(crate::settings_worker::SettingsOperation::Reload)
        .unwrap();
    pump_until(|| {
        c.shell.tick();
        c.shell.settings_summary().last_request == 2
    });
    assert_eq!(c.shell.preferences().document(), &desired);
    assert!(c.shell.preferences().message().contains("Choose Save"));
    preference_button(&c, "Save local edits").emit_clicked();
    pump_until(|| {
        c.shell.tick();
        !c.shell.preferences().dirty()
    });
    assert_eq!(file.load().unwrap().settings(), &desired);
    c.driver
        .complete_shutdown(pecan_pipewire::shutdown::ShutdownReport::default());
    c.shell.request_quit();
    pump_until(|| {
        c.shell.tick();
        matches!(c.shell.phase(), crate::lifetime::QuitPhase::Failed { .. })
    });
    assert!(!c.shell.settings_summary().unsaved_preferences);
    button(c.shell.window(), "Exit after finalization error").emit_clicked();
    assert_eq!(c.shell.exit_code(), Some(1));
    fs::remove_dir_all(dir).unwrap();
}

#[gtk::test]
fn rec_writeback_callback_graph_replacement_cannot_dispatch_to_replacement() {
    for gain in [false, true] {
        let (owner, fake, driver) = levels_owner::setup_levels();
        let weak = Rc::downgrade(&owner);
        owner.bind_preference_edits(Rc::new(move |_| {
            let owner = weak.upgrade().unwrap();
            let graph = (*owner.session.graph().unwrap().value).clone();
            let mut ids = generations(&graph);
            *ids.get_mut(&30).unwrap() += 1000;
            driver.publish(graph, ids);
            owner.session.poll();
            Ok(())
        }));
        let origin = owner.origin().unwrap();
        let result = if gain {
            let capture = levels_owner::capture(&owner, "rec-1-2");
            owner
                .level_event(Event::Set(capture.propose(Change::Db(-6.)).unwrap()))
                .map(|_| ())
        } else {
            let before = origin
                .0
                .config
                .as_ref()
                .unwrap()
                .configuration
                .as_ref()
                .unwrap()
                .buses
                .clone();
            owner
                .configure(
                    &origin,
                    ui::Action::Arm {
                        bus: "rec-1-2".into(),
                        additive: true,
                    },
                    Some(before.arm("rec-1-2", true).unwrap()),
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        assert!(result.is_err());
        assert!(fake.configs.borrow().is_empty() && fake.gains.borrow().is_empty());
        assert!(!owner.session.busy());
    }
}
