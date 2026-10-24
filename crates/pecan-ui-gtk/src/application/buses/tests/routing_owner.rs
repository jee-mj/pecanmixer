//! Real owner/controller handoffs using only injected synthetic services.
use super::*;
use crate::application::{routes::Routes, NativeWorkspace};
use pecan_core::{
    graph::{Kabel, Opdracht},
    native_command::{Counts, NativeFailure, NativeProgress},
};

fn buttons(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Button> {
    let mut todo = vec![root.as_ref().clone()];
    let mut out = vec![];
    while let Some(widget) = todo.pop() {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            out.push(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            todo.push(current);
        }
    }
    out
}
fn text(button: &gtk::Button) -> String {
    button
        .child()
        .and_downcast::<gtk::Label>()
        .map(|l| l.label().to_string())
        .or_else(|| button.label().map(|s| s.to_string()))
        .unwrap_or_default()
}
fn route_button(root: &impl IsA<gtk::Widget>) -> gtk::Button {
    buttons(root)
        .into_iter()
        .find(|b| {
            let label = text(b);
            !label.starts_with("Rec ") && (label.ends_with(": off") || label.contains(": on"))
        })
        .expect("synthetic output route button")
}
fn choice(root: &impl IsA<gtk::Widget>, label: &str) -> gtk::Button {
    buttons(root)
        .into_iter()
        .find(|b| text(b) == label)
        .unwrap()
}
fn setup_routes() -> (Rc<BusOwner>, Rc<Fake>, FixtureDriver, Rc<Routes>) {
    let (buses, fake, driver) = setup();
    let mut native_graph = graph();
    native_graph
        .knopen
        .iter_mut()
        .find(|n| n.id == 30)
        .unwrap()
        .naam = pecan_core::domain::audio::REC_NODE.into();
    publish(&driver, native_graph);
    buses.session.poll();
    buses.poll();
    let routes = Routes::new(buses.session.clone(), buses.clone());
    (buses, fake, driver, routes)
}
fn apply(mut graph: Momentopname, commands: &[Opdracht]) -> Momentopname {
    for command in commands {
        match command {
            Opdracht::Verbind { uit, naar } => {
                let out = graph.poorten.iter().find(|p| p.id == *uit).unwrap();
                let input = graph.poorten.iter().find(|p| p.id == *naar).unwrap();
                let id = graph.kabels.iter().map(|l| l.id).max().unwrap_or(10000) + 1;
                graph.kabels.push(Kabel {
                    id,
                    uit_knoop: out.knoop,
                    uit_poort: *uit,
                    in_knoop: input.knoop,
                    in_poort: *naar,
                    client: None,
                });
            }
            Opdracht::Ontkoppel { kabel } => graph.kabels.retain(|l| l.id != *kabel),
            Opdracht::ZetDoel { knoop, doel } => {
                graph
                    .knopen
                    .iter_mut()
                    .find(|n| n.id == *knoop)
                    .unwrap()
                    .doel = doel.clone()
            }
            _ => panic!("unexpected synthetic routing operation"),
        }
    }
    graph
}
fn publish(driver: &FixtureDriver, graph: Momentopname) -> Graph {
    let mut ids = generations(&graph);
    for link in &graph.kabels {
        ids.insert(link.id, u64::from(link.id) + 1000);
    }
    driver.publish(graph, ids)
}
fn complete(buses: &BusOwner, driver: &FixtureDriver, routes: &Routes, noop: bool) {
    let request = driver.take_requests().pop().unwrap();
    let graph = apply(
        (*request.intent.origin.value).clone(),
        &request.intent.commands,
    );
    let next = publish(driver, graph);
    request.reporter.set(if noop {
        NativeProgress::ObservedNoOp {
            stamp: next.stamp,
            counts: Counts::default(),
        }
    } else {
        NativeProgress::Observed {
            stamp: next.stamp,
            counts: Counts::default(),
        }
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
}

#[gtk::test]
fn routed_widget_uses_shared_ticket_report_graph_barrier_and_one_history_step() {
    let (buses, _, driver, routes) = setup_routes();
    route_button(routes.widget()).emit_clicked();
    assert!(buses.session.busy());
    let request = driver.take_requests().pop().unwrap();
    assert_eq!(
        request.intent.origin.stamp,
        buses.session.graph().unwrap().stamp
    );
    assert!(Arc::ptr_eq(
        &request.intent.origin.value,
        &buses.session.graph().unwrap().value
    ));
    request
        .reporter
        .set(NativeProgress::ExecutedLocally(Counts::default()));
    buses.session.poll();
    buses.poll();
    routes.poll();
    assert!(!route_button(routes.widget()).is_sensitive());
    assert_eq!(buses.session.history_lengths(), (0, 0));
    let next = publish(
        &driver,
        apply(
            (*request.intent.origin.value).clone(),
            &request.intent.commands,
        ),
    );
    request.reporter.set(NativeProgress::Observed {
        stamp: next.stamp,
        counts: Counts::default(),
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    assert_eq!(buses.session.history_lengths(), (1, 0));
    assert!(route_button(routes.widget()).is_sensitive());
    buses
        .session
        .undo(&buses.session.graph().unwrap())
        .unwrap()
        .unwrap();
    complete(&buses, &driver, &routes, false);
    assert_eq!(buses.session.history_lengths(), (0, 1));
    buses
        .session
        .redo(&buses.session.graph().unwrap())
        .unwrap()
        .unwrap();
    complete(&buses, &driver, &routes, true);
    assert_eq!(buses.session.history_lengths(), (1, 0));
}

#[gtk::test]
fn ordinary_routing_is_allowed_while_recording_without_an_idle_configuration_gate() {
    let (buses, fake, driver, routes) = setup_routes();
    fake.config.borrow_mut().recording = true;
    buses.poll();
    routes.poll();
    route_button(routes.widget()).emit_clicked();
    assert_eq!(driver.take_requests().len(), 1);
    assert!(fake.configs.borrow().is_empty());
    assert!(fake.starts.borrow().is_empty());
}

#[gtk::test]
fn bus_definitions_and_destination_choices_share_the_authoritative_bus_model() {
    let (buses, fake, driver, routes) = setup_routes();
    {
        let mut snapshot = fake.config.borrow_mut();
        let configuration = snapshot.configuration.as_mut().unwrap();
        configuration.buses = configuration.buses.change_coupling(0).unwrap();
    }
    fake.config.borrow_mut().revision.revision += 1;
    buses.poll();
    routes.poll();
    let keys: Vec<_> = buses
        .session
        .volume_model()
        .destinations
        .iter()
        .map(|d| d.key.clone())
        .collect();
    assert!(
        keys.contains(&"bus|rec-1".into()) && keys.contains(&"bus|rec-2".into()),
        "{keys:?}"
    );
    let output = buses.view.selected_output_key();
    choice(routes.widget(), "Rec 2").emit_clicked();
    assert_eq!(buses.view.selected_rec_key(), Some("bus|rec-2".into()));
    assert_eq!(buses.view.selected_output_key(), output);
    let binding = buses.view.inspect_model(|m| m.bind().unwrap()).unwrap();
    buses
        .view
        .select_destination(&binding, "bus|rec-1")
        .unwrap();
    routes.poll();
    assert!(buttons(routes.widget())
        .iter()
        .any(|b| text(b) == "Selected: Rec 1"));
    assert!(driver.take_requests().is_empty());
    assert!(fake.configs.borrow().is_empty());
}

#[gtk::test]
fn stale_configuration_or_busy_preflight_submits_nothing_and_fresh_readback_recovers() {
    let (buses, fake, driver, routes) = setup_routes();
    fake.busy.set(true);
    route_button(routes.widget()).emit_clicked();
    assert!(driver.take_requests().is_empty());
    fake.busy.set(false);
    fake.config.borrow_mut().revision.revision += 1;
    route_button(routes.widget()).emit_clicked();
    assert!(driver.take_requests().is_empty());
    assert_eq!(buses.session.history_lengths(), (0, 0));
    buses.poll();
    routes.poll();
    route_button(routes.widget()).emit_clicked();
    assert_eq!(driver.take_requests().len(), 1);
}

#[gtk::test]
fn retained_native_report_arrives_after_disconnect_invalidates_history_delivery() {
    let (buses, _, driver, routes) = setup_routes();
    route_button(routes.widget()).emit_clicked();
    let request = driver.take_requests().pop().unwrap();
    let mut disconnected = graph();
    disconnected.verbonden = false;
    publish(&driver, disconnected);
    buses.session.poll();
    buses.poll();
    routes.poll();
    request.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts::default(),
    });
    routes.poll();
    let report = routes.retained_report().unwrap();
    assert_eq!(report.ticket, request.intent.ticket);
    assert_eq!(report.source, request.intent.origin.stamp);
    assert!(matches!(report.progress, NativeProgress::Rejected { .. }));
    assert_eq!(buses.session.history_lengths(), (0, 0));
    buses.session.poll();
    buses.poll();
    routes.poll();
    assert!(
        !buses.session.busy(),
        "actual terminal zero-in-progress proof releases the shared gate"
    );
}

#[gtk::test]
fn terminal_in_progress_work_and_partial_uncertainty_never_gain_automatic_recovery() {
    let (buses, _, driver, routes) = setup_routes();
    route_button(routes.widget()).emit_clicked();
    let request = driver.take_requests().pop().unwrap();
    request.reporter.set(NativeProgress::Unconfirmed {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts {
            in_progress: 1,
            ..Counts::default()
        },
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    buses.refresh();
    routes.poll();
    assert!(buses.session.busy());
    assert!(!buses.session.enabled());
    let origin = buses.session.graph().unwrap();
    assert!(buses
        .session
        .execute(
            &origin,
            "Blocked",
            GraphAction::Mute {
                node: 20,
                muted: true
            }
        )
        .is_err());
    assert!(buses.session.reserve_external().is_err());
    let source = buses.session.volume_model().apps[0].key.clone();
    assert!(buses.session.begin_volume(&origin, &source).is_err());
    assert!(!route_button(routes.widget()).is_sensitive());
    assert!(matches!(
        routes.retained_report().unwrap().progress,
        NativeProgress::Unconfirmed {
            counts: Counts { in_progress: 1, .. },
            ..
        }
    ));
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn unchanged_clock_ticks_retain_routing_controls_instead_of_rebinding_each_tick() {
    let (buses, _, driver, routes) = setup_routes();
    let original = route_button(routes.widget());
    for _ in 0..1000 {
        buses.poll();
        routes.poll();
    }
    assert_eq!(route_button(routes.widget()), original);
    assert!(driver.take_requests().is_empty());
}

#[gtk::test]
fn workspace_mounts_routing_through_the_same_session_and_injected_bus_owner() {
    let (buses, _, driver) = setup();
    let workspace = NativeWorkspace::new(buses.session.clone());
    workspace.attach_test_buses(buses.clone());
    workspace.poll();
    let mut todo = vec![workspace.widget().clone().upcast::<gtk::Widget>()];
    let mut routing = None;
    while let Some(w) = todo.pop() {
        if let Some(stack) = w.downcast_ref::<gtk::Stack>() {
            if let Some(page) = stack.child_by_name("routing") {
                routing = Some(page);
            }
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            todo.push(c);
        }
    }
    let routing = routing.expect("routing page mounted");
    route_button(&routing).emit_clicked();
    assert_eq!(driver.take_requests().len(), 1);
    assert!(buses.session.busy());
}

#[gtk::test]
fn retired_routing_callback_cannot_adopt_a_replacement_object_or_new_origin() {
    let (buses, _, driver, routes) = setup_routes();
    let retired = route_button(routes.widget());
    let mut replacement = (*buses.session.graph().unwrap().value).clone();
    replacement
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .serial = Some(90020);
    let mut ids = generations(&replacement);
    *ids.get_mut(&20).unwrap() += 10000;
    let current = driver.publish(replacement, ids);
    buses.session.poll();
    buses.poll();
    routes.poll();
    retired.emit_clicked();
    assert!(driver.take_requests().is_empty());
    route_button(routes.widget()).emit_clicked();
    let request = driver.take_requests().pop().unwrap();
    assert_eq!(request.intent.origin.stamp, current.stamp);
    assert!(Arc::ptr_eq(&request.intent.origin.value, &current.value));
}

#[gtk::test]
fn custom_route_navigation_uses_the_existing_patchbay_callback_without_audio_intent() {
    let (buses, fake, driver, routes) = setup_routes();
    let mut custom = (*buses.session.graph().unwrap().value).clone();
    custom.kabels.push(Kabel {
        id: 7001,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: 10,
        in_poort: 101,
        client: None,
    });
    publish(&driver, custom);
    buses.session.poll();
    fake.config.borrow_mut().recording = true;
    buses.poll();
    routes.poll();
    let navigations = Rc::new(Cell::new(0));
    let seen = navigations.clone();
    routes.bind_navigation(move || seen.set(seen.get() + 1));
    let custom = buttons(routes.widget())
        .into_iter()
        .find(|b| text(b).contains("custom; open patchbay"))
        .unwrap();
    custom.emit_clicked();
    assert_eq!(navigations.get(), 1);
    assert!(driver.take_requests().is_empty());
    assert!(!buses.session.busy());
}

#[gtk::test]
fn reset_cancel_and_confirmation_use_one_real_shared_history_request() {
    let (buses, _, driver, routes) = setup_routes();
    let reset = || {
        buttons(routes.widget())
            .into_iter()
            .find(|b| text(b).starts_with("Back to auto ("))
            .unwrap()
    };
    assert!(reset().is_sensitive());
    reset().emit_clicked();
    choice(routes.widget(), "Cancel").emit_clicked();
    assert!(driver.take_requests().is_empty());
    reset().emit_clicked();
    choice(routes.widget(), "Remove manual routes").emit_clicked();
    assert!(buses.session.busy());
    complete(&buses, &driver, &routes, false);
    assert_eq!(buses.session.history_lengths(), (1, 0));
    assert!(matches!(
        routes.retained_report().unwrap().progress,
        NativeProgress::Observed { .. }
    ));
}

#[gtk::test]
fn hidden_routing_widget_keeps_its_real_receipt_and_delivers_terminal_evidence() {
    let (buses, _, driver, routes) = setup_routes();
    let window = gtk::Window::builder()
        .child(routes.widget())
        .default_width(900)
        .default_height(700)
        .build();
    window.present();
    let context = gtk::glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    route_button(routes.widget()).emit_clicked();
    let request = driver.take_requests().pop().unwrap();
    window.set_visible(false);
    while context.pending() {
        context.iteration(false);
    }
    request.reporter.set(NativeProgress::Rejected {
        reason: NativeFailure::ObservationTimedOut,
        counts: Counts::default(),
    });
    buses.session.poll();
    buses.poll();
    routes.poll();
    assert!(!window.is_visible());
    assert_eq!(
        routes.retained_report().unwrap().ticket,
        request.intent.ticket
    );
    assert!(matches!(
        routes.retained_report().unwrap().progress,
        NativeProgress::Rejected { .. }
    ));
    assert_eq!(buses.session.history_lengths(), (0, 0));
    window.close();
}
