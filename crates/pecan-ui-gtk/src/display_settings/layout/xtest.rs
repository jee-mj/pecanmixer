//! Actual GTK event delivery on an explicitly marked synthetic Xvfb connection.
use super::*;
use crate::display_settings::DisplayEdit;
use pecan_core::settings::SettingsV1;
use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    time::{Duration, Instant},
};
#[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XOpenDisplay(n: *const c_char) -> *mut c_void;
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
    fn XSync(d: *mut c_void, discard: c_int) -> c_int;
}
#[link(name = "libXtst.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XTestFakeMotionEvent(d: *mut c_void, s: c_int, x: c_int, y: c_int, delay: c_ulong) -> c_int;
    fn XTestFakeButtonEvent(d: *mut c_void, b: c_uint, down: c_int, delay: c_ulong) -> c_int;
}
unsafe extern "C" {
    fn gdk_x11_surface_get_xid(s: *mut c_void) -> c_ulong;
}
struct Mouse {
    d: *mut c_void,
    w: c_ulong,
}
impl Mouse {
    fn new(window: &gtk::Window) -> Self {
        assert_eq!(
            std::env::var("DISPLAY").ok(),
            std::env::var("PECAN_XTEST_FIXTURE_DISPLAY").ok()
        );
        assert!(std::env::var_os("PECAN_XTEST_FIXTURE_DISPLAY").is_some());
        assert!(WidgetExt::display(window).type_().name().contains("X11"));
        unsafe {
            let d = XOpenDisplay(std::ptr::null());
            assert!(!d.is_null());
            Self {
                d,
                w: gdk_x11_surface_get_xid(window.surface().unwrap().as_ptr().cast()),
            }
        }
    }
    fn point(&self, x: i32, y: i32) {
        unsafe {
            let (mut rx, mut ry, mut c) = (0, 0, 0);
            assert_ne!(
                XTranslateCoordinates(
                    self.d,
                    self.w,
                    XDefaultRootWindow(self.d),
                    x,
                    y,
                    &mut rx,
                    &mut ry,
                    &mut c
                ),
                0
            );
            XTestFakeMotionEvent(self.d, -1, rx, ry, 0);
            XSync(self.d, 0);
        }
        settle(30);
    }
    fn button(&self, down: bool) {
        unsafe {
            XTestFakeButtonEvent(self.d, 1, i32::from(down), 0);
            XSync(self.d, 0);
        }
        settle(30);
    }
}
impl Drop for Mouse {
    fn drop(&mut self) {
        unsafe {
            XTestFakeButtonEvent(self.d, 1, 0, 0);
            XSync(self.d, 0);
            XCloseDisplay(self.d);
        }
    }
}
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
fn real_handle_drag_saves_only_user_geometry_and_double_click_resets_pair() {
    let view = PreferenceLayout::new();
    for (i, lane) in view.0.lanes.iter().enumerate() {
        lane.append(&gtk::Label::new(Some(&format!("Synthetic lane {i}"))));
    }
    view.changed();
    let document = Rc::new(RefCell::new(SettingsV1::default()));
    let edits = Rc::new(RefCell::new(Vec::new()));
    let weak = Rc::downgrade(&view.0);
    let doc = document.clone();
    let output = edits.clone();
    view.edit_handler(Rc::new(move |edit: DisplayEdit| {
        let next = edit.apply(&doc.borrow()).unwrap();
        *doc.borrow_mut() = next.clone();
        if let Some(s) = weak.upgrade() {
            *s.preferences.borrow_mut() = DisplayPreferences::from(&next);
            s.dirty.set(true);
        }
        output.borrow_mut().push(edit);
    }));
    view.set_editable(true);
    let window = gtk::Window::builder()
        .child(view.widget())
        .default_width(900)
        .default_height(900)
        .build();
    window.present();
    settle(120);
    let input = Mouse::new(&window);
    let a = view
        .0
        .outer
        .start_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let b = view
        .0
        .outer
        .end_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let y = ((a.y() + a.height() + b.y()) / 2.) as i32;
    input.point(20, y);
    input.button(true);
    input.point(20, y + 30);
    input.button(false);
    assert!(!edits.borrow().is_empty());
    assert!(
        document.borrow().preferences().rows.apps > document.borrow().preferences().rows.hardware
    );
    assert_eq!(document.borrow().preferences().rows.destinations, 1.);
    settle(120);
    let a = view
        .0
        .outer
        .start_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let b = view
        .0
        .outer
        .end_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let y = ((a.y() + a.height() + b.y()) / 2.) as i32;
    let count = edits.borrow().len();
    input.point(20, y);
    input.button(true);
    input.button(false);
    input.point(20, 50);
    input.button(true);
    input.button(false);
    input.point(20, y);
    input.button(true);
    input.button(false);
    assert_eq!(
        edits.borrow().len(),
        count,
        "intervening content click breaks double-click sequence"
    );
    input.point(20, y);
    input.button(true);
    input.button(false);
    input.button(true);
    input.button(false);
    assert_eq!(
        document.borrow().preferences().rows.apps,
        document.borrow().preferences().rows.hardware
    );
    settle(100);
    let a = view
        .0
        .outputs
        .start_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let b = view
        .0
        .outputs
        .end_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let x = ((a.x() + a.width() + b.x()) / 2.) as i32;
    let sy = (a.y() + 20.) as i32;
    input.point(x, sy);
    input.button(true);
    input.point(x - 300, sy);
    input.button(false);
    assert_eq!(
        document.borrow().preferences().destination_width_px,
        Some(120.)
    );
    settle(100);
    let a = view
        .0
        .outputs
        .start_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let b = view
        .0
        .outputs
        .end_child()
        .unwrap()
        .compute_bounds(&window)
        .unwrap();
    let x = ((a.x() + a.width() + b.x()) / 2.) as i32;
    let sy = (a.y() + 20.) as i32;
    input.point(x, sy);
    input.button(true);
    input.button(false);
    input.button(true);
    input.button(false);
    assert_eq!(document.borrow().preferences().destination_width_px, None);
    let n = edits.borrow().len();
    input.point(20, 50);
    input.button(true);
    input.point(20, 70);
    input.button(false);
    assert_eq!(
        edits.borrow().len(),
        n,
        "child content is not a resize handle"
    );
    view.widget().set_visible(false);
    settle(60);
    input.point(20, y);
    input.button(true);
    input.point(20, y + 30);
    input.button(false);
    assert_eq!(
        edits.borrow().len(),
        n,
        "unmapped layout cannot intercept another page"
    );
    window.close();
}
