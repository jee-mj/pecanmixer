use super::frames::{Binding, MeterFeed};
use crate::widgets::accessibility::set_name;
use gtk::{glib, prelude::*};
use pecan_core::domain::levels::Meter;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterMode {
    Channel,
    Mini,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeterReading {
    /// Position on the existing -60..0 dBFS meter scale, not linear amplitude.
    pub level: f64,
    pub held: f64,
    pub clipped: bool,
    pub bound: bool,
    pub has_frame: bool,
}
#[derive(Clone)]
pub struct MeterWidget(pub(super) Rc<View>);
pub(super) struct View {
    feed: MeterFeed,
    root: gtk::Box,
    title: gtk::Label,
    bar: gtk::LevelBar,
    held: gtk::DrawingArea,
    clip: gtk::Label,
    readout: gtk::Label,
    mode: MeterMode,
    name: RefCell<String>,
    binding: RefCell<Option<Binding>>,
    meter: RefCell<Meter>,
    last_tick: Cell<Option<f64>>,
    reading: Cell<MeterReading>,
    rendered: Cell<MeterReading>,
    active: Cell<bool>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    #[cfg(test)]
    pub renders: Cell<usize>,
    #[cfg(test)]
    pub advances: Cell<usize>,
}
impl MeterWidget {
    /// Create an unbound widget. Channel indices/groups come from the pure
    /// strip/module model; this widget does not infer stereo or routing policy.
    pub fn new(feed: &MeterFeed, name: &str, mode: MeterMode) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = gtk::Label::builder()
            .label(name)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(12)
            .build();
        let bar = gtk::LevelBar::builder()
            .min_value(0.0)
            .max_value(1.0)
            .orientation(gtk::Orientation::Vertical)
            // GTK's vertical default fills from the top. dBFS grows upward.
            .inverted(true)
            .mode(gtk::LevelBarMode::Continuous)
            .build();
        bar.add_offset_value("low", 0.7);
        bar.add_offset_value("high", 0.9);
        bar.add_offset_value("full", 1.0);
        bar.set_vexpand(mode == MeterMode::Channel);
        bar.set_size_request(
            if mode == MeterMode::Mini { 6 } else { 14 },
            if mode == MeterMode::Mini { 32 } else { 120 },
        );
        let held = gtk::DrawingArea::builder()
            .can_target(false)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&bar));
        overlay.add_overlay(&held);
        let clip = gtk::Label::new(Some("Clip"));
        clip.add_css_class("error");
        clip.set_visible(false);
        let readout = gtk::Label::new(Some("Unavailable"));
        title.set_visible(mode == MeterMode::Channel);
        readout.set_visible(mode == MeterMode::Channel);
        root.append(&title);
        root.append(&clip);
        root.append(&overlay);
        root.append(&readout);
        let view = Rc::new(View {
            feed: feed.clone(),
            root,
            title,
            bar,
            held,
            clip,
            readout,
            mode,
            name: RefCell::new(name.into()),
            binding: RefCell::new(None),
            meter: RefCell::default(),
            last_tick: Cell::new(None),
            reading: Cell::default(),
            rendered: Cell::default(),
            active: Cell::new(true),
            tick: RefCell::new(None),
            #[cfg(test)]
            renders: Cell::new(0),
            #[cfg(test)]
            advances: Cell::new(0),
        });
        let weak = Rc::downgrade(&view);
        view.held
            .set_draw_func(move |area, context, width, height| {
                let Some(view) = weak.upgrade() else { return };
                let reading = view.reading.get();
                if view.mode == MeterMode::Mini || reading.held <= 0.02 || !reading.bound {
                    return;
                }
                #[allow(deprecated)]
                let color = area.style_context().color();
                context.set_source_rgba(
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    1.0,
                );
                context.set_line_width(1.5);
                let y = (f64::from(height) * (1.0 - reading.held))
                    .clamp(0.75, (f64::from(height) - 0.75).max(0.75));
                context.move_to(0.0, y);
                context.line_to(f64::from(width), y);
                let _ = context.stroke();
            });
        let weak = Rc::downgrade(&view);
        view.root.connect_map(move |_| {
            if let Some(view) = weak.upgrade() {
                view.start_ticks();
            }
        });
        let weak = Rc::downgrade(&view);
        view.root.connect_unmap(move |_| {
            if let Some(view) = weak.upgrade() {
                view.stop_ticks();
                view.reset();
            }
        });
        feed.register(&view);
        view.set_name(name);
        view.render();
        Self(view)
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    /// Capture a source in the current graph. Replacement/removal/disconnect
    /// invalidates it; the owning shell explicitly binds its next domain model.
    /// Missing serials and empty channel selections remain unavailable.
    pub fn bind(&self, node: u32, channels: &[usize]) -> bool {
        let binding = self.0.feed.0.frames.borrow().bind(node, channels);
        let changed = *self.0.binding.borrow() != binding;
        *self.0.binding.borrow_mut() = binding;
        if changed {
            self.0.stop_ticks();
            self.0.reset();
        }
        self.0.start_ticks();
        self.0.render();
        self.0.binding.borrow().is_some()
    }
    pub fn set_label(&self, name: &str) {
        self.0.set_name(name);
        self.0.render();
    }
    /// Stop tick registration while the owning shell hides/off-screens a meter.
    /// GTK unmapping also stops it. Resuming starts from the latest frame;
    /// hidden time never produces an animation catch-up loop.
    pub fn set_active(&self, active: bool) {
        if self.0.active.replace(active) == active {
            return;
        }
        self.0.stop_ticks();
        self.0.reset();
        if active {
            self.0.start_ticks();
        }
    }
    pub fn reading(&self) -> MeterReading {
        self.0.reading.get()
    }
}
impl View {
    pub fn graph_changed(self: &Rc<Self>) {
        let valid = self
            .binding
            .borrow()
            .as_ref()
            .is_some_and(|binding| self.feed.0.frames.borrow().current(binding));
        if !valid {
            *self.binding.borrow_mut() = None;
            self.stop_ticks();
            self.reset();
        } else {
            // A graph revision invalidates the previous samples, while known
            // source identity retains its decay/hold state until a fresh frame.
            let mut reading = self.reading.get();
            reading.has_frame = false;
            self.reading.set(reading);
            self.render();
        }
    }
    fn set_name(&self, name: &str) {
        *self.name.borrow_mut() = name.into();
        self.title.set_text(name);
        set_name(
            &self.bar,
            &format!("{name} · audio level, minus 60 to zero dBFS"),
        );
        set_name(&self.clip, &format!("{name} · clipping"));
        self.root.set_tooltip_text(Some(name));
    }
    fn start_ticks(self: &Rc<Self>) {
        if !self.active.get()
            || !self.root.is_mapped()
            || self.binding.borrow().is_none()
            || self.tick.borrow().is_some()
        {
            return;
        }
        let weak = Rc::downgrade(self);
        let id = self.root.add_tick_callback(move |_, clock| {
            let Some(view) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            view.advance_at(clock.frame_time() as f64 / 1000.0);
            glib::ControlFlow::Continue
        });
        *self.tick.borrow_mut() = Some(id);
    }
    fn stop_ticks(&self) {
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        self.last_tick.set(None);
    }
    pub(super) fn reset(&self) {
        *self.meter.borrow_mut() = Meter::default();
        self.last_tick.set(None);
        self.reading.set(MeterReading {
            bound: self.binding.borrow().is_some(),
            ..Default::default()
        });
        self.render();
    }
    pub(super) fn advance_at(&self, now_ms: f64) {
        if !self.active.get() || !self.root.is_mapped() || !now_ms.is_finite() {
            return;
        }
        let Some(binding) = self.binding.borrow().clone() else {
            return;
        };
        if self
            .last_tick
            .get()
            .is_some_and(|previous| now_ms < previous)
        {
            return;
        }
        let elapsed = self
            .last_tick
            .replace(Some(now_ms))
            .map_or(0.0, |previous| (now_ms - previous).max(0.0));
        let samples = self.feed.0.frames.borrow().samples(&binding);
        let channels = samples.as_ref().and_then(|frame| frame.get(&binding.node));
        let mut meter = self.meter.borrow_mut();
        meter.tick(
            channels.map_or(&[], Vec::as_slice),
            &binding.channels,
            now_ms,
            elapsed,
        );
        let next = MeterReading {
            level: meter.level,
            held: meter.held,
            clipped: meter.clipped,
            bound: true,
            has_frame: channels.is_some(),
        };
        drop(meter);
        #[cfg(test)]
        self.advances.set(self.advances.get() + 1);
        self.reading.set(next);
        let previous = self.rendered.get();
        if (previous.level - next.level).abs() > 0.002
            || (previous.held - next.held).abs() > 0.002
            || previous.clipped != next.clipped
            || previous.has_frame != next.has_frame
            || previous.bound != next.bound
        {
            self.render();
        }
    }
    fn render(&self) {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        let reading = self.reading.get();
        self.rendered.set(reading);
        self.bar.set_value(reading.level);
        self.bar.set_sensitive(reading.bound);
        self.clip
            .set_visible(self.mode == MeterMode::Channel && reading.clipped);
        self.readout.set_text(&if !reading.bound {
            "Unavailable".into()
        } else if !reading.has_frame {
            "No current frame".into()
        } else {
            format!("{} dBFS", db_label(reading.level))
        });
        let name = self.name.borrow().clone();
        self.root.set_tooltip_text(Some(&format!(
            "{} · {} · peak {} dBFS{}",
            name,
            if reading.has_frame {
                format!("{} dBFS", db_label(reading.level))
            } else {
                "No current frame".into()
            },
            db_label(reading.held),
            if reading.clipped { " · clipping" } else { "" }
        )));
        self.held.queue_draw();
    }
}
fn db_label(position: f64) -> String {
    if position <= 0.0 {
        "≤ −60".into()
    } else {
        format!("{:.1}", position * 60.0 - 60.0)
    }
}

impl Drop for View {
    fn drop(&mut self) {
        if let Some(tick) = self.tick.get_mut().take() {
            tick.remove();
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
