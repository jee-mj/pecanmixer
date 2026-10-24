//! Clearly synthetic graph/worker probes; no server, hardware or runtime capture.
use super::*;
use native::{satisfied, OBSERVATION_LIMIT};
use pecan_core::graph::{Knoop, Poort, Soort};
use pecan_core::native_command::*;
use std::time::Instant;

fn node(id: u32) -> Knoop {
    Knoop {
        id,
        serial: Some(10_000 + u64::from(id)),
        naam: format!("synthetic.node.{id}"),
        beschrijving: None,
        bijnaam: None,
        klasse: None,
        apparaat: None,
        app: None,
        media: None,
        stand: None,
        doel: None,
        volume: Some(vec![1.0]),
        mute: Some(false),
    }
}
fn port(id: u32, owner: u32, input: bool) -> Poort {
    Poort {
        id,
        knoop: owner,
        volgnummer: Some(0),
        naam: format!("synthetic.port.{id}"),
        alias: None,
        ingang: input,
        soort: Soort::Audio,
        kanaal: Some("MONO".into()),
        monitor: false,
        fysiek: false,
        groep: None,
    }
}
fn graph() -> Momentopname {
    Momentopname {
        verbonden: true,
        rate: Some(48_000),
        quantum: Some(128),
        knopen: vec![node(10), node(20)],
        poorten: vec![port(11, 10, false), port(21, 20, true)],
        ..Default::default()
    }
}
fn setup(graph: Momentopname) -> Commands {
    let commands = Commands::default();
    for id in graph
        .knopen
        .iter()
        .map(|n| n.id)
        .chain(graph.poorten.iter().map(|p| p.id))
        .chain(graph.kabels.iter().map(|l| l.id))
    {
        commands.object_added(id);
    }
    commands.publish(graph);
    commands
}
fn request(commands: &Commands, ops: Vec<Opdracht>) -> NativeIntent {
    NativeIntent {
        ticket: 1,
        origin: commands.current(),
        commands: ops,
    }
}
fn mute(node: u32, value: bool) -> Opdracht {
    Opdracht::ZetMute {
        knoop: node,
        mute: value,
    }
}
fn drain(
    commands: &Commands,
    live: &Momentopname,
    execute: impl FnMut(Opdracht) -> LocalExecution,
) {
    assert!(commands
        .drain_with(|_| true, |c| satisfied(c, live), execute)
        .is_empty());
}

#[test]
fn origin_is_the_observed_publication_not_the_graph_at_admission() {
    let initial = graph();
    let commands = setup(initial.clone());
    let old = commands.current();
    commands.object_removed(10);
    commands.object_added(10);
    let mut replacement = initial;
    replacement.knopen[0].serial = Some(99_999);
    commands.publish(replacement);
    assert_eq!(
        commands
            .enqueue_native(NativeIntent {
                ticket: 1,
                origin: old,
                commands: vec![mute(10, true)]
            })
            .unwrap_err(),
        AdmissionError::InvalidObject
    );
    let current = commands.current();
    let copied = Stamped {
        stamp: current.stamp,
        value: Arc::new((*current.value).clone()),
    };
    assert_eq!(
        commands
            .enqueue_native(NativeIntent {
                ticket: 2,
                origin: copied,
                commands: vec![mute(10, true)]
            })
            .unwrap_err(),
        AdmissionError::OriginUnavailable
    );
    let other_backend = setup(graph());
    assert_eq!(
        commands
            .enqueue_native(request(&other_backend, vec![mute(10, true)]))
            .unwrap_err(),
        AdmissionError::OriginUnavailable
    );
    drain(&commands, &graph(), |_| {
        panic!("rejected origins must never execute")
    });
}

