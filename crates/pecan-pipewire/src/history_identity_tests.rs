//! Production registry/queue seams with synthetic graphs; no PipeWire session.
use super::*;
use pecan_core::{
    domain::history::{Action, Controller, Input},
    history_backend::{bind_history_plan, HistoryResults},
    native_command::{AdmissionError, NativeProgress, NativeReceipt},
};

fn setup() -> (Commands, Momentopname) {
    let graph: Momentopname =
        serde_json::from_str(include_str!("../../../fixtures/domain/audio-policy.json")).unwrap();
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
    commands.publish(graph.clone());
    (commands, graph)
}
fn frame(commands: &Commands, c: &mut Controller) {
    c.reduce(Input::NativeGraph(
        commands.history_graph(&commands.current()).unwrap(),
    ))
    .unwrap();
}
fn terminal(c: &mut Controller, receipt: &NativeReceipt) {
    c.reduce(
        HistoryResults::default()
            .terminal(&receipt.report())
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn identity_accessor_rejects_copies_other_owners_and_evicted_publications() {
    let (commands, graph) = setup();
    let (other, _) = setup();
    let original = commands.current();
    assert!(commands.history_graph(&original).is_ok());
    let copy = Stamped {
        stamp: original.stamp,
        value: Arc::new((*original.value).clone()),
    };
    assert!(matches!(
        commands.history_graph(&copy),
        Err(AdmissionError::OriginUnavailable)
    ));
    assert!(matches!(
        other.history_graph(&original),
        Err(AdmissionError::OriginUnavailable)
    ));
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY {
        commands.publish(graph.clone());
    }
    assert!(matches!(
        commands.history_graph(&original),
        Err(AdmissionError::OriginUnavailable)
    ));
}

#[test]
fn retained_history_survives_origin_cache_eviction_without_retaining_old_graphs() {
    let (commands, mut graph) = setup();
    let mut c = Controller::default();
    let original = commands.current();
    let weak = Arc::downgrade(&original.value);
    frame(&commands, &mut c);
    c.reduce(Input::RecordVolume {
        node: 20,
        before: vec![0.5, 0.5],
        after: vec![1.0, 1.0],
    })
    .unwrap();
    drop(original);
    for _ in 0..200 {
        commands.publish(graph.clone());
        frame(&commands, &mut c);
    }
    assert!(
        weak.upgrade().is_none(),
        "history guards retain generations, not historical graph Arcs"
    );
    assert_eq!(
        commands.0.lock().unwrap().origins.len(),
        pecan_core::native_command::NATIVE_SOURCE_HISTORY
    );
    let plan = c.reduce(Input::Undo).unwrap().unwrap();
    let receipt = commands
        .enqueue_native(bind_history_plan(commands.current(), plan).unwrap())
        .unwrap();
    let mut executed = vec![];
    commands.drain_with(
        |_| true,
        |_| false,
        |op| {
            executed.push(op);
            LocalExecution::Submitted
        },
    );
    assert_eq!(
        executed,
        [Opdracht::ZetVolume {
            knoop: 20,
            volume: vec![0.5, 0.5]
        }]
    );
    graph.knopen.iter_mut().find(|n| n.id == 20).unwrap().volume = Some(vec![0.5, 0.5]);
    commands.publish(graph);
    frame(&commands, &mut c);
    terminal(&mut c, &receipt);
    assert_eq!(c.lengths(), (0, 1));
}

#[test]
fn coalesced_identical_port_reuse_invalidates_history_at_the_production_source() {
    let (commands, mut graph) = setup();
    let mut c = Controller::default();
    frame(&commands, &mut c);
    let p = c
        .reduce(Input::Execute {
            label: "Synthetic disconnect".into(),
            action: Action::Disconnect(vec![[200, 100]]),
        })
        .unwrap()
        .unwrap();
    let receipt = commands
        .enqueue_native(bind_history_plan(commands.current(), p).unwrap())
        .unwrap();
    commands.drain_with(|_| true, |_| false, |_| LocalExecution::Submitted);
    graph.kabels.retain(|l| l.id != 1);
    commands.object_removed(1);
    commands.publish(graph.clone());
    frame(&commands, &mut c);
    terminal(&mut c, &receipt);
    assert_eq!(c.lengths(), (1, 0));
    let original = commands.current();
    commands.object_removed(200);
    commands.object_added(200);
    commands.publish(graph);
    assert_eq!(
        original.value,
        commands.current().value,
        "replacement is invisible in serialized graph data"
    );
    frame(&commands, &mut c);
    assert_eq!(c.lengths(), (0, 0));
    assert!(c.reduce(Input::Undo).unwrap().is_none());
}

#[test]
fn replacement_after_undo_planning_is_rejected_before_native_admission() {
    let (commands, graph) = setup();
    let mut c = Controller::default();
    frame(&commands, &mut c);
    c.reduce(Input::RecordVolume {
        node: 20,
        before: vec![0.5, 0.5],
        after: vec![1.0, 1.0],
    })
    .unwrap();
    let plan = c.reduce(Input::Undo).unwrap().unwrap();
    let source = commands.current();
    let ticket = plan.ticket;
    commands.object_removed(20);
    commands.object_added(20); // No graph publication yet.
    let reason = commands
        .enqueue_native(bind_history_plan(source.clone(), plan).unwrap())
        .unwrap_err();
    assert_eq!(reason, AdmissionError::InvalidObject);
    let input = HistoryResults::default()
        .admission_rejected(ticket, source.stamp, reason)
        .unwrap();
    assert!(matches!(
        c.reduce(input),
        Err(pecan_core::domain::history::DomainError::Backend(_))
    ));
    assert!(!c.busy());
    assert!(commands.0.lock().unwrap().pending.is_empty());
    commands.publish(graph);
    frame(&commands, &mut c);
    assert_eq!(c.lengths(), (0, 0));
}

#[test]
fn unchanged_identity_noop_undo_uses_observation_barrier_and_reconnect_clears_redo() {
    let (commands, graph) = setup();
    let mut c = Controller::default();
    frame(&commands, &mut c);
    let values = graph
        .knopen
        .iter()
        .find(|n| n.id == 20)
        .unwrap()
        .volume
        .clone()
        .unwrap();
    c.reduce(Input::RecordVolume {
        node: 20,
        before: values,
        after: vec![0.1, 0.1],
    })
    .unwrap();
    let plan = c.reduce(Input::Undo).unwrap().unwrap();
    let receipt = commands
        .enqueue_native(bind_history_plan(commands.current(), plan).unwrap())
        .unwrap();
    commands.drain_with(
        |_| true,
        |_| true,
        |_| panic!("already-satisfied undo must not call the local executor"),
    );
    assert!(!receipt.report().progress.is_terminal());
    commands.publish(graph.clone());
    assert!(matches!(
        receipt.report().progress,
        NativeProgress::ObservedNoOp { .. }
    ));
    terminal(&mut c, &receipt);
    assert!(c.busy(), "receipt alone cannot substitute the graph frame");
    frame(&commands, &mut c);
    assert_eq!(c.lengths(), (0, 1));
    commands.publish(Momentopname::default());
    frame(&commands, &mut c);
    assert_eq!(c.lengths(), (0, 0));
    commands.publish(graph);
    frame(&commands, &mut c);
    assert!(c.reduce(Input::Redo).unwrap().is_none());
}
