//! Window capture observes the complete pointer sequence. Paned's internal
//! descendant gesture can deny an ancestor GestureDrag, so resize ownership is
//! explicit here. Only public child geometry identifies a separator.
use super::{layout::State, DisplayEdit, RowGrip, RowResize, SplitResize};
use gtk::{gdk, glib, graphene, prelude::*};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy)]
enum Resize {
    Rows(RowResize),
    Split(SplitResize),
}
#[derive(Clone, PartialEq)]
enum Input {
    Mouse,
    Touch(gdk::EventSequence),
}
struct Active {
    input: Input,
    resize: Resize,
    start: (f64, f64),
}
struct Pointer {
    active: Option<Active>,
    click: Option<(u32, (f64, f64), usize)>,
    ignored: Option<Input>,
}
pub(super) fn connect(state: &Rc<State>) {
    let pointer = Rc::new(RefCell::new(Pointer {
        active: None,
        click: None,
        ignored: None,
    }));
    let weak = Rc::downgrade(state);
    let p = pointer.clone();
    state.root.connect_unmap(move |_| {
        p.borrow_mut().active = None;
        p.borrow_mut().click = None;
        p.borrow_mut().ignored = None;
    });
    state.root.connect_map(move |root| {
        let (Some(s), Some(window)) = (weak.upgrade(), root.root().and_downcast::<gtk::Window>())
        else {
            return;
        };
        if s.interaction
            .borrow()
            .as_ref()
            .is_some_and(|c| c.widget().as_ref() == Some(window.upcast_ref()))
        {
            return;
        }
        if let Some(old) = s.interaction.borrow_mut().take() {
            if let Some(w) = old.widget() {
                w.remove_controller(&old);
            }
        }
        let controller = gtk::EventControllerLegacy::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&s);
        let p = pointer.clone();
        controller.connect_event(move |_, event| {
            let Some(s) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !s.root.is_mapped() {
                return glib::Propagation::Proceed;
            }
            use gdk::EventType;
            if event.event_type() == EventType::GrabBroken {
                let mut pointer = p.borrow_mut();
                pointer.active = None;
                pointer.ignored = None;
                pointer.click = None;
                return glib::Propagation::Proceed;
            }
            if event.event_type() == EventType::MotionNotify
                && !event
                    .modifier_state()
                    .contains(gdk::ModifierType::BUTTON1_MASK)
            {
                p.borrow_mut().active = None;
                return glib::Propagation::Proceed;
            }
            if event.event_type() == EventType::ButtonPress
                && event
                    .downcast_ref::<gdk::ButtonEvent>()
                    .is_some_and(|e| e.button() != 1)
            {
                p.borrow_mut().click = None;
            }
            let input = match event.event_type() {
                EventType::ButtonPress | EventType::ButtonRelease
                    if event
                        .downcast_ref::<gdk::ButtonEvent>()
                        .is_some_and(|e| e.button() == 1) =>
                {
                    Some(Input::Mouse)
                }
                EventType::TouchBegin
                | EventType::TouchUpdate
                | EventType::TouchEnd
                | EventType::TouchCancel => Some(Input::Touch(event.event_sequence())),
                EventType::MotionNotify => Some(Input::Mouse),
                _ => None,
            };
            let Some(input) = input else {
                return glib::Propagation::Proceed;
            };
            let start = matches!(
                event.event_type(),
                EventType::ButtonPress | EventType::TouchBegin
            );
            let end = matches!(
                event.event_type(),
                EventType::ButtonRelease | EventType::TouchEnd | EventType::TouchCancel
            );
            if end {
                let mut p = p.borrow_mut();
                if p.ignored.as_ref() == Some(&input) {
                    p.ignored = None;
                    return glib::Propagation::Stop;
                }
                if p.active.as_ref().is_some_and(|a| a.input == input) {
                    p.active = None;
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            let Some(position) = event.position() else {
                return glib::Propagation::Proceed;
            };
            if start {
                if p.borrow().active.is_some() {
                    return glib::Propagation::Proceed;
                }
                for (index, (pane, grip)) in [
                    (&s.outer, Some(RowGrip::AppsHardware)),
                    (&s.inner, Some(RowGrip::HardwareDestinations)),
                    (&s.outputs, None),
                ]
                .into_iter()
                .enumerate()
                {
                    let Some((x, y)) = local(pane, position) else {
                        continue;
                    };
                    if !handle(pane, x, y) {
                        continue;
                    }
                    if !s.editable.get() {
                        p.borrow_mut().ignored = Some(input);
                        return glib::Propagation::Stop;
                    }
                    let resize = if let Some(grip) = grip {
                        let prefs = s.preferences.borrow();
                        if !grip.rows().iter().all(|r| prefs.row_visible(*r)) {
                            return glib::Propagation::Stop;
                        }
                        let (a, b) = match grip {
                            RowGrip::AppsHardware => {
                                (s.sections[0].height(), s.sections[1].height())
                            }
                            RowGrip::HardwareDestinations => {
                                (s.sections[1].height(), s.sections[2].height())
                            }
                        };
                        RowResize::new(grip, prefs.weights(), a as f64, b as f64).map(Resize::Rows)
                    } else {
                        let a = pane.start_child().unwrap();
                        let b = pane.end_child().unwrap().compute_bounds(pane).unwrap();
                        SplitResize::new(
                            a.width() as f64,
                            pane.width() as f64,
                            f64::from(b.x()) - a.width() as f64,
                        )
                        .map(Resize::Split)
                    };
                    let settings = gtk::Settings::default().unwrap();
                    let time = settings.property::<i32>("gtk-double-click-time").max(0) as u32;
                    let distance =
                        f64::from(settings.property::<i32>("gtk-double-click-distance").max(0));
                    let double = input == Input::Mouse
                        && p.borrow().click.is_some_and(|(t, (x, y), i)| {
                            i == index
                                && event.time().wrapping_sub(t) <= time
                                && (x - position.0).abs() <= distance
                                && (y - position.1).abs() <= distance
                        });
                    if double {
                        p.borrow_mut().click = None;
                        p.borrow_mut().ignored = Some(input);
                        let edit = grip.map_or(DisplayEdit::DestinationWidth(None), |g| {
                            DisplayEdit::Rows(g.equal(s.preferences.borrow().weights()))
                        });
                        s.emit(edit);
                    } else {
                        let mut p = p.borrow_mut();
                        if input == Input::Mouse {
                            p.click = Some((event.time(), position, index));
                        }
                        p.active = resize.map(|resize| Active {
                            input,
                            resize,
                            start: position,
                        });
                    }
                    return glib::Propagation::Stop;
                }
                // An intervening click elsewhere is not part of this handle's
                // double-click sequence.
                p.borrow_mut().click = None;
            } else {
                let edit = {
                    let p = p.borrow();
                    p.active
                        .as_ref()
                        .filter(|a| a.input == input)
                        .map(|a| match a.resize {
                            Resize::Rows(r) => {
                                r.drag(position.1 - a.start.1).map(DisplayEdit::Rows)
                            }
                            Resize::Split(r) => r
                                .drag(position.0 - a.start.0)
                                .map(|w| DisplayEdit::DestinationWidth(Some(w))),
                        })
                };
                if let Some(edit) = edit {
                    if s.editable.get() {
                        if let Some(edit) = edit {
                            s.emit(edit);
                        }
                    }
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        window.add_controller(controller.clone());
        *s.interaction.borrow_mut() = Some(controller);
    });
}
fn local(pane: &gtk::Paned, position: (f64, f64)) -> Option<(f64, f64)> {
    let native = pane.native()?;
    let (tx, ty) = native.surface_transform();
    let widget = native.dynamic_cast::<gtk::Widget>().ok()?;
    let p = widget.compute_point(
        pane,
        &graphene::Point::new((position.0 + tx) as f32, (position.1 + ty) as f32),
    )?;
    Some((f64::from(p.x()), f64::from(p.y())))
}
fn handle(pane: &gtk::Paned, x: f64, y: f64) -> bool {
    let (Some(a), Some(b)) = (pane.start_child(), pane.end_child()) else {
        return false;
    };
    if !a.is_visible() || !b.is_visible() {
        return false;
    }
    let (Some(a), Some(b)) = (a.compute_bounds(pane), b.compute_bounds(pane)) else {
        return false;
    };
    match pane.orientation() {
        gtk::Orientation::Vertical => {
            y >= f64::from(a.y() + a.height())
                && y <= f64::from(b.y())
                && x >= 0.
                && x <= f64::from(pane.width())
        }
        _ => {
            x >= f64::from(a.x() + a.width())
                && x <= f64::from(b.x())
                && y >= 0.
                && y <= f64::from(pane.height())
        }
    }
}
