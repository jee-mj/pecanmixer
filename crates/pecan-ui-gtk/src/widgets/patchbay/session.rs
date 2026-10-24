//! Session-only canvas placement. Positions never become settings, audio or
//! history commands. A held move retains one target across viewport changes.
use super::{
    layout::{Layout, Point},
    pointer::Pointer,
};
use gtk::{glib, prelude::*};
use pecan_core::{domain::audio, graph::Momentopname};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type Role = (u32, bool);
struct Move {
    key: String,
    role: Role,
    generation: u64,
    origin: Point,
    point: Point,
    cancelled: bool,
}
struct State {
    base: RefCell<Layout>,
    geometry: Rc<RefCell<Layout>>,
    positions: RefCell<BTreeMap<String, Point>>,
    keys: RefCell<BTreeMap<Role, String>>,
    front: RefCell<Vec<String>>,
    frames: RefCell<BTreeMap<Role, glib::WeakRef<gtk::Widget>>>,
    fixed: glib::WeakRef<gtk::Fixed>,
    canvas: glib::WeakRef<gtk::Overlay>,
    scroll: glib::WeakRef<gtk::ScrolledWindow>,
    drawing: glib::WeakRef<gtk::DrawingArea>,
    pointer: Pointer,
    movement: RefCell<Option<Move>>,
    generation: Cell<u64>,
    dirty: Cell<bool>,
    viewport: Cell<i32>,
    retired: Cell<bool>,
}
#[derive(Clone)]
pub(super) struct Session(Rc<State>);
impl Session {
    pub fn new(
        geometry: &Rc<RefCell<Layout>>,
        pointer: &Pointer,
        canvas: &gtk::Overlay,
        fixed: &gtk::Fixed,
        scroll: &gtk::ScrolledWindow,
        drawing: &gtk::DrawingArea,
    ) -> Self {
        let session = Self(Rc::new(State {
            base: RefCell::new(Layout::default()),
            geometry: geometry.clone(),
            positions: RefCell::new(BTreeMap::new()),
            keys: RefCell::new(BTreeMap::new()),
            front: RefCell::new(vec![]),
            frames: RefCell::new(BTreeMap::new()),
            fixed: fixed.downgrade(),
            canvas: canvas.downgrade(),
            scroll: scroll.downgrade(),
            drawing: drawing.downgrade(),
            pointer: pointer.clone(),
            movement: RefCell::new(None),
            generation: Cell::new(0),
            dirty: Cell::new(false),
            viewport: Cell::new(0),
            retired: Cell::new(false),
        }));
        let weak = Rc::downgrade(&session.0);
        pointer.watch_session(move || {
            if let Some(state) = weak.upgrade() {
                Self(state).cancel();
            }
        });
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&session.0);
        drag.connect_drag_begin(move |gesture, x, y| {
            let Some(state) = weak.upgrade() else { return };
            if Self(state).begin(Point { x, y }) {
                if let Some(widget) = gesture.widget() {
                    widget.grab_focus();
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
            } else {
                gesture.set_state(gtk::EventSequenceState::Denied);
            }
        });
        let weak = Rc::downgrade(&session.0);
        drag.connect_drag_update(move |_, x, y| {
            if let Some(state) = weak.upgrade() {
                Self(state).motion(x, y);
            }
        });
        let weak = Rc::downgrade(&session.0);
        drag.connect_drag_end(move |_, x, y| {
            if let Some(state) = weak.upgrade() {
                Self(state).release(x, y);
            }
        });
        let weak = Rc::downgrade(&session.0);
        drag.connect_cancel(move |_, _| {
            if let Some(state) = weak.upgrade() {
                Self(state).end_cancel();
            }
        });
        canvas.add_controller(drag);
        let weak = Rc::downgrade(&session.0);
        canvas.add_tick_callback(move |_, _| {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            Self(state).flush();
            glib::ControlFlow::Continue
        });
        session
    }
    /// Full snapshots reuse core's existing module keys, including duplicate
    /// suffixes. Overview-only views retain their projection's display keys.
    pub fn graph_changed(&self, graph: Option<&Momentopname>) {
        self.0.pointer.cancel();
        let Some(generation) = self.0.generation.get().checked_add(1) else {
            self.retire();
            return;
        };
        self.0.generation.set(generation);
        let keys = graph
            .map(|graph| {
                audio::layout(graph)
                    .groups
                    .into_iter()
                    .flat_map(|group| group.modules)
                    .map(|module| ((module.node, module.side == audio::Side::Right), module.key))
                    .collect()
            })
            .unwrap_or_default();
        *self.0.keys.borrow_mut() = keys;
        for module in &mut self.0.base.borrow_mut().modules {
            if let Some(key) = self.0.keys.borrow().get(&(module.node, module.input)) {
                module.key = key.clone();
            }
        }
        self.0.dirty.set(true);
    }
    pub fn set_layout(&self, mut layout: Layout) -> Layout {
        for module in &mut layout.modules {
            if let Some(key) = self.0.keys.borrow().get(&(module.node, module.input)) {
                module.key = key.clone();
            }
        }
        self.0
            .front
            .borrow_mut()
            .retain(|key| layout.modules.iter().any(|m| &m.key == key));
        self.0.frames.borrow_mut().clear();
        *self.0.base.borrow_mut() = layout;
        self.0.dirty.set(true);
        self.flush();
        self.0.geometry.borrow().clone()
    }
    pub fn bind_reset(&self, button: &gtk::Button) {
        let weak = Rc::downgrade(&self.0);
        button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                Self(state).rearrange();
            }
        });
    }
    pub fn register(&self, node: u32, input: bool, frame: &gtk::Frame) {
        self.0.frames.borrow_mut().insert(
            (node, input),
            frame.clone().upcast::<gtk::Widget>().downgrade(),
        );
    }
    fn begin(&self, start: Point) -> bool {
        if self.0.retired.get()
            || self.0.movement.borrow().is_some()
            || !start.x.is_finite()
            || !start.y.is_finite()
        {
            return false;
        }
        let Some(module) = self.0.geometry.borrow().header_at(start).cloned() else {
            return false;
        };
        self.0.pointer.cancel();
        {
            let mut front = self.0.front.borrow_mut();
            front.retain(|key| key != &module.key);
            front.push(module.key.clone());
        }
        *self.0.movement.borrow_mut() = Some(Move {
            key: module.key,
            role: (module.node, module.input),
            generation: self.0.generation.get(),
            origin: module.origin,
            point: module.origin,
            cancelled: false,
        });
        self.0.dirty.set(true);
        self.flush();
        true
    }
    fn motion(&self, dx: f64, dy: f64) {
        if !dx.is_finite() || !dy.is_finite() {
            self.cancel();
            return;
        }
        let mut movement = self.0.movement.borrow_mut();
        let Some(active) = movement
            .as_mut()
            .filter(|m| !m.cancelled && m.generation == self.0.generation.get())
        else {
            return;
        };
        let point = Point {
            x: (active.origin.x + dx).max(0.),
            y: (active.origin.y + dy).max(0.),
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            active.cancelled = true;
            return;
        }
        if !self
            .0
            .base
            .borrow()
            .modules
            .iter()
            .any(|m| (m.node, m.input) == active.role && m.key == active.key)
        {
            active.cancelled = true;
            return;
        }
        active.point = point;
        if dx != 0. || dy != 0. {
            self.0
                .positions
                .borrow_mut()
                .insert(active.key.clone(), point);
        }
        self.0.dirty.set(true);
    }
    fn release(&self, dx: f64, dy: f64) {
        self.motion(dx, dy);
        self.0.movement.borrow_mut().take();
        self.0.dirty.set(true);
        self.flush();
    }
    fn cancel(&self) {
        if let Some(active) = self.0.movement.borrow_mut().as_mut() {
            active.cancelled = true;
        }
        self.0.dirty.set(true);
    }
    fn end_cancel(&self) {
        self.0.movement.borrow_mut().take();
        self.0.dirty.set(true);
        self.flush();
    }
    /// Clear every override, including currently absent modules. Keep cable
    /// selection and already-submitted audio requests; retire held gestures.
    pub fn rearrange(&self) {
        self.0.pointer.cancel();
        self.0.positions.borrow_mut().clear();
        self.0.dirty.set(true);
        self.flush();
    }
    pub fn retire(&self) {
        self.0.retired.set(true);
        self.0.movement.borrow_mut().take();
    }
    pub fn flush(&self) {
        if self.0.retired.get() {
            return;
        }
        let viewport = self.0.scroll.upgrade().map_or(0, |scroll| {
            let page = scroll.hadjustment().page_size();
            if page > 0.0 {
                page.round() as i32
            } else {
                scroll.width()
            }
        });
        if self.0.viewport.replace(viewport) != viewport {
            self.0.dirty.set(true);
        }
        if !self.0.dirty.replace(false) {
            return;
        }
        let layout = {
            let movement = self.0.movement.borrow();
            let preview = movement
                .as_ref()
                .filter(|m| !m.cancelled && m.generation == self.0.generation.get())
                .map(|m| (m.key.as_str(), m.point));
            self.0.base.borrow().positioned(
                &self.0.positions.borrow(),
                preview,
                &self.0.front.borrow(),
                f64::from(viewport),
            )
        };
        if let Some(fixed) = self.0.fixed.upgrade() {
            let mut previous: Option<gtk::Widget> = None;
            for module in &layout.modules {
                let frame = self
                    .0
                    .frames
                    .borrow()
                    .get(&(module.node, module.input))
                    .and_then(|w| w.upgrade());
                if let Some(frame) = frame {
                    fixed.move_(&frame, module.origin.x, module.origin.y);
                    if frame.prev_sibling().as_ref() != previous.as_ref() {
                        frame.insert_after(&fixed, previous.as_ref());
                    }
                    previous = Some(frame);
                }
            }
            fixed.set_size_request(layout.width as i32, layout.height as i32);
        }
        if let Some(canvas) = self.0.canvas.upgrade() {
            canvas.set_size_request(layout.width as i32, layout.height as i32);
        }
        if let Some(drawing) = self.0.drawing.upgrade() {
            drawing.set_content_width(layout.width as i32);
            drawing.set_content_height(layout.height as i32);
            drawing.queue_draw();
        }
        *self.0.geometry.borrow_mut() = layout;
    }
}

#[cfg(test)]
mod tests;
