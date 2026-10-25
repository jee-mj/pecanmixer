//! Retained bus-row input. All admission/execution/coalescing belongs to the
//! supplied owner callback; this module only retains its opaque request.
use super::super::levels as intent;
use super::*;
use crate::controls::fader::{self, Fader, Output};
use gtk::{gdk, glib};
use std::{rc::Weak, time::Duration};

pub(super) type Callback<O, R> = dyn Fn(intent::Event<O, R>) -> Result<intent::Reply<R>, String>;
pub(super) struct Work<O, R> {
    key: String,
    token: Rc<()>,
    request: R,
    capture: intent::Capture<O>,
    callback: Rc<Callback<O, R>>,
    ended: bool,
    cleanup: Option<Box<dyn Fn()>>,
}
impl<O, R> Drop for Work<O, R> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}
pub(super) struct LevelRow<O, R, T> {
    root: gtk::Box,
    scale: gtk::Scale,
    draft: gtk::SpinButton,
    actions: gtk::Box,
    status: gtk::Label,
    key: String,
    owner: Weak<State<O, R, T>>,
    policy: RefCell<Fader<Rc<()>>>,
    staged: RefCell<Option<(Rc<()>, intent::Capture<O>, Rc<Callback<O, R>>)>>,
    epoch: RefCell<Rc<()>>,
    wheel_blocked_until: Cell<Duration>,
    pointer_last: Cell<Option<f64>>,
}
impl<O: Clone + 'static, R: Clone + Eq + 'static, T: Clone + Ord + 'static> BusControls<O, R, T> {
    /// Separate UI-level callback. Begin/Set must return the owner's Tracked
    /// interaction identity; subsequent phases use that exact identity.
    pub fn bind_levels(
        &self,
        callback: impl Fn(intent::Event<O, R>) -> Result<intent::Reply<R>, String> + 'static,
    ) {
        self.0.abandon_level();
        for row in self.0.level_rows.borrow().values() {
            row.invalidate();
        }
        *self.0.level_callback.borrow_mut() = Some(Rc::new(callback));
        self.0.retire();
        self.0.render();
    }
    /// Existing owner clock, no additional timer or receipt consumer. Wheel
    /// idle finalization must continue even when a replaceable frame is queued.
    pub fn tick_levels(&self, now: Duration) {
        self.0.level_clock.set(now.max(self.0.level_clock.get()));
        let rows: Vec<_> = self.0.level_rows.borrow().values().cloned().collect();
        for row in rows {
            let output = row
                .policy
                .borrow_mut()
                .advance(self.0.level_clock.get())
                .unwrap_or_default();
            row.deliver(output);
        }
    }
    /// Only after the owner has reconciled the retained partial/unconfirmed
    /// work against fresh authoritative readback. An ordinary frame is no proof.
    pub fn levels_reconciled(&self) -> Result<(), ViewError> {
        self.inspect_model(|_| ())?;
        if self.0.handing_off.get()
            || self.0.awaited.borrow().is_some()
            || self.0.level_work.borrow().is_some()
            || self.0.level_protocol.get()
        {
            return Err(ViewError::Model(Error::Disabled(
                Availability::PendingOperation,
            )));
        }
        if !self.0.model.borrow().snapshot().is_some_and(|s| {
            s.connected
                && !s.closing
                && matches!(s.activity, Activity::Idle | Activity::Recording)
                && s.configuration.is_some()
        }) {
            return Err(ViewError::Model(Error::Disabled(
                Availability::Unavailable(Unavailable::UnknownConfiguration),
            )));
        }
        if self
            .0
            .model
            .borrow()
            .snapshot()
            .and_then(|s| s.operation.as_ref())
            .is_some_and(|o| {
                matches!(
                    o.progress,
                    Progress::Queued
                        | Progress::Applying
                        | Progress::AwaitingObservation
                        | Progress::PartiallyApplied(_)
                        | Progress::Unconfirmed(_)
                )
            })
        {
            return Err(ViewError::Model(Error::Disabled(
                Availability::Unavailable(Unavailable::ReconciliationRequired),
            )));
        }
        self.0.level_reconcile.set(false);
        self.0.render();
        Ok(())
    }
}
impl<O: Clone + 'static, R: Clone + Eq + 'static, T: Clone + Ord + 'static> State<O, R, T> {
    fn level_capture(&self, key: &str) -> Option<intent::Capture<O>> {
        if self.applying.get()
            || self.offered.borrow().is_some()
            || self.invalid.borrow().is_some()
            || self.awaiting()
            || self.level_protocol.get()
            || self.level_reconcile.get()
            || self.level_work.borrow().is_some()
            || self.input.blocked()
        {
            return None;
        }
        let m = self.model.borrow();
        m.capture_level(&m.bind().ok()?, key).ok()
    }
    fn level_admit(
        self: &Rc<Self>,
        capture: intent::Capture<O>,
        token: Rc<()>,
        kind: Option<fader::Kind>,
        change: Option<intent::Change>,
    ) {
        let Some(callback) = self.level_callback.borrow().clone() else {
            return;
        };
        for (key, row) in self.level_rows.borrow().iter() {
            if key != &capture.bus.key || kind.is_none() {
                row.invalidate();
            }
        }
        let event = if let Some(kind) = kind {
            intent::Event::Begin {
                capture: capture.clone(),
                kind,
            }
        } else {
            let Some(proposed) = change.and_then(|c| capture.clone().propose(c).ok()) else {
                return;
            };
            intent::Event::Set(proposed)
        };
        *self.level_starting.borrow_mut() = kind.map(|_| capture.bus.key.clone());
        self.handing_off.set(true);
        let result = callback(event);
        self.handing_off.set(false);
        self.level_starting.borrow_mut().take();
        match result {
            Ok(intent::Reply::Tracked(request)) => {
                let cleanup_callback = callback.clone();
                let cleanup_request = request.clone();
                let ended = kind.is_none();
                let cleanup: Option<Box<dyn Fn()>> = (!ended).then(|| {
                    Box::new(move || {
                        let _ = cleanup_callback(intent::Event::Abandon {
                            request: cleanup_request.clone(),
                        });
                    }) as Box<dyn Fn()>
                });
                *self.awaited.borrow_mut() = Some(request.clone());
                *self.level_work.borrow_mut() = Some(Work {
                    key: capture.bus.key.clone(),
                    token,
                    request,
                    capture,
                    callback: callback.clone(),
                    ended,
                    cleanup,
                });
                self.level_observed();
                let rebound = self
                    .level_callback
                    .borrow()
                    .as_ref()
                    .is_none_or(|c| !Rc::ptr_eq(c, &callback));
                if rebound {
                    self.abandon_level();
                }
            }
            Ok(intent::Reply::Accepted) => {
                self.level_protocol.set(true);
                *self.error.borrow_mut() =
                    Some("Untracked level handoff; effects unknown. Retain owner evidence.".into());
            }
            Err(error) => *self.error.borrow_mut() = Some(error.chars().take(1024).collect()),
        }
        self.retire();
        self.render();
    }
    fn level_send(
        self: &Rc<Self>,
        token: &Rc<()>,
        phase: impl FnOnce(R) -> intent::Event<O, R>,
        ended: bool,
    ) {
        let current = {
            let mut work = self.level_work.borrow_mut();
            let Some(w) = work
                .as_mut()
                .filter(|w| Rc::ptr_eq(token, &w.token) && !w.ended)
            else {
                return;
            };
            if ended {
                w.ended = true;
                w.cleanup.take();
            }
            (w.request.clone(), w.callback.clone())
        };
        let result = current.1(phase(current.0));
        // A callback can finish this request and synchronously admit another.
        // Its older return value cannot cancel or relabel the fresh interaction.
        if !self
            .level_work
            .borrow()
            .as_ref()
            .is_some_and(|w| Rc::ptr_eq(token, &w.token))
        {
            return;
        }
        match result {
            Ok(intent::Reply::Accepted) => {}
            Ok(intent::Reply::Tracked(_)) => {
                self.level_protocol.set(true);
                if ended {
                    if let Some(w) = self
                        .level_work
                        .borrow_mut()
                        .as_mut()
                        .filter(|w| Rc::ptr_eq(token, &w.token))
                    {
                        w.ended = false;
                    }
                }
                self.abandon_level();
                *self.error.borrow_mut() =
                    Some("Unexpected extra level request; retain owner evidence.".into());
            }
            Err(error) => {
                if ended {
                    if let Some(w) = self
                        .level_work
                        .borrow_mut()
                        .as_mut()
                        .filter(|w| Rc::ptr_eq(token, &w.token))
                    {
                        w.ended = false;
                    }
                }
                self.abandon_level();
                *self.error.borrow_mut() = Some(error.chars().take(1024).collect());
            }
        }
        self.render();
    }
    pub(super) fn abandon_level(self: &Rc<Self>) {
        let pending = {
            let mut work = self.level_work.borrow_mut();
            work.as_mut().filter(|w| !w.ended).map(|w| {
                w.ended = true;
                w.cleanup.take();
                (w.key.clone(), w.request.clone(), w.callback.clone())
            })
        };
        if let Some((key, request, callback)) = pending {
            if let Some(row) = self.level_rows.borrow().get(&key) {
                row.invalidate();
            }
            if !matches!(
                callback(intent::Event::Abandon { request }),
                Ok(intent::Reply::Accepted)
            ) {
                *self.error.borrow_mut() =
                    Some("Input ended; owner still retains level work and its outcome.".into());
            }
        }
    }
    pub(super) fn level_observed(self: &Rc<Self>) {
        let state = {
            let work = self.level_work.borrow();
            let Some(w) = work.as_ref() else { return };
            let model = self.model.borrow();
            let s = model.snapshot();
            let terminal = self.invalid.borrow().is_none()
                && terminal_for(s.and_then(|s| s.operation.as_ref()), &w.request);
            let uncertain = terminal
                && s.and_then(|s| s.operation.as_ref()).is_some_and(|o| {
                    matches!(
                        o.progress,
                        Progress::PartiallyApplied(_) | Progress::Unconfirmed(_)
                    )
                });
            let valid = self.invalid.borrow().is_none()
                && s.is_some_and(|s| {
                    s.connected
                        && !s.closing
                        && match s.activity {
                            Activity::Idle => s.capabilities.idle_levels == Support::Available,
                            Activity::Recording | Activity::Finalizing | Activity::Unknown => {
                                s.capabilities.active_levels == Support::Available
                            }
                        }
                        && s.configuration.as_ref().is_some_and(|c| {
                            c.widths == w.capture.configuration.widths
                                && c.definitions()
                                    .ok()
                                    .is_some_and(|defs| defs.iter().any(|d| d == &w.capture.bus))
                        })
                });
            (terminal, uncertain, valid)
        };
        if state.0 {
            let work = self.level_work.borrow_mut().take();
            if let Some(mut w) = work {
                w.cleanup.take();
                if let Some(row) = self.level_rows.borrow().get(&w.key) {
                    row.invalidate();
                }
            }
            self.awaited.borrow_mut().take();
            if state.1 {
                self.level_reconcile.set(true);
            }
        } else if !state.2 {
            self.abandon_level();
        }
    }
    pub(super) fn sync_levels(
        self: &Rc<Self>,
        rows: &[Row<T>],
        widgets: &BTreeMap<String, BusRow>,
    ) {
        let mut levels = self.level_rows.borrow_mut();
        levels.retain(|key, _| rows.iter().any(|r| &r.definition.key == key));
        for row in rows {
            let control = levels.entry(row.definition.key.clone()).or_insert_with(|| {
                let control = LevelRow::new(self, row);
                widgets[&row.definition.key].levels.append(&control.root);
                control
            });
            control.refresh(row);
        }
    }
}
impl<O: Clone + 'static, R: Clone + Eq + 'static, T: Clone + Ord + 'static> LevelRow<O, R, T> {
    fn new(owner: &Rc<State<O, R, T>>, row: &Row<T>) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Vertical)
            .adjustment(&gtk::Adjustment::new(0., 0., 1., 0.005, 0.05, 0.))
            .draw_value(false)
            .inverted(true)
            .focusable(false)
            .hexpand(true)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        scale.set_size_request(72, 160);
        scale.set_tooltip_text(Some("Drag relatively, Shift for fine, wheel, double-click for 0 dB. Keyboard: edit the dB proposal and activate Set gain."));
        let draft = gtk::SpinButton::with_range(-90., 6., 0.1);
        draft.set_digits(1);
        set_name(
            &draft,
            &format!("Gain proposal in dB for {}", row.definition.name),
        );
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let status = label("");
        root.append(&scale);
        root.append(&draft);
        root.append(&actions);
        root.append(&status);
        let initial = if row.state.db.is_finite() {
            row.state.db
        } else {
            0.
        };
        let r = Rc::new(Self {
            root,
            scale,
            draft,
            actions,
            status,
            key: row.definition.key.clone(),
            owner: Rc::downgrade(owner),
            policy: RefCell::new(Fader::new(initial).unwrap()),
            staged: RefCell::default(),
            epoch: RefCell::new(Rc::new(())),
            wheel_blocked_until: Cell::new(Duration::ZERO),
            pointer_last: Cell::new(None),
        });
        r.connect();
        r
    }
    fn invalidate(&self) {
        self.pointer_last.set(None);
        self.staged.borrow_mut().take();
        if let Some(s) = self.owner.upgrade() {
            if let Some(deadline) = self.policy.borrow().wheel_deadline() {
                self.wheel_blocked_until
                    .set(deadline.max(s.level_clock.get()));
            }
        }
        self.policy.borrow_mut().invalidate();
    }
    fn fresh(&self) -> Option<(Rc<State<O, R, T>>, intent::Capture<O>)> {
        let s = self.owner.upgrade()?;
        let capture = s.level_capture(&self.key)?;
        Some((s, capture))
    }
    fn deliver(&self, output: Vec<Output<Rc<()>>>) {
        for event in output {
            let Some(s) = self.owner.upgrade() else {
                return;
            };
            match event {
                Output::Begin { token, kind } => {
                    if let Some(capture) = s.level_capture(&self.key) {
                        if kind == fader::Kind::Pointer {
                            if let Some(callback) = s.level_callback.borrow().clone() {
                                *self.staged.borrow_mut() = Some((token, capture, callback));
                            }
                        } else {
                            s.level_admit(capture, token, Some(kind), None);
                        }
                    } else {
                        // Rejecting another Begin must not discard the only
                        // finalizer for this row's already-admitted request.
                        let owns_open = s
                            .level_work
                            .borrow()
                            .as_ref()
                            .is_some_and(|w| w.key == self.key && !w.ended);
                        if owns_open {
                            s.abandon_level();
                        } else {
                            self.invalidate();
                        }
                    }
                }
                Output::Update { token, db } => {
                    let staged = self.staged.borrow_mut().take();
                    if let Some((original, capture, callback)) = staged {
                        if Rc::ptr_eq(&original, &token)
                            && self.fresh().is_some()
                            && s.level_callback
                                .borrow()
                                .as_ref()
                                .is_some_and(|c| Rc::ptr_eq(c, &callback))
                        {
                            s.level_admit(capture, token.clone(), Some(fader::Kind::Pointer), None);
                        } else {
                            self.invalidate();
                        }
                    }
                    self.draft.set_value(db);
                    s.level_send(
                        &token,
                        |request| intent::Event::Update { request, db },
                        false,
                    );
                }
                Output::End { token, db, reason } => {
                    self.staged.borrow_mut().take();
                    s.level_send(
                        &token,
                        |request| intent::Event::End {
                            request,
                            db,
                            reason,
                        },
                        true,
                    );
                }
                Output::Abandon { .. } => s.abandon_level(),
            }
        }
    }
    fn pointer_down(&self, y: f64) {
        if self.fresh().is_none() {
            return;
        }
        self.pointer_last.set(Some(y));
        let output = self
            .policy
            .borrow_mut()
            .pointer_down(Rc::new(()), fader::PointerButton::Primary, y)
            .unwrap_or_default();
        self.deliver(output);
    }
    fn pointer_move(&self, y: f64, shift: bool) {
        // GTK can deliver a zero-displacement update with the initial press.
        // A press alone is not legacy onChange, nor a reset/empty admission.
        if self.pointer_last.get().is_none_or(|last| last == y) {
            return;
        }
        self.pointer_last.set(Some(y));
        let output = self
            .policy
            .borrow_mut()
            .pointer_move(y, Some(self.scale.height().max(160) as f64), shift)
            .unwrap_or_default();
        self.deliver(output);
    }
    fn pointer_end(&self, cancelled: bool) {
        self.pointer_last.set(None);
        let output = if cancelled {
            self.policy.borrow_mut().pointer_cancel()
        } else {
            self.policy.borrow_mut().pointer_up()
        };
        self.deliver(output);
    }
    fn wheel(&self, dy: f64, shift: bool) {
        // The pure policy permits independent pointer/wheel slots; the owner
        // admits one interaction. Preserve the pointer's release/cancel path,
        // including a press staged before its first nonzero movement.
        if self.policy.borrow().dragging() {
            return;
        }
        let Some(s) = self.owner.upgrade() else {
            return;
        };
        if s.level_clock.get() < self.wheel_blocked_until.get() {
            return;
        }
        let ongoing = s
            .level_work
            .borrow()
            .as_ref()
            .is_some_and(|w| w.key == self.key && !w.ended);
        if !ongoing && self.fresh().is_none() {
            return;
        }
        let output = self
            .policy
            .borrow_mut()
            .wheel(Rc::new(()), dy, shift, s.level_clock.get())
            .unwrap_or_default();
        self.deliver(output);
    }
    fn reset(&self) {
        if self.fresh().is_none() {
            return;
        }
        let output = self.policy.borrow().double_click(Rc::new(()));
        self.deliver(output);
    }
    fn refresh(self: &Rc<Self>, row: &Row<T>) {
        let Some(s) = self.owner.upgrade() else {
            return;
        };
        let _ = self.policy.borrow_mut().observe(row.state.db);
        self.scale.set_value(self.policy.borrow().position());
        if !self.draft.has_focus()
            && !s
                .level_work
                .borrow()
                .as_ref()
                .is_some_and(|w| w.key == self.key && !w.ended)
        {
            self.draft.set_value(row.state.db);
        }
        let pending = s.level_work.borrow();
        let current = pending.as_ref().filter(|w| w.key == self.key);
        let availability = s
            .model
            .borrow()
            .level_availability(&self.key)
            .unwrap_or(Availability::Unavailable(Unavailable::UnknownConfiguration));
        let attached = s.level_callback.borrow().is_some();
        let idle_ready = attached
            && !s.level_protocol.get()
            && !s.level_reconcile.get()
            && s.invalid.borrow().is_none()
            && s.offered.borrow().is_none();
        let open_gesture = current.is_some_and(|w| !w.ended)
            || s.level_starting.borrow().as_deref() == Some(self.key.as_str());
        self.scale.set_sensitive(
            idle_ready
                && (open_gesture || (!s.awaiting() && availability == Availability::Available)),
        );
        self.draft
            .set_sensitive(idle_ready && !s.awaiting() && availability == Availability::Available);
        let message = if s.level_protocol.get() {
            "Level handoff unconfirmed; retain owner evidence".into()
        } else if s.level_reconcile.get() {
            "Reconcile the actual level outcome before retry".into()
        } else if current.is_some() {
            "Level request pending; proposed values are not completion".into()
        } else if !attached {
            "Level controls awaiting owner integration".into()
        } else {
            availability_message(&availability, true, s.awaiting())
        };
        self.status.set_text(&message);
        if Rc::ptr_eq(&self.epoch.borrow(), &s.epoch.borrow()) {
            let mut child = self.actions.first_child();
            while let Some(w) = child {
                w.set_sensitive(
                    idle_ready
                        && !s.awaiting()
                        && availability == Availability::Available
                        && !s.input.blocked(),
                );
                child = w.next_sibling();
            }
            return;
        }
        *self.epoch.borrow_mut() = s.epoch.borrow().clone();
        if let Some(window) = self.root.root().and_downcast::<gtk::Window>() {
            if GtkWindowExt::focus(&window).is_some_and(|w| w.is_ancestor(&self.actions)) {
                GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
            }
        }
        clear(&self.actions);
        for (text, name, change) in [
            (
                "Set gain",
                format!("Set gain for {}", row.definition.name),
                None,
            ),
            (
                if row.state.mute {
                    "Unmute bus"
                } else {
                    "Mute bus"
                },
                format!(
                    "{} {}",
                    if row.state.mute { "Unmute" } else { "Mute" },
                    row.definition.name
                ),
                Some(intent::Change::Mute(!row.state.mute)),
            ),
        ] {
            let button = gtk::Button::with_label(text);
            set_name(&button, &name);
            button.set_sensitive(
                idle_ready
                    && !s.awaiting()
                    && availability == Availability::Available
                    && !s.input.blocked(),
            );
            let binding = s.model.borrow().bind().ok();
            let capture = binding
                .as_ref()
                .and_then(|b| s.model.borrow().capture_level(b, &self.key).ok());
            let callback = s.level_callback.borrow().clone();
            let epoch = s.epoch.borrow().clone();
            let live = Rc::new(Cell::new(true));
            s.input.capture(&button, live.clone());
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |b| {
                let Some(row) = weak.upgrade() else { return };
                let Some(s) = row.owner.upgrade() else { return };
                if !b.is_sensitive()
                    || !s.current(&epoch, &live)
                    || s.awaiting()
                    || s.level_protocol.get()
                    || s.level_reconcile.get()
                {
                    return;
                }
                // Both context and delegate are immutable for this activation.
                if !callback
                    .as_ref()
                    .zip(s.level_callback.borrow().as_ref())
                    .is_some_and(|(a, b)| Rc::ptr_eq(a, b))
                {
                    return;
                }
                let Some(capture) = capture.clone() else {
                    return;
                };
                let change = change
                    .clone()
                    .unwrap_or(intent::Change::Db(row.draft.value()));
                s.level_admit(capture, Rc::new(()), None, Some(change));
            });
            self.actions.append(&button);
        }
    }
    fn connect(self: &Rc<Self>) {
        // Reviewed vertical relative-motion law; a press alone submits nothing.
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |g, _, y| {
            if let Some(r) = weak.upgrade() {
                r.pointer_down(y);
                g.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |g, _, dy| {
            if let Some(r) = weak.upgrade() {
                let y = g.start_point().map_or(dy, |(_, y)| y + dy);
                r.pointer_move(
                    y,
                    g.current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                );
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(r) = weak.upgrade() {
                r.pointer_end(false);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            if let Some(r) = weak.upgrade() {
                r.pointer_end(true);
            }
        });
        let click = gtk::GestureClick::new();
        click.group_with(&drag);
        self.scale.add_controller(drag);
        click.set_button(1);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |g, n, _, _| {
            if n == 2 {
                if let Some(r) = weak.upgrade() {
                    r.pointer_end(true);
                    r.reset();
                    g.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
        self.scale.add_controller(click);
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |c, _, dy| {
            if let Some(r) = weak.upgrade() {
                r.wheel(
                    dy,
                    c.current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                );
            }
            glib::Propagation::Stop
        });
        self.scale.add_controller(scroll);
        let weak = Rc::downgrade(self);
        self.root.connect_unmap(move |_| {
            if let Some(r) = weak.upgrade() {
                if let Some(s) = r.owner.upgrade() {
                    if s.applying.get() {
                        return;
                    }
                    r.invalidate();
                    let own = s
                        .level_work
                        .borrow()
                        .as_ref()
                        .is_some_and(|w| w.key == r.key);
                    if own {
                        s.abandon_level();
                    }
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "levels_tests.rs"]
mod tests;
