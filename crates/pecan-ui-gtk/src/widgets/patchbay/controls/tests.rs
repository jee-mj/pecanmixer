use super::*;
use crate::widgets::PatchbayOverview;
use pecan_core::{
    backend::Stamp,
    domain::history::{CommandPlan, Controller},
    graph::{Kabel, Opdracht},
};
fn fixture(revision: u64) -> Stamped<Arc<Momentopname>> {
    Stamped {
        stamp: Stamp { epoch: 1, revision },
        value: Arc::new(
            serde_json::from_str(include_str!(
                "../../../../../../fixtures/graph/audio-basic.json"
            ))
            .unwrap(),
        ),
    }
}
fn choose(controls: &Controls, source: u32, destination: u32) {
    let model = controls.0.model.borrow();
    let source = model.outputs.iter().position(|p| p.id == source).unwrap() as u32 + 1;
    let destination = model
        .inputs
        .iter()
        .position(|p| p.id == destination)
        .unwrap() as u32
        + 1;
    drop(model);
    controls.0.source.set_selected(source);
    controls.0.destination.set_selected(destination);
}
fn key(list: &gtk::ListBox, key: gtk::gdk::Key) {
    let controllers = list.observe_controllers();
    let keyboard = (0..controllers.n_items())
        .find_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .unwrap();
    assert!(keyboard.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()]
    ));
}
#[gtk::test]
fn keyboard_intents_pending_rejection_move_cancel_and_snapshot_boundary() {
    let mut view = PatchbayOverview::new();
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(view.widget())
        .build();
    window.present();
    let graph = fixture(1);
    view.update_graph(graph.clone());
    assert!(!view.actions.0.source.is_sensitive());
    assert!(view.actions.0.status.text().contains("unavailable"));
    let history = Rc::new(RefCell::new(Controller::default()));
    let plans = Rc::new(RefCell::new(Vec::<CommandPlan>::new()));
    let origins = Rc::new(RefCell::new(Vec::new()));
    let (recorded, shared, seen) = (plans.clone(), history.clone(), origins.clone());
    view.bind_history_actions(move |origin, input| {
        seen.borrow_mut().push(origin.stamp);
        let mut history = shared.borrow_mut();
        history
            .reduce(Input::Graph(origin))
            .map_err(|e| format!("{e:?}"))?;
        if let Some(plan) = history.reduce(input).map_err(|e| format!("{e:?}"))? {
            recorded.borrow_mut().push(plan);
        }
        Ok(())
    });
    choose(&view.actions, 201, 101);
    assert!(
        !view.actions.0.connect.is_sensitive(),
        "duplicate connection cannot submit"
    );
    choose(&view.actions, 201, 301);
    assert!(view.actions.0.connect.is_sensitive());
    view.actions.0.connect.emit_clicked();
    assert_eq!(plans.borrow().len(), 1);
    assert_eq!(origins.borrow().as_slice(), &[graph.stamp]);
    assert_eq!(
        plans.borrow()[0].commands,
        vec![Opdracht::Verbind {
            uit: 201,
            naar: 301
        }]
    );
    assert!(view.actions.0.pending.get());
    assert_eq!(view.rows.len(), 2, "no optimistic link insertion");
    view.actions.0.connect.emit_clicked();
    assert_eq!(plans.borrow().len(), 1);
    let mut newer = fixture(2);
    Arc::make_mut(&mut newer.value).kabels.push(Kabel {
        id: 900,
        uit_knoop: 20,
        uit_poort: 201,
        in_knoop: 30,
        in_poort: 301,
        client: None,
    });
    view.update_graph(newer.clone());
    history
        .borrow_mut()
        .reduce(Input::Graph(newer.clone()))
        .unwrap();
    assert!(
        view.actions.0.pending.get(),
        "graph alone is not completion"
    );
    history
        .borrow_mut()
        .reduce(Input::CommandResult {
            ticket: plans.borrow()[0].ticket,
            result: Ok(newer.stamp),
        })
        .unwrap();
    assert!(!history.borrow().busy());
    view.set_actions_pending(false, None);
    assert_eq!(history.borrow().labels().0, Some("Connect"));
    // Multi-select and Delete produce one undo intent, not one per row.
    view.connections.select_row(Some(&view.rows[&500]));
    assert!(view.rows[&500].grab_focus());
    view.connections
        .emit_move_cursor(gtk::MovementStep::DisplayLines, 1, true, false);
    assert_eq!(
        view.connections.selected_rows().len(),
        2,
        "native extended row selection supports keyboard multi-select"
    );
    key(&view.connections, gtk::gdk::Key::Delete);
    assert_eq!(plans.borrow().len(), 2);
    assert_eq!(
        plans.borrow()[1].commands,
        vec![
            Opdracht::Ontkoppel { kabel: 500 },
            Opdracht::Ontkoppel { kabel: 501 }
        ]
    );
    key(&view.connections, gtk::gdk::Key::Escape);
    assert!(view.connections.selected_rows().is_empty());
    assert!(
        view.actions.0.pending.get(),
        "Escape cannot cancel submitted backend work"
    );
    assert!(view.actions.0.status.text().contains("still pending"));
    let ticket = plans.borrow()[1].ticket;
    history
        .borrow_mut()
        .reduce(Input::CommandResult {
            ticket,
            result: Err("Synthetic rejection".into()),
        })
        .unwrap_err();
    view.set_actions_pending(false, Some("Synthetic rejection"));
    assert!(view.actions.0.status.text().contains("Synthetic rejection"));
    assert_eq!(view.rows.len(), 3);
    view.connections.select_row(Some(&view.rows[&500]));
    view.actions.0.move_link.emit_clicked();
    assert_eq!(view.actions.0.connect.label().as_deref(), Some("Move"));
    assert!(!view.actions.0.source.is_sensitive());
    view.actions.0.cancel.emit_clicked();
    assert!(view.actions.0.model.borrow().moving.is_none());
    assert_eq!(plans.borrow().len(), 2, "cancel keeps original connection");
    view.connections.select_row(Some(&view.rows[&500]));
    view.actions.0.move_link.emit_clicked();
    choose(&view.actions, 201, 102);
    view.actions.0.connect.emit_clicked();
    assert_eq!(
        plans.borrow()[2].commands,
        vec![
            Opdracht::Ontkoppel { kabel: 500 },
            Opdracht::Verbind {
                uit: 201,
                naar: 102
            }
        ]
    );
    history
        .borrow_mut()
        .reduce(Input::CommandResult {
            ticket: plans.borrow()[2].ticket,
            result: Err("Synthetic move rejected".into()),
        })
        .unwrap_err();
    view.set_actions_pending(false, Some("Synthetic move rejected"));
    view.connections.select_row(Some(&view.rows[&500]));
    choose(&view.actions, 202, 301);
    assert!(
        !view.update_graph(fixture(1)),
        "stale snapshot rejected before widget changes"
    );
    assert_eq!(view.actions.0.model.borrow().source, Some(202));
    assert!(view.update_graph(fixture(3)));
    assert!(view.connections.selected_rows().is_empty());
    assert!(view.actions.0.model.borrow().source.is_none());
    let mut missing = fixture(4);
    Arc::make_mut(&mut missing.value).kabels[0].uit_poort = 99999;
    view.update_graph(missing);
    view.connections.select_row(Some(&view.rows[&500]));
    assert!(!view.actions.0.disconnect.is_sensitive());
    assert!(!view.actions.0.move_link.is_sensitive());
    let mut disconnected = fixture(5);
    Arc::make_mut(&mut disconnected.value).verbonden = false;
    view.update_graph(disconnected);
    assert!(!view.actions.0.source.is_sensitive());
    assert!(view.actions.0.model.borrow().selected.is_empty());
    window.close();
}
#[gtk::test]
fn synchronous_handoff_rejection_does_not_latch_pending_or_edit_graph() {
    let mut view = PatchbayOverview::new();
    view.update_graph(fixture(1));
    view.bind_history_actions(|_, _| Err("Synthetic admission rejection".into()));
    choose(&view.actions, 201, 301);
    view.actions.0.connect.emit_clicked();
    assert!(!view.actions.0.pending.get());
    assert_eq!(view.rows.len(), 2);
    assert!(view.actions.0.connect.is_sensitive());
    assert!(view
        .actions
        .0
        .status
        .text()
        .contains("Synthetic admission rejection"));
}

