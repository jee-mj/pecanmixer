//! Hand-authored synthetic graph fixtures; no server or device access.
use super::*;
use crate::widgets::PatchbayOverview;
use pecan_core::{
    backend::Stamp,
    domain::{
        buses::BusConfig,
        history::{self, Input},
    },
    graph::{Kabel, Opdracht},
};

type Calls = Rc<RefCell<Vec<(Stamped<Arc<Momentopname>>, Input)>>>;
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
fn setup() -> (PatchbayOverview, Calls, Stamped<Arc<Momentopname>>) {
    let mut view = PatchbayOverview::new();
    let source = fixture(1);
    view.update_graph(source.clone());
    let calls = Rc::new(RefCell::new(vec![]));
    let log = calls.clone();
    view.bind_history_actions(move |origin, input| {
        log.borrow_mut().push((origin, input));
        Ok(())
    });
    (view, calls, source)
}
fn port(view: &PatchbayOverview, id: u32) -> Point {
    let layout = view.geometry.borrow();
    let port = layout.ports.iter().find(|p| p.id == id).unwrap();
    Point {
        x: port.point.x + if port.input { 12.0 } else { -12.0 },
        y: port.point.y,
    }
}
fn release(pointer: &Pointer, from: Point, to: Point) {
    pointer.motion(to.x - from.x, to.y - from.y);
    pointer.release(to.x - from.x, to.y - from.y);
}
fn action(calls: &Calls, index: usize) -> Action {
    let calls = calls.borrow();
    let Input::Execute { action, .. } = &calls[index].1 else {
        panic!("one history execute intent")
    };
    action.clone()
}
fn cable_point(view: &PatchbayOverview, id: u32) -> Point {
    let layout = view.geometry.borrow();
    let c = layout.cables.iter().find(|c| c.id == id).unwrap();
    Point {
        x: (c.from.x + c.to.x) * 0.125 + (c.control_from.x + c.control_to.x) * 0.375,
        y: (c.from.y + c.to.y) * 0.125 + (c.control_from.y + c.control_to.y) * 0.375,
    }
}
fn key(canvas: &gtk::Overlay, key: gtk::gdk::Key) {
    let controllers = canvas.observe_controllers();
    let keys = (0..controllers.n_items())
        .find_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .unwrap();
    assert!(keys.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()]
    ));
}
#[gtk::test]
fn both_directions_use_exact_origin_and_one_core_history_action_per_release() {
    for (from, to) in [(201, 301), (301, 201)] {
        let (view, calls, graph) = setup();
        let a = port(&view, from);
        let b = port(&view, to);
        assert!(view.pointer.begin(a));
        assert!(Arc::ptr_eq(
            &view.pointer.0.draft.borrow().as_ref().unwrap().origin.value,
            &graph.value
        ));
        assert!(calls.borrow().is_empty());
        release(&view.pointer, a, b);
        view.pointer.release(b.x - a.x, b.y - a.y);
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(action(&calls, 0), Action::Connect([201, 301]));
        assert!(Arc::ptr_eq(&calls.borrow()[0].0.value, &graph.value));
        assert_eq!(
            history::plan(&action(&calls, 0), &graph.value, &BusConfig::default()).unwrap(),
            vec![Opdracht::Verbind {
                uit: 201,
                naar: 301
            }]
        );
        assert_eq!(view.rows.len(), 2);
        assert_eq!(graph.value.kabels.len(), 2);
        assert!(
            !view.pointer.begin(a),
            "pending admission cannot become another request"
        );
    }
}
#[gtk::test]
fn duplicate_wrong_side_and_motion_storms_do_not_dispatch_or_rebuild() {
    let (view, calls, _) = setup();
    let a = port(&view, 201);
    let valid = port(&view, 301);
    let module = view.modules.first_child().unwrap();
    let layout = view.geometry.borrow().clone();
    assert!(view.pointer.begin(a));
    for _ in 0..3000 {
        view.pointer.motion(valid.x - a.x, valid.y - a.y);
    }
    assert!(calls.borrow().is_empty());
    assert_eq!(view.modules.first_child().unwrap(), module);
    assert_eq!(*view.geometry.borrow(), layout);
    let duplicate = port(&view, 101);
    release(&view.pointer, a, duplicate);
    assert!(calls.borrow().is_empty());
    assert!(view.pointer.begin(a));
    let wrong = port(&view, 202);
    release(&view.pointer, a, wrong);
    assert!(calls.borrow().is_empty());
}
#[gtk::test]
fn move_empty_original_target_and_cancel_have_distinct_legacy_semantics() {
    for mode in 0..5 {
        let (view, calls, _) = setup();
        let from = port(&view, 101);
        let empty = Point { x: 350., y: 350. };
        assert!(view.pointer.begin(from));
        let to = match mode {
            0 => port(&view, 301),
            1 => from,
            _ => empty,
        };
        if mode == 3 {
            key(&view.canvas, gtk::gdk::Key::Escape);
            assert!(
                !view.pointer.begin(from),
                "cancelled held sequence cannot rearm before release"
            );
        }
        if mode == 4 {
            view.pointer.finish_cancel();
        }
        release(&view.pointer, from, to);
        match mode {
            0 => assert_eq!(
                action(&calls, 0),
                Action::Move {
                    old: [201, 101],
                    new: [201, 301]
                }
            ),
            1 | 2 => assert_eq!(action(&calls, 0), Action::Disconnect(vec![[201, 101]])),
            _ => assert!(
                calls.borrow().is_empty(),
                "Escape/native cancellation preserves original"
            ),
        }
    }
}
#[gtk::test]
fn incoming_selection_or_sole_cable_controls_move_pickup() {
    let (mut view, calls, mut graph) = setup();
    Arc::make_mut(&mut graph.value).kabels.push(Kabel {
        id: 900,
        uit_knoop: 20,
        uit_poort: 202,
        in_knoop: 10,
        in_poort: 101,
        client: None,
    });
    graph.stamp.revision += 1;
    view.update_graph(graph);
    let from = port(&view, 101);
    assert!(view.pointer.begin(from));
    assert!(
        view.pointer
            .0
            .draft
            .borrow()
            .as_ref()
            .unwrap()
            .moving
            .is_none(),
        "ambiguous unselected input starts a reverse connection"
    );
    view.pointer.finish_cancel();
    view.actions.select([900]);
    assert!(view.pointer.begin(from));
    let to = port(&view, 301);
    release(&view.pointer, from, to);
    assert_eq!(
        action(&calls, 0),
        Action::Move {
            old: [202, 101],
            new: [202, 301]
        }
    );
}
#[gtk::test]
fn canvas_selection_matches_list_and_delete_is_one_history_intent() {
    let (view, calls, _) = setup();
    let a = cable_point(&view, 500);
    let b = cable_point(&view, 501);
    assert!(view.pointer.click(a, false));
    assert_eq!(view.connections.selected_rows().len(), 1);
    assert!(view.pointer.click(b, true));
    assert_eq!(view.connections.selected_rows().len(), 2);
    key(&view.canvas, gtk::gdk::Key::Delete);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        action(&calls, 0),
        Action::Disconnect(vec![[201, 101], [202, 102]])
    );
    key(&view.canvas, gtk::gdk::Key::Escape);
    assert!(view.connections.selected_rows().is_empty());
    assert!(
        !view.actions.enabled(),
        "selection cancellation cannot cancel submitted work"
    );
}
#[gtk::test]
fn all_new_graphs_cancel_held_origins_without_rebinding_late_release() {
    for mode in 0..5 {
        let (mut view, calls, source) = setup();
        let from = port(&view, 201);
        let to = port(&view, 301);
        assert!(view.pointer.begin(from));
        view.pointer.motion(to.x - from.x, to.y - from.y);
        assert!(!view.update_graph(source.clone()));
        assert!(!view.pointer.0.draft.borrow().as_ref().unwrap().cancelled);
        let mut next = source.clone();
        next.stamp.revision += 1;
        match mode {
            0 => {}
            1 => next.stamp.epoch += 1,
            2 => Arc::make_mut(&mut next.value).knopen[1].serial = Some(9999),
            3 => Arc::make_mut(&mut next.value)
                .poorten
                .retain(|p| p.id != 201),
            _ => Arc::make_mut(&mut next.value).verbonden = false,
        }
        assert!(view.update_graph(next));
        assert!(view.pointer.0.draft.borrow().as_ref().unwrap().cancelled);
        assert!(!view.pointer.begin(from));
        release(&view.pointer, from, to);
        assert!(calls.borrow().is_empty());
        let recreated = Stamped {
            stamp: source.stamp,
            value: Arc::new(source.value.as_ref().clone()),
        };
        assert!(view
            .actions
            .submit_action(recreated, Action::Connect([201, 301]))
            .is_err());
    }
}
#[gtk::test]
fn unbound_rejection_pending_and_retirement_stay_truthful() {
    let mut view = PatchbayOverview::new();
    view.update_graph(fixture(1));
    let from = port(&view, 201);
    let to = port(&view, 301);
    assert!(!view.pointer.begin(from));
    let calls = Rc::new(Cell::new(0));
    let log = calls.clone();
    view.bind_history_actions(move |_, _| {
        log.set(log.get() + 1);
        Err("Synthetic handoff rejection".into())
    });
    assert!(view.pointer.begin(from));
    release(&view.pointer, from, to);
    assert_eq!(calls.get(), 1);
    assert!(view.actions.enabled());
    assert_eq!(view.rows.len(), 2);
    assert!(view.pointer.begin(from));
    view.set_actions_pending(true, None);
    view.set_actions_pending(false, Some("Synthetic execution rejection"));
    release(&view.pointer, from, to);
    assert_eq!(
        calls.get(),
        1,
        "pending cancellation cannot later submit against released capacity"
    );
    let retained = view.pointer.clone();
    drop(view);
    assert!(!retained.begin(from));
}

