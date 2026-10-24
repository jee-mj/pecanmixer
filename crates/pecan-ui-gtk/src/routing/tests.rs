use super::*;
use pecan_core::{
    graph::Opdracht,
    native_command::{Counts, NativeFailure},
};
use serde_json::{json, Value};

pub(super) type Owner = buses::Model<Rc<()>, u64, u64>;
pub(super) fn graph() -> Stamped<Arc<Momentopname>> {
    Stamped {
        stamp: Stamp {
            epoch: 3,
            revision: 1,
        },
        value: Arc::new(serde_json::from_str(include_str!("fixtures/graph.json")).unwrap()),
    }
}
pub(super) fn config() -> pecan_core::domain::buses::BusConfig {
    serde_json::from_str(include_str!("fixtures/buses.json")).unwrap()
}
pub(super) fn snapshot(g: &Stamped<Arc<Momentopname>>) -> buses::Snapshot<Rc<()>, u64, u64> {
    let configuration = config();
    buses::Snapshot {
        origin: Rc::new(()),
        connected: g.value.verbonden,
        closing: false,
        configuration: Some(configuration.clone()),
        mixer: Arc::new(Mixer::build(
            &g.value,
            &configuration.definitions().unwrap(),
        )),
        default_sink: g.value.standaard_sink.clone(),
        has_bus_routes: true,
        activity: buses::Activity::Idle,
        tracks: vec![],
        operation: None,
        capabilities: buses::Capabilities::default(),
    }
}
pub(super) fn owner(g: &Stamped<Arc<Momentopname>>) -> Owner {
    let mut m = Owner::new(buses::Budget::default());
    m.observe(snapshot(g)).unwrap();
    m
}
pub(super) fn capture(g: &Stamped<Arc<Momentopname>>, m: &Owner) -> Capture<Rc<()>> {
    Capture::from_bus_model(g.clone(), m, Budget::default()).unwrap()
}
pub(super) fn first(m: &Model<Rc<()>>) -> (String, String) {
    let row = m.rows().into_iter().next().unwrap();
    (row.key, row.output.unwrap().destination)
}
pub(super) fn intent(e: Event<Rc<()>>) -> Intent<Rc<()>> {
    let Event::History(i) = e else {
        panic!("expected history intent")
    };
    i
}
pub(super) fn count(i: &Intent<Rc<()>>) -> usize {
    let Input::Execute { action, .. } = &i.history else {
        panic!()
    };
    history::plan(action, &i.graph.value, &i.bus_configuration)
        .unwrap()
        .len()
}
pub(super) fn report(i: &Intent<Rc<()>>, progress: NativeProgress) -> NativeReport {
    NativeReport {
        ticket: 71,
        source: i.graph.stamp,
        command_count: count(i),
        progress,
    }
}
fn commands(values: &[Opdracht]) -> Value {
    json!(values
        .iter()
        .map(|v| match v {
            Opdracht::Verbind { uit, naar } => json!({"kind":"connect","from":uit,"to":naar}),
            Opdracht::Ontkoppel { kabel } => json!({"kind":"disconnect","id":kabel}),
            Opdracht::ZetDoel { knoop, doel } =>
                json!({"kind":"target","node":knoop,"target":doel}),
            other => panic!("unexpected routing command: {other:?}"),
        })
        .collect::<Vec<_>>())
}

