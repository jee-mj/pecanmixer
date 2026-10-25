//! Authenticated synthetic frames and injected config receipts only.
use super::*;
use crate::recording_buses::levels::{Capture, Change, Event, Reply};

pub(super) fn setup_levels() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver) {
    let (owner, fake, driver) = setup();
    let mut native = (*owner.session.graph().unwrap().value).clone();
    native.knopen.iter_mut().find(|n| n.id == 30).unwrap().naam =
        pecan_core::domain::audio::REC_NODE.into();
    driver.publish(native.clone(), generations(&native));
    owner.session.poll();
    fake.levels_enabled.set(true);
    owner.refresh();
    assert_eq!(owner.level_support(), ui::Support::Available);
    (owner, fake, driver)
}
pub(super) fn capture(owner: &BusOwner, key: &str) -> Capture<Origin> {
    let origin = owner.origin().unwrap();
    let configuration = origin
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .clone();
    Capture {
        origin,
        bus: configuration
            .definitions()
            .unwrap()
            .into_iter()
            .find(|b| b.key == key)
            .unwrap(),
        configuration,
    }
}
fn begin(owner: &BusOwner) -> Request {
    match owner
        .level_event(Event::Begin {
            capture: capture(owner, "rec-1-2"),
            kind: crate::controls::fader::Kind::Pointer,
        })
        .unwrap()
    {
        Reply::Tracked(r) => r,
        _ => panic!("tracked"),
    }
}
fn update(owner: &BusOwner, request: &Request, db: f64) {
    assert!(matches!(
        owner
            .level_event(Event::Update {
                request: request.clone(),
                db
            })
            .unwrap(),
        Reply::Accepted
    ));
}
fn end(owner: &BusOwner, request: &Request, db: f64) {
    assert!(matches!(
        owner
            .level_event(Event::End {
                request: request.clone(),
                db,
                reason: crate::controls::fader::End::Released
            })
            .unwrap(),
        Reply::Accepted
    ));
}
pub(super) fn complete(
    fake: &Fake,
    index: usize,
    outcome: LocalConfigOutcome,
    publish: bool,
) -> ConfigReport {
    let (_, intent, receipt) = fake.gains.borrow()[index].clone();
    let mut snapshot = fake.config.borrow().clone();
    match &outcome {
        LocalConfigOutcome::Applied | LocalConfigOutcome::NoOp => {
            if outcome == LocalConfigOutcome::Applied {
                snapshot.revision.revision += 1;
            }
            snapshot.configuration = Some(intent.configuration.clone());
            let pecan_core::graph::Opdracht::ZetBus { gain, .. } =
                intent.configuration.buses.backend_config().unwrap()
            else {
                panic!()
            };
            snapshot.local.as_mut().unwrap().gains = gain;
        }
        LocalConfigOutcome::PartiallyApplied(_) => {
            snapshot.revision.revision += 1;
            snapshot.configuration = None;
        }
        LocalConfigOutcome::Rejected(_) => {}
    }
    if publish {
        *fake.config.borrow_mut() = snapshot.clone();
    }
    let report = ConfigReport {
        ticket: intent.ticket,
        expected: intent.expected,
        progress: ConfigProgress::Completed(ConfigCompletion {
            outcome,
            snapshot,
            resize: None,
            cancellation_requested: false,
        }),
    };
    *receipt.report.borrow_mut() = Some(report.clone());
    report
}
fn polls(owner: &BusOwner, n: usize) {
    for _ in 0..n {
        owner.poll();
    }
}
fn spin(root: &impl IsA<gtk::Widget>) -> gtk::SpinButton {
    let mut pending = vec![root.as_ref().clone()];
    while let Some(w) = pending.pop() {
        if let Some(spin) = w.downcast_ref::<gtk::SpinButton>() {
            return spin.clone();
        }
        let mut child = w.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            pending.push(w);
        }
    }
    panic!("gain proposal")
}