#[test]
fn admission_and_local_submission_are_not_graph_observation_or_server_success() {
    let live = graph();
    let commands = setup(live.clone());
    let receipt = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    assert_eq!(receipt.report().progress, NativeProgress::Queued);
    drain(&commands, &live, |_| LocalExecution::Submitted);
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::ExecutedLocally(Counts {
            submitted: 1,
            observed: 0,
            ..
        })
    ));
    let mut unrelated = live.clone();
    unrelated.quantum = Some(256);
    commands.publish(unrelated);
    assert!(!receipt.report().progress.is_terminal());
    let mut observed = live;
    observed.knopen[0].mute = Some(true);
    let stamp = commands.publish(observed).stamp;
    assert!(
        matches!(receipt.report().progress, NativeProgress::Observed { stamp: actual, counts: Counts { submitted: 1, observed: 1, .. } } if actual == stamp)
    );
}

#[test]
fn nonempty_confirmed_no_op_publishes_fresh_graph_and_releases_history_busy() {
    use pecan_core::domain::history::{Action, Controller, Input};
    use pecan_core::history_backend::{bind_history_plan, HistoryResults};
    let live = graph();
    let commands = setup(live.clone());
    let origin = commands.current();
    let mut history = Controller::default();
    history.reduce(Input::Graph(origin.clone())).unwrap();
    let plan = history
        .reduce(Input::Execute {
            label: "Synthetic no-op".into(),
            action: Action::Mute {
                node: 10,
                muted: false,
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(plan.commands.len(), 1);
    let receipt = commands
        .enqueue_native(bind_history_plan(origin.clone(), plan).unwrap())
        .unwrap();
    let mut bridge = HistoryResults::default();
    assert!(bridge.terminal(&receipt.report()).is_none());
    drain(&commands, &live, |_| {
        panic!("confirmed no-op must not issue an RPC")
    });
    assert!(bridge.terminal(&receipt.report()).is_none());
    assert!(commands.native_needs_barrier());
    commands.publish(live);
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::ObservedNoOp {
            counts: Counts {
                submitted: 0,
                no_ops: 1,
                observed: 1,
                ..
            },
            ..
        }
    ));
    history
        .reduce(bridge.terminal(&receipt.report()).unwrap())
        .unwrap();
    assert!(
        history.busy(),
        "result still requires delivery of its fresh graph"
    );
    history.reduce(Input::Graph(commands.current())).unwrap();
    assert!(!history.busy());
    assert_eq!(history.lengths(), (1, 0));
    assert!(bridge.terminal(&receipt.report()).is_none());
    assert!(commands.current().stamp.revision > origin.stamp.revision);
}

#[test]
fn queued_replacement_is_rejected_even_if_replacement_satisfies_the_goal() {
    let mut live = graph();
    let commands = setup(live.clone());
    let receipt = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    commands.object_removed(10);
    commands.object_added(10);
    live.knopen[0].serial = Some(88_888);
    live.knopen[0].mute = Some(true);
    commands.publish(live.clone());
    drain(&commands, &live, |_| panic!("must not mutate replacement"));
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::InvalidObject,
            counts: Counts { attempted: 0, .. }
        }
    ));
}

#[test]
fn disconnect_and_stop_resolve_queued_receipts_without_claiming_completion() {
    let live = graph();
    let commands = setup(live.clone());
    let queued = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    commands.stop();
    assert!(matches!(
        queued.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::Stopped,
            ..
        }
    ));
    assert_eq!(
        commands
            .enqueue_native(request(&commands, vec![]))
            .unwrap_err(),
        AdmissionError::Stopped
    );
    let commands = setup(live.clone());
    let submitted = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    drain(&commands, &live, |_| LocalExecution::Submitted);
    commands.publish(Momentopname::default());
    assert!(matches!(
        submitted.report().progress,
        NativeProgress::Unconfirmed {
            reason: NativeFailure::Disconnected,
            counts: Counts { submitted: 1, .. }
        }
    ));
    assert_eq!(
        commands
            .enqueue_native(request(&commands, vec![]))
            .unwrap_err(),
        AdmissionError::Disconnected
    );
}

