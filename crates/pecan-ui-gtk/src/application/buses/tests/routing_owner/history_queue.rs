//! The mounted failure surface reuses current authenticated bus inspection.
use super::*;
fn undo(view: &NativeWorkspace) -> gtk::Button {
    buttons(view.widget())
        .into_iter()
        .find(|b| text(b).starts_with("Undo "))
        .unwrap()
}
#[gtk::test]
fn queued_history_widget_retains_failed_receipt_and_requires_closed_current_explicit_inspection() {
    let (buses, fake, driver, _routes) = setup_routes();
    let view = NativeWorkspace::new(buses.session.clone());
    view.attach_test_buses(buses.clone());
    for node in [10, 20] {
        let graph = buses.session.graph().unwrap();
        buses
            .session
            .execute(
                &graph,
                "Synthetic mute",
                GraphAction::Mute { node, muted: true },
            )
            .unwrap()
            .unwrap();
        let req = driver.take_requests().pop().unwrap();
        let mut value = (*graph.value).clone();
        value.knopen.iter_mut().find(|n| n.id == node).unwrap().mute = Some(true);
        let frame = publish(&driver, value);
        req.reporter.seal_execution();
        req.reporter.set(NativeProgress::Observed {
            stamp: frame.stamp,
            counts: Counts::default(),
        });
        view.poll();
    }
    undo(&view).emit_clicked();
    undo(&view).emit_clicked();
    let req = driver.take_requests().pop().unwrap();
    let call = req.reporter.claim_execution().unwrap();
    req.reporter.seal_execution();
    req.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            in_progress: 1,
            ..Default::default()
        },
    });
    view.poll();
    let recovery = choice(view.widget(), "Inspect and refresh history status");
    recovery.emit_clicked();
    assert!(buses.session.busy());
    assert!(driver.take_requests().is_empty());
    drop(call);
    fake.busy.set(true);
    recovery.emit_clicked();
    assert!(buses.session.busy());
    fake.busy.set(false);
    recovery.emit_clicked();
    assert!(!buses.session.busy());
    assert!(undo(&view).is_sensitive());
    assert_eq!(
        buses.session.history_lengths(),
        (2, 0),
        "failed prefix is not undone or recorded"
    );
    for _ in 0..10 {
        view.poll();
        assert!(
            driver.take_requests().is_empty(),
            "no canceled input retries after recovery"
        );
    }
    recovery.emit_clicked();
    assert!(
        driver.take_requests().is_empty(),
        "retired recovery callback cannot submit"
    );
}