#[gtk::test]
fn levels_owner_real_buttons_preserve_muted_db_and_no_graph_undo() {
    let (owner, fake, _) = setup_levels();
    let widget = owner.widget();
    spin(widget).set_value(-12.);
    button(widget, "Set gain").emit_clicked();
    assert_eq!(fake.gains.borrow().len(), 1);
    assert!(owner.session.busy());
    assert!(owner.view.awaited_request().is_some());
    assert_eq!(
        owner
            .origin()
            .unwrap()
            .0
            .config
            .as_ref()
            .unwrap()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .state("rec-1-2")
            .db,
        0.
    );
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    owner.poll();
    assert!(
        owner.session.busy(),
        "terminal receipt needs a later inspection"
    );
    owner.poll();
    assert!(!owner.session.busy());
    button(widget, "Mute bus").emit_clicked();
    assert_eq!(fake.gains.borrow().len(), 2);
    assert_eq!(
        fake.gains.borrow()[1]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -12.
    );
    complete(&fake, 1, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    spin(widget).set_value(-18.);
    button(widget, "Set gain").emit_clicked();
    assert!(
        fake.gains.borrow()[2]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .mute
    );
    complete(&fake, 2, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    button(widget, "Unmute bus").emit_clicked();
    assert_eq!(
        fake.gains.borrow()[3]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -18.
    );
    complete(&fake, 3, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert_eq!(owner.session.history_lengths(), (0, 0));
    assert!(fake.configs.borrow().is_empty());
}

#[gtk::test]
fn levels_owner_storm_coalesces_original_witness_final_flush_and_shared_tickets() {
    let (owner, fake, driver) = setup_levels();
    let frame = owner.session.witnessed_graph().unwrap();
    let request = begin(&owner);
    assert!(fake.gains.borrow().is_empty());
    update(&owner, &request, -1.);
    for n in 0..1000 {
        update(&owner, &request, -2. - (n % 30) as f64);
    }
    assert_eq!(fake.gains.borrow().len(), 1);
    let mut next = (*owner.session.graph().unwrap().value).clone();
    next.knopen[0].beschrijving = Some("Synthetic ordinary update".into());
    driver.publish(next.clone(), generations(&next));
    owner.session.poll();
    owner.poll();
    end(&owner, &request, -18.);
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    owner.poll();
    assert_eq!(fake.gains.borrow().len(), 1);
    owner.poll();
    assert_eq!(fake.gains.borrow().len(), 2);
    assert_eq!(
        fake.gains.borrow()[1]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -18.
    );
    assert_eq!(fake.gains.borrow()[0].1.ticket, 1);
    assert_eq!(fake.gains.borrow()[1].1.ticket, 2);
    for (captured, _, _) in fake.gains.borrow().iter() {
        assert!(Arc::ptr_eq(&captured.graph().value, &frame.graph().value));
        assert_eq!(captured.graph().stamp, frame.graph().stamp);
    }
    complete(&fake, 1, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert_eq!(owner.session.history_lengths(), (0, 0));
    let graph = owner.session.graph().unwrap();
    owner
        .session
        .execute(
            &graph,
            "Synthetic graph action",
            GraphAction::Mute {
                node: 20,
                muted: true,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(driver.take_requests().pop().unwrap().intent.ticket, 3);
}

#[gtk::test]
fn levels_owner_active_gain_does_not_enter_structural_or_recording_paths() {
    let (owner, fake, _) = setup_levels();
    owner.start(&owner.origin().unwrap(), None).unwrap();
    fake.started(0, 41);
    fake.config.borrow_mut().recording = true;
    owner.poll();
    assert!(button(owner.widget(), "Set gain").is_sensitive());
    spin(owner.widget()).set_value(-6.);
    button(owner.widget(), "Set gain").emit_clicked();
    assert_eq!(fake.gains.borrow().len(), 1);
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert_eq!(fake.starts.borrow().len(), 1);
    assert!(fake.stops.borrow().is_empty());
    assert!(fake.arms.borrow().is_empty());
    assert!(fake.migrations.borrow().is_empty());
    assert!(fake.configs.borrow().is_empty());
    assert_eq!(owner.session.history_lengths(), (0, 0));
}

#[gtk::test]
fn levels_owner_noop_and_rejection_wait_through_busy_post_report_inspection() {
    for outcome in [
        LocalConfigOutcome::NoOp,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
    ] {
        let (owner, fake, _) = setup_levels();
        spin(owner.widget()).set_value(0.);
        button(owner.widget(), "Set gain").emit_clicked();
        let request = owner.view.awaited_request().unwrap();
        complete(&fake, 0, outcome, true);
        owner.poll();
        assert!(owner.session.busy());
        fake.busy.set(true);
        polls(&owner, 3);
        assert!(owner.session.busy());
        assert_eq!(owner.view.awaited_request(), Some(request));
        fake.busy.set(false);
        owner.poll();
        assert!(!owner.session.busy());
        assert!(owner.view.awaited_request().is_none());
    }
}

fn set(owner: &BusOwner, change: Change) -> Result<Reply<Request>, String> {
    owner.level_event(Event::Set(
        capture(owner, "rec-1-2").propose(change).unwrap(),
    ))
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

#[gtk::test]
fn levels_owner_retained_capture_accepts_ordinary_revision_but_not_replacement() {
    for replace in [false, true] {
        let (owner, fake, driver) = setup_levels();
        let old = capture(&owner, "rec-1-2");
        let mut graph = (*owner.session.graph().unwrap().value).clone();
        graph.knopen[0].beschrijving = Some("Synthetic unrelated revision".into());
        let mut ids = generations(&graph);
        if replace {
            *ids.get_mut(&30).unwrap() += 1000;
        }
        driver.publish(graph, ids);
        owner.session.poll();
        owner.poll();
        let result = owner.level_event(Event::Set(old.propose(Change::Db(-6.)).unwrap()));
        assert_eq!(result.is_ok(), !replace);
        assert_eq!(fake.gains.borrow().len(), usize::from(!replace));
        assert_eq!(owner.session.busy(), !replace);
    }
    let (a, _, _) = setup_levels();
    let (b, fake, _) = setup_levels();
    assert!(b
        .level_event(Event::Set(
            capture(&a, "rec-1-2").propose(Change::Mute(true)).unwrap()
        ))
        .is_err());
    assert!(fake.gains.borrow().is_empty());
}

#[gtk::test]
fn levels_owner_replacement_retains_receipt_cancels_final_flush_and_requires_recovery() {
    let (owner, fake, driver) = setup_levels();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    end(&owner, &request, -18.);
    let graph = (*owner.session.graph().unwrap().value).clone();
    let mut ids = generations(&graph);
    *ids.get_mut(&30).unwrap() += 1000;
    driver.publish(graph, ids);
    owner.session.poll();
    owner.poll();
    assert!(owner.session.busy());
    complete(
        &fake,
        0,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
        true,
    );
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert!(
        matches!(progress(&owner), ui::Progress::Unconfirmed(_)),
        "late rejection cannot erase identity-loss evidence"
    );
    assert_eq!(fake.gains.borrow().len(), 1);
    assert!(set(&owner, Change::Mute(true)).is_err());
    owner.refresh();
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(set(&owner, Change::Mute(true)).is_ok());
    assert_eq!(fake.gains.borrow().len(), 2);
}

#[gtk::test]
fn levels_owner_busy_retry_uses_same_ticket_coalesces_and_does_not_retry_per_input() {
    let (owner, fake, _) = setup_levels();
    fake.gain_failures.borrow_mut().extend([
        Error::Configuration(ConfigError::Busy),
        Error::Configuration(ConfigError::QueueFull),
    ]);
    let request = begin(&owner);
    update(&owner, &request, -1.);
    for n in 0..1000 {
        update(&owner, &request, -2. - (n % 20) as f64);
    }
    end(&owner, &request, -12.);
    assert_eq!(fake.gain_attempts.borrow().len(), 1);
    owner.poll();
    assert_eq!(fake.gain_attempts.borrow().len(), 2);
    owner.poll();
    assert_eq!(fake.gain_attempts.borrow().len(), 3);
    assert_eq!(fake.gains.borrow().len(), 1);
    assert!(fake.gain_attempts.borrow().iter().all(|i| i.ticket == 1));
    assert_eq!(
        fake.gains.borrow()[0]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -12.
    );
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.session.busy());
}

#[gtk::test]
fn levels_owner_external_newer_readback_is_uncertain_not_permanent_success_wait() {
    for reconnect in [false, true] {
        let (owner, fake, driver) = setup_levels();
        set(&owner, Change::Db(-6.)).unwrap();
        complete(&fake, 0, LocalConfigOutcome::Applied, false);
        owner.poll();
        assert!(owner.session.busy());
        // The actual state has superseded the completed snapshot. Never replay.
        fake.config.borrow_mut().revision.revision += 10;
        if reconnect {
            let mut graph = (*owner.session.graph().unwrap().value).clone();
            graph.verbonden = false;
            driver.publish(graph.clone(), BTreeMap::new());
            owner.session.poll();
            graph.verbonden = true;
            let current = driver.publish(graph.clone(), generations(&graph));
            owner.session.poll();
            fake.config.borrow_mut().revision.epoch = current.stamp.epoch;
        }
        polls(&owner, 3);
        assert!(!owner.session.busy());
        assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
        assert_eq!(fake.gains.borrow().len(), 1);
        assert!(owner.configuration_report().is_some());
        owner.refresh();
        owner.poll();
        assert!(owner.data.borrow().operation.is_none());
        assert!(set(&owner, Change::Db(-12.)).is_ok());
    }
}

#[gtk::test]
fn levels_owner_old_readback_waits_until_completion_is_actually_visible() {
    let (owner, fake, _) = setup_levels();
    set(&owner, Change::Db(-6.)).unwrap();
    let report = complete(&fake, 0, LocalConfigOutcome::Applied, false);
    polls(&owner, 4);
    assert!(owner.session.busy());
    let ConfigProgress::Completed(done) = report.progress else {
        panic!()
    };
    *fake.config.borrow_mut() = done.snapshot;
    owner.poll();
    assert!(!owner.session.busy());
    assert_eq!(progress(&owner), ui::Progress::AppliedLocally);
}

#[gtk::test]
fn levels_owner_partial_metadata_needs_explicit_initialization_before_fresh_levels() {
    let (owner, fake, _) = setup_levels();
    let retained = capture(&owner, "rec-1-2");
    set(&owner, Change::Db(-6.)).unwrap();
    let raw = complete(
        &fake,
        0,
        LocalConfigOutcome::PartiallyApplied(ConfigError::ReadbackMismatch),
        true,
    );
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert!(matches!(
        progress(&owner),
        ui::Progress::PartiallyApplied(_)
    ));
    assert!(owner
        .level_event(Event::Set(retained.propose(Change::Db(-12.)).unwrap()))
        .is_err());
    owner.refresh();
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert!(owner.data.borrow().level_recovery);
    assert!(owner
        .origin()
        .unwrap()
        .0
        .config
        .as_ref()
        .unwrap()
        .configuration
        .is_none());
    assert_ne!(owner.level_support(), ui::Support::Available);
    assert_eq!(owner.data.borrow().level_reports.back(), Some(&raw));
    assert!(button(owner.widget(), "Initialize native bus defaults").is_sensitive());
    owner.initialize(&owner.origin().unwrap()).unwrap();
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.data.borrow().level_recovery);
    assert!(set(&owner, Change::Mute(true)).is_ok());
    assert_eq!(owner.session.history_lengths(), (0, 0));
}

#[gtk::test]
fn levels_owner_reentrant_inspection_and_receipt_do_not_reborrow_or_rebind() {
    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    let weak = Rc::downgrade(&owner);
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        owner.recording_status();
        owner.session.close_admission();
    }));
    update(&owner, &request, -6.);
    assert!(fake.gains.borrow().is_empty());
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));

    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    let weak = Rc::downgrade(&owner);
    let old = request.clone();
    *fake.gains.borrow()[0].2.hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        owner.recording_status();
        end(&owner, &old, -18.);
        owner.poll(); // Poll reentrancy is deferred, not recursively consumed.
    }));
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert_eq!(fake.gains.borrow().len(), 2);
    complete(&fake, 1, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    let newer = begin(&owner);
    assert!(owner.level_event(Event::Abandon { request }).is_err());
    assert!(owner.view.awaited_request().is_none()); // Direct controller begin isn't a widget request.
    end(&owner, &newer, -12.);
    assert_eq!(fake.gains.borrow().len(), 3);
}