#[test]
fn local_rejection_unreported_execution_and_timeout_are_distinct_terminal_results() {
    let live = graph();
    let commands = setup(live.clone());
    let rejected = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    drain(&commands, &live, |_| {
        LocalExecution::Rejected("synthetic binding unavailable".into())
    });
    assert!(matches!(
        rejected.report().progress,
        NativeProgress::Rejected {
            counts: Counts {
                attempted: 1,
                submitted: 0,
                in_progress: 0,
                ..
            },
            ..
        }
    ));
    let unknown = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    drain(&commands, &live, |_| LocalExecution::Unreported);
    assert!(matches!(
        unknown.report().progress,
        NativeProgress::Unconfirmed {
            counts: Counts { in_progress: 1, .. },
            ..
        }
    ));
    let now = Instant::now();
    let timed = commands
        .enqueue_native_at(request(&commands, vec![mute(10, true)]), now)
        .unwrap();
    drain(&commands, &live, |_| LocalExecution::Submitted);
    commands.expire_native(now + OBSERVATION_LIMIT);
    assert!(matches!(
        timed.report().progress,
        NativeProgress::Unconfirmed {
            reason: NativeFailure::ObservationTimedOut,
            counts: Counts { submitted: 1, .. }
        }
    ));
    let old_terminal = timed.report();
    let mut late = live;
    late.knopen[0].mute = Some(true);
    commands.publish(late);
    assert_eq!(
        timed.report(),
        old_terminal,
        "late graphs cannot rewrite a terminal report"
    );
}

#[test]
fn partial_batch_has_no_atomic_success_and_cancels_only_its_remaining_commands() {
    let live = graph();
    let commands = setup(live.clone());
    let receipt = commands
        .enqueue_native(request(
            &commands,
            vec![
                mute(10, true),
                mute(20, true),
                Opdracht::ZetVolume {
                    knoop: 10,
                    volume: vec![0.5],
                },
            ],
        ))
        .unwrap();
    commands
        .legacy(Opdracht::StopBus("synthetic-other-work".into()))
        .unwrap();
    let mut executed = Vec::new();
    drain(&commands, &live, |c| {
        executed.push(c.clone());
        if c == mute(20, true) {
            LocalExecution::Rejected("synthetic local failure".into())
        } else {
            LocalExecution::Submitted
        }
    });
    assert_eq!(
        executed,
        [
            mute(10, true),
            mute(20, true),
            Opdracht::StopBus("synthetic-other-work".into())
        ]
    );
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::Unconfirmed {
            counts: Counts {
                attempted: 2,
                submitted: 1,
                ..
            },
            ..
        }
    ));
}

#[test]
fn limits_are_bounded_and_native_batch_admission_is_all_or_nothing() {
    let commands = setup(graph());
    for i in 0..NORMAL_CAPACITY - 1 {
        commands
            .legacy(Opdracht::StopBus(format!("synthetic-{i}")))
            .unwrap();
    }
    assert_eq!(
        commands
            .enqueue_native(request(&commands, vec![mute(10, true), mute(20, true)]))
            .unwrap_err(),
        AdmissionError::QueueFull
    );
    assert!(commands.0.lock().unwrap().native.is_empty());
    assert_eq!(
        commands.0.lock().unwrap().pending.len(),
        NORMAL_CAPACITY - 1
    );
    commands.legacy(Opdracht::Stop).unwrap(); // Stop reservation remains available.
    let commands = setup(graph());
    let now = Instant::now();
    let mut receipts = Vec::new();
    for _ in 0..MAX_NATIVE_IN_FLIGHT {
        receipts.push(
            commands
                .enqueue_native_at(request(&commands, vec![]), now)
                .unwrap(),
        );
    }
    assert_eq!(
        commands
            .enqueue_native(request(&commands, vec![]))
            .unwrap_err(),
        AdmissionError::TooManyInFlight
    );
    commands.expire_native(now + OBSERVATION_LIMIT);
    assert!(receipts.iter().all(|r| r.report().progress.is_terminal()));
    assert!(commands.0.lock().unwrap().native.is_empty());
    let old = commands.current();
    for _ in 0..NATIVE_SOURCE_HISTORY {
        commands.publish(graph());
    }
    assert_eq!(
        commands
            .enqueue_native(NativeIntent {
                ticket: 2,
                origin: old,
                commands: vec![mute(10, true)]
            })
            .unwrap_err(),
        AdmissionError::OriginUnavailable
    );
}