#[test]
fn javascript_oracle_routes_selections_and_reset_are_delegated_to_existing_core_and_bus_model() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let mut route_cases = 0;
    let mut selection_cases = 0;
    for case in oracle["scenarios"].as_array().unwrap() {
        let g = Stamped {
            stamp: Stamp {
                epoch: 1,
                revision: 1,
            },
            value: Arc::new(serde_json::from_value(case["graph"].clone()).unwrap()),
        };
        for expected in case["routes"].as_array().unwrap() {
            let mut bus = owner(&g);
            let bind = bus.bind().unwrap();
            let destination = expected["destination"].as_str().unwrap();
            bus.select_destination(&bind, destination).unwrap();
            let mut m = Model::default();
            m.observe(capture(&g, &bus)).unwrap();
            let source = expected["source"].as_str().unwrap();
            let row = m.rows().into_iter().find(|r| r.key == source).unwrap();
            let cell = [row.output, row.rec]
                .into_iter()
                .flatten()
                .find(|r| r.destination == destination)
                .unwrap();
            assert_eq!(
                serde_json::to_value(cell.state.clone()).unwrap(),
                expected["state"],
                "{} {source} → {destination}",
                case["name"]
            );
            let binding = m.bind().unwrap();
            let event = m.route(&binding, source, destination).unwrap();
            match event {
                Event::OpenPatchbay { graph, origin, .. } => {
                    assert_eq!(expected["state"]["kind"], "custom");
                    assert!(Arc::ptr_eq(&graph.value, &g.value));
                    assert!(Rc::ptr_eq(&origin, &bus.snapshot().unwrap().origin));
                    assert!(m.pending().is_none());
                }
                Event::History(i) => {
                    assert!(Arc::ptr_eq(&i.graph.value, &g.value));
                    assert!(Rc::ptr_eq(&i.origin, &bus.snapshot().unwrap().origin));
                    let Input::Execute { action, label } = &i.history else {
                        panic!()
                    };
                    assert_eq!(
                        label,
                        if expected["state"]["kind"] == "off" {
                            "Route on"
                        } else {
                            "Route off"
                        }
                    );
                    assert_eq!(
                        commands(&history::plan(action, &g.value, &config()).unwrap()),
                        expected["commands"]
                    );
                }
                _ => panic!(),
            }
            route_cases += 1;
        }
        for expected in case["selections"].as_array().unwrap() {
            let mut bus = owner(&g);
            for key in [&expected["output"], &expected["rec"]]
                .into_iter()
                .filter_map(Value::as_str)
            {
                if bus.snapshot().unwrap().mixer.destination(key).is_some() {
                    let b = bus.bind().unwrap();
                    bus.select_destination(&b, key).unwrap();
                }
            }
            let mut m = Model::default();
            m.observe(capture(&g, &bus)).unwrap();
            let selected = |kind| {
                m.choices()
                    .iter()
                    .find(|c| c.kind == kind && c.selected)
                    .map(|c| c.key.clone())
            };
            assert_eq!(
                json!({"output":selected(Destination::Output),"rec":selected(Destination::Rec)}),
                expected["expected"]
            );
            selection_cases += 1;
        }
        let bus = owner(&g);
        let mut m = Model::default();
        m.observe(capture(&g, &bus)).unwrap();
        assert_eq!(
            json!({"links":m.counts().links,"targets":m.counts().targets,"detached":m.counts().detached}),
            case["reset"]["counts"]
        );
        assert_eq!(
            m.counts().total(),
            case["reset"]["total"].as_u64().unwrap() as usize
        );
        if m.counts().total() > 0 {
            let b = m.bind().unwrap();
            m.begin_reset(&b).unwrap();
            let confirmation = m.confirmation_binding().unwrap();
            let i = intent(m.confirm_reset(&confirmation).unwrap());
            let Input::Execute { action, .. } = i.history else {
                panic!()
            };
            let planned = history::plan(&action, &g.value, &config()).unwrap();
            assert_eq!(commands(&planned), case["reset"]["commands"]);
            for link in g.value.kabels.iter().filter(|l| l.in_knoop == 30) {
                assert!(
                    !planned.contains(&Opdracht::Ontkoppel { kabel: link.id }),
                    "reset must preserve Rec links"
                );
            }
        }
    }
    assert_eq!(route_cases, 92);
    assert_eq!(selection_cases, 94);
}

#[test]
fn output_and_rec_choices_use_the_same_owner_model_and_never_optimistically_change() {
    let g = graph();
    let mut bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let b = m.bind().unwrap();
    let output = m
        .choices()
        .into_iter()
        .find(|c| c.kind == Destination::Output && c.name == "synthetic.output.other")
        .unwrap()
        .key;
    let rec = m
        .choices()
        .into_iter()
        .find(|c| c.kind == Destination::Rec && !c.selected)
        .unwrap()
        .key;
    let Event::Selection { binding, key, .. } = m.select(&b, &rec).unwrap() else {
        panic!()
    };
    assert_ne!(m.capture.as_ref().unwrap().rec.as_ref(), Some(&rec));
    bus.select_destination(&binding, &key).unwrap();
    m.observe(capture(&g, &bus)).unwrap();
    assert_eq!(m.capture.as_ref().unwrap().rec.as_ref(), Some(&rec));
    let Event::Selection { binding, key, .. } = m.select(&m.bind().unwrap(), &output).unwrap()
    else {
        panic!()
    };
    bus.select_destination(&binding, &key).unwrap();
    m.observe(capture(&g, &bus)).unwrap();
    assert_eq!(m.capture.as_ref().unwrap().output.as_ref(), Some(&output));
    assert_eq!(m.capture.as_ref().unwrap().rec.as_ref(), Some(&rec));
    let old = m.bind().unwrap();
    bus.observe(snapshot(&g)).unwrap();
    let Event::Selection { binding, key, .. } = m.select(&old, &output).unwrap() else {
        panic!()
    };
    assert!(
        matches!(
            bus.select_destination(&binding, &key),
            Err(buses::Error::StaleBinding)
        ),
        "owner can reject a publication not yet delivered to the routing view"
    );
}

