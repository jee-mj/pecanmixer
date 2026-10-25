//! Large synthetic scenes; no audio backend, writer, filesystem or new clock.
use super::*;
use pecan_core::{
    domain::history,
    history_backend::{SequenceProgress, SequenceReport},
    native_command::MAX_NATIVE_COMMANDS,
};
use std::collections::BTreeSet;

pub(in crate::application) fn scene(extra: usize) -> Momentopname {
    let mut g: Momentopname =
        serde_json::from_str(include_str!("../../routing/fixtures/graph.json")).unwrap();
    let template = g.knopen.iter().find(|n| n.id == 11).unwrap().clone();
    let ports: Vec<_> = g
        .poorten
        .iter()
        .filter(|p| p.knoop == 11 && !p.ingang)
        .cloned()
        .collect();
    for i in 0..extra as u32 {
        let mut n = template.clone();
        n.id = 2000 + i;
        n.serial = Some(100_000 + i as u64);
        n.naam = format!("synthetic.extra.{i}");
        g.knopen.push(n);
        for (j, p) in ports.iter().enumerate() {
            let mut p = p.clone();
            p.id = 10_000 + 2 * i + j as u32;
            p.knoop = 2000 + i;
            g.kabels.push(Kabel {
                id: 5_000 + 2 * i + j as u32,
                client: None,
                uit_knoop: p.knoop,
                uit_poort: p.id,
                in_knoop: 21,
                in_poort: 220 + j as u32,
            });
            g.poorten.push(p);
        }
    }
    g.knopen.iter_mut().find(|n| n.id == 50).unwrap().doel = Some("41001".into());
    g
}
fn large() -> (ApplicationOwner, Session, FixtureDriver, Graph) {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    let g = scene(25);
    let graph = driver.publish(g.clone(), generations(&g));
    session.poll();
    assert!(
        history::plan(
            &Action::BackToAuto,
            &graph.value,
            &default_idle_configuration().buses
        )
        .unwrap()
        .len()
            > 64
    );
    (owner, session, driver, graph)
}
fn report(request: &Request) -> SequenceReport {
    match request.report() {
        super::super::coordinator::RequestReport::Sequence(r) => r,
        _ => panic!("real parent evidence required"),
    }
}
pub(in crate::application) fn apply(
    driver: &FixtureDriver,
    session: &Session,
    request: &FixtureRequest,
    no_op: bool,
) -> Graph {
    let mut g = (*session.graph().unwrap().value).clone();
    for command in &request.intent.commands {
        match command {
            Opdracht::Ontkoppel { kabel } => g.kabels.retain(|l| l.id != *kabel),
            Opdracht::Verbind { uit, naar } => {
                if !g
                    .kabels
                    .iter()
                    .any(|l| l.uit_poort == *uit && l.in_poort == *naar)
                {
                    let from = g.poorten.iter().find(|p| p.id == *uit).unwrap().knoop;
                    let to = g.poorten.iter().find(|p| p.id == *naar).unwrap().knoop;
                    let id = g.kabels.iter().map(|l| l.id).max().unwrap_or(9000) + 1;
                    g.kabels.push(Kabel {
                        id,
                        client: None,
                        uit_knoop: from,
                        uit_poort: *uit,
                        in_knoop: to,
                        in_poort: *naar,
                    });
                }
            }
            Opdracht::ZetDoel { knoop, doel } => {
                g.knopen.iter_mut().find(|n| n.id == *knoop).unwrap().doel = doel.clone()
            }
            c => panic!("unexpected reset opcode {c:?}"),
        }
    }
    let graph = driver.publish(g.clone(), generations(&g));
    request.reporter.seal_execution();
    let counts = Counts {
        attempted: request.intent.commands.len(),
        observed: request.intent.commands.len(),
        submitted: if no_op {
            0
        } else {
            request.intent.commands.len()
        },
        no_ops: if no_op {
            request.intent.commands.len()
        } else {
            0
        },
        ..Default::default()
    };
    request.reporter.set(if no_op {
        NativeProgress::ObservedNoOp {
            stamp: graph.stamp,
            counts,
        }
    } else {
        NativeProgress::Observed {
            stamp: graph.stamp,
            counts,
        }
    });
    graph
}
fn finish(session: &Session, driver: &FixtureDriver, parent: &Request) -> Vec<Vec<Opdracht>> {
    let mut commands = vec![];
    for _ in 0..1000 {
        let submitted = driver.take_requests();
        assert!(submitted.len() <= 1, "Only one child may be outstanding");
        for child in submitted {
            assert!(
                !child.intent.commands.is_empty()
                    && child.intent.commands.len() <= MAX_NATIVE_COMMANDS
            );
            commands.push(child.intent.commands.clone());
            apply(driver, session, &child, false);
        }
        session.poll();
        match report(parent).progress {
            SequenceProgress::Observed { .. } => return commands,
            SequenceProgress::Failed { message } => panic!("Unexpected parent failure: {message}"),
            _ => assert!(session.busy()),
        }
    }
    panic!("bounded fixture did not converge")
}
fn pairs(g: &Momentopname) -> BTreeSet<[u32; 2]> {
    g.kabels.iter().map(|l| [l.uit_poort, l.in_poort]).collect()
}

