//! Explicit recovery uses the same retained native history guards as adoption.
//! No audio backend, settings/profile, rollback or failed-prefix history.
use super::*;

fn failed(
    session: &Session,
    driver: &FixtureDriver,
    graph: &Graph,
    held: bool,
    closed: bool,
) -> (Request, FixtureRequest) {
    let request = mute(session, graph, false);
    let submitted = driver.take_requests().pop().unwrap();
    submitted.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            in_progress: usize::from(held),
            ..Default::default()
        },
    });
    if closed {
        submitted.reporter.seal_execution();
    }
    session.poll();
    (request, submitted)
}
fn changed_recovery(redo: bool) {
    let (_owner, session, driver, graph, _) = with_history(redo);
    let prior = session.history_lengths();
    let (request, _) = failed(&session, &driver, &graph, true, true);
    let frozen = request.native_report();
    assert_eq!(
        session.history_lengths(),
        prior,
        "Failure supplies no Undo step"
    );
    assert!(
        session.busy(),
        "Frozen in-progress evidence keeps explicit ownership"
    );
    session.recover_request(&request, &graph, &split()).unwrap();
    // Ordinary adoption now sees equal definitions and cannot repair a bypassed
    // recovery. Nor should another graph event be required for fresh labels.
    session.adopt_buses(&graph, &split()).unwrap();
    assert_eq!(session.history_lengths(), (1, 0));
    assert_eq!(session.history_labels(), (Some("Mute".into()), None));
    assert_eq!(session.graph().unwrap().stamp, graph.stamp);
    assert!(Arc::ptr_eq(&session.graph().unwrap().value, &graph.value));
    assert!(session.poll().graph.is_none());
    assert!(
        driver.take_requests().is_empty(),
        "Reconciliation is read-only"
    );
    assert_eq!(request.native_report(), frozen, "Failure is not upgraded");
    assert!(matches!(session.status(), Status::Unconfirmed(_)));
    assert!(!session.busy());
    session.undo(&graph).unwrap().unwrap();
    let next = driver.take_requests().pop().unwrap();
    assert_eq!(next.intent.ticket, frozen.ticket + 1);
    assert_eq!(
        next.intent.commands,
        vec![Opdracht::ZetMute {
            knoop: 20,
            mute: false
        }]
    );
}

#[test]
fn explicit_recovery_history_prunes_changed_rec_undo_and_preserves_unrelated_mute() {
    changed_recovery(false);
}
#[test]
fn explicit_recovery_history_prunes_changed_rec_redo_and_preserves_unrelated_mute() {
    changed_recovery(true);
}

#[test]
fn explicit_recovery_history_unchanged_append_and_metadata_preserve_undo_redo_and_order() {
    let metadata = layout()
        .with_db("rec-1-2", -12.)
        .unwrap()
        .with_mute("rec-1-2", true)
        .unwrap()
        .arm("rec-1-2", false)
        .unwrap();
    for buses in [layout(), layout().add().unwrap(), metadata] {
        let (_owner, session, driver, graph, ids) = with_history(true);
        let (request, _) = failed(&session, &driver, &graph, true, true);
        let frozen = request.native_report();
        session.recover_request(&request, &graph, &buses).unwrap();
        assert_eq!(session.history_lengths(), (1, 1));
        assert_eq!(
            session.history_labels(),
            (Some("Mute".into()), Some("Synthetic Rec route".into()))
        );
        assert!(driver.take_requests().is_empty());
        assert_eq!(request.native_report(), frozen);
        session.redo(&graph).unwrap().unwrap();
        let admitted = driver.take_requests();
        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted[0].intent.ticket, frozen.ticket + 1);
        assert_eq!(
            admitted[0].intent.commands,
            vec![
                Opdracht::Verbind {
                    uit: 201,
                    naar: 301
                },
                Opdracht::Verbind {
                    uit: 202,
                    naar: 302
                }
            ]
        );
        let mut next = (*graph.value).clone();
        for (i, c) in admitted[0].intent.commands.iter().enumerate() {
            if let Opdracht::Verbind { uit, naar } = c {
                next.kabels.push(Kabel {
                    id: 950 + i as u32,
                    client: None,
                    uit_knoop: 20,
                    uit_poort: *uit,
                    in_knoop: 30,
                    in_poort: *naar,
                });
            }
        }
        let next = driver.publish(next, ids);
        observed(&admitted[0], next.stamp, false);
        session.poll();
        assert_eq!(session.history_lengths(), (2, 0));
        assert_eq!(request.native_report(), frozen);
    }
}

#[test]
fn explicit_recovery_history_existing_work_authority_and_invalid_input_gates_preserve_state() {
    for gate in 0..6 {
        let (_owner, session, driver, graph, _) = with_history(false);
        let held = matches!(gate, 0 | 4 | 5);
        let (request, _) = failed(&session, &driver, &graph, held, gate != 0);
        let frozen = request.native_report();
        let context = session.history_context();
        let lengths = session.history_lengths();
        let mut buses = split();
        let mut lease = None;
        let mut gesture = None;
        match gate {
            0 => {} // local execution has not closed
            1 => lease = Some(session.reserve_external().unwrap().1),
            2 => {
                let key = session
                    .volume_model()
                    .apps
                    .iter()
                    .find(|s| s.node.id == 20)
                    .unwrap()
                    .key
                    .clone();
                gesture = Some(session.begin_volume(&graph, &key).unwrap());
            }
            3 => {
                mute(&session, &graph, false);
            } // another command is pending
            4 => buses.widths = vec![0],
            _ => driver.forget_origins(), // retained graph alone is not fresh inspection
        }
        assert!(
            session.recover_request(&request, &graph, &buses).is_err(),
            "gate {gate}"
        );
        assert_eq!(session.history_lengths(), lengths);
        assert_eq!(request.native_report(), frozen);
        // Lease/gesture/pending acquisition rotates activation context itself;
        // a rejected recovery must leave that resulting context untouched.
        if matches!(gate, 0 | 4 | 5) {
            assert!(Rc::ptr_eq(&session.history_context(), &context));
        }
        if let Some(lease) = lease {
            session.release_external(&lease);
        }
        if let Some(gesture) = gesture {
            session.end_volume(&gesture).unwrap();
        }
        if held {
            assert!(session.busy());
        }
    }
}
