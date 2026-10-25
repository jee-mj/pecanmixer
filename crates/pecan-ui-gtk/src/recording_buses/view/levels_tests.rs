//! Synthetic typed callbacks only; no backend, filesystem, audio or Undo owner.
use super::super::tests::{frame, View};
use super::*;
type Events = Rc<RefCell<Vec<intent::Event<Rc<()>, u64>>>>;
fn snapshot() -> Snapshot<Rc<()>, u64, u64> {
    let mut s = frame();
    s.capabilities.idle_levels = Support::Available;
    s.has_bus_routes = true;
    s
}
fn row(view: &View, key: &str) -> Rc<LevelRow<Rc<()>, u64, u64>> {
    view.0.level_rows.borrow()[key].clone()
}
fn action(row: &LevelRow<Rc<()>, u64, u64>, text: &str) -> gtk::Button {
    let mut w = row.actions.first_child();
    while let Some(current) = w {
        if let Some(b) = current.downcast_ref::<gtk::Button>() {
            if b.label().as_deref() == Some(text) {
                return b.clone();
            }
        }
        w = current.next_sibling();
    }
    panic!("missing synthetic level action {text}")
}
fn setup() -> (View, Events) {
    let v = View::new(Budget::default());
    let log: Events = Rc::new(RefCell::default());
    let events = log.clone();
    let next = Cell::new(41);
    v.bind_levels(move |e| {
        let tracked = matches!(e, intent::Event::Begin { .. } | intent::Event::Set(_));
        events.borrow_mut().push(e);
        if tracked {
            next.set(next.get() + 1);
            Ok(intent::Reply::Tracked(next.get()))
        } else {
            Ok(intent::Reply::Accepted)
        }
    });
    v.observe(snapshot()).unwrap();
    (v, log)
}
fn report(v: &View, request: u64, progress: Progress, configuration: Option<BusConfig>) {
    let mut s = snapshot();
    s.operation = Some(Operation { request, progress });
    if let Some(c) = configuration {
        s.configuration = Some(c);
    }
    v.observe(s).unwrap();
}
#[gtk::test]
fn levels_are_independent_of_structural_routes_but_default_and_active_capabilities_stay_closed() {
    let v = View::new(Budget::default());
    v.observe(snapshot()).unwrap();
    assert!(!action(&row(&v, "rec-1-2"), "Set gain").is_sensitive());
    let (v, _) = setup();
    let mut s = snapshot();
    s.capabilities.idle_configuration = Support::Pending(Dependency::IdleConfigurationAdapter);
    v.observe(s).unwrap();
    assert!(action(&row(&v, "rec-1-2"), "Set gain").is_sensitive());
    for activity in [Activity::Recording, Activity::Finalizing, Activity::Unknown] {
        let mut s = snapshot();
        s.activity = activity;
        s.capabilities.active_layout = Support::Available;
        s.capabilities.active_arming = Support::Available;
        v.observe(s).unwrap();
        assert!(!action(&row(&v, "rec-1-2"), "Mute bus").is_sensitive());
    }
    let bad = View::new(Budget::default());
    bad.bind_levels(|_| Ok(intent::Reply::Accepted));
    bad.observe(snapshot()).unwrap();
    action(&row(&bad, "rec-1-2"), "Mute bus").emit_clicked();
    assert!(
        bad.configuration_pending(),
        "unknown admission must also gate companion surfaces"
    );
    bad.observe(snapshot()).unwrap();
    assert!(bad.configuration_pending());
    assert!(bad.levels_reconciled().is_err());
}
#[gtk::test]
fn explicit_gain_and_mute_keep_actual_metadata_and_exact_pending_identity_until_terminal() {
    let (v, events) = setup();
    let r = row(&v, "rec-4");
    r.draft.set_value(-18.);
    let old = action(&r, "Set gain");
    old.emit_clicked();
    assert_eq!(v.awaited_request(), Some(42));
    let proposed = match &events.borrow()[0] {
        intent::Event::Set(i) => {
            assert_eq!(i.change, intent::Change::Db(-18.));
            assert!(i.proposed_configuration.state("rec-4").mute);
            assert!(!i.proposed_configuration.state("rec-4").armed);
            i.proposed_configuration.clone()
        }
        _ => panic!(),
    };
    assert_eq!(
        v.inspect_model(|m| m
            .snapshot()
            .unwrap()
            .configuration
            .as_ref()
            .unwrap()
            .state("rec-4")
            .db)
            .unwrap(),
        -12.
    );
    for p in [Progress::Queued, Progress::Applying] {
        report(&v, 42, p, None);
        assert_eq!(v.awaited_request(), Some(42));
    }
    v.observe(snapshot()).unwrap();
    report(&v, 999, Progress::AppliedLocally, None);
    assert_eq!(v.awaited_request(), Some(42));
    old.emit_clicked();
    assert_eq!(events.borrow().len(), 1);
    report(&v, 42, Progress::AppliedLocally, Some(proposed.clone()));
    assert!(v.awaited_request().is_none());
    assert!(Rc::ptr_eq(&r, &row(&v, "rec-4")));
    action(&r, "Unmute bus").emit_clicked();
    let next = match &events.borrow()[1] {
        intent::Event::Set(i) => {
            assert_eq!(i.change, intent::Change::Mute(false));
            assert_eq!(i.proposed_configuration.state("rec-4").db, -18.);
            i.proposed_configuration.clone()
        }
        _ => panic!(),
    };
    report(
        &v,
        43,
        Progress::PartiallyApplied("synthetic partial".into()),
        Some(next),
    );
    v.observe(snapshot()).unwrap();
    assert!(!action(&r, "Unmute bus").is_sensitive());
    assert!(v.configuration_pending());
    report(&v, 999, Progress::Queued, None);
    assert!(v.levels_reconciled().is_err());
    v.observe(snapshot()).unwrap();
    assert!(v.configuration_pending());
    assert!(v.levels_reconciled().is_ok());
    assert!(!v.configuration_pending());
    assert!(action(&r, "Unmute bus").is_sensitive());
}
#[gtk::test]
fn relative_drag_keeps_original_capture_through_refresh_storm_and_flushes_one_end() {
    let (v, events) = setup();
    let r = row(&v, "rec-1-2");
    let origin = v
        .inspect_model(|m| m.snapshot().unwrap().origin.clone())
        .unwrap();
    r.pointer_down(100.);
    r.pointer_end(false);
    assert!(events.borrow().is_empty());
    r.pointer_down(100.);
    v.observe(snapshot()).unwrap(); // retain the press's original origin
    for i in 1..=400 {
        r.pointer_move(100. - (i as f64) / 100., i % 2 == 0);
    }
    assert!(
        matches!(&events.borrow()[0],intent::Event::Begin{capture,kind:fader::Kind::Pointer} if Rc::ptr_eq(&capture.origin,&origin))
    );
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, intent::Event::Begin { .. }))
            .count(),
        1
    );
    let before = r.policy.borrow().position();
    report(&v, 42, Progress::Applying, None);
    assert_eq!(r.policy.borrow().position(), before);
    r.pointer_move(90., false);
    r.pointer_end(false);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, intent::Event::End { request: 42, .. }))
            .count(),
        1
    );
    let last = events
        .borrow()
        .iter()
        .rev()
        .find_map(|e| {
            if let intent::Event::End { db, .. } = e {
                Some(*db)
            } else {
                None
            }
        })
        .unwrap();
    let c = snapshot()
        .configuration
        .unwrap()
        .with_db("rec-1-2", last)
        .unwrap();
    report(&v, 42, Progress::AppliedLocally, Some(c));
    let n = events.borrow().len();
    r.pointer_move(50., false);
    r.pointer_end(false);
    assert_eq!(events.borrow().len(), n);
}
#[gtk::test]
fn wheel_uses_observed_db_and_one_clock_end_while_reset_and_rebinding_reject_old_callbacks() {
    let (v, events) = setup();
    let r = row(&v, "rec-1-2");
    for _ in 0..100 {
        r.wheel(-1., false);
    }
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, intent::Event::Begin { .. }))
            .count(),
        1
    );
    assert!(events
        .borrow()
        .iter()
        .filter_map(|e| if let intent::Event::Update { db, .. } = e {
            Some(*db)
        } else {
            None
        })
        .all(|db| db == 1.));
    v.tick_levels(Duration::from_millis(599));
    assert!(!events
        .borrow()
        .iter()
        .any(|e| matches!(e, intent::Event::End { .. })));
    v.tick_levels(Duration::from_millis(600));
    assert!(matches!(
        events.borrow().last(),
        Some(intent::Event::End {
            request: 42,
            reason: fader::End::WheelIdle,
            ..
        })
    ));
    report(&v, 42, Progress::AppliedLocally, None);
    r.reset();
    assert!(matches!(
        &events.borrow()[events.borrow().len() - 3],
        intent::Event::Begin {
            kind: fader::Kind::Reset,
            ..
        }
    ));
    assert!(matches!(
        events.borrow().last(),
        Some(intent::Event::End {
            request: 43,
            db: 0.,
            reason: fader::End::Reset
        })
    ));
    report(
        &v,
        43,
        Progress::Rejected("synthetic no admission".into()),
        None,
    );
    let old = action(&r, "Mute bus");
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    v.bind_levels(move |_| {
        seen.set(seen.get() + 1);
        Ok(intent::Reply::Tracked(99))
    });
    old.emit_clicked();
    assert_eq!(count.get(), 0);
}
#[gtk::test]
fn end_failure_abandons_exact_owner_request_and_disconnect_never_synthesizes_completion() {
    let v = View::new(Budget::default());
    let log: Events = Rc::new(RefCell::default());
    let seen = log.clone();
    v.bind_levels(move |e| {
        let begin = matches!(e, intent::Event::Begin { .. });
        let end = matches!(e, intent::Event::End { .. });
        seen.borrow_mut().push(e);
        if end {
            Err("synthetic end refusal".into())
        } else if begin {
            Ok(intent::Reply::Tracked(55))
        } else {
            Ok(intent::Reply::Accepted)
        }
    });
    v.observe(snapshot()).unwrap();
    let r = row(&v, "rec-1-2");
    r.pointer_down(100.);
    r.pointer_move(90., false);
    r.pointer_end(false);
    assert!(matches!(
        log.borrow().last(),
        Some(intent::Event::Abandon { request: 55 })
    ));
    assert_eq!(v.awaited_request(), Some(55));
    let mut disconnected = snapshot();
    disconnected.connected = false;
    v.observe(disconnected).unwrap();
    assert_eq!(v.awaited_request(), Some(55));
    let n = log.borrow().len();
    r.pointer_move(20., false);
    assert_eq!(log.borrow().len(), n);
    report(&v, 55, Progress::Rejected("closed".into()), None);
    assert!(v.awaited_request().is_none());
    let (v, events) = setup();
    let r = row(&v, "rec-1-2");
    r.pointer_down(100.);
    r.pointer_move(90., false);
    let mut changed = snapshot();
    changed.configuration = Some(changed.configuration.unwrap().add().unwrap());
    v.observe(changed).unwrap();
    assert!(matches!(
        events.borrow().last(),
        Some(intent::Event::Abandon { request: 42 })
    ));
    assert_eq!(v.awaited_request(), Some(42));
    let n = events.borrow().len();
    r.pointer_move(80., false);
    r.pointer_end(false);
    assert_eq!(events.borrow().len(), n);
}
#[gtk::test]
fn reentrant_publication_keeps_request_and_drop_abandons_an_open_group_once() {
    let v = View::new(Budget::default());
    let weak = Rc::downgrade(&v.0);
    let log: Events = Rc::new(RefCell::default());
    let seen = log.clone();
    v.bind_levels(move |e| {
        let begin = matches!(e, intent::Event::Begin { .. });
        seen.borrow_mut().push(e);
        if begin {
            let v = BusControls(weak.upgrade().unwrap());
            report(&v, 71, Progress::Queued, None);
            Ok(intent::Reply::Tracked(71))
        } else {
            Ok(intent::Reply::Accepted)
        }
    });
    v.observe(snapshot()).unwrap();
    let r = row(&v, "rec-1-2");
    r.pointer_down(100.);
    r.pointer_move(90., false);
    assert_eq!(v.awaited_request(), Some(71));
    assert!(matches!(
        log.borrow().last(),
        Some(intent::Event::Update { request: 71, .. })
    ));
    drop(v);
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| matches!(e, intent::Event::Abandon { request: 71 }))
            .count(),
        1
    );
}

