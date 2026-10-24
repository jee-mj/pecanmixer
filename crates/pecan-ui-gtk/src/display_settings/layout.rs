use super::DisplayPreferences;
use crate::widgets::accessibility::set_name;
use gtk::{glib, prelude::*};
use pecan_core::domain::mixer::{Row, Strip};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct State {
    root: gtk::Box,
    outer: gtk::Paned,
    inner: gtk::Paned,
    outputs: gtk::Paned,
    sections: [gtk::Box; 3],
    lanes: [gtk::Box; 4],
    preferences: RefCell<DisplayPreferences>,
    allocation: Cell<(i32, i32, i32, i32)>,
    dirty: Cell<bool>,
}
/// One-way preference application. This widget neither edits/saves preferences
/// nor owns audio/history state. User resize gestures are a separate parity task.
#[derive(Clone)]
pub struct PreferenceLayout(Rc<State>);
impl Default for PreferenceLayout {
    fn default() -> Self {
        Self::new()
    }
}
impl PreferenceLayout {
    pub fn new() -> Self {
        let lanes = std::array::from_fn(|_| gtk::Box::new(gtk::Orientation::Vertical, 8));
        let outputs = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&scroll(&lanes[2], "Output destinations"))
            .end_child(&scroll(&lanes[3], "Rec bus presentation"))
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(true)
            .shrink_end_child(true)
            .build();
        let sections = [
            section("Apps", &scroll(&lanes[0], "Apps")),
            section("Hardware in", &scroll(&lanes[1], "Hardware in")),
            section("Destinations", &outputs),
        ];
        let inner = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&sections[1])
            .end_child(&sections[2])
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(true)
            .shrink_end_child(true)
            .build();
        let outer = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&sections[0])
            .end_child(&inner)
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(true)
            .shrink_end_child(true)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        outer.set_vexpand(true);
        root.append(&outer);
        let view = Self(Rc::new(State {
            root,
            outer,
            inner,
            outputs,
            sections,
            lanes,
            preferences: RefCell::new(DisplayPreferences::default()),
            allocation: Cell::new((-1, -1, -1, -1)),
            dirty: Cell::new(true),
        }));
        let weak = Rc::downgrade(&view.0);
        view.0.root.add_tick_callback(move |_, _| {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            state.allocate();
            glib::ControlFlow::Continue
        });
        view
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    pub fn set_preferences(&self, preferences: DisplayPreferences) {
        for (section, row) in
            self.0
                .sections
                .iter()
                .zip([Row::Apps, Row::Hardware, Row::Destinations])
        {
            section.set_visible(preferences.row_visible(row));
        }
        self.0.inner.set_visible(
            preferences.row_visible(Row::Hardware) || preferences.row_visible(Row::Destinations),
        );
        *self.0.preferences.borrow_mut() = preferences;
        self.0.dirty.set(true);
    }
    pub(crate) fn lane_index(strip: &Strip) -> usize {
        match strip.row {
            Row::Apps => 0,
            Row::Hardware => 1,
            Row::Destinations => {
                if strip.bus.is_some() {
                    3
                } else {
                    2
                }
            }
        }
    }
    pub(crate) fn lane(&self, index: usize) -> &gtk::Box {
        &self.0.lanes[index]
    }
    pub(crate) fn changed(&self) {
        let rec_visible = self.0.lanes[3].first_child().is_some();
        if let Some(child) = self.0.outputs.end_child() {
            child.set_visible(rec_visible);
        }
        if let Some(child) = self.0.outputs.start_child() {
            child.set_visible(self.0.lanes[2].first_child().is_some() || !rec_visible);
        }
        self.0.dirty.set(true);
    }
}
impl State {
    fn allocate(&self) {
        let allocation = (
            self.outer.height(),
            self.inner.height(),
            self.outputs.width(),
            self.lanes[2].measure(gtk::Orientation::Horizontal, -1).1,
        );
        if self.allocation.replace(allocation) == allocation && !self.dirty.replace(false) {
            return;
        }
        let p = self.preferences.borrow();
        let f = p.fractions();
        self.outer
            .set_position((f[0] * f64::from(allocation.0.max(0))).round() as i32);
        let tail = f[1] + f[2];
        self.inner.set_position(if tail > 0.0 {
            (f[1] / tail * f64::from(allocation.1.max(0))).round() as i32
        } else {
            0
        });
        self.outputs
            .set_position(p.output_width(allocation.2, allocation.3));
    }
}
fn scroll(child: &gtk::Box, name: &str) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::builder()
        .child(child)
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    set_name(&scroll, name);
    scroll
}
fn section(title: &str, child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 4);
    section.append(&gtk::Label::builder().label(title).xalign(0.0).build());
    section.append(child);
    set_name(&section, title);
    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecan_core::settings::{HiddenKey, RowWeights, SettingsV1};
    #[gtk::test]
    fn saved_weights_visibility_and_width_are_applied_on_allocation() {
        let view = PreferenceLayout::new();
        for lane in &view.0.lanes {
            lane.append(&gtk::Label::new(Some("Synthetic strip")));
        }
        let mut settings = SettingsV1::default().into_preferences();
        settings.rows = RowWeights {
            apps: 2.0,
            hardware: 1.0,
            destinations: 1.0,
        };
        settings.destination_width_px = Some(230.0);
        view.set_preferences(DisplayPreferences::from(
            &SettingsV1::new(settings.clone()).unwrap(),
        ));
        let window = gtk::Window::builder()
            .default_width(900)
            .default_height(800)
            .child(view.widget())
            .build();
        window.present();
        settle();
        assert!((view.0.outer.position() - view.0.outer.height() / 2).abs() <= 2);
        assert!((view.0.inner.position() - view.0.inner.height() / 2).abs() <= 2);
        assert_eq!(view.0.outputs.position(), 230);
        settings.hidden.insert(HiddenKey::new("rij|apps").unwrap());
        settings.rows.hardware = 3.0;
        view.set_preferences(DisplayPreferences::from(
            &SettingsV1::new(settings).unwrap(),
        ));
        settle();
        assert!(!view.0.sections[0].is_visible());
        assert!(
            (view.0.inner.position() - (f64::from(view.0.inner.height()) * 0.75).round() as i32)
                .abs()
                <= 2
        );
        window.close();
    }
    fn settle() {
        let until = std::time::Instant::now() + std::time::Duration::from_millis(120);
        let context = glib::MainContext::default();
        while std::time::Instant::now() < until {
            while context.pending() && std::time::Instant::now() < until {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
