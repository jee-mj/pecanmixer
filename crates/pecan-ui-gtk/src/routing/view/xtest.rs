//! Native event delivery only on an explicitly marked synthetic Xvfb display.
use super::tests::{button, view};
use super::*;
use crate::routing::tests::*;
use gtk::{glib, graphene};
use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    time::{Duration, Instant},
};
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
}
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            for key in [SPACE, ESCAPE, RETURN, TAB] {
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

#[gtk::test]
fn xtest_held_route_release_replacement_and_return_failure_repeat_require_fresh_activation() {
    let (v, g, _) = view();
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    v.bind_intents(move |e| {
        seen.borrow_mut().push(e);
        Ok(())
    });
    let window = gtk::Window::builder()
        .child(v.widget())
        .default_width(1000)
        .default_height(800)
        .build();
    window.present();
    let input = Input::new(&window);
    let (s, d) = first(&v.0.model.borrow());
    let key = Key::Route(s, d);
    assert!(button(&v, &key).grab_focus());
    settle(40);
    input.key(SPACE, true);
    assert!(events.borrow().is_empty());
    let mut value = (*g.value).clone();
    value.kabels.retain(|l| ![1000, 1001].contains(&l.id));
    let next = Stamped {
        stamp: Stamp {
            epoch: 3,
            revision: 2,
        },
        value: Arc::new(value),
    };
    let owner = owner(&next);
    v.observe(capture(&next, &owner)).unwrap();
    input.point(&button(&v, &Key::Reset), &window);
    input.key(SPACE, false);
    settle(50);
    assert!(
        events.borrow().is_empty(),
        "held Space release cannot take the replacement off→on action"
    );
    assert!(button(&v, &key).grab_focus());
    input.key(RETURN, true);
    settle(300); // GTK's native button activation animation is asynchronous.
    assert_eq!(events.borrow().len(), 1);
    let i = intent(events.borrow_mut().pop().unwrap());
    let pecan_core::domain::history::Input::Execute { action, .. } = &i.history else {
        panic!()
    };
    assert!(matches!(action, Action::Route { enabled: true, .. }));
    v.report(
        &i.request,
        report(
            &i,
            NativeProgress::Rejected {
                reason: pecan_core::native_command::NativeFailure::ObservationTimedOut,
                counts: pecan_core::native_command::Counts::default(),
            },
        ),
    );
    settle(750);
    assert!(
        events.borrow().is_empty(),
        "held Return autorepeat cannot create a retry after rejection"
    );
    input.key(RETURN, false);
    settle(50);
    assert!(button(&v, &key).grab_focus());
    input.key(RETURN, true);
    input.key(RETURN, false);
    settle(300);
    assert_eq!(events.borrow().len(), 1, "fresh activation recovers");
    window.close();
    settle(40);
}

#[gtk::test]
fn xtest_reset_escape_tab_and_off_widget_release_preserve_confirmation_boundary() {
    let (v, g, owner) = view();
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    v.bind_intents(move |e| {
        seen.borrow_mut().push(e);
        Ok(())
    });
    let window = gtk::Window::builder()
        .child(v.widget())
        .default_width(1000)
        .default_height(850)
        .build();
    window.present();
    let input = Input::new(&window);
    button(&v, &Key::Reset).emit_clicked();
    settle(40);
    assert!(button(&v, &Key::Cancel).has_focus());
    input.key(TAB, true);
    input.key(TAB, false);
    assert!(!v.0.input.blocked());
    assert!(events.borrow().is_empty());
    input.key(ESCAPE, true);
    input.key(ESCAPE, false);
    assert!(!v.0.confirmation.is_visible());
    assert!(events.borrow().is_empty());
    button(&v, &Key::Reset).emit_clicked();
    settle(40);
    input.point(&button(&v, &Key::Confirm), &window);
    input.button(true);
    v.observe(capture(&g, &owner)).unwrap();
    input.point(&button(&v, &Key::Reset), &window);
    input.button(false);
    assert!(events.borrow().is_empty());
    button(&v, &Key::Reset).emit_clicked();
    settle(40);
    input.point(&button(&v, &Key::Confirm), &window);
    input.button(true);
    input.button(false);
    assert_eq!(events.borrow().len(), 1);
    let i = intent(events.borrow_mut().pop().unwrap());
    assert!(matches!(
        i.history,
        pecan_core::domain::history::Input::Execute {
            action: Action::BackToAuto,
            ..
        }
    ));
    assert!(v.pending().is_some());
    window.close();
    settle(40);
}
