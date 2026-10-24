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
