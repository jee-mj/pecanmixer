//! Queued ownership is activity even before the filter's recording flag rises.
//! All service/owner handoffs are injected; no writer or audio session starts.
use super::*;

fn queued_at_service(owner: &BusOwner, fake: &Fake) {
    let origin = owner.origin().unwrap();
    fake.admit(
        RecordingStart {
            ticket: 99,
            origin: origin.0.graph.clone().unwrap(),
            expected_config: origin.0.config.as_ref().unwrap().revision,
            root: std::env::temp_dir().join("pecan-synthetic-queued-restore-root"),
        },
        None,
        None,
    )
    .unwrap();
    assert!(!fake.config.borrow().recording);
}
fn stays_skipped(owner: &BusOwner, fake: &Fake) {
    for _ in 0..4 {
        owner.poll();
        owner.restore_preferences(&saved(vec![2, 1]));
    }
    assert!(
        fake.saved_attempts.borrow().is_empty(),
        "Observed/admitted queued ownership must skip this epoch after joining"
    );
    assert!(!owner.session.busy());
}

#[gtk::test]
fn saved_recording_skip_service_preflight_queued_then_joined_before_owner_tick() {
    let (owner, fake, _) = setup_saved(false);
    queued_at_service(&owner, &fake);
    owner.restore_preferences(&saved(vec![2, 1]));
    assert!(fake.saved_attempts.borrow().is_empty());
    fake.joined(0, None);
    stays_skipped(&owner, &fake);
}

#[gtk::test]
fn saved_recording_skip_owner_start_and_stop_join_before_first_tick() {
    let (owner, fake, _) = setup_saved(false);
    owner.start(&owner.origin().unwrap(), None).unwrap();
    owner.stop_all().unwrap();
    fake.joined(0, None);
    stays_skipped(&owner, &fake);
}

#[gtk::test]
fn saved_recording_skip_preflight_reader_reentry_start_stop_and_join_before_return() {
    let (owner, fake, _) = setup_saved(false);
    let weak = Rc::downgrade(&owner);
    let endpoint = fake.clone();
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        let o = weak.upgrade().unwrap();
        o.start(&o.origin().unwrap(), None).unwrap();
        o.stop_all().unwrap();
        endpoint.joined(0, None);
        o.poll();
    }));
    owner.restore_preferences(&saved(vec![2, 1]));
    assert!(fake.saved_attempts.borrow().is_empty());
    stays_skipped(&owner, &fake);
}

#[gtk::test]
fn saved_recording_skip_control_earlier_poll_of_queued_ownership_already_skips() {
    let (owner, fake, _) = setup_saved(false);
    queued_at_service(&owner, &fake);
    owner.poll();
    fake.joined(0, None);
    stays_skipped(&owner, &fake);
}

#[gtk::test]
fn saved_recording_skip_completed_old_epoch_does_not_poison_idle_reconnect() {
    let (owner, fake, driver) = setup_saved(false);
    let original = fake.config.borrow().revision;
    owner.start(&owner.origin().unwrap(), None).unwrap();
    owner.stop_all().unwrap();
    fake.joined(0, None);
    owner.poll();
    reconnect::reconnect(&owner, &fake, &driver);
    owner.restore_preferences(&saved(vec![2, 1]));
    let attempts = fake.saved_attempts.borrow();
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].expected_config.epoch > original.epoch);
    assert_eq!(attempts[0].expected_config.owner, original.owner);
}
