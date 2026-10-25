//! Injected local receipts only; no native constructor, capture or file I/O.
use super::*;
use pecan_pipewire::recording_arming::{ArmingOutcome, ArmingProgress};

pub(super) struct Arm(pub RefCell<Option<ArmingStatus>>);
impl ArmPoll for Arm {
    fn report(&self) -> Option<ArmingStatus> {
        self.0.borrow().clone()
    }
}
fn live() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let (owner, fake, driver) = setup();
    {
        let mut c = fake.config.borrow_mut();
        let b = &mut c.configuration.as_mut().unwrap().buses;
        b.widths = vec![1, 1, 1];
        b.states.clear();
        b.states.insert(
            "rec-1".into(),
            pecan_core::domain::buses::BusState {
                armed: true,
                ..Default::default()
            },
        );
        c.local.as_mut().unwrap().channels = 3;
        c.local.as_mut().unwrap().gains = vec![1.; 3];
    }
    owner.poll();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 1);
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .tracks[0]
        .bus_key = "rec-1".into();
    fake.config.borrow_mut().recording = true;
    owner.poll();
    (owner, fake, driver)
}
fn change(owner: &BusOwner, bus: &str, additive: bool) -> Result<Request, Error> {
    let origin = owner.origin().unwrap();
    let buses = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .arm(bus, additive)
        .unwrap();
    owner.configure(
        &origin,
        ui::Action::Arm {
            bus: bus.into(),
            additive,
        },
        Some(buses),
    )
}
fn child(fake: &Fake, arm: usize, bus: &str) -> Rc<Take> {
    let (intent, parent) = fake.arms.borrow()[arm].clone();
    let source = Source::new(intent.ticket, &intent.origin, intent.expected_config);
    let take = Rc::new(Take(RefCell::new(Some(TakeStatus {
        source,
        progress: RecordingProgress::Queued,
        writer: None,
        writer_outcome: None,
        failure: None,
        complete: false,
        take: None,
        directory: None,
        tracks: vec![],
    }))));
    parent
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .additions
        .push(ArmingChild {
            bus: bus.into(),
            new_take: true,
            receipt: take.clone(),
        });
    fake.takes.borrow_mut().push(take.clone());
    take
}
fn complete(fake: &Fake, index: usize, outcome: ArmingOutcome, publish: bool) -> ConfigSnapshot {
    let (intent, receipt) = fake.arms.borrow()[index].clone();
    let mut config = fake.config.borrow().clone();
    config.revision.revision += 1;
    for d in config
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .definitions()
        .unwrap()
    {
        config
            .configuration
            .as_mut()
            .unwrap()
            .buses
            .states
            .entry(d.key.clone())
            .or_default()
            .armed = intent.armed.contains(&d.key);
    }
    let mut report = receipt.0.borrow_mut();
    let r = report.as_mut().unwrap();
    r.progress = ArmingProgress::Completed(outcome);
    r.configuration = config.clone();
    r.recording.revision += 1;
    if publish {
        *fake.config.borrow_mut() = config.clone();
        fake.membership.set(r.recording);
    }
    config
}
#[gtk::test]
fn live_arm_widget_uses_original_graph_revisions_and_shared_admission() {
    let (owner, fake, _) = live();
    let origin = owner.origin().unwrap();
    // The existing exclusive Arm action now reaches the native arming adapter.
    button(owner.widget(), "Arm only").emit_clicked();
    let (intent, _) = fake.arms.borrow()[0].clone();
    assert!(Arc::ptr_eq(
        &intent.origin.value,
        &origin.0.graph.as_ref().unwrap().value
    ));
    assert_eq!(intent.origin.stamp, origin.0.graph.as_ref().unwrap().stamp);
    assert_eq!(
        Some(intent.expected_config),
        origin.0.config.as_ref().map(|c| c.revision)
    );
    assert_eq!(Some(intent.expected_recording), origin.0.membership);
    assert_eq!(Some(intent.root), origin.0.root);
    assert_eq!(intent.armed.len(), 1);
    assert!(owner.session.busy());
    assert!(fake.configs.borrow().is_empty());
    assert!(change(&owner, "rec-3", true).is_err());
    assert!(button(owner.widget(), "Stop all").is_sensitive());
    complete(&fake, 0, ArmingOutcome::AppliedLocally, true);
    owner.poll();
    assert!(!owner.session.busy());
    assert!(matches!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::AppliedLocally
    ));
    assert!(owner.view.awaited_request().is_none());
}
#[gtk::test]
fn arming_rejects_stale_membership_config_origin_and_reentrant_handoff() {
    let (owner, fake, driver) = live();
    let origin = owner.origin().unwrap();
    fake.membership.set(RecordingRevision {
        revision: 9,
        ..fake.membership.get()
    });
    assert!(change(&owner, "rec-2", true).is_err());
    owner.poll();
    fake.config.borrow_mut().revision.revision += 1;
    assert!(change(&owner, "rec-2", true).is_err());
    owner.poll();
    let g = graph();
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    let proposal = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .arm("rec-2", true)
        .unwrap();
    assert!(owner
        .configure(
            &origin,
            ui::Action::Arm {
                bus: "rec-2".into(),
                additive: true
            },
            Some(proposal)
        )
        .is_err());
    owner.poll();
    let weak = Rc::downgrade(&owner);
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        owner.poll();
        assert!(change(&owner, "rec-3", true).is_err());
        assert!(owner.session.busy());
    }));
    change(&owner, "rec-2", true).unwrap();
    assert_eq!(fake.arms.borrow().len(), 1);
}
#[gtk::test]
fn sibling_children_with_same_source_survive_opening_snapshot_omission_and_late_errors() {
    let (owner, fake, _) = live();
    fake.config
        .borrow_mut()
        .configuration
        .as_mut()
        .unwrap()
        .buses
        .states
        .entry("rec-3".into())
        .or_default()
        .armed = true;
    owner.poll();
    change(&owner, "rec-2", true).unwrap();
    let a = child(&fake, 0, "rec-2");
    let b = child(&fake, 0, "rec-3");
    assert_eq!(
        a.0.borrow().as_ref().unwrap().source,
        b.0.borrow().as_ref().unwrap().source
    );
    fake.visible.set(false);
    owner.poll();
    assert_eq!(
        owner.recording_status().len(),
        3,
        "siblings have no writer IDs yet; Source is not identity"
    );
    *b.0.borrow_mut() = None;
    owner.poll();
    assert_eq!(owner.recording_status().len(), 3);
    assert!(!owner.data.borrow().protocol_invalid);
    fake.started(1, 2);
    let source = a.0.borrow().as_ref().unwrap().source;
    *b.0.borrow_mut() = Some(TakeStatus {
        source,
        progress: RecordingProgress::Failed,
        writer: None,
        writer_outcome: None,
        failure: Some(RecordingError::InvalidInput),
        complete: true,
        take: None,
        directory: None,
        tracks: vec![],
    });
    fake.joined(0, None);
    fake.joined(1, Some(RecordingError::Busy));
    complete(&fake, 0, ArmingOutcome::AppliedLocally, true);
    owner.poll();
    let errors = owner.finalization_errors(false).unwrap();
    assert!(errors.iter().any(|e| e.contains("Busy")), "{errors:?}");
    assert!(
        errors.iter().any(|e| e.contains("InvalidInput")),
        "{errors:?}"
    );
    assert_eq!(owner.recording_status().len(), 3);
}
#[gtk::test]
fn partial_arming_retains_readback_barrier_children_and_diagnostics_through_recovery() {
    let (owner, fake, _) = live();
    let request = change(&owner, "rec-2", false).unwrap();
    child(&fake, 0, "rec-2");
    let completion = complete(&fake, 0, ArmingOutcome::PartiallyApplied, false);
    fake.arms.borrow()[0]
        .1
         .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .failure = Some(RecordingError::Busy);
    owner.poll();
    assert!(
        !owner.session.busy(),
        "local decision releases shared admission, not child writers"
    );
    assert!(change(&owner, "rec-3", true).is_err());
    fake.busy.set(true);
    owner.refresh();
    assert_eq!(
        owner.data.borrow().operation.as_ref().unwrap().request,
        request
    );
    fake.busy.set(false);
    owner.poll();
    assert!(
        owner.data.borrow().operation.is_some(),
        "old cached config cannot satisfy terminal evidence"
    );
    *fake.config.borrow_mut() = completion;
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert_eq!(owner.recording_status().len(), 2);
    assert!(owner
        .data
        .borrow()
        .past_failures
        .iter()
        .any(|e| e.contains("PartiallyApplied")));
}
#[gtk::test]
fn no_op_arming_is_not_file_completion_and_stable_stops_survive_disconnect() {
    let (owner, fake, driver) = live();
    // Capture old stable IDs; neither reconnect nor a new graph can retarget them.
    let stop_origin = owner.origin().unwrap();
    change(&owner, "rec-2", true).unwrap();
    let child = child(&fake, 0, "rec-2");
    let original = child.0.borrow().clone().unwrap();
    *child.0.borrow_mut() = None;
    owner.stop_all().unwrap();
    assert!(owner.data.borrow().all_stopped);
    *child.0.borrow_mut() = Some(original);
    complete(&fake, 0, ArmingOutcome::NoOpLocally, true);
    let mut g = graph();
    g.verbonden = false;
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    owner.poll();
    assert!(
        owner.data.borrow().all_stopped,
        "late arming children cannot undo a reserved Stop all"
    );
    assert_eq!(owner.recording_status().len(), 2);
    assert!(owner.recording_status().iter().all(|r| !r.complete));
    owner.stop_bus(&stop_origin, "rec-1").unwrap();
    assert!(change(&owner, "rec-3", true).is_err());
    assert_eq!(fake.stops.borrow().as_slice(), &[Stop::All]);
}
#[gtk::test]
fn invalid_arming_parent_and_child_identity_never_acknowledge_or_release_the_lease() {
    let (owner, fake, _) = live();
    change(&owner, "rec-2", true).unwrap();
    child(&fake, 0, "rec-2")
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .source
        .ticket += 1;
    complete(&fake, 0, ArmingOutcome::AppliedLocally, true);
    owner.poll();
    assert!(owner.data.borrow().protocol_invalid);
    assert!(owner.session.busy());
    assert!(matches!(
        owner.data.borrow().operation.as_ref().unwrap().progress,
        ui::Progress::Unconfirmed(_)
    ));
    owner.stop_all().unwrap();
    assert_eq!(fake.stops.borrow().as_slice(), &[Stop::All]);
    let (other, f, _) = live();
    change(&other, "rec-2", true).unwrap();
    complete(&f, 0, ArmingOutcome::AppliedLocally, true);
    f.arms.borrow()[0]
        .1
         .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .expected_recording
        .revision += 1;
    other.poll();
    assert!(other.session.busy());
    assert!(other.data.borrow().protocol_invalid);
}
#[gtk::test]
fn active_layout_opening_finalization_missing_directory_and_quit_remain_gated() {
    let (owner, fake, _) = live();
    let origin = owner.origin().unwrap();
    let config = &origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses;
    assert!(owner
        .configure(&origin, ui::Action::AddStereo, Some(config.add().unwrap()))
        .is_err());
    owner.set_recording_root(None).unwrap();
    assert!(change(&owner, "rec-2", true).is_err());
    owner.set_recording_root(origin.0.root.clone()).unwrap();
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .progress = RecordingProgress::Opening;
    owner.poll();
    assert!(change(&owner, "rec-2", true).is_err());
    fake.config.borrow_mut().recording = false;
    fake.takes.borrow()[0]
        .0
        .borrow_mut()
        .as_mut()
        .unwrap()
        .progress = RecordingProgress::Finalizing;
    owner.poll();
    assert!(change(&owner, "rec-2", true).is_err());
    fake.started(0, 1);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    change(&owner, "rec-2", true).unwrap();
    owner.session.close_admission();
    assert!(change(&owner, "rec-3", true).is_err());
    owner.stop_all().unwrap();
    assert!(fake.arms.borrow().len() == 1);
}
