use super::gesture::{GestureOrigin, Input};
use super::{Adapter, Binding, ControlError, Controller, Phase, ReadOnly};
use crate::widgets::accessibility::set_name;
use gtk::prelude::*;
use pecan_core::backend::Stamp;
use pecan_core::domain::{
    levels::{db_to_position, position_to_db, OFF_DB},
    mixer,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

struct Row<A: Adapter> {
    root: gtk::Box,
    title: gtk::Label,
    observed: gtk::Label,
    scale: gtk::Scale,
    mute: gtk::ToggleButton,
    volume_origin: Rc<RefCell<GestureOrigin<A>>>,
    mute_origin: Rc<RefCell<GestureOrigin<A>>>,
    identity: (u32, Option<u64>, Vec<usize>, Vec<u32>),
}
struct State<A: Adapter> {
    controller: RefCell<Controller<A>>,
    root: gtk::Box,
    status: gtk::Label,
    content: gtk::Box,
    rows: RefCell<BTreeMap<String, Row<A>>>,
    applying: Cell<bool>,
    error: RefCell<Option<ControlError>>,
}
/// Volume/mute controls driven by an adapter's owned snapshot and request types.
/// The default is read-only. GTK signals never directly access a PipeWire worker.
/// One request is pending at a time; values continue to display observed state.
pub struct MixerControls<A: Adapter = ReadOnly>(Rc<State<A>>);
impl Default for MixerControls<ReadOnly> {
    fn default() -> Self {
        Self::new(Controller::default())
    }
}
impl<A: Adapter + 'static> MixerControls<A> {
    pub fn new(controller: Controller<A>) -> Self {
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        set_name(&scroll, "Mixer volume and mute controls");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.append(&status);
        root.append(&scroll);
        let view = Self(Rc::new(State {
            controller: RefCell::new(controller),
            root,
            status,
            content,
            rows: RefCell::new(BTreeMap::new()),
            applying: Cell::new(false),
            error: RefCell::new(None),
        }));
        view.0.refresh();
        view
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    pub fn observe(&self, snapshot: A::Snapshot) -> bool {
        let accepted = self.0.controller.borrow_mut().observe(snapshot);
        if accepted {
            self.0.refresh();
        }
        accepted
    }
    pub fn observed_result(&self, request: &A::Request, stamp: Stamp) -> bool {
        let accepted = self
            .0
            .controller
            .borrow_mut()
            .observed_result(request, stamp);
        if accepted {
            self.0.refresh();
        }
        accepted
    }
    pub fn rejected(&self, request: &A::Request, message: impl Into<String>) -> bool {
        let accepted = self.0.controller.borrow_mut().rejected(request, message);
        if accepted {
            self.0.refresh();
        }
        accepted
    }
    pub fn phase(&self) -> Phase {
        self.0.controller.borrow().phase()
    }
}
impl<A: Adapter + 'static> State<A> {
    fn refresh(self: &Rc<Self>) {
        // set_value/set_active emit signals too. Never turn a graph refresh into
        // an audio mutation, including after a failure or a reconnect.
        self.applying.set(true);
        let controller = self.controller.borrow();
        let mut rows = self.rows.borrow_mut();
        let items = controller
            .model()
            .apps
            .iter()
            .chain(&controller.model().hardware)
            .chain(&controller.model().destinations)
            .collect::<Vec<_>>();
        rows.retain(|key, row| {
            if items
                .iter()
                .any(|strip| &strip.key == key && identity(strip) == row.identity)
            {
                true
            } else {
                row.volume_origin.borrow_mut().retire();
                row.mute_origin.borrow_mut().retire();
                self.content.remove(&row.root);
                false
            }
        });
        let mut previous: Option<gtk::Widget> = None;
        for strip in items {
            let row = rows.entry(strip.key.clone()).or_insert_with(|| {
                self.row(strip, controller.bind(&strip.key).expect("projected strip"))
            });
            let binding = controller.bind(&strip.key).expect("projected strip");
            row.volume_origin.borrow_mut().present(binding.clone());
            row.mute_origin.borrow_mut().present(binding);
            row.title
                .set_text(&format!("{} · {}", strip.name, strip.subtitle));
            let db = mixer::strip_db(strip);
            row.observed.set_text(&match db {
                Some(v) if v <= OFF_DB => "Observed: −∞ dB".into(),
                Some(v) => format!("Observed: {v:.1} dB"),
                None => "Observed volume unavailable".into(),
            });
            row.scale.set_value(db.map_or(0.0, db_to_position));
            row.scale.set_sensitive(controller.volume_available(strip));
            row.mute.set_active(strip.node.mute.unwrap_or(false));
            row.mute.set_sensitive(controller.mute_available(strip));
            set_name(
                &row.scale,
                &format!("Volume for {} · {}", strip.name, strip.subtitle),
            );
            set_name(
                &row.mute,
                &format!("Mute for {} · {}", strip.name, strip.subtitle),
            );
            self.content
                .reorder_child_after(&row.root, previous.as_ref());
            previous = Some(row.root.clone().upcast());
        }
        let phase = controller.phase().message();
        self.status
            .set_text(&if let Some(error) = self.error.borrow().as_ref() {
                format!("{phase} · {}", message(error))
            } else {
                phase
            });
        self.applying.set(false);
    }
    fn row(self: &Rc<Self>, strip: &mixer::Strip, binding: Binding<A::Snapshot>) -> Row<A> {
        let volume_origin = Rc::new(RefCell::new(GestureOrigin::new(binding.clone())));
        let mute_origin = Rc::new(RefCell::new(GestureOrigin::new(binding)));
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = gtk::Label::builder().xalign(0.0).wrap(true).build();
        let observed = gtk::Label::builder().xalign(0.0).build();
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.005);
        // The control position uses the legacy fader curve. The separate label
        // reports observed dB, so pending values cannot impersonate server state.
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        let mute = gtk::ToggleButton::with_label("Mute");
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.append(&scale);
        line.append(&mute);
        root.append(&title);
        root.append(&observed);
        root.append(&line);
        self.content.append(&root);
        capture_input(&scale, &volume_origin);
        capture_input(&mute, &mute_origin);
        let weak = Rc::downgrade(self);
        let scale_origin = volume_origin.clone();
        scale.connect_value_changed(move |scale| {
            let Some(state) = weak.upgrade() else { return };
            if state.applying.get() {
                return;
            }
            let binding = scale_origin.borrow().binding();
            let result = binding.and_then(|binding| {
                state
                    .controller
                    .borrow_mut()
                    .set_volume(&binding, position_to_db(scale.value()))
            });
            *state.error.borrow_mut() = result.err();
            state.refresh();
        });
        let weak = Rc::downgrade(self);
        let button_origin = mute_origin.clone();
        mute.connect_toggled(move |button| {
            let Some(state) = weak.upgrade() else { return };
            if state.applying.get() {
                return;
            }
            let binding = button_origin.borrow().binding();
            let result = binding.and_then(|binding| {
                state
                    .controller
                    .borrow_mut()
                    .set_mute(&binding, button.is_active())
            });
            *state.error.borrow_mut() = result.err();
            state.refresh();
        });
        Row {
            root,
            title,
            observed,
            scale,
            mute,
            volume_origin,
            mute_origin,
            identity: identity(strip),
        }
    }
}
fn identity(strip: &mixer::Strip) -> (u32, Option<u64>, Vec<usize>, Vec<u32>) {
    (
        strip.node.id,
        strip.node.serial,
        strip.channels.clone(),
        strip.ports.iter().map(|port| port.id).collect(),
    )
}
/// Observe input in capture phase without claiming GTK's scale/button gestures.
/// Value-change callbacks retain this control's origin, never a row-key lookup.
fn capture_input<A: Adapter + 'static>(
    widget: &impl IsA<gtk::Widget>,
    origin: &Rc<RefCell<GestureOrigin<A>>>,
) {
    let events = gtk::EventControllerLegacy::new();
    events.set_propagation_phase(gtk::PropagationPhase::Capture);
    let input_origin = origin.clone();
    events.connect_event(move |_, event| {
        use gtk::gdk::{EventType, ScrollDirection};
        let mut origin = input_origin.borrow_mut();
        match event.event_type() {
            EventType::ButtonPress | EventType::ButtonRelease => {
                if let Some(button) = event.downcast_ref::<gtk::gdk::ButtonEvent>() {
                    let source = Input::Mouse(button.button());
                    if event.event_type() == EventType::ButtonPress {
                        origin.begin(source);
                    } else {
                        origin.end(&source);
                    }
                }
            }
            EventType::TouchBegin => origin.begin(Input::Touch(event.event_sequence())),
            EventType::TouchEnd => origin.end(&Input::Touch(event.event_sequence())),
            EventType::TouchCancel => {
                origin.end(&Input::Touch(event.event_sequence()));
                origin.cancel();
            }
            EventType::GrabBroken => origin.cancel_pointer(),
            EventType::KeyPress | EventType::KeyRelease => {
                if let Some(key) = event.downcast_ref::<gtk::gdk::KeyEvent>() {
                    let source = Input::Key(key.keycode());
                    if event.event_type() == EventType::KeyPress {
                        origin.begin(source);
                    } else {
                        origin.end(&source);
                    }
                }
            }
            EventType::Scroll => {
                if let Some(scroll) = event.downcast_ref::<gtk::gdk::ScrollEvent>() {
                    if scroll.is_stop() {
                        origin.end(&Input::Scroll);
                    } else {
                        origin.begin(Input::Scroll);
                        if scroll.direction() != ScrollDirection::Smooth {
                            origin.end(&Input::Scroll);
                        }
                    }
                }
            }
            _ => {}
        }
        gtk::glib::Propagation::Proceed
    });
    widget.add_controller(events);
    let focus = gtk::EventControllerFocus::new();
    let focus_origin = origin.clone();
    focus.connect_enter(move |_| focus_origin.borrow_mut().focus_enter());
    let focus_origin = origin.clone();
    focus.connect_leave(move |_| focus_origin.borrow_mut().cancel());
    widget.add_controller(focus);
    let map_origin = origin.clone();
    widget.connect_unmap(move |_| map_origin.borrow_mut().cancel());
}

