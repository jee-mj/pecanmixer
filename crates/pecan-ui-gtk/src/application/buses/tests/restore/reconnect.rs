//! Completed synthetic user work, retained receipts and authenticated epochs.
use super::*;
use crate::recording_buses::levels::{Change, Event};
fn reconnect(owner: &BusOwner, fake: &Fake, driver: &FixtureDriver) {
    let mut graph = (*owner.session.graph().unwrap().value).clone();
    graph.verbonden = false;
    driver.publish(graph.clone(), generations(&graph));
    owner.session.poll();
    owner.poll();
    graph.verbonden = true;
    let fresh = driver.publish(graph.clone(), generations(&graph));
    owner.session.poll();
    fake.config.borrow_mut().revision.epoch = fresh.stamp.epoch;
    owner.poll();
}
#[gtk::test]
fn restore_reconnect_after_completed_user_config_uses_new_epoch_once() {
    let (owner, fake, driver) = setup_restore();
    arm(&owner);
    let old = fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    assert!(!owner.session.busy());
    reconnect(&owner, &fake, &driver);
    let p = preferences();
    owner.restore_preferences(&p);
    owner.restore_preferences(&p);
    assert_eq!(
        fake.restore_origins.borrow().len(),
        1,
        "completed user config must not suppress a new-epoch restore"
    );
    let next = fake.configs.borrow()[1].0.clone();
    assert!(next.expected.epoch > old.expected.epoch);
    assert!(next.ticket > old.ticket);
    assert_eq!(next.configuration.buses.state("rec-1-2").db, -18.);
    for _ in 0..10 {
        owner.restore_preferences(&p);
    }
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_after_completed_user_gain_preserves_latest_saved_intent() {
    let (owner, fake, driver) = setup_restore();
    let capture = levels_owner::capture(&owner, "rec-1-2");
    owner
        .level_event(Event::Set(capture.propose(Change::Db(-6.)).unwrap()))
        .unwrap();
    let old = levels_owner::complete(&fake, 0, LocalConfigOutcome::Applied, true);
    owner.poll();
    owner.poll();
    assert!(!owner.session.busy());
    reconnect(&owner, &fake, &driver);
    let mut p = preferences();
    owner.restore_preferences(&p);
    let mut latest = p.document().clone().into_preferences();
    latest.buses = latest.buses.with_db("rec-1-2", -23.).unwrap();
    p.edit(SettingsV1::new(latest).unwrap()).unwrap();
    owner.restore_preferences(&p);
    assert_eq!(
        fake.restore_origins.borrow().len(),
        1,
        "completed user gain must not suppress a new-epoch restore"
    );
    let next = fake.configs.borrow()[0].0.clone();
    assert!(next.ticket > old.ticket);
    assert!(next.expected.epoch > old.expected.epoch);
    assert_eq!(next.configuration.buses.state("rec-1-2").db, -23.);
    assert_eq!(fake.gains.borrow().len(), 1, "never replay the old gain");
}

#[gtk::test]
fn restore_reconnect_closed_noop_archives_original_receipt_without_restamping() {
    let (owner, fake, driver) = setup_restore();
    arm(&owner);
    let old = fake.complete_config(0, LocalConfigOutcome::NoOp, true);
    owner.poll();
    let receipt = fake.configs.borrow()[0].1.clone() as Rc<dyn ConfigPoll>;
    reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&preferences());
    let state = owner.restore.borrow();
    let archived = state.retired_completed.back().unwrap();
    assert!(Rc::ptr_eq(archived.receipt.as_ref().unwrap(), &receipt));
    assert_eq!(archived.receipt.as_ref().unwrap().report(), Some(old));
    assert!(
        fake.restore_origins.borrow().is_empty(),
        "retirement publishes first, not an audio handoff"
    );
}
#[gtk::test]
fn restore_reconnect_pending_failed_or_unconfirmed_user_work_is_not_discarded() {
    for phase in [0, 1, 2, 3, 4] {
        let (owner, fake, driver) = setup_restore();
        let request = arm(&owner);
        match phase {
            1 => {
                fake.configs.borrow()[0]
                    .1
                    .report
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .progress = ConfigProgress::Applying
            }
            2 => {
                fake.configs.borrow()[0]
                    .1
                    .report
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .progress = ConfigProgress::Unconfirmed
            }
            3 => {
                fake.complete_config(
                    0,
                    LocalConfigOutcome::PartiallyApplied(ConfigError::ReadbackMismatch),
                    false,
                );
            }
            4 => {
                fake.complete_config(
                    0,
                    LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
                    false,
                );
            }
            _ => {}
        }
        owner.poll();
        reconnect(&owner, &fake, &driver);
        for _ in 0..5 {
            owner.restore_preferences(&preferences());
        }
        assert_eq!(
            owner.data.borrow().operation.as_ref().unwrap().request,
            request
        );
        assert!(owner.restore.borrow().retired_completed.is_empty());
        assert!(fake.restore_origins.borrow().is_empty());
        assert_eq!(fake.configs.borrow().len(), 1);
        assert!(button(owner.widget(), "Stop all").is_sensitive());
    }
}
#[gtk::test]
fn restore_reconnect_requires_released_shared_ownership_and_fresh_inspection() {
    let (owner, fake, driver) = setup_restore();
    let old = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    let (_, other) = owner.session.reserve_external().unwrap();
    owner.restore_preferences(&preferences());
    assert_eq!(owner.data.borrow().operation.as_ref().unwrap().request, old);
    owner.session.release_external(&other);
    fake.busy.set(true);
    owner.restore_preferences(&preferences());
    assert_eq!(owner.data.borrow().operation.as_ref().unwrap().request, old);
    assert!(owner.restore.borrow().retired_completed.is_empty());
    fake.busy.set(false);
    owner.restore_preferences(&preferences());
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_waits_for_complete_authenticated_current_inputs() {
    let (owner, fake, driver) = setup_restore();
    let old = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    let full = (*owner.session.graph().unwrap().value).clone();
    let mut partial = full.clone();
    partial.poorten.retain(|p| p.id != 302);
    driver.publish(partial.clone(), generations(&partial));
    owner.session.poll();
    owner.poll();
    owner.restore_preferences(&preferences());
    assert_eq!(owner.data.borrow().operation.as_ref().unwrap().request, old);
    assert!(fake.restore_origins.borrow().is_empty());
    driver.publish(full.clone(), generations(&full));
    owner.session.poll();
    owner.poll();
    owner.restore_preferences(&preferences());
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_reentrant_receipt_rebind_cannot_retire_through_old_origin() {
    let (owner, fake, driver) = setup_restore();
    let old = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    let weak = Rc::downgrade(&owner);
    let publish = driver.clone();
    *fake.configs.borrow()[0].1.hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        let graph = (*owner.session.graph().unwrap().value).clone();
        publish.publish(graph.clone(), generations(&graph));
        owner.session.poll();
        owner.poll();
    }));
    owner.restore_preferences(&preferences());
    assert_eq!(owner.data.borrow().operation.as_ref().unwrap().request, old);
    assert!(owner.restore.borrow().retired_completed.is_empty());
    owner.restore_preferences(&preferences());
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_completed_arming_waits_for_joined_children_and_idle_owner() {
    use pecan_pipewire::recording_arming::ArmingOutcome;
    let (owner, fake, driver) = setup_restore();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 1);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    arm(&owner);
    let expected = fake.arms.borrow()[0].0.expected_config;
    arming::complete(&fake, 0, ArmingOutcome::AppliedLocally, true);
    owner.poll();
    assert!(!owner.session.busy());
    fake.joined(0, None);
    fake.config.borrow_mut().recording = false;
    owner.poll();
    reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&preferences());
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    assert_eq!(
        fake.starts.borrow().len(),
        1,
        "no old recording start replay"
    );
    let state = owner.restore.borrow();
    let archived = state.retired_completed.back().unwrap();
    assert_eq!(archived.expected, expected);
    assert!(archived.arming.is_some());
    assert_eq!(
        owner.recording_status()[0].progress,
        RecordingProgress::JoinedSuccess
    );
}

