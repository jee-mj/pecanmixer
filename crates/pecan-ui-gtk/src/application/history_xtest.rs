//! Opt-in real GTK event delivery, confined to an explicitly marked Xvfb fixture.
use super::*;
use gtk::{glib, graphene, prelude::*};
use pecan_core::{
    domain::history::Action,
    graph::Opdracht,
    native_command::{Counts, NativeProgress},
};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    time::{Duration, Instant},
};
#[path = "history_shortcut_xtest.rs"]
mod shortcuts;
#[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(d: *mut c_void) -> c_int;
    fn XDefaultRootWindow(d: *mut c_void) -> c_ulong;
    fn XTranslateCoordinates(
        d: *mut c_void,
        src: c_ulong,
        dst: c_ulong,
        x: c_int,
        y: c_int,
        rx: *mut c_int,
        ry: *mut c_int,
        child: *mut c_ulong,
    ) -> c_int;
    fn XSetInputFocus(d: *mut c_void, w: c_ulong, revert: c_int, time: c_ulong) -> c_int;
    fn XKeysymToKeycode(d: *mut c_void, key: c_ulong) -> u8;
    fn XSync(d: *mut c_void, discard: c_int) -> c_int;
}
#[link(name = "libXtst.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XTestFakeMotionEvent(d: *mut c_void, s: c_int, x: c_int, y: c_int, delay: c_ulong) -> c_int;
    fn XTestFakeButtonEvent(d: *mut c_void, b: c_uint, down: c_int, delay: c_ulong) -> c_int;
    fn XTestFakeKeyEvent(d: *mut c_void, key: c_uint, down: c_int, delay: c_ulong) -> c_int;
}
unsafe extern "C" {
    fn gdk_x11_surface_get_xid(s: *mut c_void) -> c_ulong;
}
struct Input {
    d: *mut c_void,
    w: c_ulong,
}
impl Input {
    fn new(window: &gtk::Window) -> Self {
        assert!(std::env::var_os("PECAN_XTEST_FIXTURE_DISPLAY").is_some());
        assert_eq!(
            std::env::var("PECAN_XTEST_FIXTURE_DISPLAY").ok(),
            std::env::var("DISPLAY").ok()
        );
        assert!(WidgetExt::display(window).type_().name().contains("X11"));
        settle(40);
        // SAFETY: this test opens only its explicitly marked Xvfb connection and uses
        // its own mapped window. Production builds contain no Xlib/XTest calls.
        unsafe {
            let d = XOpenDisplay(std::ptr::null());
            assert!(!d.is_null());
            let w = gdk_x11_surface_get_xid(window.surface().unwrap().as_ptr().cast());
            XSetInputFocus(d, w, 2, 0);
            XSync(d, 0);
            settle(40);
            Self { d, w }
        }
    }
    fn point(&self, button: &gtk::Button, window: &gtk::Window) {
        let p = button
            .compute_point(
                window,
                &graphene::Point::new(button.width() as f32 / 2., button.height() as f32 / 2.),
            )
            .unwrap();
        unsafe {
            let (mut x, mut y, mut child) = (0, 0, 0);
            assert_ne!(
                XTranslateCoordinates(
                    self.d,
                    self.w,
                    XDefaultRootWindow(self.d),
                    p.x() as c_int,
                    p.y() as c_int,
                    &mut x,
                    &mut y,
                    &mut child
                ),
                0
            );
            assert_ne!(XTestFakeMotionEvent(self.d, -1, x, y, 0), 0);
            XSync(self.d, 0);
        }
        settle(35);
    }
    fn button(&self, down: bool) {
        unsafe {
            assert_ne!(XTestFakeButtonEvent(self.d, 1, i32::from(down), 0), 0);
            XSync(self.d, 0);
        }
        settle(35);
    }
    fn key(&self, key: c_ulong, down: bool) {
        unsafe {
            let code = XKeysymToKeycode(self.d, key);
            assert_ne!(code, 0);
            assert_ne!(
                XTestFakeKeyEvent(self.d, code.into(), i32::from(down), 0),
                0
            );
            XSync(self.d, 0);
        }
        settle(35);
    }
    fn focus(&self) {
        unsafe {
            XSetInputFocus(self.d, self.w, 2, 0);
            XSync(self.d, 0);
        }
        settle(35);
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            for key in [
                SPACE, ESCAPE, RETURN, TAB, 0x7a, 0x79, 0xffe1, 0xffe3, 0xffeb,
            ] {
                let code = XKeysymToKeycode(self.d, key);
                XTestFakeKeyEvent(self.d, code.into(), 0, 0);
            }
            XTestFakeButtonEvent(self.d, 1, 0, 0);
            XSync(self.d, 0);
            XCloseDisplay(self.d);
        }
    }
}
const SPACE: c_ulong = 0x20;
const RETURN: c_ulong = 0xff0d;
const ESCAPE: c_ulong = 0xff1b;
const TAB: c_ulong = 0xff09;
fn settle(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    let context = glib::MainContext::default();
    while Instant::now() < end {
        while context.pending() && Instant::now() < end {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn descendants(root: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut all = vec![root.as_ref().clone()];
    let mut i = 0;
    while i < all.len() {
        let mut child = all[i].first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            all.push(w);
        }
        i += 1;
    }
    all
}
fn button(view: &NativeWorkspace, undo: bool) -> gtk::Button {
    let prefix = if undo { "Undo " } else { "Redo " };
    descendants(view.widget())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().is_some_and(|s| s.starts_with(prefix)))
        .unwrap()
}
struct Fixture {
    _owner: ApplicationOwner,
    s: Session,
    d: FixtureDriver,
    view: Rc<NativeWorkspace>,
    window: gtk::Window,
    outside: gtk::Entry,
    origin: Graph,
    gens: BTreeMap<u32, u64>,
    input: Input,
}
fn complete(s: &Session, d: &FixtureDriver, origin: &Graph, gens: &BTreeMap<u32, u64>) -> Graph {
    let req = d.take_requests().pop().unwrap();
    let mut graph = (*origin.value).clone();
    for command in &req.intent.commands {
        match command {
            Opdracht::ZetMute { knoop, mute } => {
                graph
                    .knopen
                    .iter_mut()
                    .find(|n| n.id == *knoop)
                    .unwrap()
                    .mute = Some(*mute)
            }
            _ => panic!("synthetic mute only"),
        }
    }
    let frame = d.publish(graph, gens.clone());
    req.reporter.set(NativeProgress::Observed {
        stamp: frame.stamp,
        counts: Counts::default(),
    });
    s.poll();
    frame
}
fn fixture(undo: bool) -> Fixture {
    let owner = ApplicationOwner::fixture();
    let s = owner.activate();
    let d = owner.fixture_driver().unwrap();
    let graph: pecan_core::graph::Momentopname =
        serde_json::from_str(include_str!("../../../../fixtures/graph/audio-basic.json")).unwrap();
    let gens: BTreeMap<_, _> = graph
        .knopen
        .iter()
        .map(|n| (n.id, n.id as u64))
        .chain(graph.poorten.iter().map(|p| (p.id, p.id as u64)))
        .collect();
    let mut origin = d.publish(graph, gens.clone());
    s.poll();
    for node in [10, 20] {
        s.execute(&origin, "Mute", Action::Mute { node, muted: true })
            .unwrap();
        origin = complete(&s, &d, &origin, &gens);
    }
    if !undo {
        for _ in 0..2 {
            s.undo(&origin).unwrap();
            origin = complete(&s, &d, &origin, &gens);
        }
    }
    assert_eq!(s.history_lengths(), if undo { (2, 0) } else { (0, 2) });
    let view = NativeWorkspace::new(s.clone());
    view.poll();
    let outside = gtk::Entry::new();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.append(view.widget());
    root.append(&outside);
    let window = gtk::Window::builder()
        .child(&root)
        .default_width(1050)
        .default_height(800)
        .decorated(false)
        .build();
    window.present();
    let input = Input::new(&window);
    Fixture {
        _owner: owner,
        s,
        d,
        view,
        window,
        outside,
        origin,
        gens,
        input,
    }
}
fn prune(f: &mut Fixture, undo: bool) {
    let node = if undo { 20 } else { 10 };
    f.gens.insert(node, 10_000 + node as u64);
    for _ in 0..3 {
        f.origin = f.d.publish((*f.origin.value).clone(), f.gens.clone());
        f.view.poll();
        settle(15);
    }
    assert_eq!(f.s.history_lengths(), if undo { (1, 0) } else { (0, 1) });
}
fn assert_fresh(f: &Fixture, undo: bool, pointer: bool) {
    let current = button(&f.view, undo);
    assert!(current.is_sensitive());
    if pointer {
        f.input.point(&current, &f.window);
        f.input.button(true);
        f.input.button(false);
    } else {
        assert!(current.grab_focus());
        settle(25);
        f.input.key(SPACE, true);
        f.input.key(SPACE, false);
    }
    let requests = f.d.take_requests();
    assert_eq!(requests.len(), 1, "fresh activation must remain usable");
    assert_eq!(
        requests[0].intent.commands,
        vec![Opdracht::ZetMute {
            knoop: if undo { 10 } else { 20 },
            mute: !undo
        }]
    );
}
fn held_prune(undo: bool, pointer: bool) {
    let mut f = fixture(undo);
    let pressed = button(&f.view, undo);
    if pointer {
        f.input.point(&pressed, &f.window);
        f.input.button(true);
    } else {
        assert!(pressed.grab_focus());
        settle(25);
        f.input.key(SPACE, true);
    }
    assert!(f.d.take_requests().is_empty());
    prune(&mut f, undo);
    if pointer {
        f.input.button(false);
    } else {
        f.input.key(SPACE, false);
    }
    assert!(
        f.d.take_requests().is_empty(),
        "release must not act on the newly exposed history entry"
    );
    assert_fresh(&f, undo, pointer);
    f.window.close();
}
#[gtk::test]
fn xtest_held_undo_pointer_cannot_retarget_after_pruning() {
    held_prune(true, true)
}
#[gtk::test]
fn xtest_held_undo_space_cannot_retarget_after_pruning() {
    held_prune(true, false)
}
#[gtk::test]
fn xtest_held_redo_pointer_cannot_retarget_after_pruning() {
    held_prune(false, true)
}
#[gtk::test]
fn xtest_held_redo_space_cannot_retarget_after_pruning() {
    held_prune(false, false)
}
#[gtk::test]
fn xtest_focus_release_elsewhere_and_escape_cancel_allow_fresh_history_activation() {
    for undo in [true, false] {
        let f = fixture(undo);
        let pressed = button(&f.view, undo);
        assert!(pressed.grab_focus());
        settle(25);
        f.input.key(SPACE, true);
        assert!(f.outside.grab_focus());
        settle(35);
        f.input.key(SPACE, false);
        assert!(f.d.take_requests().is_empty());
        let current = button(&f.view, undo);
        f.input.point(&current, &f.window);
        f.input.button(true);
        f.input.key(ESCAPE, true);
        f.input.key(ESCAPE, false);
        f.input.button(false);
        assert!(f.d.take_requests().is_empty());
        let current = button(&f.view, undo);
        assert!(current.grab_focus());
        settle(25);
        f.input.key(RETURN, true);
        f.input.key(RETURN, false);
        settle(300);
        let reqs = f.d.take_requests();
        assert_eq!(
            reqs.len(),
            1,
            "fresh Return after focus/Escape cancellation"
        );
        assert_eq!(
            reqs[0].intent.commands,
            vec![Opdracht::ZetMute {
                knoop: if undo { 20 } else { 10 },
                mute: !undo
            }]
        );
        f.window.close();
    }
}
#[gtk::test]
fn xtest_retired_activation_signal_and_held_repeat_cannot_migrate_to_new_controls() {
    for undo in [true, false] {
        let mut f = fixture(undo);
        let old = button(&f.view, undo);
        assert!(old.grab_focus());
        settle(20);
        f.input.key(SPACE, true);
        prune(&mut f, undo);
        f.input.key(SPACE, true);
        f.input.key(SPACE, false);
        old.emit_activate();
        settle(300);
        assert!(f.d.take_requests().is_empty());
        let current = button(&f.view, undo);
        current.emit_activate();
        settle(300);
        let reqs = f.d.take_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            reqs[0].intent.commands,
            vec![Opdracht::ZetMute {
                knoop: if undo { 10 } else { 20 },
                mute: !undo
            }]
        );
        f.window.close();
    }
}