#[gtk::test]
fn levels_owner_quit_keeps_receipt_and_never_flushes_unsent_after_close() {
    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    end(&owner, &request, -18.);
    owner.session.close_admission();
    *fake.gains.borrow()[0].2.report.borrow_mut() = None;
    owner.poll();
    assert!(owner.session.busy());
    assert!(owner.finalization_errors(false).is_none());
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert_eq!(fake.gains.borrow().len(), 1);
    assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
    assert!(owner.finalization_errors(false).is_some());
    assert_eq!(owner.session.history_lengths(), (0, 0));
}

#[gtk::test]
fn levels_owner_shared_sample_allocator_rejects_stale_foreign_and_closed_leases() {
    let (owner, _, _) = setup_levels();
    let (first, lease) = owner.session.reserve_external().unwrap();
    assert_eq!(first, 1);
    assert!(owner.session.external_sample_ticket(&Rc::new(())).is_err());
    assert_eq!(owner.session.external_sample_ticket(&lease).unwrap(), 2);
    owner.session.release_external(&lease);
    assert!(owner.session.external_sample_ticket(&lease).is_err());
    let (next, current) = owner.session.reserve_external().unwrap();
    assert_eq!(next, 3);
    owner.session.close_admission();
    assert!(owner.session.external_sample_ticket(&current).is_err());
    owner.session.release_external(&lease);
    assert!(owner.session.busy());
    owner.session.release_external(&current);
    assert!(!owner.session.busy());
}

