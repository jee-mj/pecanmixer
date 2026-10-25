//! Synthetic saved proposals and native-owner generations only; no audio/files.
use super::*;
use pecan_core::{
    graph::{BusOpname, Kabel, Opties, Soort},
    history_identity::HistoryGraph,
};
use pecan_pipewire::bus_migration::*;

fn setup_saved(unknown: bool) -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let (owner, fake, driver) = setup_restore();
    fake.saved_restore_enabled.set(true);
    if unknown {
        fake.config.borrow_mut().configuration = None;
        fake.config.borrow_mut().transport = None;
        owner.poll();
    }
    (owner, fake, driver)
}
fn saved(widths: Vec<usize>) -> PreferencesState {
    let mut p = preferences();
    let mut v = p.document().clone().into_preferences();
    v.buses.widths = widths;
    for (index, b) in v.buses.definitions().unwrap().into_iter().enumerate() {
        v.buses = v.buses.with_db(&b.key, -(index as f64 + 6.)).unwrap();
        v.buses = v.buses.with_mute(&b.key, index % 2 == 1).unwrap();
    }
    p.edit(SettingsV1::new(v).unwrap()).unwrap();
    p
}
fn route(owner: &BusOwner, driver: &FixtureDriver) {
    let mut g = (*owner.session.graph().unwrap().value).clone();
    g.kabels.push(Kabel {
        id: 9000,
        client: None,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: 30,
        in_poort: 301,
    });
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    owner.poll();
}
fn complete_saved(fake: &Fake, outcome: MigrationOutcome, publish: bool) -> MigrationReport {
    let receipt = fake.saved_layouts.borrow().last().unwrap().1.clone();
    let mut r = receipt.current();
    r.progress = MigrationProgress::Completed(outcome);
    if outcome != MigrationOutcome::NoOpLocally {
        r.configuration.revision.revision += 1;
    }
    r.configuration.configuration = Some(r.proposed.clone());
    let pecan_core::graph::Opdracht::ZetBus { kanalen, gain } =
        r.proposed.buses.backend_config().unwrap()
    else {
        panic!()
    };
    r.configuration.local.as_mut().unwrap().channels = kanalen;
    r.configuration.local.as_mut().unwrap().gains = gain;
    r.configuration.transport = Some(Opties {
        bussen: r
            .proposed
            .buses
            .armed()
            .unwrap()
            .into_iter()
            .map(|b| BusOpname {
                naam: b.file,
                kanalen: b.channels,
            })
            .collect(),
        midi: false,
        klok: None,
        start_stop: false,
    });
    r.local_readback = r.configuration.local.clone();
    if publish {
        *fake.config.borrow_mut() = r.configuration.clone();
    }
    migration::set_report(&receipt, r.clone());
    r
}
fn extend(owner: &BusOwner, driver: &FixtureDriver, channels: usize) -> HistoryGraph {
    let mut g = (*owner.session.graph().unwrap().value).clone();
    let template = g.poorten.iter().find(|p| p.id == 301).unwrap().clone();
    for n in 3..=channels {
        let mut p = template.clone();
        p.id = 300 + n as u32;
        p.naam = format!("rec_{n}");
        p.volgnummer = Some(n as u32 - 1);
        g.poorten.push(p);
    }
    g.knopen.iter_mut().find(|n| n.id == 30).unwrap().volume = Some(vec![1.; channels]);
    // The actual completion witness is deliberately limited to owned inputs.
    let owned = generations(&g)
        .into_iter()
        .filter(|(id, _)| {
            *id == 30
                || g.poorten
                    .iter()
                    .any(|p| p.id == *id && p.knoop == 30 && p.ingang && p.soort == Soort::Audio)
        })
        .collect();
    driver.publish(g.clone(), owned);
    owner.session.poll();
    let proof = owner.session.witnessed_graph().unwrap();
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    proof
}
fn observed(
    owner: &BusOwner,
    fake: &Fake,
    driver: &FixtureDriver,
    channels: usize,
) -> MigrationReport {
    let proof = extend(owner, driver, channels);
    let mut r = complete_saved(fake, MigrationOutcome::Observed, true);
    r.ports_observed = Some(proof.graph().stamp);
    r.observed_inputs = Some(proof);
    migration::set_report(&fake.saved_layouts.borrow().last().unwrap().1, r.clone());
    r
}
fn progress(owner: &BusOwner) -> ui::Progress {
    owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .progress
        .clone()
}
fn released(owner: &BusOwner) -> bool {
    owner
        .data
        .borrow()
        .operation
        .as_ref()
        .is_some_and(|w| w.terminal && w.migration.as_ref().unwrap().released)
}