#[gtk::test]
fn functional_choices_remain_bounded_and_focusable_with_long_large_synthetic_graphs() {
    let mut graph = fixture(1);
    let template = graph.value.as_ref().clone();
    let data = Arc::make_mut(&mut graph.value);
    data.knopen.clear();
    data.poorten.clear();
    data.kabels.clear();
    for i in 0..80_u32 {
        for node in &template.knopen {
            let mut node = node.clone();
            node.id += i * 1000;
            node.naam = format!("fixture-node-{i}-{}", node.id);
            node.app = Some(format!(
                "Synthetic application {i} {}",
                "long label ".repeat(16)
            ));
            data.knopen.push(node);
        }
        for port in &template.poorten {
            let mut port = port.clone();
            port.id += i * 1000;
            port.knoop += i * 1000;
            data.poorten.push(port);
        }
        for link in &template.kabels {
            let mut link = link.clone();
            link.id += i * 1000;
            link.uit_knoop += i * 1000;
            link.in_knoop += i * 1000;
            link.uit_poort += i * 1000;
            link.in_poort += i * 1000;
            data.kabels.push(link);
        }
    }
    let mut view = PatchbayOverview::new();
    view.update_graph(graph);
    view.bind_history_actions(|_, _| Err("Synthetic adapter; no backend is running".into()));
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(view.widget())
        .build();
    window.present();
    for (width, height) in [(1400, 900), (900, 600), (420, 500)] {
        window.set_default_size(width, height);
        let context = glib::MainContext::default();
        let end = std::time::Instant::now() + std::time::Duration::from_millis(100);
        while std::time::Instant::now() < end {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            window.width(),
            width,
            "endpoint choices must not widen the window"
        );
        assert!(view.actions.0.source.grab_focus());
        assert!(view
            .actions
            .widget()
            .child_focus(gtk::DirectionType::TabForward));
    }
    assert_eq!(view.actions.0.outputs.n_items(), 161);
    assert_eq!(view.actions.0.inputs.n_items(), 321);
    #[cfg(feature = "a11y-tests")]
    {
        assert!(gtk::test_accessible_has_property(
            &view.actions.0.source,
            gtk::AccessibleProperty::Label
        ));
        assert!(gtk::test_accessible_has_property(
            &view.actions.0.destination,
            gtk::AccessibleProperty::Label
        ));
    }
    window.close();
}
