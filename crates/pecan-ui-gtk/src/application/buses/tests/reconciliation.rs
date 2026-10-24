//! Busy inspection must preserve the terminal configuration reconciliation fence.
use super::*;

fn assert_fenced(owner: &BusOwner, fake: &Fake, request: &Request) {
    assert!(
        !button(owner.widget(), "Record all armed").is_sensitive(),
        "cached configuration must not reenable recording during reconciliation"
    );
    assert_eq!(
        owner.data.borrow().operation.as_ref().map(|w| &w.request),
        Some(request),
        "retain the original operation and completion evidence"
    );
    assert!(owner.configuration_report().is_some());
    let origin = owner.origin().unwrap();
    let proposal = origin
        .0
        .config
        .as_ref()
        .and_then(|c| c.configuration.as_ref())
        .map(|c| c.buses.arm("rec-1-2", true).unwrap());
    assert!(owner
        .configure(
            &origin,
            ui::Action::Arm {
                bus: "rec-1-2".into(),
                additive: true,
            },
            proposal,
        )
        .is_err());
    assert!(owner.start(&origin, None).is_err());
    assert!(owner.start(&origin, Some("rec-1-2".into())).is_err());
    assert_eq!(fake.configs.borrow().len(), 1);
    assert!(fake.starts.borrow().is_empty());
    assert!(owner.view.awaited_request().is_none());
    assert!(button(owner.widget(), "Stop all").is_sensitive());
}

fn busy_refresh_after(outcome: LocalConfigOutcome, publish: bool) {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(0, outcome, publish);
    owner.poll();
    fake.busy.set(true);
    owner.refresh();
    assert_fenced(&owner, &fake, &request);
    for _ in 0..3 {
        owner.refresh();
        owner.poll();
        assert_fenced(&owner, &fake, &request);
    }
}

#[gtk::test]
fn refresh_barrier_applied_completion_survives_busy_inspection() {
    busy_refresh_after(LocalConfigOutcome::Applied, false);
}

#[gtk::test]
fn refresh_barrier_partial_completion_survives_busy_inspection_of_old_metadata() {
    busy_refresh_after(
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed),
        false,
    );
}

#[gtk::test]
fn refresh_barrier_partial_completion_survives_busy_inspection_of_unknown_metadata() {
    busy_refresh_after(
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed),
        true,
    );
}

#[gtk::test]
fn refresh_barrier_unconfirmed_completion_survives_busy_inspection() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.configs.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .progress = ConfigProgress::Unconfirmed;
    owner.poll();
    fake.busy.set(true);
    owner.refresh();
    assert_fenced(&owner, &fake, &request);
}

fn completion(owner: &BusOwner) -> ConfigSnapshot {
    owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .completion
        .clone()
        .unwrap()
}

#[gtk::test]
fn reconciliation_success_requires_a_new_inspection_but_allows_ordinary_graph_revisions() {
    let (owner, fake, driver) = setup();
    arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    let request = owner
        .data
        .borrow()
        .operation
        .as_ref()
        .unwrap()
        .request
        .clone();
    assert!(button(owner.widget(), "Record all armed").is_sensitive());
    fake.busy.set(true);
    owner.refresh();
    assert_fenced(&owner, &fake, &request);
    let retired = owner.origin().unwrap();
    let g = graph();
    driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    fake.busy.set(false);
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(button(owner.widget(), "Record all armed").is_sensitive());
    assert!(owner.start(&retired, None).is_err());
    owner.start(&owner.origin().unwrap(), None).unwrap();
    assert_eq!(fake.starts.borrow().len(), 1);
}

#[gtk::test]
fn reconciliation_older_successful_read_cannot_satisfy_a_newer_completion() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, false);
    owner.poll();
    let completed = completion(&owner);
    fake.busy.set(true);
    owner.refresh();
    fake.busy.set(false);
    owner.poll();
    assert_fenced(&owner, &fake, &request);
    *fake.config.borrow_mut() = completed;
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    owner.start(&owner.origin().unwrap(), None).unwrap();
}