#[gtk::test]
fn saved_restore_unknown_routed_grouping_uses_exact_saved_authority_and_never_writes_preferences() {
    for unknown in [false, true] {
        let (owner, fake, driver) = setup_saved(unknown);
        route(&owner, &driver);
        let p = saved(vec![1, 1]);
        let before = p.document().clone();
        let origin = owner.origin().unwrap();
        owner.bind_preference_edits(Rc::new(|_| panic!("Restore cannot write preferences")));
        owner.restore_preferences(&p);
        let intent = fake.saved_attempts.borrow()[0].clone();
        assert!(Arc::ptr_eq(
            &intent.origin.graph().value,
            &origin.0.graph.as_ref().unwrap().value
        ));
        assert_eq!(
            intent.expected_config,
            origin.0.config.as_ref().unwrap().revision
        );
        assert_eq!(Some(intent.expected_recording), origin.0.membership);
        assert!(!intent.configuration.buses.midi);
        assert!(intent.configuration.clock.is_none());
        assert!(!intent.configuration.start_stop);
        assert_eq!(intent.configuration.buses.widths, [1, 1]);
        for _ in 0..1000 {
            owner.restore_preferences(&p);
        }
        assert_eq!(fake.saved_attempts.borrow().len(), 1);
        complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
        owner.poll();
        assert!(
            owner.session.busy(),
            "Inspection precedes this first terminal delivery"
        );
        owner.poll();
        assert!(released(&owner));
        assert_eq!(progress(&owner), ui::Progress::AppliedLocally);
        assert!(!owner.session.busy());
        assert_eq!(owner.session.graph().unwrap().value.kabels.len(), 1);
        assert_eq!(p.document(), &before);
        assert!(
            fake.configs.borrow().is_empty(),
            "No weak configure fallback"
        );
        assert!(
            driver.take_requests().is_empty(),
            "No graph history/audio commands"
        );
    }
}
#[gtk::test]
fn saved_restore_odd_and_max_growth_wait_for_retained_observation_and_later_fresh_readback() {
    for (unknown, widths) in [
        (false, vec![1, 2]),
        (true, vec![2, 1, 2]),
        (true, vec![2; 32]),
    ] {
        let (owner, fake, driver) = setup_saved(unknown);
        route(&owner, &driver);
        let channels = widths.iter().sum();
        owner.restore_preferences(&saved(widths));
        let receipt = fake.saved_layouts.borrow()[0].1.clone();
        let mut pending = receipt.current();
        pending.progress = MigrationProgress::AwaitingPorts;
        migration::set_report(&receipt, pending);
        for _ in 0..3 {
            owner.poll();
            assert!(owner.session.busy());
        }
        let r = observed(&owner, &fake, &driver, channels);
        owner.poll();
        assert!(owner.session.busy());
        fake.busy.set(true);
        owner.refresh();
        assert!(owner.session.busy());
        fake.busy.set(false);
        // A later frame may coalesce the original observation and evict its
        // cache entry. Generations come from the retained report, not lookup.
        let g = (*owner.session.graph().unwrap().value).clone();
        for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
            driver.publish(g.clone(), generations(&g));
            owner.session.poll();
        }
        owner.poll();
        assert!(released(&owner));
        assert_eq!(progress(&owner), ui::Progress::Observed);
        assert_eq!(
            owner.migration_report().unwrap().ports_observed,
            r.ports_observed
        );
        assert_eq!(
            owner
                .origin()
                .unwrap()
                .0
                .config
                .as_ref()
                .unwrap()
                .local
                .as_ref()
                .unwrap()
                .channels,
            channels
        );
        assert!(driver.take_requests().is_empty());
    }
}
#[gtk::test]
fn saved_restore_noop_busy_and_inflight_reports_cannot_release_early() {
    let (owner, fake, _) = setup_saved(false);
    let buses = fake
        .config
        .borrow()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    fake.config.borrow_mut().transport = Some(Opties {
        bussen: buses
            .armed()
            .unwrap()
            .into_iter()
            .map(|b| BusOpname {
                naam: b.file,
                kanalen: b.channels,
            })
            .collect(),
        midi: false,
        klok: None,
        start_stop: false,
    });
    owner.poll();
    owner.restore_preferences(&PreferencesState::new(false));
    let receipt = fake.saved_layouts.borrow()[0].1.clone();
    let mut r = complete_saved(&fake, MigrationOutcome::NoOpLocally, true);
    r.in_progress = 1;
    migration::set_report(&receipt, r.clone());
    for _ in 0..3 {
        owner.poll();
        assert!(owner.session.busy());
    }
    r.in_progress = 0;
    migration::set_report(&receipt, r);
    owner.poll();
    assert!(owner.session.busy());
    fake.busy.set(true);
    owner.poll();
    assert!(owner.session.busy());
    fake.busy.set(false);
    owner.poll();
    assert!(released(&owner));
    assert_eq!(progress(&owner), ui::Progress::NoOpLocally);
}
#[gtk::test]
fn saved_restore_original_source_prefix_and_final_suffix_replacement_never_rebind_or_replay() {
    for id in [20, 201, 30, 301, 303] {
        let (owner, fake, driver) = setup_saved(true);
        route(&owner, &driver);
        let p = saved(vec![2, 1]);
        owner.restore_preferences(&p);
        observed(&owner, &fake, &driver, 3);
        owner.poll();
        let g = (*owner.session.graph().unwrap().value).clone();
        let mut ids = generations(&g);
        *ids.get_mut(&id).unwrap() += 100_000;
        driver.publish(g, ids);
        owner.session.poll();
        owner.poll();
        assert!(released(&owner));
        assert!(
            matches!(progress(&owner), ui::Progress::Unconfirmed(_)),
            "replacement {id}"
        );
        for _ in 0..4 {
            owner.restore_preferences(&p);
            owner.refresh();
        }
        assert_eq!(fake.saved_attempts.borrow().len(), 1);
    }
}
#[gtk::test]
fn saved_restore_equal_capacity_requires_original_route_pairs_but_not_link_ids() {
    for changed_pair in [false, true] {
        let (owner, fake, driver) = setup_saved(true);
        route(&owner, &driver);
        owner.restore_preferences(&saved(vec![1, 1]));
        complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
        owner.poll();
        let mut g = (*owner.session.graph().unwrap().value).clone();
        g.kabels[0].id += 1000;
        if changed_pair {
            g.kabels[0].uit_poort = 202;
        }
        driver.publish(g.clone(), generations(&g));
        owner.session.poll();
        owner.poll();
        assert!(released(&owner));
        assert_eq!(
            matches!(progress(&owner), ui::Progress::Unconfirmed(_)),
            changed_pair
        );
    }
}
#[gtk::test]
fn saved_restore_partial_unconfirmed_and_admission_rejection_keep_evidence_no_fallback_or_retry() {
    for outcome in [
        None,
        Some(MigrationOutcome::PartiallyApplied),
        Some(MigrationOutcome::Unconfirmed),
        Some(MigrationOutcome::Rejected),
    ] {
        let (owner, fake, _) = setup_saved(true);
        if outcome.is_none() {
            *fake.saved_failure.borrow_mut() = Some(Error::Migration(MigrationError::QueueFull));
        }
        let p = saved(vec![2, 1]);
        owner.restore_preferences(&p);
        if let Some(outcome) = outcome {
            complete_saved(&fake, outcome, true);
            owner.poll();
        }
        fake.busy.set(true);
        owner.poll();
        assert!(owner.session.busy());
        assert!(button(owner.widget(), "Stop all").is_sensitive());
        fake.busy.set(false);
        owner.poll();
        assert!(released(&owner));
        assert!(matches!(
            progress(&owner),
            ui::Progress::PartiallyApplied(_)
                | ui::Progress::Unconfirmed(_)
                | ui::Progress::Rejected(_)
        ));
        for _ in 0..4 {
            owner.restore_preferences(&p);
            owner.refresh();
        }
        assert_eq!(fake.saved_attempts.borrow().len(), 1);
        assert!(fake.configs.borrow().is_empty());
    }
}
#[gtk::test]
fn saved_restore_same_epoch_supersession_and_lost_connection_become_uncertainty_without_replay() {
    for reconnect in [false, true] {
        let (owner, fake, driver) = setup_saved(true);
        let p = saved(vec![1, 1]);
        owner.restore_preferences(&p);
        let original = complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
        owner.poll();
        if reconnect {
            reconnect::reconnect(&owner, &fake, &driver);
        } else {
            fake.config.borrow_mut().revision.revision += 1;
            fake.config.borrow_mut().configuration = None;
        }
        owner.poll();
        assert!(released(&owner));
        assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
        for _ in 0..4 {
            owner.restore_preferences(&p);
            owner.poll();
        }
        assert_eq!(fake.saved_attempts.borrow().len(), 1);
        // Explicit Refresh is the pre-existing user reconciliation boundary,
        // not an automatic tick. It archives the original failed evidence.
        owner.refresh();
        assert_eq!(
            owner.data.borrow().migrations.back().unwrap().source,
            original.source
        );
    }
}
#[gtk::test]
fn saved_restore_successful_reconnect_uses_latest_desired_once_and_retains_old_receipt() {
    let (owner, fake, driver) = setup_saved(true);
    owner.restore_preferences(&saved(vec![1, 1]));
    complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
    owner.poll();
    owner.poll();
    let old = fake.saved_layouts.borrow()[0].0.clone();
    reconnect::reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&saved(vec![2, 1])); // retirement only
    assert_eq!(fake.saved_attempts.borrow().len(), 1);
    owner.restore_preferences(&saved(vec![2, 2]));
    owner.restore_preferences(&saved(vec![2, 2]));
    let next = fake.saved_attempts.borrow()[1].clone();
    assert_eq!(next.configuration.buses.widths, [2, 2]);
    assert!(next.expected_config.epoch > old.expected_config.epoch);
    assert!(next.ticket > old.ticket);
    assert_eq!(owner.restore.borrow().retired_completed.len(), 1);
}
#[gtk::test]
fn saved_restore_missing_prefix_shrink_recording_user_work_and_quit_keep_existing_gates() {
    for gate in 0..5 {
        let (owner, fake, driver) = setup_saved(false);
        let mut p = saved(vec![2, 1]);
        match gate {
            0 => {
                let mut g = (*owner.session.graph().unwrap().value).clone();
                g.poorten.retain(|p| p.id != 302);
                driver.publish(g.clone(), generations(&g));
                owner.session.poll();
                owner.poll();
            }
            1 => p = saved(vec![1]),
            2 => {
                fake.config.borrow_mut().recording = true;
                owner.poll();
            }
            3 => {
                arm(&owner);
            }
            _ => owner.session.close_admission(),
        }
        owner.restore_preferences(&p);
        assert!(fake.saved_attempts.borrow().is_empty(), "gate {gate}");
    }
}
#[gtk::test]
fn saved_restore_admission_and_receipt_reentry_keep_one_owner_without_ui_borrow() {
    let (owner, fake, _) = setup_saved(true);
    let weak = Rc::downgrade(&owner);
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        owner.recording_status();
        owner.restore_preferences(&saved(vec![2, 1]));
        owner.poll();
    }));
    owner.restore_preferences(&saved(vec![1, 1]));
    let weak = Rc::downgrade(&owner);
    fake.saved_layouts.borrow()[0]
        .1
        .on_report(Box::new(move || {
            let o = weak.upgrade().unwrap();
            o.recording_status();
            o.refresh();
        }));
    complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
    owner.poll();
    owner.poll();
    assert!(released(&owner));
    assert_eq!(fake.saved_attempts.borrow().len(), 1);
}

