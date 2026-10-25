//! Opt-in X11 fixture input tests. Requires Xvfb, GDK_BACKEND=x11 and installed
//! X11/XTest runtime libraries. Never connects to an audio server or user desktop.
use super::*;
use crate::controls::tests::{app_binding, mutate, setup};
use gtk::glib;
use pecan_core::graph::Opdracht;
use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    time::{Duration, Instant},
};

#[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(display: *mut c_void) -> c_int;
    fn XKeysymToKeycode(display: *mut c_void, keysym: c_ulong) -> u8;
    fn XSetInputFocus(display: *mut c_void, focus: c_ulong, revert: c_int, time: c_ulong) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
}
#[link(name = "libXtst.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XTestFakeKeyEvent(
        display: *mut c_void,
        keycode: c_uint,
        pressed: c_int,
        delay: c_ulong,
    ) -> c_int;
}
unsafe extern "C" {
    fn gdk_x11_surface_get_xid(surface: *mut c_void) -> c_ulong;
}
struct Input(*mut c_void);
impl Input {
    fn new(window: &gtk::Window) -> Self {
        assert_eq!(
            std::env::var("PECAN_XTEST_FIXTURE_DISPLAY").ok(),
            std::env::var("DISPLAY").ok(),
            "mark the isolated fixture display explicitly"
        );
        assert!(
            std::env::var_os("PECAN_XTEST_FIXTURE_DISPLAY").is_some(),
            "XTest requires an explicitly marked fixture display"
        );
        assert!(
            gtk::prelude::WidgetExt::display(window)
                .type_()
                .name()
                .contains("X11"),
            "XTest requires GDK_BACKEND=x11"
        );
        settle();
        // SAFETY: this opt-in fixture only uses its current Xvfb display, its
        // own mapped X11 surface and a separate Xlib connection on this thread.
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            assert!(!display.is_null());
            let surface = window.surface().expect("mapped fixture window");
            let xid = gdk_x11_surface_get_xid(surface.as_ptr().cast());
            XSetInputFocus(display, xid, 2, 0);
            XSync(display, 0);
            let input = Self(display);
            settle();
            input
        }
    }
    fn key(&self, keysym: c_ulong, down: bool) {
        // SAFETY: this connection remains open; XKeysymToKeycode validates the
        // server mapping. Input is synthetic and confined to the fixture server.
        unsafe {
            let keycode = XKeysymToKeycode(self.0, keysym);
            assert_ne!(keycode, 0);
            assert_ne!(
                XTestFakeKeyEvent(self.0, u32::from(keycode), i32::from(down), 0),
                0
            );
            XSync(self.0, 0);
        }
        settle();
    }
    fn tap(&self, keysym: c_ulong) {
        self.key(keysym, true);
        self.key(keysym, false);
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            XCloseDisplay(self.0);
        }
    }
}
const TAB: c_ulong = 0xff09;
const RIGHT: c_ulong = 0xff53;
const SHIFT: c_ulong = 0xffe1;
fn settle() {
    let until = Instant::now() + Duration::from_millis(65);
    let context = glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() && Instant::now() < until {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[gtk::test]
fn xtest_tab_release_elsewhere_does_not_strand_a_fresh_arrow() {
    let (controller, log, _) = setup();
    let key = app_binding(&controller).key;
    let view = MixerControls::new(controller);
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(view.widget())
        .build();
    window.present();
    let input = Input::new(&window);
    let (scale, mute) = {
        let rows = view.0.rows.borrow();
        (rows[&key].scale.clone(), rows[&key].mute.clone())
    };
    assert!(scale.grab_focus());
    settle();
    input.key(TAB, true);
    assert!(mute.has_focus(), "Tab must move focus from fader to mute");
    input.key(TAB, false);
    assert!(scale.grab_focus());
    settle();
    input.tap(RIGHT);
    assert_eq!(
        log.borrow().calls.len(),
        1,
        "fresh arrow after off-widget Tab release"
    );
    window.close();
}
#[gtk::test]
fn xtest_modifier_release_elsewhere_and_refresh_allow_fresh_arrow() {
    let (controller, log, mut next) = setup();
    let key = app_binding(&controller).key;
    let view = MixerControls::new(controller);
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(view.widget())
        .build();
    window.present();
    let input = Input::new(&window);
    let (scale, mute) = {
        let rows = view.0.rows.borrow();
        (rows[&key].scale.clone(), rows[&key].mute.clone())
    };
    assert!(scale.grab_focus());
    settle();
    input.key(SHIFT, true);
    assert!(mute.grab_focus());
    settle();
    input.key(SHIFT, false);
    next.graph.stamp.revision += 1;
    view.observe(next);
    assert!(scale.grab_focus());
    settle();
    input.tap(RIGHT);
    assert_eq!(
        log.borrow().calls.len(),
        1,
        "released modifier cannot strand future input"
    );
    window.close();
}
#[gtk::test]
fn xtest_observed_held_arrow_releases_off_widget_before_fresh_arrow() {
    let (controller, log, mut next) = setup();
    let key = app_binding(&controller).key;
    let view = MixerControls::new(controller);
    let window = gtk::Window::builder()
        .default_width(900)
        .default_height(600)
        .child(view.widget())
        .build();
    window.present();
    let input = Input::new(&window);
    let scale = view.0.rows.borrow()[&key].scale.clone();
    assert!(scale.grab_focus());
    settle();
    input.key(RIGHT, true);
    assert_eq!(log.borrow().calls.len(), 1);
    let Opdracht::ZetVolume { knoop, volume } = log.borrow().calls[0].2.clone() else {
        panic!("volume")
    };
    next.graph.stamp.revision += 1;
    mutate(&mut next, |graph| {
        graph
            .knopen
            .iter_mut()
            .find(|n| n.id == knoop)
            .unwrap()
            .volume = Some(volume)
    });
    view.observed_result(&1, next.graph.stamp);
    view.observe(next);
    assert!(scale.grab_focus());
    settle();
    input.key(RIGHT, true);
    assert_eq!(
        log.borrow().calls.len(),
        1,
        "held-key repeat must remain cancelled after observation/refocus"
    );
    // All controls were disabled while pending. Release is delivered to the
    // window/current focus, not necessarily the original fader.
    input.key(RIGHT, false);
    assert!(scale.grab_focus());
    settle();
    input.tap(RIGHT);
    assert_eq!(
        log.borrow().calls.len(),
        2,
        "fresh arrow after observed held-arrow request"
    );
    window.close();
}

// Release on an unrelated widget, rather than returning focus to the fader
// before release. The original fader cannot observe these key-up events itself.
fn observed_arrow_release_elsewhere(release_before_ack: bool) {
    let (controller, log, mut next) = setup();
    let key = app_binding(&controller).key;
    let view = MixerControls::new(controller);
    let outside = gtk::Entry::new();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.append(view.widget());
    root.append(&outside);
    let window = gtk::Window::builder()
        .default_width(950)
        .default_height(700)
        .child(&root)
        .build();
    window.present();
    let input = Input::new(&window);
    let scale = view.0.rows.borrow()[&key].scale.clone();
    assert!(scale.grab_focus());
    settle();
    input.key(RIGHT, true);
    assert_eq!(log.borrow().calls.len(), 1);
    assert_eq!(view.phase(), Phase::WaitingForCompletion);
    if release_before_ack {
        assert!(outside.grab_focus());
        settle();
        input.key(RIGHT, false);
    }
    let Opdracht::ZetVolume { knoop, volume } = log.borrow().calls[0].2.clone() else {
        panic!("volume")
    };
    next.graph.stamp.revision += 1;
    mutate(&mut next, |graph| {
        graph
            .knopen
            .iter_mut()
            .find(|node| node.id == knoop)
            .unwrap()
            .volume = Some(volume)
    });
    assert!(view.observed_result(&1, next.graph.stamp));
    assert!(view.observe(next.clone()));
    assert_eq!(view.phase(), Phase::Ready);
    if !release_before_ack {
        assert!(scale.grab_focus());
        settle();
        input.key(RIGHT, true);
        assert_eq!(
            log.borrow().calls.len(),
            1,
            "held repeat across observation/refocus must remain cancelled"
        );
        assert!(outside.grab_focus());
        settle();
        input.key(RIGHT, false);
    }
    assert!(scale.grab_focus());
    settle();
    input.tap(RIGHT);
    assert_eq!(
        log.borrow().calls.len(),
        2,
        "off-widget release must permit a fresh press after observation"
    );
    assert_eq!(log.borrow().calls[1].1, next.graph.stamp);
    window.close();
}
#[gtk::test]
fn xtest_release_while_disabled_then_observed_ack_recovers() {
    observed_arrow_release_elsewhere(true);
}
#[gtk::test]
fn xtest_held_repeat_after_observed_ack_then_release_elsewhere_recovers() {
    observed_arrow_release_elsewhere(false);
}
