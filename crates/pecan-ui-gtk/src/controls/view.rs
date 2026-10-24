use super::{Adapter, Binding, ControlError, Controller, Phase, ReadOnly};
use crate::widgets::accessibility::set_name;
use gtk::prelude::*;
use pecan_core::domain::{
    levels::{db_to_position, position_to_db, OFF_DB},
    mixer,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

struct Row<S> {
    root: gtk::Box,
    title: gtk::Label,
    observed: gtk::Label,
    scale: gtk::Scale,
    mute: gtk::ToggleButton,
    binding: RefCell<Option<Binding<S>>>,
}
struct State<A: Adapter> {
    controller: RefCell<Controller<A>>,
    root: gtk::Box,
    status: gtk::Label,
    content: gtk::Box,
    rows: RefCell<BTreeMap<String, Row<A::Snapshot>>>,
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
    pub fn confirmed(&self, request: &A::Request) -> bool {
        let accepted = self.0.controller.borrow_mut().confirmed(request);
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
            if items.iter().any(|strip| &strip.key == key) {
                true
            } else {
                self.content.remove(&row.root);
                false
            }
        });
        let mut previous: Option<gtk::Widget> = None;
        for strip in items {
            let row = rows
                .entry(strip.key.clone())
                .or_insert_with(|| self.row(&strip.key));
            *row.binding.borrow_mut() = controller.bind(&strip.key).ok();
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
    fn row(self: &Rc<Self>, key: &str) -> Row<A::Snapshot> {
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
        let weak = Rc::downgrade(self);
        let scale_key = key.to_string();
        scale.connect_value_changed(move |scale| {
            let Some(state) = weak.upgrade() else { return };
            if state.applying.get() {
                return;
            }
            let binding = state
                .rows
                .borrow()
                .get(&scale_key)
                .and_then(|row| row.binding.borrow().clone());
            if let Some(binding) = binding {
                let result = state
                    .controller
                    .borrow_mut()
                    .set_volume(&binding, position_to_db(scale.value()));
                *state.error.borrow_mut() = result.err();
                state.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        let mute_key = key.to_string();
        mute.connect_toggled(move |button| {
            let Some(state) = weak.upgrade() else { return };
            if state.applying.get() {
                return;
            }
            let binding = state
                .rows
                .borrow()
                .get(&mute_key)
                .and_then(|row| row.binding.borrow().clone());
            if let Some(binding) = binding {
                let result = state
                    .controller
                    .borrow_mut()
                    .set_mute(&binding, button.is_active());
                *state.error.borrow_mut() = result.err();
                state.refresh();
            }
        });
        Row {
            root,
            title,
            observed,
            scale,
            mute,
            binding: RefCell::new(None),
        }
    }
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
        scale.set_value(db_to_position(-6.0));
        assert_eq!(log.borrow().calls.len(), 1);
        assert_eq!(view.phase(), Phase::WaitingForCompletion);
        assert!(!scale.is_sensitive());
        assert!(!mute.is_sensitive());
        let Opdracht::ZetVolume { knoop, volume } = log.borrow().calls[0].2.clone() else {
            panic!("volume")
        };
        view.confirmed(&1);
        assert_eq!(view.phase(), Phase::WaitingForObservation);
        next.graph.stamp.revision += 1;
        mutate(&mut next, |g| {
            g.knopen.iter_mut().find(|n| n.id == knoop).unwrap().volume = Some(volume)
        });
        view.observe(next.clone());
        assert_eq!(view.phase(), Phase::Ready);
        assert!(scale.is_sensitive());
        assert_eq!(log.borrow().calls.len(), 1);
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