#[gtk::test]
fn older_end_return_cannot_abandon_a_fresh_reentrant_request() {
    let v = View::new(Budget::default());
    let weak = Rc::downgrade(&v.0);
    let log: Events = Rc::new(RefCell::default());
    let seen = log.clone();
    v.bind_levels(move |e| {
        let begin = matches!(e, intent::Event::Begin { .. });
        let end = matches!(e, intent::Event::End { .. });
        let set = matches!(e, intent::Event::Set(_));
        seen.borrow_mut().push(e);
        if begin {
            Ok(intent::Reply::Tracked(81))
        } else if set {
            Ok(intent::Reply::Tracked(82))
        } else if end {
            let v = BusControls(weak.upgrade().unwrap());
            report(&v, 81, Progress::AppliedLocally, None);
            action(&row(&v, "rec-1-2"), "Mute bus").emit_clicked();
            Err("older synthetic end return".into())
        } else {
            Ok(intent::Reply::Accepted)
        }
    });
    v.observe(snapshot()).unwrap();
    let r = row(&v, "rec-1-2");
    r.pointer_down(100.);
    r.pointer_move(90., false);
    r.pointer_end(false);
    assert_eq!(v.awaited_request(), Some(82));
    assert!(!log
        .borrow()
        .iter()
        .any(|e| matches!(e, intent::Event::Abandon { request: 82 })));
    report(
        &v,
        82,
        Progress::Rejected("fresh synthetic outcome".into()),
        None,
    );
    assert!(v.awaited_request().is_none());
}
