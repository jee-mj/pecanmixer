use super::*;
use crate::application::{ApplicationOwner, FixtureDriver, FixtureRequest};
use pecan_core::{
    domain::history::Action,
    graph::{Momentopname, Opdracht},
    native_command::{Counts, NativeFailure},
};
use std::{collections::BTreeMap, sync::Arc};

fn ids(g: &Momentopname) -> BTreeMap<u32, u64> {
    g.knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect()
}
fn apply(s: &Session, d: &FixtureDriver, r: &FixtureRequest) -> Graph {
    let mut g = (*s.graph().unwrap().value).clone();
    for command in &r.intent.commands {
        match command {
            Opdracht::ZetMute { knoop, mute } => {
                g.knopen.iter_mut().find(|n| n.id == *knoop).unwrap().mute = Some(*mute)
            }
            _ => panic!("synthetic mute-only queue"),
        }
    }
    let graph = d.publish(g.clone(), ids(&g));
    r.reporter.seal_execution();
    r.reporter.set(NativeProgress::Observed {
        stamp: graph.stamp,
        counts: Counts {
            attempted: 1,
            submitted: 1,
            observed: 1,
            ..Default::default()
        },
    });
    s.poll();
    graph
}
fn setup() -> (ApplicationOwner, Session, FixtureDriver, HistoryQueue) {
    let owner = ApplicationOwner::fixture();
    let s = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let g: Momentopname = serde_json::from_str(include_str!(
        "../../../../../fixtures/graph/audio-basic.json"
    ))
    .unwrap();
    d.publish(g.clone(), ids(&g));
    s.poll();
    for node in [10, 20] {
        s.execute(
            &s.graph().unwrap(),
            "Synthetic mute",
            Action::Mute { node, muted: true },
        )
        .unwrap()
        .unwrap();
        let r = d.take_requests().pop().unwrap();
        apply(&s, &d, &r);
    }
    assert_eq!(s.history_lengths(), (2, 0));
    let q = HistoryQueue::new(s.clone());
    (owner, s, d, q)
}
fn command(r: &FixtureRequest, node: u32, muted: bool) {
    assert_eq!(
        r.intent.commands,
        vec![Opdracht::ZetMute {
            knoop: node,
            mute: muted
        }]
    );
}

#[test]
fn queued_history_mixed_directions_wait_for_real_completion_and_spacing_then_use_current_authority()
{
    let (_o, s, d, q) = setup();
    let start = s.graph().unwrap();
    q.enqueue(&start, true).unwrap();
    q.enqueue(&start, true).unwrap();
    q.enqueue(&start, false).unwrap();
    let first = d.take_requests().pop().unwrap();
    command(&first, 20, false);
    let due = q.state.borrow().next;
    for _ in 0..20 {
        q.tick_at(due + Duration::from_secs(5));
        assert!(d.take_requests().is_empty());
    }
    let graph = apply(&s, &d, &first);
    q.tick_at(due - Duration::from_nanos(1));
    assert!(d.take_requests().is_empty());
    q.tick_at(due);
    let second = d.take_requests().pop().unwrap();
    command(&second, 10, false);
    assert_eq!(second.intent.origin.stamp, graph.stamp);
    assert!(Arc::ptr_eq(&second.intent.origin.value, &graph.value));
    apply(&s, &d, &second);
    q.tick_at(due + SPACING);
    let third = d.take_requests().pop().unwrap();
    command(&third, 10, true);
    apply(&s, &d, &third);
    q.tick_at(due + SPACING * 2);
    assert!(!q.has_work());
    assert_eq!(s.history_lengths(), (1, 1));
    assert!(d.take_requests().is_empty());
}
#[test]
fn queued_history_observed_receipt_without_graph_or_local_closure_cannot_advance() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, true).unwrap();
    q.enqueue(&g, true).unwrap();
    let first = d.take_requests().pop().unwrap();
    let call = first.reporter.claim_execution().unwrap();
    first.reporter.seal_execution();
    let mut expected = g.stamp;
    expected.revision += 1;
    first.reporter.set(NativeProgress::Observed {
        stamp: expected,
        counts: Counts::default(),
    });
    s.poll();
    q.tick_at(Instant::now() + Duration::from_secs(1));
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (2, 0));
    let mut value = (*g.value).clone();
    value.knopen.iter_mut().find(|n| n.id == 20).unwrap().mute = Some(false);
    let next = d.publish(value.clone(), ids(&value));
    assert_eq!(next.stamp, expected);
    s.poll();
    q.tick_at(Instant::now() + Duration::from_secs(1));
    assert!(d.take_requests().is_empty());
    drop(call);
    q.tick_at(Instant::now() + Duration::from_secs(1));
    command(&d.take_requests().pop().unwrap(), 10, false);
}
#[test]
fn queued_history_empty_direction_is_noop_and_still_has_legacy_spacing() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, false).unwrap();
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (2, 0));
    q.enqueue(&g, true).unwrap();
    assert!(d.take_requests().is_empty());
    let due = q.state.borrow().next;
    q.tick_at(due - Duration::from_nanos(1));
    assert!(d.take_requests().is_empty());
    q.tick_at(due);
    command(&d.take_requests().pop().unwrap(), 20, false);
}
#[test]
fn queued_history_overflow_preserves_fifo_and_quit_cancels_only_unsent_inputs() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, true).unwrap();
    for _ in 0..HISTORY_LIMIT {
        q.enqueue(&g, false).unwrap();
    }
    assert!(q.enqueue(&g, true).unwrap_err().contains("full"));
    assert_eq!(q.state.borrow().directions.len(), HISTORY_LIMIT);
    let first = d.take_requests().pop().unwrap();
    s.close_admission();
    q.tick();
    assert!(q.state.borrow().directions.is_empty());
    assert!(q.has_work());
    apply(&s, &d, &first);
    q.tick_at(Instant::now() + Duration::from_secs(2));
    assert!(!q.has_work());
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (1, 1));
}
#[test]
fn queued_history_reconnect_cancels_remaining_directions_without_replaying_old_targets() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, true).unwrap();
    q.enqueue(&g, true).unwrap();
    let first = d.take_requests().pop().unwrap();
    let mut value = (*g.value).clone();
    value.verbonden = false;
    d.publish(value.clone(), ids(&value));
    s.poll();
    q.tick();
    first.reporter.seal_execution();
    first.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::Stopped,
        counts: Counts::default(),
    });
    value.verbonden = true;
    d.publish(value.clone(), ids(&value));
    s.poll();
    q.tick();
    s.poll();
    q.tick();
    assert!(q.state.borrow().directions.is_empty());
    assert!(d.take_requests().is_empty());
    assert_eq!(s.history_lengths(), (0, 0));
    q.enqueue(&s.graph().unwrap(), true).unwrap();
    q.tick_at(Instant::now() + Duration::from_secs(1));
    assert!(d.take_requests().is_empty());
}
#[test]
fn queued_history_failure_cancels_remainder_and_keeps_real_partial_receipt_for_explicit_recovery() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, true).unwrap();
    q.enqueue(&g, true).unwrap();
    let first = d.take_requests().pop().unwrap();
    let call = first.reporter.claim_execution().unwrap();
    first.reporter.seal_execution();
    first.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            in_progress: 1,
            ..Default::default()
        },
    });
    s.poll();
    q.tick();
    let failed = q.recovery().unwrap();
    let frozen = failed.native_report();
    for _ in 0..20 {
        s.poll();
        q.tick_at(Instant::now() + Duration::from_secs(5));
        assert!(d.take_requests().is_empty());
    }
    assert!(!q.has_work());
    assert!(q.enqueue(&g, true).is_err());
    assert_eq!(s.history_lengths(), (2, 0));
    assert!(s.recovery_ready(&failed, &g).is_err());
    drop(call);
    assert!(s.recovery_ready(&failed, &g).is_ok());
    assert_eq!(
        q.state.borrow().last.as_ref().unwrap().native_report(),
        frozen
    );
}
#[test]
fn queued_history_direction_resolves_guarded_current_stack_never_replaced_numeric_objects() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    q.enqueue(&g, true).unwrap();
    q.enqueue(&g, true).unwrap();
    let first = d.take_requests().pop().unwrap();
    let applied = apply(&s, &d, &first);
    let mut generations = ids(&applied.value);
    generations.insert(10, 100_010);
    d.publish((*applied.value).clone(), generations);
    s.poll();
    q.tick_at(Instant::now() + Duration::from_secs(1));
    assert!(
        d.take_requests().is_empty(),
        "pruned step never dispatches to replacement node 10"
    );
    assert_eq!(s.history_lengths(), (0, 1));
}