#[test]
fn cancelled_or_replaced_confirmations_and_old_callbacks_cannot_authorize_reset() {
    let g = graph();
    let bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let old = m.bind().unwrap();
    m.begin_reset(&old).unwrap();
    let cancelled = m.confirmation_binding().unwrap();
    m.cancel_reset();
    assert!(matches!(
        m.confirm_reset(&cancelled),
        Err(Error::NoConfirmation)
    ));
    m.begin_reset(&old).unwrap();
    let fresh = m.confirmation_binding().unwrap();
    assert!(matches!(
        m.confirm_reset(&cancelled),
        Err(Error::NoConfirmation)
    ));
    m.observe(capture(&g, &bus)).unwrap();
    assert!(matches!(m.confirm_reset(&fresh), Err(Error::StaleBinding)));
    let (s, d) = first(&m);
    assert!(matches!(m.route(&old, &s, &d), Err(Error::StaleBinding)));
    assert!(m.pending().is_none());
    assert!(m.confirmation().is_none());
}

#[test]
fn pending_is_retained_across_queued_local_updates_reconnect_and_mismatched_reports() {
    let g = graph();
    let mut bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let (s, d) = first(&m);
    let i = intent(m.route(&m.bind().unwrap(), &s, &d).unwrap());
    let token = i.request.clone();
    assert!(m.report(&token, report(&i, NativeProgress::Queued)));
    let mut wrong = report(&i, NativeProgress::Queued);
    wrong.ticket += 1;
    assert!(!m.report(&token, wrong));
    assert!(!m.report(
        &RequestToken(Rc::new(())),
        report(&i, NativeProgress::Queued)
    ));
    assert!(m.report(
        &token,
        report(
            &i,
            NativeProgress::ExecutedLocally(Counts {
                submitted: 1,
                ..Default::default()
            })
        )
    ));
    let reconnected = Stamped {
        stamp: Stamp {
            epoch: 4,
            revision: 1,
        },
        value: Arc::new((*g.value).clone()),
    };
    bus.observe(snapshot(&reconnected)).unwrap();
    m.observe(capture(&reconnected, &bus)).unwrap();
    assert_eq!(m.pending(), Some(token.clone()));
    assert_eq!(m.availability(), Err(Error::Pending));
    assert!(m.report(
        &token,
        report(
            &i,
            NativeProgress::Rejected {
                reason: NativeFailure::Disconnected,
                counts: Counts {
                    submitted: 1,
                    ..Default::default()
                }
            }
        )
    ));
    assert_eq!(m.availability(), Err(Error::ReconciliationRequired));
    m.observe(capture(&reconnected, &bus)).unwrap();
    assert_eq!(m.availability(), Err(Error::ReconciliationRequired));
    m.reconciled(capture(&reconnected, &bus)).unwrap();
    assert!(m.availability().is_ok());
}

#[test]
fn observed_results_require_matching_source_and_a_rendered_graph_barrier() {
    let g = graph();
    let mut bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let (s, d) = first(&m);
    let i = intent(m.route(&m.bind().unwrap(), &s, &d).unwrap());
    let token = i.request.clone();
    let observed = Stamp {
        epoch: 3,
        revision: 2,
    };
    let progress = NativeProgress::Observed {
        stamp: observed,
        counts: Counts {
            attempted: count(&i),
            submitted: count(&i),
            observed: count(&i),
            ..Default::default()
        },
    };
    let mut wrong = report(&i, progress.clone());
    wrong.source.revision += 1;
    assert!(!m.report(&token, wrong));
    let mut wrong = report(&i, progress.clone());
    wrong.command_count += 1;
    assert!(!m.report(&token, wrong));
    assert!(!m.report(
        &token,
        report(
            &i,
            NativeProgress::Observed {
                stamp: Stamp {
                    epoch: 4,
                    revision: 2
                },
                counts: Counts::default()
            }
        )
    ));
    assert!(m.report(&token, report(&i, progress)));
    assert!(m.pending().is_none());
    assert_eq!(m.availability(), Err(Error::AwaitingGraph));
    let next = Stamped {
        stamp: observed,
        value: Arc::new((*g.value).clone()),
    };
    bus.observe(snapshot(&next)).unwrap();
    m.observe(capture(&next, &bus)).unwrap();
    assert!(m.availability().is_ok());
}