#[test]
fn invalid_native_input_and_unsupported_recording_commands_are_not_dispatched() {
    let commands = setup(graph());
    for command in [
        Opdracht::Start,
        Opdracht::Stop,
        Opdracht::ZetBus {
            kanalen: 1,
            gain: vec![1.0],
        },
        Opdracht::ZetTransport(pecan_core::graph::Opties {
            bussen: vec![],
            midi: false,
            klok: None,
            start_stop: false,
        }),
        Opdracht::StartBus(pecan_core::graph::BusOpname {
            naam: "Synthetic bus".into(),
            kanalen: vec![0],
        }),
        Opdracht::StopBus("Synthetic bus".into()),
    ] {
        assert_eq!(
            commands
                .enqueue_native(request(&commands, vec![command]))
                .unwrap_err(),
            AdmissionError::UnsupportedCommand
        );
    }
    for command in [
        Opdracht::ZetVolume {
            knoop: 10,
            volume: vec![f32::NAN],
        },
        Opdracht::ZetDoel {
            knoop: 10,
            doel: Some("synthetic\0target".into()),
        },
    ] {
        assert_eq!(
            commands
                .enqueue_native(request(&commands, vec![command]))
                .unwrap_err(),
            AdmissionError::InvalidInput
        );
    }
    assert_eq!(
        commands
            .enqueue_native(request(
                &commands,
                vec![mute(10, true); MAX_NATIVE_COMMANDS + 1]
            ))
            .unwrap_err(),
        AdmissionError::TooManyCommands
    );
}

#[test]
fn cancelled_claim_can_finish_but_receipt_remains_explicitly_unconfirmed() {
    let live = graph();
    let commands = setup(live);
    let receipt = commands
        .enqueue_native(request(&commands, vec![mute(10, true), mute(20, true)]))
        .unwrap();
    let mut calls = 0;
    assert!(commands
        .drain_with(
            |_| {
                commands.stop();
                true
            },
            |_| false,
            |_| {
                calls += 1;
                LocalExecution::Submitted
            }
        )
        .is_empty());
    assert_eq!(calls, 1);
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::Unconfirmed {
            reason: NativeFailure::Stopped,
            counts: Counts {
                attempted: 1,
                in_progress: 1,
                ..
            }
        }
    ));
}

#[test]
fn target_serial_and_name_keep_the_identity_observed_at_origin() {
    for by_name in [false, true] {
        let live = graph();
        let target = if by_name {
            live.knopen[1].naam.clone()
        } else {
            live.knopen[1].serial.unwrap().to_string()
        };
        let commands = setup(live.clone());
        let origin = commands.current();
        let op = Opdracht::ZetDoel {
            knoop: 10,
            doel: Some(target.clone()),
        };
        let receipt = commands
            .enqueue_native(NativeIntent {
                ticket: 1,
                origin: origin.clone(),
                commands: vec![op.clone()],
            })
            .unwrap();
        commands.object_removed(20);
        commands.object_added(20);
        let mut replacement = live;
        replacement.knopen[1].serial = Some(99_999);
        replacement.knopen[0].doel = Some(target);
        commands.publish(replacement.clone());
        assert_eq!(
            commands
                .enqueue_native(NativeIntent {
                    ticket: 2,
                    origin,
                    commands: vec![op]
                })
                .unwrap_err(),
            AdmissionError::InvalidObject
        );
        drain(&commands, &replacement, |_| {
            panic!("target replacement must not receive old intent")
        });
        assert!(matches!(
            receipt.report().progress,
            NativeProgress::Rejected {
                reason: NativeFailure::InvalidObject,
                counts: Counts { attempted: 0, .. }
            }
        ));
    }
}