#[test]
fn large_reset_and_inverse_use_bounded_children_but_one_history_entry_and_keep_rec_links() {
    let (_owner, session, driver, origin) = large();
    let original = pairs(&origin.value);
    let recording: BTreeSet<_> = origin
        .value
        .kabels
        .iter()
        .filter(|l| l.in_knoop == 30)
        .map(|l| [l.uit_poort, l.in_poort])
        .collect();
    let planned = history::plan(
        &Action::BackToAuto,
        &origin.value,
        &default_idle_configuration().buses,
    )
    .unwrap();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    assert!(
        parent.native_report().is_none(),
        "Do not invent a parent NativeReport"
    );
    assert!(session.reserve_external().is_err());
    assert!(session.undo(&origin).is_err());
    assert_eq!(session.history_lengths(), (0, 0));
    let chunks = finish(&session, &driver, &parent);
    assert_eq!(
        chunks.concat(),
        planned,
        "Preserve original command ordering"
    );
    assert!(chunks.len() > 2);
    assert_eq!(session.history_lengths(), (1, 0));
    let current = session.graph().unwrap();
    let rec: BTreeSet<_> = current
        .value
        .kabels
        .iter()
        .filter(|l| l.in_knoop == 30)
        .map(|l| [l.uit_poort, l.in_poort])
        .collect();
    assert_eq!(rec, recording);
    let legacy = pecan_core::domain::mixer::Mixer::build(
        &origin.value,
        &default_idle_configuration().buses.definitions().unwrap(),
    );
    let actual = pairs(&current.value);
    for id in &legacy.manual.links {
        let l = origin.value.kabels.iter().find(|l| l.id == *id).unwrap();
        assert!(!actual.contains(&[l.uit_poort, l.in_poort]));
    }
    assert!(legacy.manual.restore.iter().all(|p| actual.contains(p)));
    assert!(legacy.manual.targets.iter().all(|id| current
        .value
        .knopen
        .iter()
        .any(|n| n.id == *id && n.doel.is_none())));
    let undo = session.undo(&current).unwrap().unwrap();
    finish(&session, &driver, &undo);
    assert_eq!(session.history_lengths(), (0, 1));
    assert_eq!(pairs(&session.graph().unwrap().value), original);
    let redo = session.redo(&session.graph().unwrap()).unwrap().unwrap();
    finish(&session, &driver, &redo);
    assert_eq!(session.history_lengths(), (1, 0));
}

#[test]
fn large_reset_does_not_send_next_child_or_history_ack_before_current_observation() {
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    first.reporter.set(NativeProgress::ExecutedLocally(Counts {
        submitted: 32,
        ..Default::default()
    }));
    for _ in 0..100 {
        session.poll();
        assert!(driver.take_requests().is_empty());
        assert_eq!(session.history_lengths(), (0, 0));
    }
    apply(&driver, &session, &first, false);
    assert!(driver.take_requests().is_empty());
    session.poll();
    assert_eq!(driver.take_requests().len(), 1);
    assert!(matches!(
        report(&parent).progress,
        SequenceProgress::Running
    ));
    assert_eq!(session.history_lengths(), (0, 0));
}

