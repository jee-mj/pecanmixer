//! Canvas input state only. Core owns routing/history plans; the shared controls
//! adapter owns handoff. One draft retains its exact origin until release/cancel.
use super::{
    actions::Operation,
    controls::Controls,
    layout::{Layout, Point},
};
use gtk::{glib, prelude::*};
use pecan_core::{backend::Stamped, domain::history::Action, graph::Momentopname};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

struct Draft {
    origin: Stamped<Arc<Momentopname>>,
    anchor: u32,
    input: bool,
    moving: Option<(u32, [u32; 2])>,
    start: Point,
    point: Point,
    cancelled: bool,
}
struct State {
    controls: Controls,
    geometry: Rc<RefCell<Layout>>,
    drawing: glib::WeakRef<gtk::DrawingArea>,
    draft: RefCell<Option<Draft>>,
    retired: Cell<bool>,
    session_cancel: RefCell<Option<Box<dyn Fn()>>>,
}
#[derive(Clone)]
pub(super) struct Pointer(Rc<State>);
impl Pointer {
    pub fn new(
        controls: &Controls,
        geometry: &Rc<RefCell<Layout>>,
        canvas: &gtk::Overlay,
        drawing: &gtk::DrawingArea,
    ) -> Self {
        let pointer = Self(Rc::new(State {
            controls: controls.clone(),
            geometry: geometry.clone(),
            drawing: drawing.downgrade(),
            draft: RefCell::new(None),
            retired: Cell::new(false),
            session_cancel: RefCell::new(None),
        }));
        let weak = Rc::downgrade(&pointer.0);
        let changed = Rc::downgrade(&pointer.0);
        controls.watch(
            move || {
                if let Some(state) = weak.upgrade() {
                    Self(state).cancel();
                }
            },
            move || {
                if let Some(state) = changed.upgrade() {
                    Self(state).redraw();
                }
            },
        );
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&pointer.0);
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
        let weak = Rc::downgrade(&pointer.0);
        drag.connect_drag_update(move |_, x, y| {
            if let Some(state) = weak.upgrade() {
                Self(state).motion(x, y);
            }
        });
        let weak = Rc::downgrade(&pointer.0);
        drag.connect_drag_end(move |_, x, y| {
            if let Some(state) = weak.upgrade() {
                Self(state).release(x, y);
            }
        });
        let weak = Rc::downgrade(&pointer.0);
        drag.connect_cancel(move |_, _| {
            if let Some(state) = weak.upgrade() {
                Self(state).finish_cancel();
            }
        });
        canvas.add_controller(drag);
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&pointer.0);
        click.connect_released(move |gesture, _, x, y| {
            if let Some(state) = weak.upgrade() {
                if Self(state).click(
                    Point { x, y },
                    gesture
                        .current_event_state()
                        .contains(gtk::gdk::ModifierType::SHIFT_MASK),
                ) {
                    if let Some(widget) = gesture.widget() {
                        widget.grab_focus();
                    }
                }
            }
        });
        canvas.add_controller(click);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&pointer.0);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(state) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gtk::gdk::Key::Escape => {
                    state.controls.cancel();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace => {
                    state.controls.request(Operation::Disconnect);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        canvas.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(&pointer.0);
        focus.connect_leave(move |_| {
            if let Some(state) = weak.upgrade() {
                Self(state).cancel();
            }
        });
        canvas.add_controller(focus);
        let weak = Rc::downgrade(&pointer.0);
        canvas.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade() {
                Self(state).cancel();
            }
        });
        canvas.set_focusable(true);
        let weak = Rc::downgrade(&pointer.0);
        let geometry = geometry.clone();
        drawing.set_draw_func(move |_, context, _, _| {
            let state = weak.upgrade();
            let draft = state.as_ref().and_then(|s| {
                s.draft
                    .borrow()
                    .as_ref()
                    .filter(|d| !d.cancelled)
                    .map(|d| (d.anchor, d.input, d.moving, d.point))
            });
            let selected = state
                .as_ref()
                .map(|s| s.controls.inspect(|m| m.selected.clone()))
                .unwrap_or_default();
            let geometry = geometry.borrow();
            for cable in &geometry.cables {
                if draft
                    .is_some_and(|(_, _, moving, _)| moving.is_some_and(|(id, _)| id == cable.id))
                {
                    continue;
                }
                if selected.contains(&cable.id) {
                    context.set_source_rgb(0.86, 0.48, 0.12);
                    context.set_line_width(4.0);
                } else {
                    context.set_source_rgba(0.25, 0.55, 0.8, 0.8);
                    context.set_line_width(2.0);
                }
                curve(context, cable.from, cable.to);
                for point in [cable.from, cable.to] {
                    dot(context, point, 3.0);
                }
            }
            if let (Some(state), Some((anchor, input, moving, point))) = (state, draft) {
                if let Some(from) = geometry.ports.iter().find(|p| p.id == anchor) {
                    let target = target(
                        &state.controls,
                        &geometry,
                        anchor,
                        input,
                        moving.map(|(_, pair)| pair),
                        point,
                    );
                    let to = target
                        .as_ref()
                        .and_then(|(id, _)| geometry.ports.iter().find(|p| p.id == *id));
                    if to.is_some() {
                        context.set_source_rgb(0.12, 0.68, 0.3);
                    } else {
                        context.set_source_rgba(0.5, 0.5, 0.5, 0.9);
                    }
                    context.set_line_width(3.0);
                    let end = to.map_or(point, |port| port.point);
                    if input {
                        curve(context, end, from.point);
                    } else {
                        curve(context, from.point, end);
                    }
                    dot(context, from.point, 5.0);
                    if let Some(port) = to {
                        dot(context, port.point, 7.0);
                    }
                }
            }
        });
        pointer
    }
    fn begin(&self, point: Point) -> bool {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || self.0.retired.get()
            || self.0.draft.borrow().is_some()
            || !self.0.controls.enabled()
        {
            return false;
        }
        let Some(port) = self.0.geometry.borrow().port_at(point).cloned() else {
            return false;
        };
        let draft = self.0.controls.inspect(|model| {
            let editable = if port.input {
                &model.inputs
            } else {
                &model.outputs
            };
            if !editable.iter().any(|p| p.id == port.id) {
                return None;
            }
            let origin = model.graph.as_ref()?.clone();
            let incoming = origin
                .value
                .kabels
                .iter()
                .filter(|c| c.in_poort == port.id && model.pair(c.id).is_some())
                .collect::<Vec<_>>();
            let picked = if port.input {
                incoming
                    .iter()
                    .find(|c| model.selected.contains(&c.id))
                    .copied()
                    .or_else(|| {
                        if incoming.len() == 1 {
                            Some(incoming[0])
                        } else {
                            None
                        }
                    })
            } else {
                None
            };
            let moving = picked.and_then(|c| model.pair(c.id).map(|pair| (c.id, pair)));
            Some(Draft {
                origin,
                anchor: moving.map_or(port.id, |(_, pair)| pair[0]),
                input: port.input && moving.is_none(),
                moving,
                start: point,
                point,
                cancelled: false,
            })
        });
        let Some(draft) = draft else {
            self.0
                .controls
                .message("Only available audio ports can be patched.");
            return false;
        };
        if let Some(cancel) = self.0.session_cancel.borrow().as_ref() {
            cancel();
        }
        self.0.controls.clear_choices();
        *self.0.draft.borrow_mut() = Some(draft);
        self.feedback();
        self.redraw();
        true
    }
    fn motion(&self, dx: f64, dy: f64) {
        if !dx.is_finite() || !dy.is_finite() {
            self.cancel();
            return;
        }
        if let Some(draft) = self.0.draft.borrow_mut().as_mut().filter(|d| !d.cancelled) {
            draft.point = Point {
                x: draft.start.x + dx,
                y: draft.start.y + dy,
            };
        }
        self.feedback();
        self.redraw();
    }
    fn release(&self, dx: f64, dy: f64) {
        if !dx.is_finite() || !dy.is_finite() {
            self.finish_cancel();
            return;
        }
        let draft = self.0.draft.borrow_mut().take();
        let Some(mut draft) = draft else { return };
        self.redraw();
        if draft.cancelled || self.0.retired.get() || !self.0.controls.enabled() {
            return;
        }
        draft.point = Point {
            x: draft.start.x + dx,
            y: draft.start.y + dy,
        };
        let action = target(
            &self.0.controls,
            &self.0.geometry.borrow(),
            draft.anchor,
            draft.input,
            draft.moving.map(|(_, pair)| pair),
            draft.point,
        )
        .map(|(_, action)| action)
        .or_else(|| draft.moving.map(|(_, pair)| Action::Disconnect(vec![pair])));
        if let Some(action) = action {
            if let Err(error) = self.0.controls.submit_action(draft.origin, action) {
                self.0.controls.message(&error);
            }
        } else {
            self.0
                .controls
                .message("No compatible target. No patch was sent.");
        }
    }
    fn click(&self, point: Point, shift: bool) -> bool {
        if self.0.retired.get() || self.0.draft.borrow().is_some() {
            return false;
        }
        let geometry = self.0.geometry.borrow();
        if geometry.port_at(point).is_some() {
            return false;
        }
        if let Some(id) = geometry.cable_at(point) {
            let mut selected = self.0.controls.inspect(|m| m.selected.clone());
            if shift {
                if !selected.remove(&id) {
                    selected.insert(id);
                }
            } else if selected.len() == 1 && selected.contains(&id) {
                selected.clear();
            } else {
                selected.clear();
                selected.insert(id);
            }
            self.0.controls.select(selected);
            true
        } else if !geometry.modules.iter().any(|m| {
            point.x >= m.origin.x
                && point.x <= m.origin.x + super::layout::WIDTH
                && point.y >= m.origin.y
                && point.y <= m.origin.y + m.height
        }) {
            self.0.controls.select([]);
            true
        } else {
            false
        }
    }
    // Retain this cancelled input until its end. A repeated begin from the held
    // sequence must never capture a newer presentation.
    pub fn watch_session(&self, cancel: impl Fn() + 'static) {
        *self.0.session_cancel.borrow_mut() = Some(Box::new(cancel));
    }
    pub fn cancel(&self) {
        if let Some(cancel) = self.0.session_cancel.borrow().as_ref() {
            cancel();
        }
        if let Some(draft) = self.0.draft.borrow_mut().as_mut() {
            draft.cancelled = true;
        }
        self.redraw();
    }
    // GTK cancellation ends the native sequence, so its draft can be released.
    fn finish_cancel(&self) {
        self.0.draft.borrow_mut().take();
        self.redraw();
    }
    pub fn retire(&self) {
        self.0.retired.set(true);
        self.finish_cancel();
    }
    fn redraw(&self) {
        if let Some(drawing) = self.0.drawing.upgrade() {
            drawing.queue_draw();
        }
    }
    fn feedback(&self) {
        let draft = self.0.draft.borrow();
        let Some(draft) = draft.as_ref().filter(|d| !d.cancelled) else {
            return;
        };
        let compatible = target(
            &self.0.controls,
            &self.0.geometry.borrow(),
            draft.anchor,
            draft.input,
            draft.moving.map(|(_, pair)| pair),
            draft.point,
        )
        .is_some();
        self.0
            .controls
            .message(match (compatible, draft.moving.is_some()) {
            (true, true) => "Release to move the cable. Escape cancels and keeps the original.",
            (true, false) => "Release to connect these audio ports. Escape cancels.",
            (false, true) => {
                "No compatible target: release disconnects the picked-up cable. Escape keeps it."
            }
            (false, false) => "Choose an opposite audio port. Releasing here sends no patch.",
        });
    }
}
fn target(
    controls: &Controls,
    layout: &Layout,
    anchor: u32,
    input: bool,
    moving: Option<[u32; 2]>,
    point: Point,
) -> Option<(u32, Action)> {
    let port = layout.port_at(point)?;
    if port.input == input {
        return None;
    }
    let pair = if input {
        [port.id, anchor]
    } else {
        [anchor, port.id]
    };
    controls
        .inspect(|model| model.connection(pair, moving).ok())
        .map(|action| (port.id, action))
}
fn curve(context: &gtk::cairo::Context, from: Point, to: Point) {
    let bend = ((to.x - from.x).abs() * 0.5).max(60.0);
    context.move_to(from.x, from.y);
    context.curve_to(from.x + bend, from.y, to.x - bend, to.y, to.x, to.y);
    let _ = context.stroke();
}
fn dot(context: &gtk::cairo::Context, point: Point, radius: f64) {
    context.arc(point.x, point.y, radius, 0.0, std::f64::consts::TAU);
    let _ = context.fill();
}

#[cfg(test)]
mod tests;