#[test]
fn queued_history_copied_input_and_configuration_lease_cannot_bypass_shared_admission() {
    let (_o, s, d, q) = setup();
    let g = s.graph().unwrap();
    let copied = Graph {
        stamp: g.stamp,
        value: Arc::new((*g.value).clone()),
    };
    assert!(q.enqueue(&copied, true).is_err());
    assert!(!q.has_work());
    let (_, lease) = s.reserve_external().unwrap();
    assert!(q.enqueue(&g, true).is_err());
    assert!(d.take_requests().is_empty());
    s.release_external(&lease);
    q.enqueue(&g, true).unwrap();
    q.enqueue(&g, true).unwrap();
    let first = d.take_requests().pop().unwrap();
    apply(&s, &d, &first);
    let (_, lease) = s.reserve_external().unwrap();
    q.tick_at(Instant::now() + Duration::from_secs(2));
    assert!(d.take_requests().is_empty());
    s.release_external(&lease);
    q.tick_at(Instant::now() + Duration::from_secs(2));
    command(&d.take_requests().pop().unwrap(), 10, false);
}

#[test]
fn queued_history_large_reset_parent_is_one_barrier_before_queued_undo_and_redo() {
    use crate::application::tests::large_reset as fixture;
    let owner = ApplicationOwner::fixture();
    let s = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let g = fixture::scene(25);
    let origin = d.publish(g.clone(), ids(&g));
    s.poll();
    s.execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let q = HistoryQueue::new(s.clone());
    q.enqueue(&origin, true).unwrap();
    q.enqueue(&origin, false).unwrap();
    let start = Instant::now();
    let mut children = 0;
    for i in 0..200 {
        let requests = d.take_requests();
        assert!(requests.len() <= 1);
        for request in requests {
            children += 1;
            fixture::apply(&d, &s, &request, false);
        }
        s.poll();
        q.tick_at(start + SPACING * i);
        if !q.has_work() {
            assert!(children > 6);
            assert_eq!(s.history_lengths(), (1, 0));
            assert!(
                matches!(q.state.borrow().last.as_ref().unwrap().report(),RequestReport::Sequence(r) if matches!(r.progress,SequenceProgress::Observed{..}))
            );
            return;
        }
    }
    panic!("bounded synthetic history queue failed to complete");
}
