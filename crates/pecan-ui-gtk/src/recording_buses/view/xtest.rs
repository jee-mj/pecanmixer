//! Native event delivery only on an explicitly marked synthetic Xvfb display.
use super::tests::{button, connected, frame};
use super::*;
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
    fn point(&self, button: &impl IsA<gtk::Widget>, window: &gtk::Window) {
        self.point_at(
            button,
            window,
            button.width() as f32 / 2.,
            button.height() as f32 / 2.,
        );
    }
    fn point_at(&self, button: &impl IsA<gtk::Widget>, window: &gtk::Window, x: f32, y: f32) {
        let p = button
            .compute_point(window, &graphene::Point::new(x, y))
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
fn xtest_held_split_pointer_cannot_activate_a_replacement_join() {
    let (view, events) = connected();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(800)
        .build();
    window.present();
    let input = Input::new(&window);
    let old = button(&view, "Split stereo");
    input.point(&old, &window);
    input.button(true);
    assert!(events.borrow().is_empty());
    let mut next = frame();
    next.configuration = Some(
        next.configuration
            .as_ref()
            .unwrap()
            .change_coupling(0)
            .unwrap(),
    );
    view.observe(next).unwrap();
    settle(40);
    input.button(false);
    assert!(events.borrow().is_empty());
    let join = button(&view, "Join mono pair");
    assert!(join.is_sensitive());
    input.point(&join, &window);
    input.button(true);
    input.button(false);
    assert_eq!(events.borrow().len(), 1);
    assert!(
        matches!(&events.borrow()[0],Event::Configuration(i) if i.action==Action::Join{bus:"rec-1".into()})
    );
    window.close();
}
#[gtk::test]
fn xtest_held_space_repeats_after_publication_need_release_before_fresh_config() {
    let (view, events) = connected();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(800)
        .build();
    window.present();
    let input = Input::new(&window);
    let old = button(&view, "Add stereo bus");
    assert!(old.grab_focus());
    settle(35);
    input.key(SPACE, true);
    view.observe(frame()).unwrap();
    settle(35);
    input.key(SPACE, true);
    input.key(SPACE, false);
    old.emit_clicked();
    assert!(events.borrow().is_empty());
    let fresh = button(&view, "Add stereo bus");
    assert!(fresh.is_sensitive());
    assert!(fresh.grab_focus());
    settle(35);
    input.key(SPACE, true);
    input.key(SPACE, false);
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(&events.borrow()[0],Event::Configuration(i) if i.action==Action::AddStereo));
    window.close();
}
#[gtk::test]
fn xtest_focus_escape_cancel_and_fresh_action_share_one_callback() {
    let (view, events) = connected();
    let outside = gtk::Entry::new();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.append(view.widget());
    root.append(&outside);
    let window = gtk::Window::builder()
        .child(&root)
        .default_width(1100)
        .default_height(800)
        .build();
    window.present();
    let input = Input::new(&window);
    let first = button(&view, "Add stereo bus");
    assert!(first.grab_focus());
    settle(35);
    input.key(SPACE, true);
    assert!(outside.grab_focus());
    settle(35);
    input.key(SPACE, false);
    assert!(events.borrow().is_empty());
    let fresh = button(&view, "Add stereo bus");
    input.point(&fresh, &window);
    input.button(true);
    input.key(ESCAPE, true);
    input.key(ESCAPE, false);
    input.button(false);
    assert!(events.borrow().is_empty());
    let fresh = button(&view, "Add stereo bus");
    assert!(fresh.grab_focus());
    settle(35);
    input.key(RETURN, true);
    input.key(RETURN, false);
    settle(300);
    assert_eq!(events.borrow().len(), 1);
    window.close();
}

