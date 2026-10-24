//! Opt-in real GTK events confined to the explicitly marked synthetic Xvfb.
use super::tests::{apply, row, setup};
use super::*;
use gtk::graphene;
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
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
struct XInput {
    d: *mut c_void,
    w: c_ulong,
}
impl XInput {
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
    fn point(&self, button: &impl IsA<gtk::Widget>, window: &gtk::Window) {
        let p = button
            .as_ref()
            .compute_point(
                window,
                &graphene::Point::new(
                    button.as_ref().width() as f32 / 2.,
                    button.as_ref().height() as f32 / 2.,
                ),
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
}
impl Drop for XInput {
    fn drop(&mut self) {
        unsafe {
            for key in [SPACE, ESCAPE, RETURN, TAB, RIGHT, SHIFT] {
                let code = XKeysymToKeycode(self.d, key);
                XTestFakeKeyEvent(self.d, code.into(), 0, 0);
            }
            XTestFakeButtonEvent(self.d, 1, 0, 0);
            XSync(self.d, 0);
            XCloseDisplay(self.d);
        }
    }
}
const RIGHT: c_ulong = 0xff53;
const SHIFT: c_ulong = 0xffe1;
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

impl XInput {
    fn move_by(&self, widget: &impl IsA<gtk::Widget>, window: &gtk::Window, dy: i32) {
        let p = widget
            .as_ref()
            .compute_point(
                window,
                &graphene::Point::new(
                    widget.as_ref().width() as f32 / 2.,
                    widget.as_ref().height() as f32 / 2. + dy as f32,
                ),
            )
            .unwrap();
        unsafe {
            let (mut x, mut y, mut child) = (0, 0, 0);
            assert_ne!(
                XTranslateCoordinates(
                    self.d,
                    self.w,
                    XDefaultRootWindow(self.d),
                    p.x() as i32,
                    p.y() as i32,
                    &mut x,
                    &mut y,
                    &mut child
                ),
                0
            );
            XTestFakeMotionEvent(self.d, -1, x, y, 0);
            XSync(self.d, 0);
        }
        settle(30);
    }
}
#[gtk::test]
fn actual_relative_drag_observation_cancel_and_fresh_drag() {
    let (s, d, v, gens, key) = setup();
    let window = gtk::Window::builder()
        .child(v.widget())
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle(120);
    let r = row(&v, &key);
    let input = XInput::new(&window);
    input.point(&r.scale, &window);
    input.button(true);
    assert!(r.policy.borrow().dragging());
    input.move_by(&r.scale, &window, -12);
    let first = d.take_requests().pop().expect("captured pointer motion");
    v.refresh();
    input.move_by(&r.scale, &window, -24);
    assert!(d.take_requests().is_empty());
    apply(&s, &d, &v, &first, &gens);
    let last = d.take_requests().pop().expect("coalesced desired target");
    assert!(r.scale.is_sensitive());
    input.button(false);
    assert!(!r.policy.borrow().dragging());
    apply(&s, &d, &v, &last, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    input.point(&r.scale, &window);
    input.button(true);
    input.move_by(&r.scale, &window, 12);
    input.button(false);
    let req = d.take_requests().pop().expect("fresh drag");
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (2, 0));
    window.close();
}
#[gtk::test]
fn actual_tab_shift_release_elsewhere_and_fresh_arrow_recover() {
    let (s, d, v, gens, key) = setup();
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let outside = gtk::Entry::new();
    outer.append(v.widget());
    outer.append(&outside);
    let window = gtk::Window::builder()
        .child(&outer)
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle(120);
    let r = row(&v, &key);
    let input = XInput::new(&window);
    r.scale.grab_focus();
    input.key(RIGHT, true);
    let req = d.take_requests().pop().unwrap();
    input.key(TAB, true);
    input.key(TAB, false);
    input.key(RIGHT, false);
    apply(&s, &d, &v, &req, &gens);
    assert!(!s.busy());
    r.scale.grab_focus();
    input.key(RIGHT, true);
    input.key(RIGHT, false);
    let req = d
        .take_requests()
        .pop()
        .expect("fresh arrow after off-widget release");
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (2, 0));
    input.key(SHIFT, true);
    outside.grab_focus();
    input.key(SHIFT, false);
    r.scale.grab_focus();
    input.key(RIGHT, true);
    input.key(RIGHT, false);
    let req = d
        .take_requests()
        .pop()
        .expect("fresh arrow after modifier released elsewhere");
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (3, 0));
    window.close();
}
#[gtk::test]
fn actual_held_key_cannot_migrate_to_replacement_row() {
    let (s, d, v, mut gens, key) = setup();
    let window = gtk::Window::builder()
        .child(v.widget())
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle(120);
    let r = row(&v, &key);
    let input = XInput::new(&window);
    r.scale.grab_focus();
    input.key(RIGHT, true);
    let req = d.take_requests().pop().unwrap();
    *gens.get_mut(&20).unwrap() += 100;
    let next = d.publish((*s.graph().unwrap().value).clone(), gens.clone());
    s.poll();
    v.refresh();
    req.reporter
        .set(pecan_core::native_command::NativeProgress::Observed {
            stamp: next.stamp,
            counts: Default::default(),
        });
    s.poll();
    v.refresh();
    let fresh = row(&v, &key);
    fresh.scale.grab_focus();
    input.key(RIGHT, true);
    assert!(d.take_requests().is_empty(), "held repeat may not retarget");
    input.key(RIGHT, false);
    input.key(RIGHT, true);
    input.key(RIGHT, false);
    let req = d.take_requests().pop().expect("fresh input on replacement");
    apply(&s, &d, &v, &req, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    window.close();
}
#[gtk::test]
fn actual_double_click_resets_zero_db_once() {
    let (s, d, v, gens, key) = setup();
    let window = gtk::Window::builder()
        .child(v.widget())
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle(120);
    let r = row(&v, &key);
    let input = XInput::new(&window);
    input.point(&r.scale, &window);
    input.button(true);
    input.button(false);
    assert!(
        d.take_requests().is_empty(),
        "click without motion must not flatten gains"
    );
    v.refresh();
    input.button(true);
    input.button(false);
    let requests = d.take_requests();
    assert_eq!(
        requests.len(),
        1,
        "double-click emits one reset, not absolute scale jumps"
    );
    let req = &requests[0];
    let [pecan_core::graph::Opdracht::ZetVolume { volume, .. }] = &req.intent.commands[..] else {
        panic!()
    };
    assert_eq!(&volume[..2], &[1., 1.]);
    apply(&s, &d, &v, req, &gens);
    assert_eq!(s.history_lengths(), (1, 0));
    window.close();
}