#[test]
fn large_reset_failed_second_child_stops_future_batches_and_preserves_actual_reports_for_recovery()
{
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    apply(&driver, &session, &first, false);
    session.poll();
    let second = driver.take_requests().pop().unwrap();
    let lease = second.reporter.claim_execution().unwrap();
    second.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 2,
            in_progress: 1,
            ..Default::default()
        },
    });
    second.reporter.seal_execution();
    session.poll();
    assert!(matches!(
        report(&parent).progress,
        SequenceProgress::Failed { .. }
    ));
    assert_eq!(report(&parent).children.len(), 2);
    assert_eq!(session.history_lengths(), (0, 0));
    let retained = report(&parent).children[1].clone();
    for _ in 0..10 {
        session.poll();
        assert!(driver.take_requests().is_empty());
    }
    let current = session.graph().unwrap();
    assert!(session.recovery_ready(&parent, &current).is_err());
    drop(lease);
    assert!(session.recovery_ready(&parent, &current).is_ok());
    session
        .recover_request(&parent, &current, &default_idle_configuration().buses)
        .unwrap();
    assert_eq!(report(&parent).children[1], retained);
    assert!(!session.busy());
    assert!(driver.take_requests().is_empty());
}

#[test]
fn large_reset_replacement_between_children_stops_without_rebinding_original_intent() {
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    let applied = apply(&driver, &session, &first, false);
    let replacement = (*applied.value).clone();
    let mut ids = generations(&replacement);
    *ids.get_mut(&2024).unwrap() += 100000;
    driver.publish(replacement, ids);
    session.poll();
    assert!(driver.take_requests().is_empty());
    assert!(matches!(
        report(&parent).progress,
        SequenceProgress::Failed { .. }
    ));
    assert_eq!(session.history_lengths(), (0, 0));
}

#[test]
fn large_reset_duplicate_pairs_cross_chunks_in_original_order_and_undo_restores_pairs() {
    let owner = ApplicationOwner::fixture();
    let session = owner.activate();
    let driver = owner.fixture_driver().unwrap();
    let mut g = scene(0);
    let template = g.kabels.iter().find(|l| l.id == 1020).unwrap().clone();
    for id in 100_000..100_070 {
        let mut link = template.clone();
        link.id = id;
        g.kabels.push(link);
    }
    let original = pairs(&g);
    let origin = driver.publish(g.clone(), generations(&g));
    session.poll();
    let planned =
        history::plan(&Action::BackToAuto, &g, &default_idle_configuration().buses).unwrap();
    assert!(planned.len() > 64);
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    assert_eq!(finish(&session, &driver, &parent).concat(), planned);
    let undo = session.undo(&session.graph().unwrap()).unwrap().unwrap();
    finish(&session, &driver, &undo);
    assert_eq!(session.history_lengths(), (0, 1));
    assert_eq!(pairs(&session.graph().unwrap().value), original);
}

#[test]
fn large_reset_coalesced_ordinary_frames_cache_eviction_and_recreated_remaining_links_keep_authority(
) {
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    let applied = apply(&driver, &session, &first, true);
    let mut g = (*applied.value).clone();
    let old = g.kabels.iter_mut().find(|l| l.id >= 5000).unwrap();
    let old_id = old.id;
    old.id += 60000;
    let new_id = old.id;
    for _ in 0..pecan_core::native_command::NATIVE_SOURCE_HISTORY + 2 {
        g.knopen.iter_mut().find(|n| n.id == 20).unwrap().volume = Some(vec![0.75, 0.75]);
        driver.publish(g.clone(), generations(&g));
    }
    session.poll();
    let next = driver.take_requests().pop().unwrap();
    assert!(next
        .intent
        .commands
        .contains(&Opdracht::Ontkoppel { kabel: new_id }));
    assert!(!next
        .intent
        .commands
        .contains(&Opdracht::Ontkoppel { kabel: old_id }));
    apply(&driver, &session, &next, false);
    session.poll();
    finish(&session, &driver, &parent);
    assert_eq!(session.history_lengths(), (1, 0));
    assert!(matches!(
        report(&parent).children[0].progress,
        NativeProgress::ObservedNoOp { .. }
    ));
}