#[gtk::test]
fn saved_restore_recording_observed_during_preflight_latches_skip_even_if_joined_before_return() {
    let (owner, fake, _) = setup_saved(false);
    let weak = Rc::downgrade(&owner);
    let endpoint = fake.clone();
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        endpoint.config.borrow_mut().recording = true;
        o.poll();
        endpoint.config.borrow_mut().recording = false;
        o.poll();
    }));
    owner.restore_preferences(&saved(vec![2, 1]));
    for _ in 0..4 {
        owner.restore_preferences(&saved(vec![2, 1]));
        owner.poll();
    }
    assert!(fake.saved_attempts.borrow().is_empty());
    assert_eq!(
        owner.restore.borrow().attempted,
        Some(fake.config.borrow().revision)
    );
}

#[gtk::test]
fn saved_restore_terminal_view_rejection_keeps_shared_lease_until_accepted_presentation() {
    let (owner, fake, driver) = setup_saved(false);
    owner.restore_preferences(&saved(vec![2, 1]));
    observed(&owner, &fake, &driver, 3);
    owner.poll();
    let source = Source::new(
        99,
        &owner.session.graph().unwrap(),
        fake.config.borrow().revision,
    );
    for id in [91, 92] {
        fake.takes
            .borrow_mut()
            .push(Rc::new(Take(RefCell::new(Some(TakeStatus {
                source: Source {
                    ticket: id,
                    ..source
                },
                progress: RecordingProgress::JoinedSuccess,
                writer: Some(WriterPhase::Joined),
                writer_outcome: Some(WriterOutcome::Finalized),
                failure: None,
                complete: true,
                take: Some(TakeKey::Synthetic(id)),
                elapsed_at: None,
                directory: None,
                tracks: vec![ui::Track {
                    identity: TrackKey::Synthetic(900),
                    bus_key: "rec-1-2".into(),
                    phase: ui::TrackPhase::Joined(Ok(())),
                    elapsed: None,
                }],
            })))));
    }
    owner.poll();
    assert!(owner.view.publication_error().is_some());
    assert!(owner.session.busy());
    assert!(!released(&owner));
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks
        .clear();
    owner.poll();
    assert!(owner.view.publication_error().is_none());
    assert!(released(&owner));
    assert!(!owner.session.busy());
}