fn message(error: &ControlError) -> &'static str {
    match error {
        ControlError::ReadOnly => "Controls are not connected",
        ControlError::Disconnected => "Connection unavailable",
        ControlError::Busy => "A change is already pending",
        ControlError::StaleView => "The graph changed; try the control again",
        ControlError::MissingStrip => "The strip is no longer available",
        ControlError::Unsupported => "This control is not supported yet",
        ControlError::InvalidValue => "Invalid volume value",
        ControlError::Rejected(_) => "The change was rejected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::tests::{app_binding, mutate, setup, snapshot};
    use pecan_core::graph::Opdracht;

    #[gtk::test]
    fn gtk_signals_submit_once_and_refresh_without_dispatch() {
        let (controller, log, mut next) = setup();
        let binding = app_binding(&controller);
        let key = binding.key.clone();
        let view = MixerControls::new(controller);
        let window = gtk::Window::builder().child(view.widget()).build();
        window.present();
        let (scale, mute) = {
            let rows = view.0.rows.borrow();
            let row = &rows[&key];
            (row.scale.clone(), row.mute.clone())
        };
        view.0.rows.borrow()[&key]
            .volume_origin
            .borrow_mut()
            .begin(Input::Mouse(1));
        scale.set_value(db_to_position(-6.0));
        assert_eq!(log.borrow().calls.len(), 1);
        assert_eq!(view.phase(), Phase::WaitingForCompletion);
        assert!(!scale.is_sensitive());
        assert!(!mute.is_sensitive());
        let Opdracht::ZetVolume { knoop, volume } = log.borrow().calls[0].2.clone() else {
            panic!("volume")
        };
        view.observed_result(
            &1,
            Stamp {
                epoch: 2,
                revision: 2,
            },
        );
        assert_eq!(view.phase(), Phase::WaitingForGraph);
        next.graph.stamp.revision += 1;
        mutate(&mut next, |g| {
            g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().volume = Some(volume)
        });
        view.observe(next.clone());
        assert_eq!(view.phase(), Phase::Ready);
        assert!(scale.is_sensitive());
        assert_eq!(log.borrow().calls.len(), 1);
        view.0.rows.borrow()[&key]
            .mute_origin
            .borrow_mut()
            .begin(Input::Mouse(1));
        mute.set_active(true);
        assert_eq!(log.borrow().calls.len(), 2);
        assert!(!mute.is_active(), "pending intent is not an observed mute");
        view.rejected(&2, "Synthetic execution failure");
        assert!(matches!(view.phase(), Phase::Failed(_)));
        assert!(view.0.status.text().contains("Synthetic execution failure"));
        assert!(mute.is_sensitive());
        next.graph.stamp.revision += 1;
        mutate(&mut next, |g| g.verbonden = false);
        view.observe(next);
        assert!(view.0.rows.borrow().is_empty());
        assert_eq!(view.phase(), Phase::Disconnected);
        assert_eq!(log.borrow().calls.len(), 2);
        window.close();
    }
    #[gtk::test]
    fn held_input_cancels_on_refresh_and_requires_a_fresh_begin() {
        for input in [Input::Mouse(1), Input::Key(42), Input::Scroll] {
            let (controller, log, mut next) = setup();
            let key = controller.model().apps[0].key.clone();
            let view = MixerControls::new(controller);
            let window = gtk::Window::builder().child(view.widget()).build();
            window.present();
            let (scale, origin) = {
                let rows = view.0.rows.borrow();
                let row = &rows[&key];
                (row.scale.clone(), row.volume_origin.clone())
            };
            origin.borrow_mut().begin(input.clone());
            next.graph.stamp.revision += 1;
            view.observe(next);
            assert_eq!(
                view.0.rows.borrow()[&key].scale,
                scale,
                "unchanged targets retain widgets"
            );
            origin.borrow_mut().begin(input.clone()); // held key / smooth scroll
            scale.set_value(db_to_position(-6.0));
            assert!(
                log.borrow().calls.is_empty(),
                "refresh must not substitute the held input's origin"
            );
            assert!(view.0.status.text().contains("graph changed"));
            origin.borrow_mut().end(&input);
            scale.set_value(db_to_position(-9.0)); // a late release callback
            assert!(log.borrow().calls.is_empty());
            origin.borrow_mut().begin(input);
            scale.set_value(db_to_position(-12.0));
            assert_eq!(log.borrow().calls.len(), 1);
            assert_eq!(log.borrow().calls[0].1.revision, 2);
            window.close();
        }
    }
    #[gtk::test]
    fn acknowledgement_does_not_resume_a_held_fader_on_a_new_origin() {
        let (controller, log, mut next) = setup();
        let key = controller.model().apps[0].key.clone();
        let view = MixerControls::new(controller);
        let (scale, origin) = {
            let rows = view.0.rows.borrow();
            (rows[&key].scale.clone(), rows[&key].volume_origin.clone())
        };
        origin.borrow_mut().begin(Input::Mouse(1));
        scale.set_value(db_to_position(-6.0));
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
        assert_eq!(view.phase(), Phase::Ready);
        assert!(scale.is_sensitive());
        scale.set_value(db_to_position(-9.0));
        assert_eq!(
            log.borrow().calls.len(),
            1,
            "acknowledgement cannot rearm a held gesture"
        );
        origin.borrow_mut().end(&Input::Mouse(1));
        origin.borrow_mut().begin(Input::Mouse(1));
        scale.set_value(db_to_position(-12.0));
        assert_eq!(log.borrow().calls.len(), 2);
        assert_eq!(log.borrow().calls[1].1.revision, 2);
    }
    #[gtk::test]
    fn retained_scale_and_mute_callbacks_cannot_adopt_a_replacement_row() {
        for disconnected_first in [false, true] {
            let (controller, log, mut next) = setup();
            let key = controller.model().apps[0].key.clone();
            let id = controller.model().apps[0].node.id;
            let view = MixerControls::new(controller);
            let (old_scale, old_mute, old_origin) = {
                let rows = view.0.rows.borrow();
                let row = &rows[&key];
                row.volume_origin.borrow_mut().begin(Input::Mouse(1));
                row.mute_origin.borrow_mut().begin(Input::Key(65));
                (
                    row.scale.clone(),
                    row.mute.clone(),
                    row.volume_origin.clone(),
                )
            };
            if disconnected_first {
                let mut disconnected = next.clone();
                disconnected.graph.stamp.revision += 1;
                mutate(&mut disconnected, |graph| graph.verbonden = false);
                view.observe(disconnected);
                next.graph.stamp.epoch += 1;
            }
            next.graph.stamp.revision += 2;
            mutate(&mut next, |graph| {
                graph.knopen.iter_mut().find(|n| n.id == id).unwrap().serial = Some(9000)
            });
            view.observe(next.clone());
            let (new_scale, new_origin) = {
                let rows = view.0.rows.borrow();
                (rows[&key].scale.clone(), rows[&key].volume_origin.clone())
            };
            assert_ne!(
                new_scale, old_scale,
                "a replacement target must have a new widget lifetime"
            );
            old_origin.borrow_mut().end(&Input::Mouse(1));
            old_origin.borrow_mut().begin(Input::Mouse(1));
            old_scale.set_value(db_to_position(-6.0));
            old_mute.set_active(true);
            assert!(
                log.borrow().calls.is_empty(),
                "retained callbacks belong to the retired row"
            );
            new_origin.borrow_mut().begin(Input::Mouse(1));
            new_scale.set_value(db_to_position(-12.0));
            assert_eq!(log.borrow().calls.len(), 1);
            assert_eq!(log.borrow().calls[0].1, next.graph.stamp);
        }
    }
    #[gtk::test]
    fn default_widgets_are_read_only_and_release_their_controller() {
        let view = MixerControls::default();
        view.observe(snapshot(2, 1).graph);
        assert_eq!(view.phase(), Phase::ReadOnly);
        assert!(view
            .0
            .rows
            .borrow()
            .values()
            .all(|row| !row.scale.is_sensitive() && !row.mute.is_sensitive()));
        let weak = Rc::downgrade(&view.0);
        drop(view);
        assert!(
            weak.upgrade().is_none(),
            "signal callbacks must not retain the controller"
        );
    }
}

impl MixerControls<crate::native_backend::NativeClient> {
    pub fn poll_native(&self) -> bool {
        let changed = self.0.controller.borrow_mut().poll_native();
        if changed {
            self.0.refresh();
        }
        changed
    }
}