#[test]
fn mixed_batch_requires_all_postconditions_in_the_same_fresh_publication() {
    let live = graph();
    let commands = setup(live.clone());
    let receipt = commands
        .enqueue_native(request(&commands, vec![mute(10, true), mute(20, true)]))
        .unwrap();
    drain(&commands, &live, |_| LocalExecution::Submitted);
    let mut first = live.clone();
    first.knopen[0].mute = Some(true);
    commands.publish(first);
    assert!(!receipt.report().progress.is_terminal());
    let mut second = live.clone();
    second.knopen[1].mute = Some(true);
    commands.publish(second);
    assert!(
        !receipt.report().progress.is_terminal(),
        "observations from incompatible frames must not add up to success"
    );
    let mut both = live;
    for node in &mut both.knopen {
        node.mute = Some(true);
    }
    let observed = commands.publish(both).stamp;
    assert!(
        matches!(receipt.report().progress, NativeProgress::Observed { stamp, counts: Counts { submitted: 2, observed: 2, .. } } if stamp == observed)
    );
}

#[test]
fn target_outcome_missing_from_published_mirror_stays_unconfirmed() {
    let live = graph();
    let commands = setup(live.clone());
    let now = Instant::now();
    // The legacy sentinel may be consumed by policy between graph frames.
    // This probe supplies only the final frame, not a claimed server trace.
    let receipt = commands
        .enqueue_native_at(
            request(
                &commands,
                vec![Opdracht::ZetDoel {
                    knoop: 10,
                    doel: Some("-1".into()),
                }],
            ),
            now,
        )
        .unwrap();
    drain(&commands, &live, |_| LocalExecution::Submitted);
    commands.publish(live);
    assert!(!receipt.report().progress.is_terminal());
    commands.expire_native(now + OBSERVATION_LIMIT);
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::Unconfirmed {
            reason: NativeFailure::ObservationTimedOut,
            counts: Counts {
                submitted: 1,
                observed: 0,
                ..
            }
        }
    ));
}

#[test]
fn retained_receipts_do_not_retain_backend_work_or_evicted_graphs() {
    let live = graph();
    let commands = setup(live.clone());
    let first = commands.current();
    let released = Arc::downgrade(&first.value);
    drop(first);
    let mut retained = Vec::new();
    for _ in 0..NATIVE_SOURCE_HISTORY + 4 {
        for _ in 0..MAX_NATIVE_IN_FLIGHT {
            retained.push(
                commands
                    .enqueue_native(request(&commands, vec![mute(10, false)]))
                    .unwrap(),
            );
        }
        assert_eq!(
            commands
                .enqueue_native(request(&commands, vec![mute(10, false)]))
                .unwrap_err(),
            AdmissionError::TooManyInFlight
        );
        drain(&commands, &live, |_| {
            panic!("synthetic no-ops must not submit")
        });
        assert!(commands.native_needs_barrier());
        commands.publish(live.clone());
        let state = commands.0.lock().unwrap();
        assert!(state.native.is_empty());
        assert!(state.pending.is_empty());
        assert!(state.origins.len() <= NATIVE_SOURCE_HISTORY);
    }
    assert!(retained
        .iter()
        .all(|r| matches!(r.report().progress, NativeProgress::ObservedNoOp { .. })));
    assert!(
        released.upgrade().is_none(),
        "caller-owned receipts must not keep expired source graphs alive"
    );
    let first_report = retained[0].report();
    commands.stop();
    assert_eq!(
        retained[0].report(),
        first_report,
        "later stop must not rewrite completed receipts"
    );
}

