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
