//! Synthetic placement fixtures only; movement never starts an audio server.
use super::*;
use crate::widgets::PatchbayOverview;
use pecan_core::{
    backend::{Stamp, Stamped},
    graph::Momentopname,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
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
fn setup() -> (
    PatchbayOverview,
    Rc<Cell<usize>>,
    Stamped<Arc<Momentopname>>,
) {
    let mut view = PatchbayOverview::new();
    let graph = fixture(1);
    view.update_graph(graph.clone());
    let calls = Rc::new(Cell::new(0));
    let log = calls.clone();
    view.bind_history_actions(move |_, _| {
        log.set(log.get() + 1);
        Ok(())
    });
    (view, calls, graph)
}
fn module(view: &PatchbayOverview, node: u32, input: bool) -> super::super::layout::Module {
    view.geometry
        .borrow()
        .modules
        .iter()
        .find(|m| m.node == node && m.input == input)
        .unwrap()
        .clone()
}
fn header(view: &PatchbayOverview, node: u32, input: bool) -> Point {
    let m = module(view, node, input);
    Point {
        x: m.origin.x + 20.,
        y: m.origin.y + 20.,
    }
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(100);
    let context = glib::MainContext::default();
    while Instant::now() < end {
        while context.pending() && Instant::now() < end {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[gtk::test]
fn movement_updates_frames_ports_and_curves_without_audio_or_widget_rebuilds() {
    let (view, calls, _) = setup();
    let initial = module(&view, 20, false);
    let frame = view.session.0.frames.borrow()[&(20, false)]
        .upgrade()
        .unwrap();
    let old_ports = view.geometry.borrow().ports.clone();
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(900)
        .child(view.widget())
        .build();
    window.present();
    settle();
    let allocated_before = frame
        .compute_point(&view.modules, &gtk::graphene::Point::new(0.0, 0.0))
        .unwrap();
    assert!(view.session.begin(header(&view, 20, false)));
    for i in 1..=3000 {
        view.session.motion(f64::from(i), 80.);
    }
    assert_eq!(view.session.0.positions.borrow().len(), 1);
    assert!(view.session.0.dirty.get());
    assert_eq!(
        module(&view, 20, false).origin,
        initial.origin,
        "motion replaces one pending position until the frame flush"
    );
    view.session.flush();
    settle();
    let allocated = frame
        .compute_point(&view.modules, &gtk::graphene::Point::new(0.0, 0.0))
        .unwrap();
    let expected = module(&view, 20, false).origin;
    assert!(
        (f64::from(allocated.x() - allocated_before.x()) - (expected.x - initial.origin.x)).abs()
            < 1.0
            && (f64::from(allocated.y() - allocated_before.y()) - (expected.y - initial.origin.y))
                .abs()
                < 1.0,
        "native allocation delta must follow geometry; frame borders remain toolkit-owned"
    );
    assert_eq!(
        window.width(),
        900,
        "far-away modules scroll instead of widening the window"
    );
    assert_eq!(
        module(&view, 20, false).origin,
        Point {
            x: initial.origin.x + 3000.,
            y: initial.origin.y + 80.
        }
    );
    assert_eq!(
        view.session.0.frames.borrow()[&(20, false)]
            .upgrade()
            .unwrap(),
        frame
    );
    for old in old_ports.iter().filter(|p| p.node == 20) {
        let geometry = view.geometry.borrow();
        let port = geometry.ports.iter().find(|p| p.id == old.id).unwrap();
        assert_eq!(
            port.point,
            Point {
                x: old.point.x + 3000.,
                y: old.point.y + 80.
            }
        );
    }
    for cable in &view.geometry.borrow().cables {
        let g = view.geometry.borrow();
        assert_eq!(
            cable.from,
            g.ports
                .iter()
                .find(|p| p.id == cable.from_port)
                .unwrap()
                .point
        );
        assert_eq!(
            cable.to,
            g.ports
                .iter()
                .find(|p| p.id == cable.to_port)
                .unwrap()
                .point
        );
    }
    view.session.release(3000., 80.);
    assert_eq!(calls.get(), 0);
    window.close();
}
#[gtk::test]
fn movement_clamps_to_zero_raises_module_and_hit_testing_respects_occlusion() {
    let (view, calls, _) = setup();
    let target = module(&view, 10, true).origin;
    let original = module(&view, 20, false).origin;
    assert!(view.session.begin(header(&view, 20, false)));
    view.session
        .release(target.x - original.x, target.y - original.y);
    let geometry = view.geometry.borrow();
    assert_eq!(geometry.modules.last().unwrap().node, 20);
    let port = geometry.ports.iter().find(|p| p.id == 201).unwrap();
    assert_eq!(
        geometry
            .port_at(Point {
                x: target.x + 20.,
                y: port.point.y
            })
            .unwrap()
            .id,
        201,
        "topmost module occludes the input beneath it"
    );
    assert_eq!(
        view.modules.last_child(),
        view.session.0.frames.borrow()[&(20, false)].upgrade()
    );
    drop(geometry);
    assert!(view.session.begin(header(&view, 20, false)));
    view.session.release(-10000., -10000.);
    assert_eq!(module(&view, 20, false).origin, Point { x: 0., y: 0. });
    assert_eq!(calls.get(), 0);
}
#[gtk::test]
fn rearrange_clears_overrides_preserves_selection_and_retires_held_moves() {
    let (view, calls, _) = setup();
    view.actions.select([500]);
    let initial = module(&view, 20, false).origin;
    assert!(view.session.begin(header(&view, 20, false)));
    view.session.motion(100., 50.);
    view.session.flush();
    assert_ne!(module(&view, 20, false).origin, initial);
    view.rearrange.emit_clicked();
    assert!(view.session.0.positions.borrow().is_empty());
    assert_eq!(module(&view, 20, false).origin, initial);
    assert_eq!(view.connections.selected_rows().len(), 1);
    assert!(!view.session.begin(header(&view, 10, true)));
    view.session.motion(500., 500.);
    view.session.release(500., 500.);
    assert_eq!(module(&view, 20, false).origin, initial);
    assert!(view.session.0.positions.borrow().is_empty());
    assert_eq!(calls.get(), 0);
}
#[gtk::test]
fn absent_module_positions_survive_reconnect_but_rearrange_clears_them_too() {
    for reset in [false, true] {
        let (mut view, calls, source) = setup();
        let initial = module(&view, 20, false).origin;
        assert!(view.session.begin(header(&view, 20, false)));
        view.session.release(130., 70.);
        let moved = module(&view, 20, false).origin;
        let mut missing = source.clone();
        missing.stamp.revision += 1;
        Arc::make_mut(&mut missing.value).verbonden = false;
        view.update_graph(missing);
        assert!(!view.session.0.positions.borrow().is_empty());
        if reset {
            view.rearrange();
        }
        let mut back = source;
        back.stamp.epoch += 1;
        back.stamp.revision += 2;
        let graph = Arc::make_mut(&mut back.value);
        for node in &mut graph.knopen {
            if node.id == 20 {
                node.id = 1020;
                node.serial = Some(9000);
            }
        }
        for port in &mut graph.poorten {
            if port.knoop == 20 {
                port.knoop = 1020;
                port.id += 1000;
            }
        }
        for link in &mut graph.kabels {
            if link.uit_knoop == 20 {
                link.uit_knoop = 1020;
                link.uit_poort += 1000;
            }
        }
        view.update_graph(back);
        assert_eq!(
            module(&view, 1020, false).origin,
            if reset { initial } else { moved }
        );
        assert_eq!(calls.get(), 0);
    }
}
#[gtk::test]
fn newer_graphs_and_replacements_cancel_without_moving_a_new_target() {
    for mode in 0..5 {
        let (mut view, calls, source) = setup();
        assert!(view.session.begin(header(&view, 20, false)));
        view.session.motion(40., 30.);
        view.session.flush();
        let saved = view.session.0.positions.borrow().clone();
        let mut next = source;
        next.stamp.revision += 1;
        match mode {
            0 => {}
            1 => next.stamp.epoch += 1,
            2 => {
                Arc::make_mut(&mut next.value)
                    .knopen
                    .iter_mut()
                    .find(|n| n.id == 20)
                    .unwrap()
                    .serial = Some(9999)
            }
            3 => {
                let graph = Arc::make_mut(&mut next.value);
                graph.knopen.retain(|n| n.id != 20);
                graph.poorten.retain(|p| p.knoop != 20);
            }
            _ => Arc::make_mut(&mut next.value).verbonden = false,
        }
        assert!(view.update_graph(next));
        assert!(view.session.0.movement.borrow().as_ref().unwrap().cancelled);
        view.session.motion(900., 900.);
        view.session.release(900., 900.);
        assert_eq!(*view.session.0.positions.borrow(), saved);
        assert_eq!(calls.get(), 0);
    }
}
#[gtk::test]
fn viewport_resize_preserves_overrides_and_captured_target_and_scroll_does_not_retarget() {
    let (view, calls, _) = setup();
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(900)
        .child(view.widget())
        .build();
    window.present();
    settle();
    let right = module(&view, 10, true).origin;
    assert!(view.session.begin(header(&view, 10, true)));
    window.set_default_size(1400, 900);
    settle();
    assert_eq!(
        module(&view, 10, true).origin,
        right,
        "held default module is pinned to its captured position"
    );
    view.session.motion(80., 40.);
    view.session.flush();
    let moved = module(&view, 10, true).origin;
    assert_eq!(
        moved,
        Point {
            x: right.x + 80.,
            y: right.y + 40.
        }
    );
    window.set_default_size(420, 600);
    settle();
    view.canvas_scroll.hadjustment().set_value(100.);
    settle();
    assert_eq!(module(&view, 10, true).origin, moved);
    view.session.release(80., 40.);
    window.set_default_size(1400, 900);
    settle();
    assert_eq!(module(&view, 10, true).origin, moved);
    view.rearrange();
    settle();
    assert_ne!(module(&view, 10, true).origin, moved);
    assert_eq!(window.width(), 1400);
    assert_eq!(calls.get(), 0);
    window.close();
}
#[gtk::test]
fn full_graph_positions_use_core_module_keys_and_retained_reset_widget_does_not_keep_controller_alive(
) {
    let (view, calls, source) = setup();
    for group in audio::layout(&source.value).groups {
        for expected in group.modules {
            assert_eq!(
                module(&view, expected.node, expected.side == audio::Side::Right).key,
                expected.key
            );
        }
    }
    let reset = view.rearrange.clone();
    let canvas = view.canvas.clone();
    let weak = Rc::downgrade(&view.session.0);
    drop(view);
    assert!(weak.upgrade().is_none());
    reset.emit_clicked();
    assert_eq!(calls.get(), 0);
    drop(canvas);
}
