//! Opt-in native GTK pointer delivery on a deliberately started Xvfb fixture.
//! Links only the test binary; no audio server or physical desktop is contacted.
use super::*;
use gtk::{glib, graphene};
use pecan_core::{backend::Stamp, domain::history::Action};
use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    time::{Duration, Instant},
};

#[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XTranslateCoordinates(
        display: *mut c_void,
        src: c_ulong,
        dst: c_ulong,
        x: c_int,
        y: c_int,
        rx: *mut c_int,
        ry: *mut c_int,
        child: *mut c_ulong,
    ) -> c_int;
    fn XSetInputFocus(display: *mut c_void, focus: c_ulong, revert: c_int, time: c_ulong) -> c_int;
    fn XKeysymToKeycode(display: *mut c_void, symbol: c_ulong) -> u8;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
}
#[link(name = "libXtst.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XTestFakeMotionEvent(
        display: *mut c_void,
        screen: c_int,
        x: c_int,
        y: c_int,
        delay: c_ulong,
    ) -> c_int;
    fn XTestFakeButtonEvent(
        display: *mut c_void,
        button: c_uint,
        pressed: c_int,
        delay: c_ulong,
    ) -> c_int;
    fn XTestFakeKeyEvent(
        display: *mut c_void,
        key: c_uint,
        pressed: c_int,
        delay: c_ulong,
    ) -> c_int;
}
unsafe extern "C" {
    fn gdk_x11_surface_get_xid(surface: *mut c_void) -> c_ulong;
}
struct Input {
    display: *mut c_void,
    window: c_ulong,
}
impl Input {
    fn new(window: &gtk::Window) -> Self {
        assert!(std::env::var_os("PECAN_XTEST_FIXTURE_DISPLAY").is_some());
        assert_eq!(
            std::env::var("PECAN_XTEST_FIXTURE_DISPLAY").ok(),
            std::env::var("DISPLAY").ok(),
            "explicitly mark the isolated fixture display"
        );
        assert!(gtk::prelude::WidgetExt::display(window)
            .type_()
            .name()
            .contains("X11"));
        settle();
        // SAFETY: this opt-in test opens only its marked Xvfb connection and uses
        // its own mapped surface. All Xlib calls stay on this fixture thread.
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            assert!(!display.is_null());
            let xid = gdk_x11_surface_get_xid(window.surface().unwrap().as_ptr().cast());
            XSetInputFocus(display, xid, 2, 0);
            XSync(display, 0);
            let input = Self {
                display,
                window: xid,
            };
            settle();
            input
        }
    }
    fn move_to(&self, view: &PatchbayOverview, window: &gtk::Window, point: layout::Point) {
        let point = view
            .canvas
            .compute_point(
                window,
                &graphene::Point::new(point.x as f32, point.y as f32),
            )
            .unwrap();
        unsafe {
            let (mut x, mut y, mut child) = (0, 0, 0);
            assert_ne!(
                XTranslateCoordinates(
                    self.display,
                    self.window,
                    XDefaultRootWindow(self.display),
                    point.x().round() as c_int,
                    point.y().round() as c_int,
                    &mut x,
                    &mut y,
                    &mut child
                ),
                0
            );
            assert_ne!(XTestFakeMotionEvent(self.display, -1, x, y, 0), 0);
            XSync(self.display, 0);
        }
        settle();
    }
    fn button(&self, down: bool) {
        unsafe {
            assert_ne!(XTestFakeButtonEvent(self.display, 1, i32::from(down), 0), 0);
            XSync(self.display, 0);
        }
        settle();
    }
    fn key(&self, symbol: c_ulong, down: bool) {
        unsafe {
            let code = XKeysymToKeycode(self.display, symbol);
            assert_ne!(code, 0);
            assert_ne!(
                XTestFakeKeyEvent(self.display, u32::from(code), i32::from(down), 0),
                0
            );
            XSync(self.display, 0);
        }
        settle();
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            XTestFakeButtonEvent(self.display, 1, 0, 0);
            XSync(self.display, 0);
            XCloseDisplay(self.display);
        }
    }
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(45);
    let context = glib::MainContext::default();
    while Instant::now() < end {
        while context.pending() && Instant::now() < end {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
type Calls = Rc<RefCell<Vec<(Stamped<Arc<Momentopname>>, Action)>>>;
fn setup() -> (
    PatchbayOverview,
    gtk::Window,
    Input,
    Calls,
    Stamped<Arc<Momentopname>>,
) {
    let graph = Stamped {
        stamp: Stamp {
            epoch: 1,
            revision: 1,
        },
        value: Arc::new(
            serde_json::from_str(include_str!(
                "../../../../../fixtures/graph/audio-basic.json"
            ))
            .unwrap(),
        ),
    };
    let mut view = PatchbayOverview::new();
    view.update_graph(graph.clone());
    let calls = Rc::new(RefCell::new(vec![]));
    let log = calls.clone();
    view.bind_history_actions(move |origin, input| {
        let pecan_core::domain::history::Input::Execute { action, .. } = input else {
            return Err("Expected synthetic Execute intent".into());
        };
        log.borrow_mut().push((origin, action));
        Ok(())
    });
    view.paned.set_position(360);
    let window = gtk::Window::builder()
        .default_width(1100)
        .default_height(1000)
        .decorated(false)
        .child(view.widget())
        .build();
    window.present();
    let input = Input::new(&window);
    (view, window, input, calls, graph)
}
fn port(view: &PatchbayOverview, id: u32) -> layout::Point {
    let geometry = view.geometry.borrow();
    let port = geometry.ports.iter().find(|p| p.id == id).unwrap();
    layout::Point {
        x: port.point.x + if port.input { 12. } else { -12. },
        y: port.point.y,
    }
}
fn drag(
    input: &Input,
    view: &PatchbayOverview,
    window: &gtk::Window,
    from: layout::Point,
    to: layout::Point,
) {
    input.move_to(view, window, from);
    input.button(true);
    input.move_to(view, window, to);
    input.button(false);
}
#[gtk::test]
fn xtest_connects_both_directions_through_native_capture_and_one_release() {
    for (from, to) in [(201, 301), (301, 201)] {
        let (view, window, input, calls, graph) = setup();
        let a = port(&view, from);
        let b = port(&view, to);
        drag(&input, &view, &window, a, b);
        assert_eq!(
            calls.borrow().len(),
            1,
            "one native drag yields one history handoff"
        );
        assert_eq!(calls.borrow()[0].1, Action::Connect([201, 301]));
        assert!(Arc::ptr_eq(&calls.borrow()[0].0.value, &graph.value));
        assert_eq!(
            view.rows.len(),
            2,
            "admission does not draw an observed new link"
        );
        window.close();
    }
}
#[gtk::test]
fn xtest_move_drop_disconnect_and_escape_cancel_are_distinct() {
    for mode in 0..3 {
        let (view, window, input, calls, _) = setup();
        let from = port(&view, 101);
        let to = if mode == 0 {
            port(&view, 301)
        } else {
            layout::Point { x: 350., y: 300. }
        };
        input.move_to(&view, &window, from);
        input.button(true);
        input.move_to(&view, &window, to);
        if mode == 2 {
            input.key(0xff1b, true);
            input.key(0xff1b, false);
        }
        input.button(false);
        match mode {
            0 => assert_eq!(
                calls.borrow()[0].1,
                Action::Move {
                    old: [201, 101],
                    new: [201, 301]
                }
            ),
            1 => assert_eq!(calls.borrow()[0].1, Action::Disconnect(vec![[201, 101]])),
            _ => assert!(calls.borrow().is_empty()),
        }
        window.close();
    }
}
#[gtk::test]
fn xtest_graph_replacement_cancels_held_pointer_until_release() {
    let (mut view, window, input, calls, mut graph) = setup();
    let a = port(&view, 201);
    let b = port(&view, 301);
    input.move_to(&view, &window, a);
    input.button(true);
    input.move_to(&view, &window, b);
    graph.stamp.revision += 1;
    Arc::make_mut(&mut graph.value)
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .serial = Some(9001);
    assert!(view.update_graph(graph.clone()));
    settle();
    input.button(false);
    assert!(
        calls.borrow().is_empty(),
        "release cannot adopt replacement snapshot"
    );
    drag(&input, &view, &window, port(&view, 201), port(&view, 301));
    assert_eq!(calls.borrow().len(), 1);
    assert!(Arc::ptr_eq(&calls.borrow()[0].0.value, &graph.value));
    window.close();
}
#[gtk::test]
fn xtest_cable_selection_shift_and_delete_share_keyboard_history_boundary() {
    let (view, window, input, calls, _) = setup();
    let points = view
        .geometry
        .borrow()
        .cables
        .iter()
        .map(|c| layout::Point {
            x: (c.from.x + c.to.x) * 0.125 + (c.control_from.x + c.control_to.x) * 0.375,
            y: (c.from.y + c.to.y) * 0.125 + (c.control_from.y + c.control_to.y) * 0.375,
        })
        .collect::<Vec<_>>();
    input.move_to(&view, &window, points[0]);
    input.button(true);
    input.button(false);
    input.key(0xffe1, true);
    input.move_to(&view, &window, points[1]);
    input.button(true);
    input.button(false);
    input.key(0xffe1, false);
    assert_eq!(view.connections.selected_rows().len(), 2);
    input.key(0xffff, true);
    input.key(0xffff, false);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        calls.borrow()[0].1,
        Action::Disconnect(vec![[201, 101], [202, 102]])
    );
    window.close();
}

#[gtk::test]
fn xtest_scroll_and_resize_during_drag_keep_canvas_coordinates_and_origin() {
    let (view, window, input, calls, graph) = setup();
    window.set_default_size(500, 1000);
    settle();
    view.canvas_scroll.hadjustment().set_value(0.);
    settle();
    let a = port(&view, 201);
    let b = port(&view, 301);
    input.move_to(&view, &window, a);
    input.button(true);
    let adjustment = view.canvas_scroll.hadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    window.set_default_size(600, 1000);
    settle();
    input.move_to(&view, &window, b);
    input.button(false);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(calls.borrow()[0].1, Action::Connect([201, 301]));
    assert!(Arc::ptr_eq(&calls.borrow()[0].0.value, &graph.value));
    window.close();
}
#[gtk::test]
fn xtest_unmap_during_move_preserves_original_and_fresh_remap_can_connect() {
    let (view, window, input, calls, _) = setup();
    input.move_to(&view, &window, port(&view, 101));
    input.button(true);
    input.move_to(&view, &window, layout::Point { x: 350., y: 300. });
    window.set_child(gtk::Widget::NONE);
    settle();
    input.button(false);
    assert!(
        calls.borrow().is_empty(),
        "unmap cannot become drop-on-empty disconnect"
    );
    window.set_child(Some(view.widget()));
    settle();
    drag(&input, &view, &window, port(&view, 201), port(&view, 301));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(calls.borrow()[0].1, Action::Connect([201, 301]));
    window.close();
}

fn module_header(view: &PatchbayOverview, node: u32, input: bool) -> layout::Point {
    let geometry = view.geometry.borrow();
    let module = geometry
        .modules
        .iter()
        .find(|m| m.node == node && m.input == input)
        .unwrap();
    layout::Point {
        x: module.origin.x + 20.,
        y: module.origin.y + 20.,
    }
}
fn module_origin(view: &PatchbayOverview, node: u32, input: bool) -> layout::Point {
    view.geometry
        .borrow()
        .modules
        .iter()
        .find(|m| m.node == node && m.input == input)
        .unwrap()
        .origin
}
#[gtk::test]
fn xtest_module_movement_updates_endpoints_then_port_patch_still_uses_one_audio_intent() {
    let (view, window, input, calls, _) = setup();
    let original = module_origin(&view, 20, false);
    let from = module_header(&view, 20, false);
    drag(
        &input,
        &view,
        &window,
        from,
        layout::Point {
            x: from.x + 120.,
            y: from.y + 70.,
        },
    );
    assert_point_near(
        module_origin(&view, 20, false),
        layout::Point {
            x: original.x + 120.,
            y: original.y + 70.,
        },
    );
    assert!(
        calls.borrow().is_empty(),
        "placement is not an undo/audio operation"
    );
    {
        let geometry = view.geometry.borrow();
        let endpoint = geometry.ports.iter().find(|p| p.id == 201).unwrap().point;
        assert_eq!(
            geometry.cables.iter().find(|c| c.id == 500).unwrap().from,
            endpoint
        );
    }
    drag(&input, &view, &window, port(&view, 201), port(&view, 301));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(calls.borrow()[0].1, Action::Connect([201, 301]));
    window.close();
}
#[gtk::test]
fn xtest_rearrange_retires_held_module_and_cable_gestures_without_audio() {
    for cable in [false, true] {
        let (view, window, input, calls, _) = setup();
        let original = module_origin(&view, 20, false);
        let from = if cable {
            port(&view, 101)
        } else {
            module_header(&view, 20, false)
        };
        input.move_to(&view, &window, from);
        input.button(true);
        input.move_to(
            &view,
            &window,
            layout::Point {
                x: from.x + 80.,
                y: from.y + 40.,
            },
        );
        view.rearrange();
        settle();
        input.move_to(&view, &window, layout::Point { x: 350., y: 300. });
        input.button(false);
        assert_eq!(module_origin(&view, 20, false), original);
        assert!(
            calls.borrow().is_empty(),
            "reset cannot become move/disconnect on late release"
        );
        window.close();
    }
}
#[gtk::test]
fn xtest_graph_replacement_cancels_module_move_and_new_press_can_move_replacement() {
    let (mut view, window, input, calls, mut graph) = setup();
    let from = module_header(&view, 20, false);
    input.move_to(&view, &window, from);
    input.button(true);
    input.move_to(
        &view,
        &window,
        layout::Point {
            x: from.x + 30.,
            y: from.y + 20.,
        },
    );
    let saved = module_origin(&view, 20, false);
    graph.stamp.revision += 1;
    Arc::make_mut(&mut graph.value)
        .knopen
        .iter_mut()
        .find(|n| n.id == 20)
        .unwrap()
        .serial = Some(9100);
    assert!(view.update_graph(graph));
    input.move_to(
        &view,
        &window,
        layout::Point {
            x: from.x + 300.,
            y: from.y + 100.,
        },
    );
    input.button(false);
    assert_eq!(module_origin(&view, 20, false), saved);
    assert!(calls.borrow().is_empty());
    let from = module_header(&view, 20, false);
    drag(
        &input,
        &view,
        &window,
        from,
        layout::Point {
            x: from.x + 20.,
            y: from.y + 10.,
        },
    );
    assert_point_near(
        module_origin(&view, 20, false),
        layout::Point {
            x: saved.x + 20.,
            y: saved.y + 10.,
        },
    );
    assert!(calls.borrow().is_empty());
    window.close();
}

fn assert_point_near(actual: layout::Point, expected: layout::Point) {
    assert!(
        (actual.x - expected.x).abs() < 0.01 && (actual.y - expected.y).abs() < 0.01,
        "actual={actual:?}, expected={expected:?}"
    );
}

#[gtk::test]
fn xtest_module_capture_survives_scroll_resize_without_switching_target() {
    let (view, window, input, calls, _) = setup();
    window.set_default_size(500, 1000);
    settle();
    view.canvas_scroll.hadjustment().set_value(0.);
    settle();
    let original = module_origin(&view, 20, false);
    let start = module_header(&view, 20, false);
    input.move_to(&view, &window, start);
    input.button(true);
    let adjustment = view.canvas_scroll.hadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    settle();
    window.set_default_size(600, 1000);
    settle();
    let end = layout::Point { x: 320., y: 100. };
    input.move_to(&view, &window, end);
    input.button(false);
    assert_point_near(
        module_origin(&view, 20, false),
        layout::Point {
            x: original.x + end.x - start.x,
            y: original.y + end.y - start.y,
        },
    );
    assert!(calls.borrow().is_empty());
    window.close();
}
#[gtk::test]
fn xtest_disconnect_during_module_drag_keeps_last_position_and_cancels_late_release() {
    let (mut view, window, input, calls, mut graph) = setup();
    let from = module_header(&view, 20, false);
    input.move_to(&view, &window, from);
    input.button(true);
    input.move_to(
        &view,
        &window,
        layout::Point {
            x: from.x + 30.,
            y: from.y + 20.,
        },
    );
    let saved = module_origin(&view, 20, false);
    let mut gone = graph.clone();
    gone.stamp.revision += 1;
    Arc::make_mut(&mut gone.value).verbonden = false;
    view.update_graph(gone);
    settle();
    input.button(false);
    graph.stamp.epoch += 1;
    graph.stamp.revision += 2;
    assert!(view.update_graph(graph));
    settle();
    assert_eq!(module_origin(&view, 20, false), saved);
    let from = module_header(&view, 20, false);
    drag(
        &input,
        &view,
        &window,
        from,
        layout::Point {
            x: from.x + 20.,
            y: from.y + 10.,
        },
    );
    assert_point_near(
        module_origin(&view, 20, false),
        layout::Point {
            x: saved.x + 20.,
            y: saved.y + 10.,
        },
    );
    assert!(calls.borrow().is_empty());
    window.close();
}