#[gtk::test]
fn levels_owner_selected_mono_preserves_other_channel_mute_arming_and_db_bits() {
    let (owner, fake, _) = setup_levels();
    {
        let mut c = fake.config.borrow_mut();
        let buses = &mut c.configuration.as_mut().unwrap().buses;
        *buses = buses
            .change_coupling(0)
            .unwrap()
            .with_db("rec-2", -13.7)
            .unwrap()
            .with_mute("rec-2", true)
            .unwrap();
        buses.states.get_mut("rec-2").unwrap().armed = false;
    }
    owner.poll();
    let before = fake.config.borrow().configuration.clone().unwrap();
    let intent = capture(&owner, "rec-1").propose(Change::Db(-90.)).unwrap();
    owner.level_event(Event::Set(intent)).unwrap();
    let after = fake.gains.borrow()[0].1.configuration.clone();
    assert_eq!(after.buses.state("rec-1").db, -90.);
    assert!(!after.buses.state("rec-1").mute);
    assert_eq!(
        after.buses.state("rec-2").db.to_bits(),
        before.buses.state("rec-2").db.to_bits()
    );
    assert_eq!(after.buses.state("rec-2"), before.buses.state("rec-2"));
    assert_eq!(after.clock, before.clock);
    assert_eq!(after.start_stop, before.start_stop);
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.session.busy());
}