#[test]
fn reconnect_reclaims_native_work_and_new_epoch_no_ops_progress() {
    let live = graph();
    let commands = setup(live.clone());
    let old_origin = commands.current();
    let old = commands
        .enqueue_native(request(&commands, vec![mute(10, true)]))
        .unwrap();
    commands.publish(Momentopname::default());
    assert!(matches!(
        old.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::Disconnected,
            ..
        }
    ));
    {
        let state = commands.0.lock().unwrap();
        assert!(state.native.is_empty());
        assert!(state.pending.is_empty());
    }
    for id in live
        .knopen
        .iter()
        .map(|n| n.id)
        .chain(live.poorten.iter().map(|p| p.id))
    {
        commands.object_added(id);
    }
    commands.publish(live.clone());
    assert_eq!(
        commands
            .enqueue_native(NativeIntent {
                ticket: 2,
                origin: old_origin,
                commands: vec![mute(10, true)]
            })
            .unwrap_err(),
        AdmissionError::StaleEpoch
    );
    let new_origin = commands.current();
    let fresh = commands
        .enqueue_native(request(&commands, vec![mute(10, false)]))
        .unwrap();
    drain(&commands, &live, |_| {
        panic!("new-epoch no-op is already satisfied")
    });
    commands.publish(live);
    assert!(
        matches!(fresh.report().progress, NativeProgress::ObservedNoOp { stamp, .. } if stamp.epoch == new_origin.stamp.epoch && stamp.revision > new_origin.stamp.revision)
    );
}

#[test]
fn expired_queued_native_work_never_submits_and_releases_capacity() {
    let live = graph();
    let commands = setup(live.clone());
    let old = Instant::now() - OBSERVATION_LIMIT - std::time::Duration::from_secs(1);
    let receipt = commands
        .enqueue_native_at(
            request(&commands, vec![mute(10, true), mute(20, true)]),
            old,
        )
        .unwrap();
    drain(&commands, &live, |_| {
        panic!("expired queue must never execute")
    });
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::ObservationTimedOut,
            counts: Counts {
                attempted: 0,
                submitted: 0,
                ..
            }
        }
    ));
    let state = commands.0.lock().unwrap();
    assert!(state.native.is_empty());
    assert!(state.pending.is_empty());
}

#[test]
fn native_payload_bounds_reject_oversized_values_without_consuming_capacity() {
    assert_eq!("synthetic-value-".repeat(256).len(), 4096);
    let commands = setup(graph());
    let volume = commands
        .enqueue_native(request(
            &commands,
            vec![Opdracht::ZetVolume {
                knoop: 10,
                volume: vec![1.0; 256],
            }],
        ))
        .unwrap();
    let target = commands
        .enqueue_native(request(
            &commands,
            vec![Opdracht::ZetDoel {
                knoop: 10,
                doel: Some("synthetic-value-".repeat(256)),
            }],
        ))
        .unwrap();
    // The repeated target string is exactly 4096 synthetic bytes.
    for command in [
        Opdracht::ZetVolume {
            knoop: 10,
            volume: vec![1.0; 257],
        },
        Opdracht::ZetVolume {
            knoop: 10,
            volume: vec![4.01],
        },
        Opdracht::ZetDoel {
            knoop: 10,
            doel: Some("synthetic-value-".repeat(256) + "x"),
        },
    ] {
        assert_eq!(
            commands
                .enqueue_native(request(&commands, vec![command]))
                .unwrap_err(),
            AdmissionError::InvalidInput
        );
    }
    {
        let state = commands.0.lock().unwrap();
        assert_eq!(state.native.len(), 2);
        assert_eq!(state.pending.len(), 2);
    }
    commands.stop();
    assert!(matches!(
        volume.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::Stopped,
            ..
        }
    ));
    assert!(matches!(
        target.report().progress,
        NativeProgress::Rejected {
            reason: NativeFailure::Stopped,
            ..
        }
    ));
}