#[gtk::test]
fn xtest_rec_levels_relative_drag_reentrant_refresh_double_reset_and_held_mute_require_fresh_input()
{
    use crate::recording_buses::levels as level;
    let view = super::tests::View::new(Budget::default());
    let weak = Rc::downgrade(&view.0);
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let next = Cell::new(200);
    let snapshot = || {
        let mut s = frame();
        s.capabilities.idle_levels = Support::Available;
        s
    };
    view.bind_levels(move |event| {
        let begin = matches!(event, level::Event::Begin { .. });
        let tracked = begin || matches!(event, level::Event::Set(_));
        seen.borrow_mut().push(event);
        if tracked {
            next.set(next.get() + 1);
            let request = next.get();
            if begin {
                let mut s = frame();
                s.capabilities.idle_levels = Support::Available;
                s.operation = Some(Operation {
                    request,
                    progress: Progress::Queued,
                });
                BusControls(weak.upgrade().unwrap()).observe(s).unwrap();
            }
            Ok(level::Reply::Tracked(request))
        } else {
            Ok(level::Reply::Accepted)
        }
    });
    view.observe(snapshot()).unwrap();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(900)
        .build();
    window.present();
    let input = Input::new(&window);
    let scale = super::tests::descendants(view.widget())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Scale>().ok())
        .unwrap();
    assert!(scale.is_mapped() && scale.is_sensitive());
    let x = scale.width() as f32 / 2.;
    let y = scale.height() as f32 / 2.;
    input.point_at(&scale, &window, x, y);
    input.button(true);
    assert!(events.borrow().is_empty());
    input.point_at(&scale, &window, x, y - 20.);
    input.button(false);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, level::Event::Begin { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, level::Event::End { request: 201, .. }))
            .count(),
        1
    );
    let db = events
        .borrow()
        .iter()
        .rev()
        .find_map(|e| {
            if let level::Event::End { db, .. } = e {
                Some(*db)
            } else {
                None
            }
        })
        .unwrap();
    let mut applied = snapshot();
    applied.configuration = Some(
        applied
            .configuration
            .unwrap()
            .with_db("rec-1-2", db)
            .unwrap(),
    );
    applied.operation = Some(Operation {
        request: 201,
        progress: Progress::AppliedLocally,
    });
    view.observe(applied).unwrap();
    assert!(view.awaited_request().is_none());
    let before = events.borrow().len();
    input.point_at(&scale, &window, x, y - 20.);
    input.button(true);
    input.button(false);
    input.button(true);
    input.button(false);
    assert_eq!(
        events.borrow().len(),
        before + 3,
        "double-click is one reset proposal group, with no empty drag admissions"
    );
    assert!(matches!(
        &events.borrow()[before],
        level::Event::Begin {
            kind: crate::controls::fader::Kind::Reset,
            ..
        }
    ));
    assert!(matches!(
        events.borrow().last(),
        Some(level::Event::End {
            request: 202,
            db: 0.,
            ..
        })
    ));
    let mut applied = snapshot();
    applied.operation = Some(Operation {
        request: 202,
        progress: Progress::AppliedLocally,
    });
    view.observe(applied).unwrap();
    let old = button(&view, "Mute bus");
    assert!(old.grab_focus());
    settle(35);
    input.key(SPACE, true);
    let mut changed = snapshot();
    changed.configuration = Some(
        changed
            .configuration
            .unwrap()
            .with_mute("rec-1-2", true)
            .unwrap(),
    );
    view.observe(changed).unwrap();
    input.key(SPACE, false);
    old.emit_clicked();
    assert_eq!(events.borrow().len(), before + 3);
    let fresh = button(&view, "Unmute bus");
    assert!(fresh.is_sensitive());
    assert!(fresh.grab_focus());
    settle(35);
    input.key(SPACE, true);
    input.key(SPACE, false);
    assert_eq!(events.borrow().len(), before + 4);
    assert!(
        matches!(events.borrow().last(),Some(level::Event::Set(i)) if i.change==level::Change::Mute(false))
    );
    window.close();
}

#[gtk::test]
fn xtest_rec_level_overlap_wheel_while_pointer_is_held_keeps_an_owner_finalizer() {
    use crate::recording_buses::levels as level;
    let view = super::tests::View::new(Budget::default());
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let next = Cell::new(300);
    view.bind_levels(move |event| {
        let tracked = matches!(event, level::Event::Begin { .. } | level::Event::Set(_));
        seen.borrow_mut().push(event);
        if tracked {
            next.set(next.get() + 1);
            Ok(level::Reply::Tracked(next.get()))
        } else {
            Ok(level::Reply::Accepted)
        }
    });
    let mut initial = frame();
    initial.capabilities.idle_levels = Support::Available;
    view.observe(initial).unwrap();
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(1100)
        .default_height(900)
        .build();
    window.present();
    let input = Input::new(&window);
    let scale = super::tests::descendants(view.widget())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Scale>().ok())
        .unwrap();
    assert!(scale.is_mapped() && scale.is_sensitive());
    let x = scale.width() as f32 / 2.;
    let y = scale.height() as f32 / 2.;
    input.point_at(&scale, &window, x, y);
    input.button(true);
    input.point_at(&scale, &window, x, y - 15.);
    assert!(matches!(
        events.borrow().first(),
        Some(level::Event::Begin {
            kind: crate::controls::fader::Kind::Pointer,
            ..
        })
    ));
    // SAFETY: Input::new verified this explicitly marked synthetic Xvfb display.
    // Scroll uses buttons 4/5; the primary button remains held until below.
    unsafe {
        for down in [1, 0] {
            assert_ne!(XTestFakeButtonEvent(input.d, 4, down, 0), 0);
        }
        XSync(input.d, 0);
    }
    settle(35);
    input.button(false);
    view.tick_levels(Duration::from_millis(600));
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, level::Event::Begin { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|e| matches!(
                e,
                level::Event::End { request: 301, .. } | level::Event::Abandon { request: 301 }
            ))
            .count(),
        1,
        "native pointer/wheel overlap lost its owner finalizer"
    );
    assert_eq!(view.awaited_request(), Some(301));
    let mut terminal = frame();
    terminal.capabilities.idle_levels = Support::Available;
    terminal.operation = Some(Operation {
        request: 301,
        progress: Progress::Rejected("synthetic overlap closed".into()),
    });
    view.observe(terminal).unwrap();
    assert!(view.awaited_request().is_none());
    window.close();
}