#[gtk::test]
fn levels_owner_widget_wheel_keeps_request_until_clock_end_final_flush_and_readback() {
    let (owner, fake, _) = setup_levels();
    let mut pending = vec![owner.widget().clone().upcast::<gtk::Widget>()];
    let scale = loop {
        let w = pending.pop().expect("Rec level scale");
        if let Some(scale) = w.downcast_ref::<gtk::Scale>() {
            break scale.clone();
        }
        let mut child = w.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            pending.push(w);
        }
    };
    let controllers = scale.observe_controllers();
    let scroll = (0..controllers.n_items())
        .find_map(|n| {
            controllers
                .item(n)
                .and_downcast::<gtk::EventControllerScroll>()
        })
        .unwrap();
    assert!(scroll.emit_by_name::<bool>("scroll", &[&0_f64, &1_f64]));
    let request = owner
        .view
        .awaited_request()
        .expect("widget retains owner request");
    assert_eq!(fake.gains.borrow().len(), 1);
    assert_eq!(
        fake.gains.borrow()[0]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -1.
    );
    owner.tick_levels(Duration::from_millis(200));
    scroll.emit_by_name::<bool>("scroll", &[&0_f64, &1_f64]);
    assert_eq!(fake.gains.borrow().len(), 1);
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert_eq!(
        fake.gains.borrow().len(),
        1,
        "legacy wheel repeats read the observed value, not the prior proposal"
    );
    owner.tick_levels(Duration::from_millis(400));
    scroll.emit_by_name::<bool>("scroll", &[&0_f64, &1_f64]);
    assert_eq!(fake.gains.borrow().len(), 2);
    assert_eq!(
        fake.gains.borrow()[1]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -2.
    );
    complete(&fake, 1, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert_eq!(owner.view.awaited_request(), Some(request.clone()));
    assert!(
        owner.session.busy(),
        "sample completion is not gesture completion"
    );
    owner.tick_levels(Duration::from_millis(999));
    owner.poll();
    assert_eq!(owner.view.awaited_request(), Some(request));
    owner.tick_levels(Duration::from_millis(1000));
    owner.poll();
    assert!(owner.view.awaited_request().is_none());
    assert!(!owner.session.busy());
    assert_eq!(fake.gains.borrow().len(), 2);
    assert_eq!(owner.session.history_lengths(), (0, 0));
}