#[gtk::test]
fn overview_only_refresh_suspends_intents_and_missing_ports_are_not_offered() {
    let (mut view, calls, source) = setup();
    let from = port(&view, 201);
    let to = port(&view, 301);
    assert!(view.pointer.begin(from));
    view.update(
        &pecan_core::overview::project(&source.value),
        &source.value.kabels,
    );
    release(&view.pointer, from, to);
    assert!(calls.borrow().is_empty());
    assert!(!view.actions.enabled());
    assert!(!view.update_graph(source.clone()));
    assert!(!view.actions.enabled());
    let mut graph = source;
    graph.stamp.revision += 1;
    assert!(view.update_graph(graph.clone()));
    assert!(view.actions.enabled());
    Arc::make_mut(&mut graph.value)
        .poorten
        .retain(|p| p.id != 201);
    graph.stamp.revision += 1;
    assert!(view.update_graph(graph));
    assert!(!view
        .geometry
        .borrow()
        .ports
        .iter()
        .any(|port| port.id == 201));
    // Reflow may put another live port in the old screen location. Only a
    // fresh press may pick that port; a held draft was cancelled above.
    assert!(view.pointer.begin(from));
    assert_eq!(view.pointer.0.draft.borrow().as_ref().unwrap().anchor, 202);
    view.pointer.finish_cancel();
    assert!(calls.borrow().is_empty());
    #[cfg(feature = "a11y-tests")]
    {
        assert!(gtk::test_accessible_has_property(
            &view.canvas,
            gtk::AccessibleProperty::Label
        ));
    }
}