#[test]
fn admission_failure_fresh_retry_and_partial_unconfirmed_reconciliation_are_distinct() {
    let g = graph();
    let bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let (s, d) = first(&m);
    let b = m.bind().unwrap();
    let i = intent(m.route(&b, &s, &d).unwrap());
    m.handoff_failed(&i.request, "synthetic queue full".into());
    assert!(m.pending().is_none());
    assert!(m.availability().is_ok());
    let i = intent(m.route(&b, &s, &d).unwrap());
    m.report(&i.request, report(&i, NativeProgress::Queued));
    m.handoff_failed(
        &i.request,
        "synthetic callback failed after admission".into(),
    );
    assert_eq!(m.pending(), Some(i.request.clone()));
    m.report(
        &i.request,
        report(
            &i,
            NativeProgress::Unconfirmed {
                reason: NativeFailure::ObservationTimedOut,
                counts: Counts::default(),
            },
        ),
    );
    assert_eq!(m.availability(), Err(Error::ReconciliationRequired));
    m.reconciled(capture(&g, &bus)).unwrap();
    assert!(m.availability().is_ok());
}

#[test]
fn input_and_plan_bounds_do_not_truncate_reset_or_mutate_the_graph() {
    let g = graph();
    let bus = owner(&g);
    assert!(matches!(
        Capture::from_bus_model(
            g.clone(),
            &bus,
            Budget {
                sources: 1,
                ..Budget::default()
            }
        ),
        Err(Error::BudgetExceeded)
    ));
    let mut value = (*g.value).clone();
    let template = value.kabels.iter().find(|l| l.id == 1020).unwrap().clone();
    for id in 2000..2040 {
        let mut l = template.clone();
        l.id = id;
        value.kabels.push(l);
    }
    let g = Stamped {
        stamp: g.stamp,
        value: Arc::new(value),
    };
    let bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let b = m.bind().unwrap();
    m.begin_reset(&b).unwrap();
    let confirmation = m.confirmation_binding().unwrap();
    assert!(m.counts().links > MAX_NATIVE_COMMANDS);
    assert!(matches!(
        m.confirm_reset(&confirmation),
        Err(Error::PlanTooLarge)
    ));
    assert!(m.pending().is_none());
    assert!(m.confirmation().is_some());
    assert_eq!(g.value.kabels.len(), 47);
}

#[test]
fn projection_mismatch_unknown_configuration_disconnect_and_stamp_replay_fail_closed() {
    let g = graph();
    let mut bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let copy = Stamped {
        stamp: g.stamp,
        value: Arc::new((*g.value).clone()),
    };
    assert_eq!(
        m.observe(capture(&copy, &bus)),
        Err(Error::ConflictingStamp)
    );
    let old = Stamped {
        stamp: Stamp {
            epoch: 2,
            revision: 99,
        },
        value: g.value.clone(),
    };
    assert_eq!(m.observe(capture(&old, &bus)), Err(Error::OldGraph));
    let mut unknown = snapshot(&g);
    unknown.configuration = None;
    bus.observe(unknown).unwrap();
    m.observe(capture(&g, &bus)).unwrap();
    assert_eq!(m.availability(), Err(Error::UnknownConfiguration));
    let disconnected = Stamped {
        stamp: Stamp {
            epoch: 4,
            revision: 1,
        },
        value: Arc::new(Momentopname::default()),
    };
    bus.observe(snapshot(&disconnected)).unwrap();
    m.observe(capture(&disconnected, &bus)).unwrap();
    assert_eq!(m.availability(), Err(Error::Disconnected));
    let mut mismatched = (*g.value).clone();
    mismatched.knopen[0].naam = "synthetic.changed.name".into();
    assert!(matches!(
        Capture::from_bus_model(
            Stamped {
                stamp: g.stamp,
                value: Arc::new(mismatched)
            },
            &owner(&g),
            Budget::default()
        ),
        Err(Error::ProjectionMismatch)
    ));
}