#[test]
fn large_reset_changed_earlier_effect_stops_before_another_native_batch() {
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    let applied = apply(&driver, &session, &first, false);
    let old = first
        .intent
        .commands
        .iter()
        .find_map(|c| match c {
            Opdracht::Ontkoppel { kabel } => origin.value.kabels.iter().find(|l| l.id == *kabel),
            _ => None,
        })
        .unwrap();
    let mut g = (*applied.value).clone();
    let mut added = old.clone();
    added.id = 900_000;
    g.kabels.push(added);
    driver.publish(g.clone(), generations(&g));
    session.poll();
    assert!(matches!(
        report(&parent).progress,
        SequenceProgress::Failed { .. }
    ));
    assert!(driver.take_requests().is_empty());
    assert_eq!(session.history_lengths(), (0, 0));
}

#[test]
fn large_reset_second_admission_error_retains_first_real_receipt_without_inventing_another() {
    let (_owner, session, driver, origin) = large();
    let parent = session
        .execute(&origin, "Back to auto", Action::BackToAuto)
        .unwrap()
        .unwrap();
    let first = driver.take_requests().pop().unwrap();
    let applied = apply(&driver, &session, &first, false);
    for i in 0..pecan_core::native_command::MAX_NATIVE_IN_FLIGHT {
        driver
            .endpoint()
            .dispatch(pecan_core::native_command::NativeIntent {
                ticket: 9000 + i as u64,
                origin: applied.clone(),
                commands: vec![Opdracht::ZetMute {
                    knoop: 10,
                    mute: false,
                }],
            })
            .unwrap();
    }
    session.poll();
    assert_eq!(
        driver.take_requests().len(),
        pecan_core::native_command::MAX_NATIVE_IN_FLIGHT
    );
    assert_eq!(report(&parent).children.len(), 1);
    assert!(
        matches!(report(&parent).progress,SequenceProgress::Failed {ref message} if message.contains("TooManyInFlight"))
    );
    assert_eq!(session.history_lengths(), (0, 0));
    for _ in 0..4 {
        session.poll();
        assert!(driver.take_requests().is_empty());
    }
    session
        .recover_request(
            &parent,
            &session.graph().unwrap(),
            &default_idle_configuration().buses,
        )
        .unwrap();
    assert!(!session.busy());
    assert_ne!(pairs(&session.graph().unwrap().value), pairs(&origin.value));
}

#[test]
fn large_reset_disconnect_or_quit_freezes_parent_and_retains_late_child_without_future_batches() {
    for quit in [false, true] {
        let (_owner, session, driver, origin) = large();
        let parent = session
            .execute(&origin, "Back to auto", Action::BackToAuto)
            .unwrap()
            .unwrap();
        let first = driver.take_requests().pop().unwrap();
        if quit {
            session.close_admission();
        } else {
            let mut g = (*origin.value).clone();
            g.verbonden = false;
            driver.publish(g.clone(), generations(&g));
        }
        session.poll();
        let outcome = report(&parent).progress;
        assert!(matches!(outcome, SequenceProgress::Failed { .. }));
        assert_eq!(session.history_lengths(), (0, 0));
        first.reporter.seal_execution();
        let actual_status = NativeProgress::Unconfirmed {
            reason: NativeFailure::Stopped,
            counts: Counts {
                submitted: 1,
                ..Default::default()
            },
        };
        first.reporter.set(actual_status.clone());
        for _ in 0..5 {
            session.poll();
            assert!(driver.take_requests().is_empty());
        }
        assert_eq!(report(&parent).progress, outcome);
        assert_eq!(report(&parent).children[0].progress, actual_status);
    }
}