#[gtk::test]
fn saved_restore_growth_cannot_accept_missing_or_inconsistent_completion_witness() {
    for mode in 0..3 {
        let (owner, fake, driver) = setup_saved(true);
        owner.restore_preferences(&saved(vec![2, 1]));
        let mut report = observed(&owner, &fake, &driver, 3);
        match mode {
            0 => report.observed_inputs = None,
            1 => report.ports_observed = None,
            _ => {
                report.ports_observed.as_mut().unwrap().revision += 100;
            }
        }
        migration::set_report(&fake.saved_layouts.borrow()[0].1, report);
        owner.poll();
        owner.poll();
        if mode == 2 {
            assert!(
                owner.session.busy(),
                "A future stamp cannot release the lease without a covering publication"
            );
        } else {
            assert!(released(&owner));
            assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
        }
    }
}

#[gtk::test]
fn saved_restore_matching_logical_metadata_cannot_hide_incompatible_actual_local_or_transport_readback(
) {
    for field in 0..4 {
        let (owner, fake, _) = setup_saved(true);
        owner.restore_preferences(&saved(vec![1, 1]));
        let mut report = complete_saved(&fake, MigrationOutcome::AppliedLocally, true);
        match field {
            0 => report.configuration.local.as_mut().unwrap().gains[0] = 1.,
            1 => report.configuration.local.as_mut().unwrap().node = Some(999),
            2 => report.configuration.transport.as_mut().unwrap().midi = true,
            _ => report.configuration.recording = true,
        }
        *fake.config.borrow_mut() = report.configuration.clone();
        migration::set_report(&fake.saved_layouts.borrow()[0].1, report);
        owner.poll();
        owner.poll();
        assert!(released(&owner));
        assert!(
            matches!(progress(&owner), ui::Progress::Unconfirmed(_)),
            "field {field}"
        );
    }
}

