//! Explicit read-only recovery over the original receipt and synthetic owners.
use super::*;
use pecan_core::native_command::NativeExecutionLease;

fn recover(routes: &Routes) -> gtk::Button {
    choice(routes.widget(), "Inspect and refresh routing status")
}
fn tick(buses: &BusOwner, routes: &Routes) {
    buses.session.poll();
    buses.poll();
    routes.poll();
}
fn newer(buses: &BusOwner, driver: &FixtureDriver, routes: &Routes) {
    let mut graph = (*buses.session.graph().unwrap().value).clone();
    graph.knopen[0].media = Some("Synthetic later observation".into());
    publish(driver, graph);
    tick(buses, routes);
}
fn fail(
    buses: &BusOwner,
    driver: &FixtureDriver,
    routes: &Routes,
) -> (crate::application::FixtureRequest, NativeExecutionLease) {
    route_button(routes.widget()).emit_clicked();
    let request = driver.take_requests().pop().unwrap();
    let lease = request.reporter.claim_execution().unwrap();
    request.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            in_progress: 1,
            ..Default::default()
        },
    });
    request.reporter.seal_execution();
    tick(buses, routes);
    (request, lease)
}

#[gtk::test]
fn explicit_recovery_needs_closed_receipt_and_current_inspection_without_upgrading_failed_history()
{
    let (buses, _, driver, routes) = setup_routes();
    route_button(routes.widget()).emit_clicked();
    complete(&buses, &driver, &routes, false);
    assert_eq!(buses.session.history_lengths(), (1, 0));
    let (request, lease) = fail(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    recover(&routes).emit_clicked();
    assert!(buses.session.busy());
    drop(lease);
    tick(&buses, &routes);
    assert!(
        buses.session.busy(),
        "local closure and ordinary polling never recover automatically"
    );
    let inspected = buses.session.graph().unwrap();
    let retired = recover(&routes);
    retired.emit_clicked();
    assert!(buses.session.enabled() && !buses.session.busy());
    assert!(route_button(routes.widget()).is_sensitive());
    assert_eq!(buses.session.graph().unwrap().stamp, inspected.stamp);
    assert_eq!(
        buses.session.history_lengths(),
        (1, 0),
        "failed prefix adds no undo step"
    );
    assert_eq!(routes.retained_report(), Some(frozen.clone()));
    request.reporter.set(NativeProgress::Observed {
        stamp: buses.session.graph().unwrap().stamp,
        counts: Counts::default(),
    });
    routes.poll();
    assert_eq!(routes.retained_report(), Some(frozen));
    retired.emit_clicked();
    assert!(driver.take_requests().is_empty());
    route_button(routes.widget()).emit_clicked();
    assert_eq!(
        driver.take_requests().len(),
        1,
        "fresh user intent recovers"
    );
}

#[gtk::test]
fn busy_or_replaced_configuration_does_not_release_recovery_but_current_readback_can() {
    let (buses, fake, driver, routes) = setup_routes();
    let (_, lease) = fail(&buses, &driver, &routes);
    drop(lease);
    newer(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    fake.busy.set(true);
    recover(&routes).emit_clicked();
    assert!(buses.session.busy() && !route_button(routes.widget()).is_sensitive());
    fake.busy.set(false);
    let (hook_buses, hook_fake) = (buses.clone(), fake.clone());
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        let mut config = hook_fake.config.borrow_mut();
        config.revision.revision += 1;
        let old = &mut config.configuration.as_mut().unwrap().buses;
        *old = old.change_coupling(0).unwrap();
        drop(config);
        hook_buses.poll();
    }));
    recover(&routes).emit_clicked();
    assert!(buses.session.busy());
    assert_eq!(routes.retained_report(), Some(frozen.clone()));
    routes.poll();
    recover(&routes).emit_clicked();
    assert!(buses.session.enabled());
    assert!(buses
        .session
        .volume_model()
        .destinations
        .iter()
        .any(|s| s.key == "bus|rec-1"));
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn pending_reentrant_history_is_retained_and_unconfirmed_zero_count_recovery_is_explicit() {
    let (buses, fake, driver, routes) = setup_routes();
    route_button(routes.widget()).emit_clicked();
    let original = driver.take_requests().pop().unwrap();
    original.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            submitted: 1,
            ..Default::default()
        },
    });
    original.reporter.seal_execution();
    tick(&buses, &routes);
    newer(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    let session = buses.session.clone();
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        session
            .execute(
                &session.graph().unwrap(),
                "Synthetic competing action",
                GraphAction::Mute {
                    node: 20,
                    muted: true,
                },
            )
            .unwrap()
            .unwrap();
    }));
    recover(&routes).emit_clicked();
    assert!(buses.session.busy());
    assert!(!route_button(routes.widget()).is_sensitive());
    let competing = driver.take_requests().pop().unwrap();
    competing.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts::default(),
    });
    competing.reporter.seal_execution();
    tick(&buses, &routes);
    recover(&routes).emit_clicked();
    assert!(buses.session.enabled());
    assert_eq!(buses.session.history_lengths(), (0, 0));
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn nested_recovery_during_inspection_cannot_discard_the_original_terminal_diagnostic() {
    let (buses, fake, driver, routes) = setup_routes();
    let (_, lease) = fail(&buses, &driver, &routes);
    drop(lease);
    let frozen = routes.retained_report().unwrap();
    let weak = Rc::downgrade(&routes);
    *fake.read_hook.borrow_mut() = Some(Box::new(move || {
        recover(&weak.upgrade().unwrap()).emit_clicked();
    }));
    recover(&routes).emit_clicked();
    assert!(buses.session.enabled());
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn recovery_refuses_undrained_or_evicted_backend_frames_and_can_retry_after_the_normal_poll() {
    let (buses, _, driver, routes) = setup_routes();
    let (_, lease) = fail(&buses, &driver, &routes);
    drop(lease);
    newer(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    publish(&driver, (*buses.session.graph().unwrap().value).clone());
    recover(&routes).emit_clicked();
    assert!(
        buses.session.busy(),
        "retained origin is not the latest owned frame"
    );
    tick(&buses, &routes);
    assert!(!route_button(routes.widget()).is_sensitive());
    driver.forget_origins();
    recover(&routes).emit_clicked();
    assert!(
        buses.session.busy(),
        "cached Session witness alone is insufficient"
    );
    newer(&buses, &driver, &routes);
    recover(&routes).emit_clicked();
    assert!(buses.session.enabled());
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn unknown_configuration_or_finalization_cannot_be_cleared_by_routing_recovery() {
    let (buses, fake, driver, routes) = setup_routes();
    let (_, lease) = fail(&buses, &driver, &routes);
    drop(lease);
    newer(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    fake.config.borrow_mut().configuration = None;
    buses.poll();
    routes.poll();
    recover(&routes).emit_clicked();
    assert!(buses.session.busy());
    fake.config.borrow_mut().configuration = Some(crate::application::default_idle_configuration());
    buses.poll();
    routes.poll();
    buses.session.close_admission();
    recover(&routes).emit_clicked();
    assert!(buses.session.admission_closed() && !buses.session.enabled());
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn reconnect_uses_new_authenticated_epoch_but_retains_the_old_frozen_failure() {
    let (buses, fake, driver, routes) = setup_routes();
    let (_, lease) = fail(&buses, &driver, &routes);
    let frozen = routes.retained_report().unwrap();
    let mut graph = (*buses.session.graph().unwrap().value).clone();
    graph.verbonden = false;
    publish(&driver, graph.clone());
    tick(&buses, &routes);
    drop(lease);
    graph.verbonden = true;
    let current = publish(&driver, graph);
    assert!(current.stamp.epoch > frozen.source.epoch);
    fake.config.borrow_mut().revision.epoch = current.stamp.epoch;
    tick(&buses, &routes);
    assert!(buses.session.busy());
    recover(&routes).emit_clicked();
    assert!(buses.session.enabled());
    assert_eq!(buses.session.history_lengths(), (0, 0));
    assert_eq!(routes.retained_report(), Some(frozen));
    assert!(driver.take_requests().is_empty());
}