#[gtk::test]
fn restore_reconnect_waiting_for_old_pending_closure_does_not_consume_epoch() {
    let (owner, fake, driver) = setup_restore();
    let request = arm(&owner);
    let old = fake.complete_config(0, LocalConfigOutcome::Applied, false);
    fake.configs.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .progress = ConfigProgress::Queued;
    reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&preferences());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(owner.restore.borrow().retired_completed.is_empty());
    *fake.configs.borrow()[0].1.report.borrow_mut() = Some(old);
    owner.poll();
    owner.restore_preferences(&preferences());
    owner.restore_preferences(&preferences());
    assert_eq!(fake.restore_origins.borrow().len(), 1);
}
#[gtk::test]
fn restore_reconnect_recording_activity_cannot_become_deferred_restore_after_join() {
    let (owner, fake, driver) = setup_restore();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    owner.restore_preferences(&preferences());
    fake.config.borrow_mut().recording = false;
    owner.poll();
    for _ in 0..5 {
        owner.restore_preferences(&preferences());
    }
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(fake.restore_origins.borrow().is_empty());
    assert!(owner.restore.borrow().retired_completed.is_empty());
}

#[gtk::test]
fn restore_reconnect_completed_migration_retains_original_observation_before_restore() {
    use pecan_pipewire::bus_migration::MigrationOutcome;
    let (owner, fake, driver, _) = migration::routed();
    fake.restore_enabled.set(true);
    button(owner.widget(), "Split stereo").emit_clicked();
    let old = migration::complete(&fake, 0, MigrationOutcome::AppliedLocally, true);
    owner.poll();
    assert!(!owner.session.busy());
    reconnect(&owner, &fake, &driver);
    let mut saved = SettingsV1::default().into_preferences();
    let actual = fake
        .config
        .borrow()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    let key = actual.definitions().unwrap()[0].key.clone();
    saved.buses = actual.with_db(&key, -14.).unwrap();
    let mut p = PreferencesState::new(false);
    p.edit(SettingsV1::new(saved).unwrap()).unwrap();
    owner.restore_preferences(&p);
    owner.restore_preferences(&p);
    assert_eq!(fake.restore_origins.borrow().len(), 1);
    assert_eq!(
        fake.migrations.borrow().len(),
        1,
        "do not replay the old migration"
    );
    assert_eq!(
        owner.data.borrow().migrations.back().unwrap().source,
        old.source
    );
    assert_eq!(
        owner.data.borrow().migrations.back().unwrap().progress,
        old.progress
    );
    assert!(owner
        .restore
        .borrow()
        .retired_completed
        .back()
        .unwrap()
        .migration
        .is_some());
}
#[gtk::test]
fn restore_reconnect_unclosed_migration_keeps_request_and_quit_authority() {
    use pecan_pipewire::bus_migration::{MigrationOutcome, MigrationProgress};
    let (owner, fake, driver, _) = migration::routed();
    fake.restore_enabled.set(true);
    button(owner.widget(), "Split stereo").emit_clicked();
    let request = owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .request
        .clone();
    let old = migration::complete(&fake, 0, MigrationOutcome::AppliedLocally, true);
    // Existing migration polling requires zero executing calls, regardless of
    // a terminal-looking native phase. Preserve that barrier across reconnect.
    let receipt = fake.migrations.borrow()[0].1.clone();
    let pending = pecan_pipewire::bus_migration::MigrationReport {
        in_progress: 1,
        ..old.clone()
    };
    // Publish through the fixture helper's original receipt, never a new request.
    migration::set_report(&receipt, pending);
    owner.poll();
    assert!(owner.session.busy());
    reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&preferences());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(fake.restore_origins.borrow().is_empty());
    assert!(owner.restore.borrow().retired_completed.is_empty());
    assert!(button(owner.widget(), "Stop all").is_sensitive());
    assert_eq!(
        owner.migration_report().unwrap().progress,
        MigrationProgress::Completed(MigrationOutcome::AppliedLocally)
    );
}
#[gtk::test]
fn restore_reconnect_closed_history_is_bounded_and_old_tickets_are_not_reused() {
    let (owner, fake, driver) = setup_restore();
    for index in 0..20 {
        arm(&owner);
        fake.complete_config(index, LocalConfigOutcome::Applied, true);
        owner.poll();
        reconnect(&owner, &fake, &driver);
        owner.restore_preferences(&preferences());
        assert!(owner.data.borrow().operation.is_none());
        assert_eq!(
            owner.restore.borrow().retired_completed.len(),
            (index + 1).min(16)
        );
        assert!(
            fake.restore_origins.borrow().is_empty(),
            "retirement itself never dispatches audio"
        );
    }
    owner.restore_preferences(&preferences());
    let next = fake.configs.borrow()[20].0.ticket;
    let state = owner.restore.borrow();
    assert!(state.retired_completed.iter().all(|w| w.ticket < next));
    assert_eq!(state.retired_completed.len(), 16);
}

