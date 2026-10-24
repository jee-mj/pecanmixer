use super::{input::ActivationInput, *};
use crate::widgets::accessibility::set_name;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Output,
    Rec,
}
/// UI requests/notifications, not a second backend protocol. Configuration
/// callback success is handoff only; Selection is local edit-target choice;
/// Refresh asks the owner for status and never replays a failed mutation.
pub enum Event<O> {
    Configuration(Intent<O>),
    Selection {
        origin: O,
        kind: Destination,
        key: String,
    },
    Refresh {
        origin: O,
    },
}
/// The adapter's own request identity, not a widget-generated backend ticket.
pub enum Handoff<R> {
    Tracked(R),
    UiOnly,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewError {
    Model(Error),
    DuplicateTrackIdentity,
    UnexpectedAdapterReply,
}
type Callback<O, R> = dyn Fn(Event<O>) -> Result<Handoff<R>, String>;
fn terminal_for<R: Eq>(operation: Option<&Operation<R>>, request: &R) -> bool {
    operation.is_some_and(|op| {
        &op.request == request
            && match op.progress {
                Progress::Queued | Progress::Applying => false,
                Progress::AppliedLocally
                | Progress::NoOpLocally
                | Progress::Rejected(_)
                | Progress::PartiallyApplied(_)
                | Progress::Unconfirmed(_) => true,
            }
    })
}
struct BusRow {
    frame: gtk::Frame,
    title: gtk::Label,
    detail: gtk::Label,
    controls: gtk::Box,
    tracks: gtk::Box,
}
struct Presentation {
    epoch: Rc<()>,
    input_epoch: Rc<()>,
    output: gtk::DropDown,
    output_keys: Vec<String>,
    rec: gtk::DropDown,
    rec_keys: Vec<String>,
    actions: Vec<(gtk::Widget, Action)>,
    refresh: gtk::Button,
}
enum Focus {
    Output,
    Rec,
    Refresh,
    Action(Action),
}
struct State<O, R, T> {
    model: RefCell<Model<O, R, T>>,
    root: gtk::Box,
    status: gtk::Label,
    selections: gtk::Box,
    tools: gtk::Box,
    list: gtk::Box,
    orphan_frame: gtk::Frame,
    orphan_tracks: gtk::Box,
    track_absence: gtk::Label,
    rows: RefCell<BTreeMap<String, BusRow>>,
    tracks: RefCell<BTreeMap<T, gtk::Label>>,
    callback: RefCell<Option<Rc<Callback<O, R>>>>,
    epoch: RefCell<Rc<()>>,
    presentation: RefCell<Option<Presentation>>,
    input: Rc<ActivationInput>,
    applying: Cell<bool>,
    handing_off: Cell<bool>,
    awaited: RefCell<Option<R>>,
    refreshing: Cell<bool>,
    invalid: RefCell<Option<ViewError>>,
    error: RefCell<Option<String>>,
    last_origin: RefCell<Option<O>>,
    recording_external: Cell<bool>,
    offered: RefCell<Option<Snapshot<O, R, T>>>,
    offer_scheduled: Cell<bool>,
    #[cfg(test)]
    renders: Cell<u64>,
}
/// Isolated GTK surface. No default dispatcher, worker, timer, recording or
/// simulated success exists. The owner supplies ordered snapshots and one callback.
pub struct BusControls<O, R, T>(Rc<State<O, R, T>>);
impl<O: Clone + 'static, R: Clone + Eq + 'static, T: Clone + Ord + 'static> BusControls<O, R, T> {
    pub fn new(budget: Budget) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let status = gtk::Label::builder()
            .label("Waiting for bus configuration")
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let selections = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let orphan_tracks = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let orphan_frame = gtk::Frame::builder()
            .label("Tracks without a current bus")
            .child(&orphan_tracks)
            .build();
        let track_absence = label("No track status reported; absence does not confirm completion.");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.append(&list);
        content.append(&orphan_frame);
        content.append(&track_absence);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&content)
            .build();
        set_name(&scroll, "Recording buses and track status");
        root.append(&status);
        root.append(&selections);
        root.append(&tools);
        root.append(&scroll);
        let input = ActivationInput::new();
        input.observe(&root);
        let state = Rc::new(State {
            model: RefCell::new(Model::new(budget)),
            root,
            status,
            selections,
            tools,
            list,
            orphan_frame,
            orphan_tracks,
            track_absence,
            rows: RefCell::new(BTreeMap::new()),
            tracks: RefCell::new(BTreeMap::new()),
            callback: RefCell::new(None),
            epoch: RefCell::new(Rc::new(())),
            presentation: RefCell::new(None),
            input: input.clone(),
            applying: Cell::new(false),
            handing_off: Cell::new(false),
            awaited: RefCell::new(None),
            refreshing: Cell::new(false),
            invalid: RefCell::new(None),
            error: RefCell::new(None),
            last_origin: RefCell::new(None),
            recording_external: Cell::new(false),
            offered: RefCell::new(None),
            offer_scheduled: Cell::new(false),
            #[cfg(test)]
            renders: Cell::new(0),
        });
        let weak = Rc::downgrade(&state);
        input.notify(move || {
            if let Some(s) = weak.upgrade() {
                s.render();
            }
        });
        state.render();
        Self(state)
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    pub fn bind_intents(
        &self,
        callback: impl Fn(Event<O>) -> Result<Handoff<R>, String> + 'static,
    ) {
        *self.0.callback.borrow_mut() = Some(Rc::new(callback));
        self.0.retire();
        self.0.render();
    }
    pub fn observe(&self, snapshot: Snapshot<O, R, T>) -> Result<(), ViewError> {
        self.0.offered.borrow_mut().take();
        let origin = snapshot.origin.clone();
        let completed = self
            .0
            .awaited
            .borrow()
            .as_ref()
            .is_some_and(|request| terminal_for(snapshot.operation.as_ref(), request));
        let result = self
            .0
            .model
            .borrow_mut()
            .observe(snapshot)
            .map_err(ViewError::Model);
        // The model checks length budgets before identity validation allocates.
        let result = result.and_then(|()| {
            let model = self.0.model.borrow();
            let mut ids = BTreeSet::new();
            if model
                .snapshot()
                .unwrap()
                .tracks
                .iter()
                .any(|t| !ids.insert(t.identity.clone()))
            {
                Err(ViewError::DuplicateTrackIdentity)
            } else {
                Ok(())
            }
        });
        *self.0.invalid.borrow_mut() = result.as_ref().err().cloned();
        if result.is_ok() {
            *self.0.last_origin.borrow_mut() = Some(origin);
        }
        *self.0.error.borrow_mut() = None;
        // Pending progress cannot release this identity: a later replaceable
        // snapshot may omit the operation while the same work is still running.
        if completed && result.is_ok() {
            self.0.awaited.borrow_mut().take();
        }
        self.0.refreshing.set(false);
        self.0.retire();
        self.0.render();
        result
    }
    /// One replaceable main-context publication slot and one weak idle wake.
    /// The owner still filters source order and retains all completion handles.
    pub fn offer_snapshot(&self, snapshot: Snapshot<O, R, T>) {
        if self.0.offered.borrow().is_none() {
            self.0.retire();
        }
        *self.0.offered.borrow_mut() = Some(snapshot);
        if self.0.offer_scheduled.replace(true) {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        gtk::glib::idle_add_local_once(move || {
            if let Some(state) = weak.upgrade() {
                state.offer_scheduled.set(false);
                let snapshot = state.offered.borrow_mut().take();
                if let Some(snapshot) = snapshot {
                    let _ = BusControls(state).observe(snapshot);
                }
            }
        });
    }
    pub fn publication_error(&self) -> Option<ViewError> {
        self.0.invalid.borrow().clone()
    }
    /// Retained until a valid matching terminal report is consumed. The owner
    /// keeps reports readable across pending/coalesced state publications.
    pub fn awaited_request(&self) -> Option<R> {
        self.0.awaited.borrow().clone()
    }
    /// The application owns separate tracked recording controls/receipts. This
    /// only removes the isolated library placeholder; it enables no opcode.
    pub(crate) fn recording_owner_attached(&self) {
        if !self.0.recording_external.replace(true) {
            self.0.retire();
            self.0.render();
        }
    }
    /// Includes the synchronous handoff before its real request ID is returned.
    /// Companion surfaces must not infer idleness from awaited_request()==None.
    pub fn configuration_pending(&self) -> bool {
        self.0.awaiting()
    }
    pub fn selected_output_key(&self) -> Option<String> {
        if self.0.invalid.borrow().is_some() || self.0.offered.borrow().is_some() {
            return None;
        }
        self.0
            .model
            .borrow()
            .selected_output()
            .map(|s| s.key.clone())
    }
    pub fn selected_rec_key(&self) -> Option<String> {
        if self.0.invalid.borrow().is_some() || self.0.offered.borrow().is_some() {
            return None;
        }
        self.0.model.borrow().selected_rec().map(|s| s.key.clone())
    }
    /// Synchronous read-only projection for another presentation surface. The
    /// closure must not reenter this view. No model/selection store is cloned.
    /// A queued publication or invalid view cannot authorize another surface.
    pub fn inspect_model<U>(
        &self,
        inspect: impl FnOnce(&Model<O, R, T>) -> U,
    ) -> Result<U, ViewError> {
        if let Some(error) = self.0.invalid.borrow().clone() {
            return Err(error);
        }
        if self.0.applying.get() || self.0.offered.borrow().is_some() {
            return Err(ViewError::Model(Error::NoSnapshot));
        }
        Ok(inspect(&self.0.model.borrow()))
    }
    /// Owner-mediated local Output/Rec choice from a companion routing surface.
    /// Uses this view's exact binding and sole model; sends no configuration or
    /// recording request and never releases awaited configuration ownership.
    pub fn select_destination(&self, binding: &Binding<O>, key: &str) -> Result<(), ViewError> {
        self.inspect_model(|_| ())?;
        self.0
            .model
            .borrow_mut()
            .select_destination(binding, key)
            .map_err(ViewError::Model)?;
        self.0.retire();
        self.0.render();
        Ok(())
    }
}
impl<O: Clone + 'static, R: Clone + Eq + 'static, T: Clone + Ord + 'static> State<O, R, T> {
    fn retire(&self) {
        *self.epoch.borrow_mut() = Rc::new(());
        self.input.retire();
    }
    fn current(&self, epoch: &Rc<()>, live: &Cell<bool>) -> bool {
        self.current_read(epoch, live) && self.invalid.borrow().is_none()
    }
    fn current_read(&self, epoch: &Rc<()>, live: &Cell<bool>) -> bool {
        !self.applying.get()
            && self.offered.borrow().is_none()
            && live.get()
            && Rc::ptr_eq(epoch, &self.epoch.borrow())
            && !self.input.blocked()
    }
    fn callback(&self) -> Option<Rc<Callback<O, R>>> {
        self.callback.borrow().clone()
    }
    fn awaiting(&self) -> bool {
        self.handing_off.get() || self.awaited.borrow().is_some()
    }
    fn config(self: &Rc<Self>, binding: &Binding<O>, action: Action, epoch: &Rc<()>) {
        if self.awaiting() {
            return;
        }
        let intent = match self.model.borrow().request(binding, action) {
            Ok(i) => i,
            Err(_) => {
                *self.error.borrow_mut() =
                    Some("Configuration changed; start a fresh action".into());
                self.render();
                return;
            }
        };
        let Some(callback) = self.callback() else {
            return;
        };
        self.handing_off.set(true);
        // Do not hold a model/callback RefCell borrow across user integration.
        let result = callback(Event::Configuration(intent));
        self.handing_off.set(false);
        match result {
            Ok(Handoff::Tracked(request)) => {
                // A callback can publish reentrantly before returning its real
                // request identity. Apply the same terminal/validity rule as
                // observe; Queued/Applying and invalid reports retain the gate.
                let completed = self.invalid.borrow().is_none()
                    && terminal_for(
                        self.model
                            .borrow()
                            .snapshot()
                            .and_then(|s| s.operation.as_ref()),
                        &request,
                    );
                if !completed {
                    *self.awaited.borrow_mut() = Some(request);
                }
            }
            Ok(Handoff::UiOnly) => {
                *self.invalid.borrow_mut() = Some(ViewError::UnexpectedAdapterReply);
                *self.error.borrow_mut() =
                    Some("Untracked configuration reply; effects unknown. Refresh status.".into())
            }
            Err(error) => {
                if Rc::ptr_eq(epoch, &self.epoch.borrow()) {
                    *self.error.borrow_mut() = Some(error);
                }
            }
        }
        self.retire();
        self.render();
    }
    fn selection(
        self: &Rc<Self>,
        binding: &Binding<O>,
        kind: Destination,
        key: String,
        _epoch: &Rc<()>,
    ) {
        let Some(callback) = self.callback() else {
            return;
        };
        // Only UI choices are staged. Exact stored keys preserve default fallback
        // on rejection, including across a reentrant owner publication.
        let checkpoint = {
            let model = self.model.borrow();
            (model.output_choice.clone(), model.rec_choice.clone())
        };
        if self
            .model
            .borrow_mut()
            .select_destination(binding, &key)
            .is_err()
        {
            return;
        }
        let result = callback(Event::Selection {
            origin: binding.origin.clone(),
            kind,
            key,
        });
        match result {
            Ok(Handoff::UiOnly) => {}
            Ok(Handoff::Tracked(_)) => {
                let mut model = self.model.borrow_mut();
                model.output_choice = checkpoint.0;
                model.rec_choice = checkpoint.1;
                *self.invalid.borrow_mut() = Some(ViewError::UnexpectedAdapterReply);
                *self.error.borrow_mut() =
                    Some("Selection must remain a local edit-target choice".into());
            }
            Err(error) => {
                let mut model = self.model.borrow_mut();
                model.output_choice = checkpoint.0;
                model.rec_choice = checkpoint.1;
                *self.error.borrow_mut() = Some(error);
            }
        }
        if self.invalid.borrow().is_some() {
            self.retire();
        }
        self.render();
    }
    fn render(self: &Rc<Self>) {
        if self.applying.replace(true) {
            return;
        }
        #[cfg(test)]
        self.renders.set(self.renders.get().saturating_add(1));
        let invalid = self.invalid.borrow().is_some();
        let model = self.model.borrow();
        let rows = if invalid {
            vec![]
        } else {
            model.rows().unwrap_or_default()
        };
        let mut widgets = self.rows.borrow_mut();
        widgets.retain(|key, row| {
            if rows.iter().any(|r| r.definition.key == *key) {
                true
            } else {
                self.list.remove(&row.frame);
                false
            }
        });
        for row in &rows {
            let w = widgets
                .entry(row.definition.key.clone())
                .or_insert_with(|| {
                    let title = label("");
                    let detail = label("");
                    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                    let tracks = gtk::Box::new(gtk::Orientation::Vertical, 4);
                    let body = gtk::Box::new(gtk::Orientation::Vertical, 5);
                    for child in [&title, &detail] {
                        body.append(child);
                    }
                    body.append(&controls);
                    body.append(&tracks);
                    let frame = gtk::Frame::builder().child(&body).build();
                    self.list.append(&frame);
                    BusRow {
                        frame,
                        title,
                        detail,
                        controls,
                        tracks,
                    }
                });
            w.title.set_text(&row.definition.name);
            set_name(&w.frame, &format!("Recording bus {}", row.definition.name));
            w.detail.set_text(&format!(
                "Gain {:+.1} dB · mute {} · {}",
                row.state.db,
                if row.state.mute { "on" } else { "off" },
                if row.destination_key.is_some() {
                    "ports present in graph"
                } else {
                    "graph ports unavailable"
                }
            ));
        }
        let mut previous: Option<gtk::Widget> = None;
        for row in &rows {
            let w = &widgets[&row.definition.key];
            self.list.reorder_child_after(&w.frame, previous.as_ref());
            previous = Some(w.frame.clone().upcast());
        }
        let need = self.presentation.borrow().as_ref().is_none_or(|p| {
            !Rc::ptr_eq(&p.epoch, &self.epoch.borrow())
                || !Rc::ptr_eq(&p.input_epoch, &self.input.epoch())
        });
        let mut restore = None;
        if need {
            restore = self.presentation.borrow().as_ref().and_then(|p| {
                if p.output.has_focus() {
                    Some(Focus::Output)
                } else if p.rec.has_focus() {
                    Some(Focus::Rec)
                } else if p.refresh.has_focus() {
                    Some(Focus::Refresh)
                } else {
                    p.actions
                        .iter()
                        .find(|(w, _)| w.has_focus())
                        .map(|(_, a)| Focus::Action(a.clone()))
                }
            });
            self.input.retire();
            clear(&self.selections);
            clear(&self.tools);
            for row in widgets.values() {
                clear(&row.controls);
            }
            let epoch = self.epoch.borrow().clone();
            let binding = model.bind().ok().map(Rc::new);
            let (output, output_keys) =
                self.dropdown(&model, binding.clone(), Destination::Output, &epoch);
            let (rec, rec_keys) = self.dropdown(&model, binding.clone(), Destination::Rec, &epoch);
            let mut actions = vec![];
            let add = self.action_button(
                "Add stereo bus",
                "Add a stereo recording bus",
                Action::AddStereo,
                binding.clone(),
                &epoch,
            );
            self.tools.append(&add);
            actions.push((add.upcast(), Action::AddStereo));
            if !self.recording_external.get() {
                let record = gtk::Button::with_label("Record armed buses");
                record.set_sensitive(false);
                record.set_tooltip_text(Some("Recording receipts are pending integration"));
                set_name(&record, "Record armed buses · unavailable");
                self.tools.append(&record);
            }
            let refresh = gtk::Button::with_label("Refresh status");
            set_name(&refresh, "Refresh bus and recording status");
            if let Some(origin) = self.last_origin.borrow().clone() {
                let weak = Rc::downgrade(self);
                let epoch = epoch.clone();
                let live = Rc::new(Cell::new(true));
                self.input.capture(&refresh, live.clone());
                refresh.connect_clicked(move |_| {
                    let Some(s) = weak.upgrade() else {
                        return;
                    };
                    if !s.current_read(&epoch, &live) || s.refreshing.get() {
                        return;
                    }
                    let Some(cb) = s.callback() else {
                        return;
                    };
                    s.refreshing.set(true);
                    let result = cb(Event::Refresh {
                        origin: origin.clone(),
                    });
                    if Rc::ptr_eq(&epoch, &s.epoch.borrow()) {
                        match result {
                            Ok(Handoff::UiOnly) => {}
                            Ok(Handoff::Tracked(_)) => {
                                s.refreshing.set(false);
                                *s.invalid.borrow_mut() = Some(ViewError::UnexpectedAdapterReply);
                                *s.error.borrow_mut() =
                                    Some("Refresh must not submit a configuration mutation".into());
                                s.retire();
                            }
                            Err(e) => {
                                s.refreshing.set(false);
                                *s.error.borrow_mut() = Some(e);
                            }
                        }
                        s.render();
                    }
                });
            }
            self.tools.append(&refresh);
            for row in &rows {
                let box_ = &widgets[&row.definition.key].controls;
                let only = model
                    .snapshot()
                    .and_then(|s| s.configuration.as_ref())
                    .and_then(|c| c.armed().ok())
                    .is_some_and(|armed| armed.len() == 1 && armed[0].key == row.definition.key);
                let action = Action::Arm {
                    bus: row.definition.key.clone(),
                    additive: false,
                };
                let button = self.action_button(
                    if only { "Disarm" } else { "Arm only" },
                    &format!(
                        "{} {}",
                        if only { "Disarm" } else { "Arm only" },
                        row.definition.name
                    ),
                    action.clone(),
                    binding.clone(),
                    &epoch,
                );
                box_.append(&button);
                actions.push((button.upcast(), action));
                let arm = gtk::CheckButton::with_label("Armed");
                arm.set_active(row.state.armed);
                set_name(
                    &arm,
                    &format!("Toggle additive arming for {}", row.definition.name),
                );
                let action = Action::Arm {
                    bus: row.definition.key.clone(),
                    additive: true,
                };
                if let Some(binding) = binding.clone() {
                    let weak = Rc::downgrade(self);
                    let epoch = epoch.clone();
                    let action = action.clone();
                    let live = Rc::new(Cell::new(true));
                    self.input.capture(&arm, live.clone());
                    arm.connect_toggled(move |_| {
                        if let Some(s) = weak.upgrade() {
                            if s.current(&epoch, &live) {
                                s.config(&binding, action.clone(), &epoch);
                            }
                        }
                    });
                }
                box_.append(&arm);
                actions.push((arm.upcast(), action));
                if let Some((coupling, _)) = &row.coupling {
                    let (name, action) = match coupling {
                        Coupling::Split => (
                            "Split stereo",
                            Action::Split {
                                bus: row.definition.key.clone(),
                            },
                        ),
                        Coupling::Join => (
                            "Join mono pair",
                            Action::Join {
                                bus: row.definition.key.clone(),
                            },
                        ),
                    };
                    let button = self.action_button(
                        name,
                        &format!("{name} {}", row.definition.name),
                        action.clone(),
                        binding.clone(),
                        &epoch,
                    );
                    box_.append(&button);
                    actions.push((button.upcast(), action));
                }
                let action = Action::Remove {
                    bus: row.definition.key.clone(),
                };
                let button = self.action_button(
                    "Remove",
                    &format!("Remove {}", row.definition.name),
                    action.clone(),
                    binding.clone(),
                    &epoch,
                );
                box_.append(&button);
                actions.push((button.upcast(), action));
            }
            *self.presentation.borrow_mut() = Some(Presentation {
                epoch,
                input_epoch: self.input.epoch(),
                output,
                output_keys,
                rec,
                rec_keys,
                actions,
                refresh,
            });
        }
        let attached = self.callback.borrow().is_some();
        let input_blocked = self.input.blocked();
        let publication_pending = self.offered.borrow().is_some();
        if let Some(p) = self.presentation.borrow().as_ref() {
            for (widget, action) in &p.actions {
                let availability = model
                    .availability(action)
                    .unwrap_or(Availability::Unavailable(Unavailable::UnknownConfiguration));
                widget.set_sensitive(
                    attached
                        && !invalid
                        && !input_blocked
                        && !publication_pending
                        && !self.awaiting()
                        && availability == Availability::Available,
                );
                widget.set_tooltip_text(Some(&availability_message(
                    &availability,
                    attached,
                    self.awaiting(),
                )));
            }
            let selection_ready = attached
                && !invalid
                && !input_blocked
                && !publication_pending
                && model.snapshot().is_some_and(|s| s.connected && !s.closing);
            p.output
                .set_sensitive(selection_ready && !p.output_keys.is_empty());
            p.rec
                .set_sensitive(selection_ready && !p.rec_keys.is_empty());
            p.output.set_selected(selected(
                &p.output_keys,
                model.selected_output().map(|s| s.key.as_str()),
            ));
            p.rec.set_selected(selected(
                &p.rec_keys,
                model.selected_rec().map(|s| s.key.as_str()),
            ));
            p.refresh.set_sensitive(
                attached
                    && !input_blocked
                    && !publication_pending
                    && !self.refreshing.get()
                    && self.last_origin.borrow().is_some()
                    && model.snapshot().is_none_or(|s| !s.closing),
            );
            if !self.input.held() {
                let target = match restore {
                    Some(Focus::Output) => Some(p.output.clone().upcast::<gtk::Widget>()),
                    Some(Focus::Rec) => Some(p.rec.clone().upcast()),
                    Some(Focus::Refresh) => Some(p.refresh.clone().upcast()),
                    Some(Focus::Action(action)) => p
                        .actions
                        .iter()
                        .find(|(_, a)| a == &action)
                        .map(|(w, _)| w.clone()),
                    None => None,
                };
                if let Some(target) = target.filter(|w| w.is_sensitive()) {
                    target.grab_focus();
                }
            }
        }
        let tracks = if invalid {
            vec![]
        } else {
            model
                .snapshot()
                .map(|s| s.tracks.clone())
                .unwrap_or_default()
        };
        let mut labels = self.tracks.borrow_mut();
        labels.retain(|id, w| {
            if tracks.iter().any(|t| &t.identity == id) {
                true
            } else {
                detach(w);
                false
            }
        });
        for (index, track) in tracks.iter().enumerate() {
            let w = labels
                .entry(track.identity.clone())
                .or_insert_with(|| label(""));
            let status = track_message(&track.phase);
            let elapsed = track
                .elapsed
                .map(|d| format!(" · {:.1} s", d.as_secs_f64()))
                .unwrap_or_default();
            w.set_text(&format!("Track {} · {status}{elapsed}", index + 1));
            set_name(w, &format!("{} track status: {status}", track.bus_key));
            let container = widgets
                .get(&track.bus_key)
                .map(|r| &r.tracks)
                .unwrap_or(&self.orphan_tracks);
            if w.parent().as_ref() != Some(container.upcast_ref()) {
                detach(w);
                container.append(w);
            }
        }
        for (key, row) in widgets.iter() {
            let mut previous: Option<gtk::Widget> = None;
            for track in tracks.iter().filter(|t| t.bus_key == *key) {
                let widget = &labels[&track.identity];
                row.tracks.reorder_child_after(widget, previous.as_ref());
                previous = Some(widget.clone().upcast());
            }
        }
        let mut previous: Option<gtk::Widget> = None;
        for track in tracks.iter().filter(|t| !widgets.contains_key(&t.bus_key)) {
            let widget = &labels[&track.identity];
            self.orphan_tracks
                .reorder_child_after(widget, previous.as_ref());
            previous = Some(widget.clone().upcast());
        }
        self.orphan_frame
            .set_visible(tracks.iter().any(|t| !widgets.contains_key(&t.bus_key)));
        self.track_absence.set_visible(tracks.is_empty());
        let mut message = if let Some(error) = self.invalid.borrow().as_ref() {
            format!("Bus status unavailable: {}", view_error_message(error))
        } else if let Some(s) = model.snapshot() {
            if !s.connected {
                "Disconnected · configuration controls unavailable".into()
            } else if s.closing {
                "Finalizing application · configuration controls unavailable".into()
            } else if let Some(operation) = &s.operation {
                operation.progress.message()
            } else if s.configuration.is_none() {
                "Logical bus configuration unavailable".into()
            } else {
                if self.recording_external.get() {
                    "Bus configuration readback · recording managed by the application".into()
                } else {
                    "Bus configuration readback · recording controls pending integration".into()
                }
            }
        } else {
            "Waiting for bus configuration".into()
        };
        if self.awaiting() {
            message.push_str(" · Request pending; waiting for status");
        }
        if self.refreshing.get() {
            message.push_str(" · Status refresh requested");
        }
        if publication_pending {
            message.push_str(" · Status update pending");
        }
        if let Some(error) = self.error.borrow().as_ref() {
            message.push_str(&format!(" · {error}"));
        }
        self.status.set_text(&message);
        self.applying.set(false);
    }
    fn action_button(
        self: &Rc<Self>,
        text: &str,
        name: &str,
        action: Action,
        binding: Option<Rc<Binding<O>>>,
        epoch: &Rc<()>,
    ) -> gtk::Button {
        let button = gtk::Button::with_label(text);
        set_name(&button, name);
        if let Some(binding) = binding {
            let weak = Rc::downgrade(self);
            let epoch = epoch.clone();
            let live = Rc::new(Cell::new(true));
            self.input.capture(&button, live.clone());
            button.connect_clicked(move |button| {
                if let Some(s) = weak.upgrade() {
                    if button.is_sensitive() && s.current(&epoch, &live) {
                        s.config(&binding, action.clone(), &epoch);
                    }
                }
            });
        }
        button
    }
    fn dropdown(
        self: &Rc<Self>,
        model: &Model<O, R, T>,
        binding: Option<Rc<Binding<O>>>,
        kind: Destination,
        epoch: &Rc<()>,
    ) -> (gtk::DropDown, Vec<String>) {
        let choices: Vec<_> = model
            .snapshot()
            .into_iter()
            .flat_map(|s| s.mixer.destinations.iter())
            .filter(|s| s.bus.is_some() == (kind == Destination::Rec))
            .filter(|s| {
                kind != Destination::Rec
                    || model
                        .snapshot()
                        .and_then(|s| s.configuration.as_ref())
                        .is_some_and(|c| {
                            c.definitions().ok().is_some_and(|defs| {
                                defs.iter()
                                    .any(|d| s.bus.as_ref().is_some_and(|b| b.key == d.key))
                            })
                        })
            })
            .map(|s| (s.key.clone(), format!("{} · {}", s.name, s.subtitle)))
            .collect();
        let keys: Vec<_> = choices.iter().map(|c| c.0.clone()).collect();
        let strings =
            gtk::StringList::new(&choices.iter().map(|c| c.1.as_str()).collect::<Vec<_>>());
        let dropdown = gtk::DropDown::new(Some(strings), None::<gtk::Expression>);
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(Some(
                &gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(36)
                    .build(),
            ));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let (Some(label), Some(value)) = (
                item.child().and_downcast::<gtk::Label>(),
                item.item().and_downcast::<gtk::StringObject>(),
            ) {
                label.set_text(&value.string());
                label.set_tooltip_text(Some(&value.string()));
            }
        });
        dropdown.set_factory(Some(&factory));
        dropdown.set_list_factory(Some(&factory));
        dropdown.set_hexpand(true);
        let name = if kind == Destination::Output {
            "Output destination to edit"
        } else {
            "Rec destination to edit"
        };
        set_name(&dropdown, name);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&label(if kind == Destination::Output {
            "Output"
        } else {
            "Rec"
        }));
        row.append(&dropdown);
        self.selections.append(&row);
        if let Some(binding) = binding {
            let weak = Rc::downgrade(self);
            let epoch = epoch.clone();
            let keys = keys.clone();
            let live = Rc::new(Cell::new(true));
            self.input.capture(&dropdown, live.clone());
            dropdown.connect_selected_notify(move |d| {
                if let Some(s) = weak.upgrade() {
                    if s.current(&epoch, &live) && d.is_sensitive() {
                        if let Some(key) = keys.get(d.selected() as usize) {
                            s.selection(&binding, kind, key.clone(), &epoch);
                        }
                    }
                }
            });
        }
        (dropdown, keys)
    }
}
fn label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .build()
}
fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
fn detach(widget: &impl IsA<gtk::Widget>) {
    if let Some(parent) = widget.parent().and_downcast::<gtk::Box>() {
        parent.remove(widget);
    }
}
fn selected(keys: &[String], key: Option<&str>) -> u32 {
    keys.iter()
        .position(|k| Some(k.as_str()) == key)
        .map_or(gtk::INVALID_LIST_POSITION, |i| i as u32)
}
fn track_message(phase: &TrackPhase) -> String {
    match phase {
        TrackPhase::Starting => "Starting; readiness pending".into(),
        TrackPhase::WriterReady => "Writer ready; capture unconfirmed".into(),
        TrackPhase::Recording => "Recording".into(),
        TrackPhase::Finalizing => "Finalizing; join pending".into(),
        TrackPhase::Finalized(Ok(())) => "Finalized; join pending".into(),
        TrackPhase::Finalized(Err(e)) => format!("Finalization error; join pending: {e}"),
        TrackPhase::NotOpened(e) => format!("Not opened: {e}"),
        TrackPhase::Unconfirmed => "Outcome unconfirmed".into(),
        TrackPhase::Joined(Ok(())) => "Joined and finalized".into(),
        TrackPhase::Joined(Err(e)) => format!("Joined with error: {e}"),
    }
}
fn availability_message(a: &Availability, attached: bool, awaiting: bool) -> String {
    if !attached {
        return "Configuration controls are not connected".into();
    }
    if awaiting {
        return "Waiting for request status".into();
    }
    match a {
        Availability::Available => "Request against this configuration".into(),
        Availability::PendingOperation => "Configuration request pending".into(),
        Availability::PendingCapability(d) => match d {
            Dependency::IdleConfigurationAdapter => {
                "Configuration controls are awaiting integration"
            }
            Dependency::RoutedLayoutMigration => {
                "Changes to routed bus layouts are awaiting integration"
            }
            Dependency::ChannelShrinkMigration => "Bus removal is awaiting integration",
            Dependency::ActiveLayoutMigration => {
                "Layout changes during recording are awaiting integration"
            }
            Dependency::DynamicArming => "Arming changes during recording are awaiting integration",
            Dependency::RecordingReceipts => "Recording controls are awaiting integration",
        }
        .into(),
        Availability::Unavailable(reason) => match reason {
            Unavailable::Disconnected => "Disconnected".into(),
            Unavailable::Closing => "Application finalization has started".into(),
            Unavailable::UnknownConfiguration => "Bus configuration is unavailable".into(),
            Unavailable::UnknownActivity => "Recording activity is unknown".into(),
            Unavailable::ReconciliationRequired => {
                "Refresh and reconcile before another request".into()
            }
            Unavailable::MissingBus => "This bus is no longer available".into(),
            Unavailable::LastBus => "Keep at least one recording bus".into(),
            Unavailable::WrongCoupling => "Split/join is unavailable for this bus".into(),
            Unavailable::RowBudget => "This view's bus limit has been reached".into(),
            Unavailable::Adapter(message) => message.clone(),
        },
    }
}
fn view_error_message(error: &ViewError) -> &'static str {
    match error {
        ViewError::Model(Error::BudgetExceeded) => "display limit exceeded",
        ViewError::Model(_) => "configuration could not be represented",
        ViewError::DuplicateTrackIdentity => "repeated track identity",
        ViewError::UnexpectedAdapterReply => "request status is unconfirmed; refresh status",
    }
}

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "x11-input-tests"))]
#[path = "view/xtest.rs"]
mod xtest;