#[test]
fn bus_configuration_pending_and_partial_status_gate_routing_without_owning_selection() {
    let g = graph();
    for (progress, expected) in [
        (buses::Progress::Queued, Error::Pending),
        (buses::Progress::Applying, Error::Pending),
        (
            buses::Progress::PartiallyApplied("synthetic partial".into()),
            Error::ReconciliationRequired,
        ),
        (
            buses::Progress::Unconfirmed("synthetic timeout".into()),
            Error::ReconciliationRequired,
        ),
    ] {
        let mut bus = owner(&g);
        let mut s = snapshot(&g);
        s.operation = Some(buses::Operation {
            request: 4,
            progress,
        });
        bus.observe(s).unwrap();
        let mut m = Model::default();
        m.observe(capture(&g, &bus)).unwrap();
        assert_eq!(m.availability(), Err(expected.clone()));
        let (s, d) = first(&m);
        assert!(m.route(&m.bind().unwrap(), &s, &d).is_err());
        assert!(m.pending().is_none());
        let choice = m.choices().into_iter().find(|c| !c.selected).unwrap();
        assert!(matches!(
            m.select(&m.bind().unwrap(), &choice.key),
            Ok(Event::Selection { .. })
        ));
        if expected == Error::ReconciliationRequired {
            assert_eq!(
                m.reconciled(capture(&g, &bus)),
                Err(Error::ReconciliationRequired)
            );
            bus.observe(snapshot(&g)).unwrap();
            m.observe(capture(&g, &bus)).unwrap();
            assert_eq!(m.availability(), Err(Error::ReconciliationRequired));
            m.reconciled(capture(&g, &bus)).unwrap();
            assert!(m.availability().is_ok());
        }
    }
}

fn configuration_uncertainty_survives_zero_effect_route_rejection(progress: buses::Progress) {
    let g = graph();
    let mut bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let (source, destination) = first(&m);
    let route = intent(m.route(&m.bind().unwrap(), &source, &destination).unwrap());
    assert!(m.report(&route.request, report(&route, NativeProgress::Queued)));

    let mut uncertain = snapshot(&g);
    uncertain.operation = Some(buses::Operation {
        request: 4,
        progress,
    });
    bus.observe(uncertain).unwrap();
    m.observe(capture(&g, &bus)).unwrap();
    assert_eq!(m.reconciled(capture(&g, &bus)), Err(Error::Pending));
    assert!(m.report(
        &route.request,
        report(
            &route,
            NativeProgress::Rejected {
                reason: NativeFailure::Disconnected,
                counts: Counts::default(),
            },
        ),
    ));
    assert!(m.pending().is_none());

    // Ordinary publication removes the displayed configuration operation; it
    // does not attest reconciliation of its earlier uncertain outcome.
    bus.observe(snapshot(&g)).unwrap();
    m.observe(capture(&g, &bus)).unwrap();
    assert_eq!(m.availability(), Err(Error::ReconciliationRequired));
    assert!(matches!(
        m.route(&m.bind().unwrap(), &source, &destination),
        Err(Error::ReconciliationRequired)
    ));
    assert_eq!(
        m.begin_reset(&m.bind().unwrap()),
        Err(Error::ReconciliationRequired)
    );

    m.reconciled(capture(&g, &bus)).unwrap();
    assert!(m.availability().is_ok());
    assert!(!m.report(&route.request, report(&route, NativeProgress::Queued)));
    assert!(m.route(&m.bind().unwrap(), &source, &destination).is_ok());
}

#[test]
fn partial_configuration_survives_zero_effect_routing_rejection() {
    configuration_uncertainty_survives_zero_effect_route_rejection(
        buses::Progress::PartiallyApplied("synthetic partial".into()),
    );
}

#[test]
fn unconfirmed_configuration_survives_zero_effect_routing_rejection() {
    configuration_uncertainty_survives_zero_effect_route_rejection(buses::Progress::Unconfirmed(
        "synthetic timeout".into(),
    ));
}

#[test]
fn zero_effect_routing_rejection_without_uncertainty_allows_a_fresh_action() {
    let g = graph();
    let bus = owner(&g);
    let mut m = Model::default();
    m.observe(capture(&g, &bus)).unwrap();
    let (source, destination) = first(&m);
    let route = intent(m.route(&m.bind().unwrap(), &source, &destination).unwrap());
    assert!(m.report(&route.request, report(&route, NativeProgress::Queued)));
    assert!(m.report(
        &route.request,
        report(
            &route,
            NativeProgress::Rejected {
                reason: NativeFailure::Disconnected,
                counts: Counts::default(),
            },
        ),
    ));
    m.observe(capture(&g, &bus)).unwrap();
    assert!(m.availability().is_ok());
    assert!(m.route(&m.bind().unwrap(), &source, &destination).is_ok());
}
