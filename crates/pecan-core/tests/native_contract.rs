//! Synthetic contract tests. No user topology, hardware capture or runtime logs.
use pecan_core::backend::{Stamp, Stamped};
use pecan_core::domain::history::{Action, Controller, DomainError, Input};
use pecan_core::graph::Momentopname;
use pecan_core::history_backend::{bind_history_plan, HistoryResults};
use pecan_core::native_command::*;
use std::sync::Arc;

fn origin() -> Stamped<Arc<Momentopname>> {
    let graph = serde_json::from_value(serde_json::json!({
        "verbonden": true, "wireplumber": [], "knopen": [{ "id": 10, "naam": "synthetic.test-node", "mute": false, "volume": [1.0] }], "poorten": [], "kabels": []
    })).unwrap();
    Stamped {
        stamp: Stamp {
            epoch: 7,
            revision: 10,
        },
        value: Arc::new(graph),
    }
}
fn planned() -> (Controller, NativeIntent) {
    let origin = origin();
    let mut history = Controller::default();
    history.reduce(Input::Graph(origin.clone())).unwrap();
    let plan = history
        .reduce(Input::Execute {
            label: "Synthetic mute".into(),
            action: Action::Mute {
                node: 10,
                muted: true,
            },
        })
        .unwrap()
        .unwrap();
    let intent = bind_history_plan(origin, plan).unwrap();
    (history, intent)
}

#[test]
fn binding_requires_the_history_source_epoch_and_revision() {
    let (_, intent) = planned();
    let plan = pecan_core::domain::history::CommandPlan {
        ticket: 1,
        epoch: 7,
        source_revision: 11,
        commands: intent.commands.clone(),
    };
    assert_eq!(
        bind_history_plan(intent.origin, plan).unwrap_err(),
        AdmissionError::OriginUnavailable
    );
}

#[test]
fn history_never_advances_on_admission_or_local_execution() {
    let (mut history, intent) = planned();
    let (receipt, producer) = native_receipt(intent.ticket, intent.origin.stamp, 1);
    let mut bridge = HistoryResults::default();
    assert!(bridge.terminal(&receipt.report()).is_none());
    producer.set(NativeProgress::ExecutedLocally(Counts {
        attempted: 1,
        submitted: 1,
        ..Default::default()
    }));
    assert!(bridge.terminal(&receipt.report()).is_none());
    assert!(history.busy());
    assert_eq!(history.lengths(), (0, 0));
    let observed = Stamp {
        revision: 11,
        ..intent.origin.stamp
    };
    producer.set(NativeProgress::Observed {
        stamp: observed,
        counts: Counts {
            attempted: 1,
            submitted: 1,
            observed: 1,
            ..Default::default()
        },
    });
    history
        .reduce(bridge.terminal(&receipt.report()).unwrap())
        .unwrap();
    assert!(
        history.busy(),
        "completion still requires observing its graph"
    );
    let mut graph = (*intent.origin.value).clone();
    graph.knopen[0].mute = Some(true);
    history
        .reduce(Input::Graph(Stamped {
            stamp: observed,
            value: Arc::new(graph),
        }))
        .unwrap();
    assert!(!history.busy());
    assert_eq!(history.lengths(), (1, 0));
    assert!(bridge.terminal(&receipt.report()).is_none());
}

#[test]
fn admission_rejections_release_busy_without_advancing_history() {
    for rejection in [
        AdmissionError::StaleEpoch,
        AdmissionError::Disconnected,
        AdmissionError::OriginUnavailable,
        AdmissionError::InvalidObject,
        AdmissionError::QueueFull,
        AdmissionError::Stopped,
    ] {
        let (mut history, intent) = planned();
        let mut bridge = HistoryResults::default();
        let input = bridge
            .admission_rejected(intent.ticket, intent.origin.stamp, rejection.clone())
            .unwrap();
        assert!(matches!(
            history.reduce(input),
            Err(DomainError::Backend(_))
        ));
        assert!(!history.busy());
        assert_eq!(history.lengths(), (0, 0));
        assert!(bridge
            .admission_rejected(intent.ticket, intent.origin.stamp, rejection)
            .is_none());
    }
}

#[test]
fn unconfirmed_effects_are_an_error_not_a_false_success_or_rollback_claim() {
    let (mut history, intent) = planned();
    let report = NativeReport {
        ticket: intent.ticket,
        source: intent.origin.stamp,
        command_count: 1,
        progress: NativeProgress::Unconfirmed {
            reason: NativeFailure::ObservationTimedOut,
            counts: Counts {
                attempted: 1,
                submitted: 1,
                ..Default::default()
            },
        },
    };
    let mut bridge = HistoryResults::default();
    match history.reduce(bridge.terminal(&report).unwrap()) {
        Err(DomainError::Backend(message)) => assert!(message.contains("effects may be present")),
        _ => panic!("unconfirmed submission must not be acknowledged"),
    }
    assert!(!history.busy());
    assert_eq!(history.lengths(), (0, 0));
}