#[gtk::test]
fn levels_owner_retired_mute_callback_and_unknown_recording_cannot_enable_new_intent() {
    let (owner, fake, _) = setup_levels();
    let old = button(owner.widget(), "Mute bus");
    let changed = fake
        .config
        .borrow()
        .configuration
        .as_ref()
        .unwrap()
        .buses
        .with_mute("rec-1-2", true)
        .unwrap();
    fake.config
        .borrow_mut()
        .configuration
        .as_mut()
        .unwrap()
        .buses = changed;
    owner.poll();
    old.set_sensitive(true);
    old.emit_clicked();
    assert!(fake.gains.borrow().is_empty());
    fake.config.borrow_mut().recording = true; // No known native take/attachment.
    owner.poll();
    assert!(!button(owner.widget(), "Set gain").is_sensitive());
    assert!(set(&owner, Change::Db(-6.)).is_err());
    assert!(fake.gains.borrow().is_empty());
}

#[gtk::test]
fn levels_owner_applied_prefix_then_rejection_keeps_partial_outcome_for_quit() {
    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    end(&owner, &request, -18.);
    complete(&fake, 0, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    complete(
        &fake,
        1,
        LocalConfigOutcome::Rejected(ConfigError::StaleRevision),
        true,
    );
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert!(matches!(
        progress(&owner),
        ui::Progress::PartiallyApplied(_)
    ));
    assert_eq!(
        fake.config
            .borrow()
            .configuration
            .as_ref()
            .unwrap()
            .buses
            .state("rec-1-2")
            .db,
        -6.
    );
    assert!(owner
        .finalization_errors(false)
        .unwrap()
        .iter()
        .any(|e| e.contains("Rec levels partially applied")));
    assert_eq!(owner.session.history_lengths(), (0, 0));
    owner.stop_all().unwrap();
    assert_eq!(*fake.stops.borrow(), vec![Stop::All]);
}

#[gtk::test]
fn levels_owner_reentrant_rejection_cannot_downgrade_an_established_cancelled_origin() {
    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    fake.gain_failures
        .borrow_mut()
        .push_back(Error::Configuration(ConfigError::StaleRevision));
    let weak = Rc::downgrade(&owner);
    *fake.hook.borrow_mut() = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        owner.session.close_admission();
        owner.poll();
    }));
    update(&owner, &request, -6.);
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
    assert!(fake.gains.borrow().is_empty());
}

