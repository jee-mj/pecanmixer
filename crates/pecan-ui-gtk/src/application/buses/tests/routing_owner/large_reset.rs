//! Exercise the mounted routing handoff with synthetic recording status only.
use super::*;
use crate::application::tests::large_reset as fixture;
use pecan_core::history_backend::SequenceProgress;
use std::collections::BTreeSet;

fn prepare() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver, Rc<Routes>) {
    let (buses, fake, driver, routes) = setup_routes();
    let g = fixture::scene(25);
    let ids = g
        .knopen
        .iter()
        .map(|n| (n.id, u64::from(n.id)))
        .chain(g.poorten.iter().map(|p| (p.id, u64::from(p.id))))
        .collect();
    driver.publish(g, ids);
    buses.session.poll();
    fake.config.borrow_mut().recording = true;
    buses.poll();
    routes.poll();
    let reset = buttons(routes.widget())
        .into_iter()
        .find(|b| text(b).starts_with("Back to auto ("))
        .unwrap();
    assert!(
        reset.is_sensitive(),
        "Ordinary routing remains available while recording"
    );
    reset.emit_clicked();
    choice(routes.widget(), "Remove manual routes").emit_clicked();
    (buses, fake, driver, routes)
}
fn rec_pairs(buses: &BusOwner) -> BTreeSet<[u32; 2]> {
    buses
        .session
        .graph()
        .unwrap()
        .value
        .kabels
        .iter()
        .filter(|l| l.in_knoop == 30)
        .map(|l| [l.uit_poort, l.in_poort])
        .collect()
}
#[gtk::test]
fn large_reset_widget_keeps_parent_pending_through_children_and_preserves_recording_links() {
    let (buses, fake, driver, routes) = prepare();
    let recording = rec_pairs(&buses);
    assert!(!recording.is_empty());
    let mut children = 0;
    for _ in 0..100 {
        let requests = driver.take_requests();
        assert!(requests.len() <= 1);
        for child in requests {
            assert!(child.intent.commands.len() <= 32);
            fixture::apply(&driver, &buses.session, &child, false);
            children += 1;
        }
        buses.session.poll();
        buses.poll();
        routes.poll();
        let evidence = routes.retained_sequence().unwrap();
        assert!(
            routes.retained_report().is_none(),
            "No invented parent NativeReport"
        );
        assert!(fake.starts.borrow().is_empty() && fake.stops.borrow().is_empty());
        assert_eq!(rec_pairs(&buses), recording);
        match evidence.progress {
            SequenceProgress::Running => {
                assert!(buses.session.busy());
                assert_eq!(buses.session.history_lengths(), (0, 0));
            }
            SequenceProgress::Observed { .. } => {
                assert!(children > 2);
                assert_eq!(evidence.children.len(), children);
                assert!(!buses.session.busy());
                assert_eq!(buses.session.history_lengths(), (1, 0));
                return;
            }
            other => panic!("unexpected reset outcome {other:?}"),
        }
    }
    panic!("bounded synthetic reset did not complete");
}
#[gtk::test]
fn large_reset_widget_partial_failure_requires_original_explicit_recovery_without_prefix_undo() {
    let (buses, _, driver, routes) = prepare();
    let first = driver.take_requests().pop().unwrap();
    fixture::apply(&driver, &buses.session, &first, false);
    buses.session.poll();
    buses.poll();
    routes.poll();
    let second = driver.take_requests().pop().unwrap();
    second.reporter.seal_execution();
    second.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            ..Default::default()
        },
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    let frozen = routes.retained_sequence().unwrap();
    assert!(matches!(frozen.progress, SequenceProgress::Failed { .. }));
    assert!(buses.session.busy());
    assert_eq!(buses.session.history_lengths(), (0, 0));
    let current = buses.session.graph().unwrap();
    choice(routes.widget(), "Inspect and refresh routing status").emit_clicked();
    assert!(!buses.session.busy());
    assert_eq!(buses.session.graph().unwrap().stamp, current.stamp);
    assert_eq!(buses.session.history_lengths(), (0, 0));
    assert_eq!(routes.retained_sequence(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

fn reconnected_unknown(buses: &BusOwner, fake: &Fake, driver: &FixtureDriver, routes: &Routes) {
    let mut g = (*buses.session.graph().unwrap().value).clone();
    g.verbonden = false;
    publish(driver, g.clone());
    buses.session.poll();
    fake.config.borrow_mut().connected = false;
    fake.config.borrow_mut().configuration = None;
    buses.poll();
    routes.poll();
    g.verbonden = true;
    let fresh = publish(driver, g);
    {
        let mut config = fake.config.borrow_mut();
        config.connected = true;
        config.recording = false;
        config.configuration = None;
        config.revision.epoch = fresh.stamp.epoch;
        config.revision.revision += 1;
        config.local.as_mut().unwrap().channels = 2;
    }
    // Ordinary clock ticks must be enough to publish new-epoch authority. No
    // recovery/initialization callback is allowed to discard the failed result.
    for _ in 0..3 {
        buses.session.poll();
        buses.poll();
        routes.poll();
    }
}
#[gtk::test]
fn large_reset_reconnect_closed_failed_children_allow_metadata_initialization_then_explicit_recovery(
) {
    let (buses, fake, driver, routes) = prepare();
    let child = driver.take_requests().pop().unwrap();
    child.reporter.seal_execution();
    child.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts::default(),
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    let failed = routes.retained_sequence().unwrap();
    assert!(matches!(failed.progress, SequenceProgress::Failed { .. }));
    assert!(
        buses.session.busy(),
        "Same-epoch explicit recovery gate remains"
    );
    reconnected_unknown(&buses, &fake, &driver, &routes);
    assert!(buses.session.graph().unwrap().stamp.epoch > failed.source.epoch);
    assert!(
        buses.routing_recovery_capture().is_err(),
        "Metadata is still unknown"
    );
    buses
        .initialize(&buses.origin().unwrap())
        .expect("closed old-epoch reset must not strand metadata initialization");
    assert_eq!(fake.configs.borrow().len(), 1);
    fake.complete_config(0, LocalConfigOutcome::Applied, true);
    for _ in 0..3 {
        buses.session.poll();
        buses.poll();
        routes.poll();
    }
    assert!(buses.routing_recovery_capture().is_ok());
    assert_eq!(routes.retained_sequence(), Some(failed.clone()));
    // Metadata initialization is not routing reconciliation or rollback.
    let reset = buttons(routes.widget())
        .into_iter()
        .find(|b| text(b).starts_with("Back to auto ("))
        .unwrap();
    assert!(!reset.is_sensitive());
    choice(routes.widget(), "Inspect and refresh routing status").emit_clicked();
    assert!(buttons(routes.widget())
        .into_iter()
        .find(|b| text(b).starts_with("Back to auto ("))
        .unwrap()
        .is_sensitive());
    assert_eq!(routes.retained_sequence(), Some(failed));
    assert_eq!(buses.session.history_lengths(), (0, 0));
    assert!(
        driver.take_requests().is_empty(),
        "No routing retry or rollback"
    );
}
#[gtk::test]
fn large_reset_reconnect_open_execution_still_blocks_initialization_until_actual_closure() {
    let (buses, fake, driver, routes) = prepare();
    let child = driver.take_requests().pop().unwrap();
    let call = child.reporter.claim_execution().unwrap();
    child.reporter.seal_execution();
    child.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::Stopped,
        counts: Counts {
            submitted: 1,
            in_progress: 1,
            ..Default::default()
        },
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    let failed = routes.retained_sequence().unwrap();
    reconnected_unknown(&buses, &fake, &driver, &routes);
    assert!(buses.initialize(&buses.origin().unwrap()).is_err());
    assert!(fake.configs.borrow().is_empty());
    assert!(buses.session.busy());
    drop(call);
    for _ in 0..3 {
        buses.session.poll();
        buses.poll();
        routes.poll();
    }
    buses.initialize(&buses.origin().unwrap()).expect(
        "real closure permits metadata initialization, frozen in-progress counts are historical",
    );
    assert_eq!(routes.retained_sequence(), Some(failed));
    assert_eq!(fake.configs.borrow().len(), 1);
    assert!(driver.take_requests().is_empty());
}