fn recording_during_retirement_callback(receipt_callback: bool, extra_active_tick: bool) {
    let (owner, fake, driver) = setup_restore();
    // Make the normal user Arm edit leave a legitimately armed recording bus.
    fake.config
        .borrow_mut()
        .configuration
        .as_mut()
        .unwrap()
        .buses
        .states
        .entry("rec-1-2".into())
        .or_default()
        .armed = false;
    owner.poll();
    let request = arm(&owner);
    let old = fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    let weak_owner = Rc::downgrade(&owner);
    let weak_fake = Rc::downgrade(&fake);
    let callback = Box::new(move || {
        let owner = weak_owner.upgrade().unwrap();
        let fake = weak_fake.upgrade().unwrap();
        let expected_config = fake.config.borrow().revision;
        fake.start_all(RecordingStart {
            ticket: 900,
            origin: owner.session.graph().unwrap(),
            expected_config,
            root: std::env::temp_dir().join("pecan-reconnect-callback-recording-synthetic"),
        })
        .unwrap();
        fake.started(0, 900);
        fake.config.borrow_mut().recording = true;
        owner.poll();
    }) as Box<dyn Fn()>;
    if receipt_callback {
        *fake.configs.borrow()[0].1.hook.borrow_mut() = Some(callback);
    } else {
        *fake.read_hook.borrow_mut() = Some(callback);
    }
    owner.restore_preferences(&preferences());
    assert_eq!(
        owner.recording_status()[0].progress,
        RecordingProgress::StartedLocally
    );
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert!(owner.restore.borrow().retired_completed.is_empty());
    if extra_active_tick {
        owner.restore_preferences(&preferences());
    }
    // No further restore call while active in the regression cases. Joining
    // cannot turn the already observed recording into a deferred settings effect.
    fake.joined(0, None);
    fake.config.borrow_mut().recording = false;
    owner.poll();
    for _ in 0..5 {
        owner.restore_preferences(&preferences());
    }
    assert_eq!(
        fake.restore_origins.borrow().len(),
        0,
        "callback-observed recording must latch the epoch skip in that same call"
    );
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert_eq!(owner.configuration_report(), Some(old));
    assert_eq!(
        owner.recording_status()[0].progress,
        RecordingProgress::JoinedSuccess
    );
    assert!(button(owner.widget(), "Stop all").is_sensitive());
}
#[gtk::test]
fn restore_reconnect_service_callback_ownership_observation_needs_no_extra_tick() {
    recording_during_retirement_callback(false, false);
}
#[gtk::test]
fn restore_reconnect_receipt_callback_ownership_observation_needs_no_extra_tick() {
    recording_during_retirement_callback(true, false);
}
#[gtk::test]
fn restore_reconnect_extra_active_tick_ownership_skip_control() {
    recording_during_retirement_callback(false, true);
}

#[gtk::test]
fn restore_reconnect_recording_readback_observation_needs_no_cached_active_tick() {
    let (owner, fake, driver) = setup_restore();
    let request = arm(&owner);
    let old = fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    reconnect(&owner, &fake, &driver);
    assert!(!owner.data.borrow().config.as_ref().unwrap().recording);
    fake.config.borrow_mut().recording = true;
    // Only the retirement inspection observes this active flag; do not publish
    // another owner tick while it is active.
    owner.restore_preferences(&preferences());
    fake.config.borrow_mut().recording = false;
    owner.poll();
    for _ in 0..5 {
        owner.restore_preferences(&preferences());
    }
    assert!(fake.restore_origins.borrow().is_empty());
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    assert_eq!(owner.configuration_report(), Some(old));
}