fn superseding_readback_with_unsent_target(unknown: bool) {
    let (owner, fake, _) = setup_levels();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    end(&owner, &request, -18.);
    let historical = complete(&fake, 0, LocalConfigOutcome::Applied, true);
    owner.poll();
    {
        let mut actual = fake.config.borrow_mut();
        actual.revision.revision += 1;
        if unknown {
            actual.configuration = None;
        } else {
            let config = actual.configuration.as_mut().unwrap();
            config.buses = config.buses.with_db("rec-1-2", -12.).unwrap();
            let pecan_core::graph::Opdracht::ZetBus { gain, .. } =
                config.buses.backend_config().unwrap()
            else {
                panic!()
            };
            actual.local.as_mut().unwrap().gains = gain;
        }
    }
    polls(&owner, 3);
    assert!(
        !owner.session.busy(),
        "superseded completion must not hold an impossible final flush"
    );
    assert!(matches!(progress(&owner), ui::Progress::Unconfirmed(_)));
    assert_eq!(
        fake.gains.borrow().len(),
        1,
        "never replay an unsent target against superseding state"
    );
    assert_eq!(owner.configuration_report(), Some(historical.clone()));
    assert_eq!(owner.session.history_lengths(), (0, 0));
    owner.refresh();
    owner.poll();
    assert!(owner.data.borrow().operation.is_none());
    assert_eq!(owner.data.borrow().level_reports.back(), Some(&historical));
    if unknown {
        assert_ne!(owner.level_support(), ui::Support::Available);
        assert!(button(owner.widget(), "Initialize native bus defaults").is_sensitive());
    } else {
        assert_eq!(owner.level_support(), ui::Support::Available);
        assert!(set(&owner, Change::Db(-9.)).is_ok());
        assert_eq!(
            fake.gains.borrow().len(),
            2,
            "only a new interaction may use current state"
        );
    }
}

#[gtk::test]
fn levels_owner_superseded_known_readback_releases_unsent_final_target_as_uncertainty() {
    superseding_readback_with_unsent_target(false);
}

#[gtk::test]
fn levels_owner_invalidated_metadata_releases_unsent_final_target_as_uncertainty() {
    superseding_readback_with_unsent_target(true);
}

#[gtk::test]
fn levels_owner_older_readback_keeps_unsent_final_target_until_matching_inspection() {
    let (owner, fake, _) = setup_levels();
    let original = owner.session.witnessed_graph().unwrap();
    let request = begin(&owner);
    update(&owner, &request, -6.);
    end(&owner, &request, -18.);
    let done = complete(&fake, 0, LocalConfigOutcome::Applied, false);
    polls(&owner, 4);
    assert!(owner.session.busy());
    assert_eq!(fake.gains.borrow().len(), 1);
    assert_eq!(progress(&owner), ui::Progress::Applying);
    let ConfigProgress::Completed(done) = done.progress else {
        panic!()
    };
    *fake.config.borrow_mut() = done.snapshot;
    owner.poll();
    assert_eq!(fake.gains.borrow().len(), 2);
    assert_eq!(
        fake.gains.borrow()[1]
            .1
            .configuration
            .buses
            .state("rec-1-2")
            .db,
        -18.
    );
    assert!(Arc::ptr_eq(
        &fake.gains.borrow()[1].0.graph().value,
        &original.graph().value
    ));
    complete(&fake, 1, LocalConfigOutcome::Applied, true);
    polls(&owner, 2);
    assert!(!owner.session.busy());
    assert_eq!(progress(&owner), ui::Progress::AppliedLocally);
}