#[gtk::test]
fn reconciliation_partial_unknown_metadata_allows_only_explicit_default_recovery() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(
        0,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ResizeFailed),
        false,
    );
    owner.poll();
    let completed = completion(&owner);
    fake.busy.set(true);
    owner.refresh();
    fake.busy.set(false);
    owner.poll();
    assert_fenced(&owner, &fake, &request);
    *fake.config.borrow_mut() = completed.clone();
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(!button(owner.widget(), "Record all armed").is_sensitive());
    assert!(button(owner.widget(), "Initialize native bus defaults").is_sensitive());
    let origin = owner.origin().unwrap();
    assert!(origin.0.config.as_ref().unwrap().configuration.is_none());
    assert!(owner.start(&origin, None).is_err());
    owner.initialize(&origin).unwrap();
    assert_eq!(fake.configs.borrow().len(), 2);
    assert_eq!(fake.configs.borrow()[1].0.expected, completed.revision);
}

#[gtk::test]
fn reconciliation_unconfirmed_fresh_read_can_preserve_the_same_revision() {
    let (owner, fake, _) = setup();
    arm(&owner);
    let revision = fake.config.borrow().revision;
    fake.configs.borrow()[0]
        .1
        .report
        .borrow_mut()
        .as_mut()
        .unwrap()
        .progress = ConfigProgress::Unconfirmed;
    owner.poll();
    fake.busy.set(true);
    owner.refresh();
    fake.busy.set(false);
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert_eq!(fake.config.borrow().revision, revision);
    assert_eq!(fake.configs.borrow().len(), 1);
    assert!(fake.starts.borrow().is_empty());
    owner.start(&owner.origin().unwrap(), None).unwrap();
}

#[gtk::test]
fn reconciliation_epoch_mismatch_does_not_authorize_cached_controls() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    let epoch = fake.config.borrow().revision.epoch;
    fake.busy.set(true);
    owner.refresh();
    fake.config.borrow_mut().revision.epoch += 1;
    fake.busy.set(false);
    owner.poll();
    assert_fenced(&owner, &fake, &request);
    fake.config.borrow_mut().revision.epoch = epoch;
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
}

#[gtk::test]
fn reconciliation_mismatched_inspection_owner_retains_the_original_receipt() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    fake.busy.set(true);
    owner.refresh();
    fake.config.borrow_mut().revision.owner += 1;
    fake.busy.set(false);
    owner.poll();
    assert!(owner.data.borrow().protocol_invalid);
    assert_fenced(&owner, &fake, &request);
}

#[gtk::test]
fn reconciliation_reentrant_refresh_does_not_consume_the_preceding_inspection() {
    let (owner, fake, _) = setup();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    let weak = Rc::downgrade(&owner);
    *fake.configs.borrow()[0].1.hook.borrow_mut() = Some(Box::new(move || {
        weak.upgrade().unwrap().refresh();
    }));
    owner.poll();
    assert_fenced(&owner, &fake, &request);
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(button(owner.widget(), "Record all armed").is_sensitive());
}

#[gtk::test]
fn reconciliation_disconnect_keeps_evidence_until_authenticated_new_epoch() {
    let (owner, fake, driver) = setup();
    let request = arm(&owner);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    owner.poll();
    fake.busy.set(true);
    owner.refresh();
    let mut disconnected = graph();
    disconnected.verbonden = false;
    driver.publish(disconnected, BTreeMap::new());
    owner.session.poll();
    fake.config.borrow_mut().connected = false;
    fake.busy.set(false);
    owner.poll();
    assert_fenced(&owner, &fake, &request);
    owner.stop_all().unwrap();
    let g = graph();
    let next = driver.publish(g.clone(), generations(&g));
    owner.session.poll();
    fake.config.borrow_mut().connected = true;
    fake.config.borrow_mut().revision.epoch = next.stamp.epoch;
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(fake.starts.borrow().is_empty());
    assert_eq!(*fake.stops.borrow(), vec![Stop::All]);
}