#[gtk::test]
fn saved_restore_shell_load_growth_and_quit_keep_one_clock_and_unchanged_settings() {
    use crate::settings_worker::SettingsWorker;
    let dir = std::env::temp_dir().join(format!(
        "pecan-saved-restore-shell-synthetic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("settings.json");
    let p = saved(vec![1, 2]);
    let bytes = serde_json::to_vec(p.document()).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let host = Rc::new(ApplicationOwner::fixture());
    let driver = host.fixture_driver().unwrap();
    let session = host.activate();
    let mut g = graph();
    g.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam =
        pecan_core::domain::audio::REC_NODE.into();
    let frame = driver.publish(g.clone(), generations(&g));
    session.poll();
    let fake = Fake::new(frame.stamp.epoch);
    fake.restore_enabled.set(true);
    fake.saved_restore_enabled.set(true);
    fake.config.borrow_mut().configuration = None;
    let owner = BusOwner::new(session, Some(fake.clone()));
    owner.restore_preferences(&PreferencesState::new(true));
    assert!(
        fake.saved_attempts.borrow().is_empty(),
        "Initial load must finish before effects"
    );
    let app = gtk::Application::builder()
        .application_id(format!(
            "org.example.PecanSavedRestore.Case{}",
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
    while fake.saved_attempts.borrow().is_empty() {
        assert!(Instant::now() < end);
        shell.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(shell.preferences().document(), p.document());
    assert!(!shell.preferences().dirty());
    shell.activate();
    shell.activate();
    shell.tick();
    assert_eq!(fake.saved_attempts.borrow().len(), 1);
    shell.request_quit();
    shell.tick();
    assert!(owner.session.admission_closed());
    assert!(shell.exit_code().is_none());
    assert_eq!(
        owner.migration_report().unwrap().progress,
        MigrationProgress::Queued
    );
    observed(&owner, &fake, &driver, 3);
    driver.complete_shutdown(pecan_pipewire::shutdown::ShutdownReport::default());
    while shell.exit_code().is_none() {
        assert!(Instant::now() < end);
        shell.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(shell.exit_code(), Some(0));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(fake.saved_attempts.borrow().len(), 1);
    assert!(fake.configs.borrow().is_empty());
    assert!(driver.take_requests().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
