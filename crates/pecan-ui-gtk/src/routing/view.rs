use super::*;
use crate::{recording_buses::input::ActivationInput, widgets::accessibility::set_name};
use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};

type Handler<O> = dyn Fn(Event<O>) -> Result<(), String>;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    Choice(String),
    Route(String, String),
    Reset,
    Confirm,
    Cancel,
    Recover,
}
struct State<O> {
    model: RefCell<Model<O>>,
    root: gtk::Box,
    status: gtk::Label,
    selections: gtk::Box,
    tools: gtk::Box,
    confirmation: gtk::Box,
    rows: gtk::Box,
    buttons: RefCell<Vec<(Key, gtk::Button)>>,
    input: Rc<ActivationInput>,
    callback: RefCell<Option<Rc<Handler<O>>>>,
    recovery: RefCell<Option<Rc<dyn Fn() -> Result<(), String>>>>,
    offered: RefCell<Option<Capture<O>>>,
    scheduled: Cell<bool>,
    presentation: RefCell<Rc<()>>,
    last_error: RefCell<Option<String>>,
    unavailable: Cell<bool>,
    was_confirmation: Cell<bool>,
    #[cfg(test)]
    renders: Cell<usize>,
}
/// Standalone main-thread routing surface. No callback is installed by default;
/// no owner/backend consumer, receipt poller, timer, or startup effect is added.
pub struct RoutingControls<O>(Rc<State<O>>);
impl<O> Clone for RoutingControls<O> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<O: Clone + 'static> RoutingControls<O> {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        set_name(&root, "Audio routing and Back to auto");
        let status = gtk::Label::builder()
            .label("Waiting for routing snapshot")
            .xalign(0.0)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let selections = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        selections.set_homogeneous(true);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let confirmation = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .accessible_role(gtk::AccessibleRole::AlertDialog)
            .build();
        set_name(&confirmation, "Confirm Back to auto");
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&rows)
            .build();
        set_name(&scroll, "Audio source routes");
        root.append(&status);
        root.append(&selections);
        root.append(&tools);
        root.append(&confirmation);
        root.append(&scroll);
        let input = ActivationInput::new();
        input.observe(&root);
        let state = Rc::new(State {
            model: RefCell::new(Model::default()),
            root,
            status,
            selections,
            tools,
            confirmation,
            rows,
            buttons: RefCell::new(vec![]),
            input: input.clone(),
            callback: RefCell::new(None),
            recovery: RefCell::new(None),
            offered: RefCell::new(None),
            scheduled: Cell::new(false),
            presentation: RefCell::new(Rc::new(())),
            last_error: RefCell::new(None),
            unavailable: Cell::new(false),
            was_confirmation: Cell::new(false),
            #[cfg(test)]
            renders: Cell::new(0),
        });
        let weak = Rc::downgrade(&state);
        input.notify(move || {
            if let Some(s) = weak.upgrade() {
                s.render();
            }
        });
        let escape = gtk::EventControllerKey::new();
        escape.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&state);
        escape.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                if let Some(s) = weak.upgrade() {
                    s.model.borrow_mut().cancel_reset();
                    s.render();
                }
            }
            glib::Propagation::Proceed
        });
        state.root.add_controller(escape);
        let weak = Rc::downgrade(&state);
        state.root.connect_unmap(move |_| {
            if let Some(s) = weak.upgrade() {
                s.model.borrow_mut().retire();
            }
        });
        let weak = Rc::downgrade(&state);
        state.root.connect_map(move |_| {
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
    pub fn bind_intents(&self, handler: impl Fn(Event<O>) -> Result<(), String> + 'static) {
        *self.0.callback.borrow_mut() = Some(Rc::new(handler));
        self.0.model.borrow_mut().retire();
        self.0.render();
    }
    pub fn observe(&self, capture: Capture<O>) -> Result<(), Error> {
        self.0.check_order(&capture.graph)?;
        self.0.offered.borrow_mut().take();
        let result = self.0.model.borrow_mut().observe(capture);
        if let Err(error) = &result {
            *self.0.last_error.borrow_mut() = Some(error.to_string());
        }
        let was_unavailable = self.0.unavailable.replace(result.is_err());
        if was_unavailable && result.is_ok() {
            self.0.last_error.borrow_mut().take();
        }
        self.0.root.set_sensitive(true);
        self.0.render();
        result
    }
    /// One replaceable graph/selection slot and one weak idle wake. Reports are
    /// deliberately delivered separately and must never be coalesced away.
    pub fn offer(&self, capture: Capture<O>) -> Result<(), Error> {
        self.0.check_order(&capture.graph)?;
        self.0.model.borrow_mut().retire();
        self.0.input.retire();
        self.0.root.set_sensitive(false);
        *self.0.offered.borrow_mut() = Some(capture);
        if !self.0.scheduled.replace(true) {
            let weak = Rc::downgrade(&self.0);
            glib::idle_add_local_once(move || {
                if let Some(s) = weak.upgrade() {
                    s.scheduled.set(false);
                    let offered = s.offered.borrow_mut().take();
                    if let Some(c) = offered {
                        let _ = RoutingControls(s).observe(c);
                    }
                }
            });
        }
        Ok(())
    }
    pub fn report(&self, token: &RequestToken, report: NativeReport) -> bool {
        let accepted = self.0.model.borrow_mut().report(token, report);
        if accepted {
            self.0.render();
        }
        accepted
    }
    /// The owner lost publication authority (busy reads are not new defaults).
    /// Retain display/pending handles but prevent all new actions until observe.
    pub fn unavailable(&self, message: &str) {
        self.0.offered.borrow_mut().take();
        self.0.unavailable.set(true);
        self.0.model.borrow_mut().retire();
        *self.0.last_error.borrow_mut() = Some(message.into());
        self.0.root.set_sensitive(true);
        self.0.render();
    }
    /// Only the owner may attest completed reconciliation/history release. This
    /// method is never called by a widget retry/cancel or a graph publication.
    pub fn reconciled(&self, capture: Capture<O>) -> Result<(), Error> {
        self.0.check_order(&capture.graph)?;
        self.0.model.borrow_mut().reconciled(capture)?;
        self.0.offered.borrow_mut().take();
        self.0.unavailable.set(false);
        self.0.last_error.borrow_mut().take();
        self.0.root.set_sensitive(true);
        self.0.render();
        Ok(())
    }
    pub fn pending(&self) -> Option<RequestToken> {
        self.0.model.borrow().pending()
    }
    /// The owner binds this to one retained original request. Reuse the same
    /// capture-or-cancel input guard as routing, including focus retirement.
    pub(crate) fn bind_recovery(&self, handler: Option<Rc<dyn Fn() -> Result<(), String>>>) {
        *self.0.recovery.borrow_mut() = handler;
        self.0.render();
    }
    /// A provisional projection refresh cannot release the retained routing
    /// latch when the owner's final native inspection/ownership check fails.
    pub(crate) fn recovery_failed(&self, message: &str) {
        self.0.model.borrow_mut().reconcile = true;
        self.unavailable(message);
    }
    pub fn status_text(&self) -> String {
        self.0.status.text().into()
    }
}
impl<O: Clone + 'static> Default for RoutingControls<O> {
    fn default() -> Self {
        Self::new()
    }
}
impl<O: Clone + 'static> State<O> {
    fn check_order(&self, graph: &Stamped<Arc<Momentopname>>) -> Result<(), Error> {
        let model = self.model.borrow();
        let offered = self.offered.borrow();
        let current = offered
            .as_ref()
            .map(|c| &c.graph)
            .or_else(|| model.capture.as_ref().map(|c| &c.graph));
        if let Some(old) = current {
            if earlier(graph.stamp, old.stamp) {
                return Err(Error::OldGraph);
            }
            if graph.stamp == old.stamp && !Arc::ptr_eq(&graph.value, &old.value) {
                return Err(Error::ConflictingStamp);
            }
        }
        Ok(())
    }
    fn render(self: &Rc<Self>) {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        let focus = self
            .buttons
            .borrow()
            .iter()
            .find(|(_, b)| b.has_focus())
            .map(|(key, _)| key.clone());
        self.input.retire();
        // GTK 4.14 can retain a focused descendant of a removed row. Its
        // keyboard events then miss the window's release observer. Unset only
        // focus in the subtrees being replaced; the real release still owns
        // clearing the held ledger before any fresh activation is enabled.
        if let Some(window) = self.root.root().and_downcast::<gtk::Window>() {
            if GtkWindowExt::focus(&window).is_some_and(|focused| {
                [
                    &self.selections,
                    &self.tools,
                    &self.confirmation,
                    &self.rows,
                ]
                .iter()
                .any(|container| focused.is_ancestor(*container))
            }) {
                GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
            }
        }
        self.buttons.borrow_mut().clear();
        *self.presentation.borrow_mut() = Rc::new(());
        for container in [
            &self.selections,
            &self.tools,
            &self.confirmation,
            &self.rows,
        ] {
            clear(container);
        }
        let model = self.model.borrow();
        let bind = model.bind().ok();
        let ready = model.availability();
        let counts = model.counts();
        let confirming = model.confirmation();
        let confirmation_binding = model.confirmation_binding();
        let choices = model.choices();
        let rows = model.rows();
        let connected = model
            .capture
            .as_ref()
            .is_some_and(|c| c.graph.value.verbonden && !c.closing);
        let known = model
            .capture
            .as_ref()
            .is_some_and(|c| c.configuration.is_some());
        let enabled = self.callback.borrow().is_some()
            && !self.unavailable.get()
            && !self.input.blocked()
            && self.offered.borrow().is_none();
        let status = self
            .last_error
            .borrow()
            .clone()
            .or_else(|| match &ready {
                Err(
                    Error::Disconnected
                    | Error::Closing
                    | Error::UnknownConfiguration
                    | Error::ReconciliationRequired
                    | Error::AwaitingGraph,
                ) => ready.as_ref().err().map(ToString::to_string),
                _ => model.notice().map(str::to_owned),
            })
            .unwrap_or_else(|| match &ready {
                Ok(()) => {
                    if enabled {
                        "Routing ready".into()
                    } else {
                        "Routing controls unavailable".into()
                    }
                }
                Err(e) => e.to_string(),
            });
        drop(model);
        self.status.set_text(&status);
        let output = gtk::Box::new(gtk::Orientation::Vertical, 4);
        output.append(&label("Output destination"));
        let rec = gtk::Box::new(gtk::Orientation::Vertical, 4);
        rec.append(&label("Rec destination"));
        for choice in choices {
            let text = format!(
                "{}{}",
                if choice.selected { "Selected: " } else { "" },
                choice.name
            );
            let key = choice.key.clone();
            let binding = bind.clone();
            let b = self.button(
                Key::Choice(key.clone()),
                &text,
                enabled && connected && (choice.kind == Destination::Output || known),
                move |s| {
                    let result = s
                        .model
                        .borrow()
                        .select(binding.as_ref().ok_or(Error::NoSnapshot)?, &key);
                    s.send(result);
                    Ok(())
                },
            );
            set_name(
                &b,
                &format!(
                    "{} destination: {}{}",
                    if choice.kind == Destination::Rec {
                        "Rec"
                    } else {
                        "Output"
                    },
                    choice.name,
                    if choice.selected { "; selected" } else { "" }
                ),
            );
            if choice.kind == Destination::Rec {
                rec.append(&b);
            } else {
                output.append(&b);
            }
        }
        for (body, name) in [(&output, "Output destinations"), (&rec, "Rec destinations")] {
            let scroll = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vscrollbar_policy(gtk::PolicyType::Automatic)
                .propagate_natural_height(true)
                .min_content_height(60)
                .max_content_height(160)
                .child(body)
                .build();
            set_name(&scroll, name);
            self.selections.append(&scroll);
        }
        let binding = bind.clone();
        let reset = self.button(
            Key::Reset,
            &format!("Back to auto ({})", counts.total()),
            enabled && ready.is_ok() && counts.total() > 0,
            move |s| {
                s.model
                    .borrow_mut()
                    .begin_reset(binding.as_ref().ok_or(Error::NoSnapshot)?)?;
                s.render();
                Ok(())
            },
        );
        self.tools.append(&reset);
        if self.recovery.borrow().is_some() {
            let recovery = self.button(
                Key::Recover,
                "Inspect and refresh routing status",
                connected && !self.input.blocked(),
                |s| {
                    let handler = s.recovery.borrow().clone();
                    if let Some(handler) = handler {
                        if let Err(error) = handler() {
                            *s.last_error.borrow_mut() = Some(error);
                        }
                    }
                    s.render();
                    Ok(())
                },
            );
            self.tools.append(&recovery);
        }
        self.confirmation.set_visible(confirming.is_some());
        if let Some(counts) = confirming {
            self.confirmation.append(&label(&format!("{} manual routes will be reset: {} links, {} target overrides, {} disconnected streams. WirePlumber takes over again. Rec routes are kept.",counts.total(),counts.links,counts.targets,counts.detached)));
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let cancel = self.button(Key::Cancel, "Cancel", true, |s| {
                s.model.borrow_mut().cancel_reset();
                s.render();
                Ok(())
            });
            actions.append(&cancel);
            let binding = confirmation_binding.clone();
            let confirm = self.button(
                Key::Confirm,
                "Remove manual routes",
                enabled && ready.is_ok(),
                move |s| {
                    let result = s
                        .model
                        .borrow_mut()
                        .confirm_reset(binding.as_ref().ok_or(Error::NoSnapshot)?);
                    s.send(result);
                    Ok(())
                },
            );
            actions.append(&confirm);
            self.confirmation.append(&actions);
        }
        for row in rows {
            let frame = gtk::Frame::new(None);
            let title = label(&row.name);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_lines(2);
            frame.set_label_widget(Some(&title));
            set_name(&frame, &row.name);
            let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
            body.append(&label(&row.subtitle));
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            for (cell, rec) in [
                row.output.as_ref().map(|r| (r, false)),
                row.rec.as_ref().map(|r| (r, true)),
            ]
            .into_iter()
            .flatten()
            {
                let custom = matches!(cell.state, RouteState::Custom { .. });
                let text = format!(
                    "{}{}: {}",
                    if rec { "Rec " } else { "" },
                    cell.name,
                    match &cell.state {
                        RouteState::Off => "off",
                        RouteState::On { manual: true } => "on; manual",
                        RouteState::On { manual: false } => "on",
                        RouteState::Custom { .. } => "custom; open patchbay",
                    }
                );
                let source = row.key.clone();
                let destination = cell.destination.clone();
                let binding = bind.clone();
                let b = self.button(
                    Key::Route(source.clone(), destination.clone()),
                    &text,
                    enabled && connected && (custom || ready.is_ok()),
                    move |s| {
                        let result = s.model.borrow_mut().route(
                            binding.as_ref().ok_or(Error::NoSnapshot)?,
                            &source,
                            &destination,
                        );
                        s.send(result);
                        Ok(())
                    },
                );
                set_name(&b, &format!("{} to {}: {}", row.name, cell.name, text));
                if let RouteState::Custom { explanation } = &cell.state {
                    b.set_tooltip_text(Some(explanation));
                    body.append(&label(explanation));
                }
                actions.append(&b);
            }
            body.append(&actions);
            if !row.elsewhere.is_empty() {
                body.append(&label(&format!("also → {}", row.elsewhere.join(", "))));
            }
            if !row.other_recs.is_empty() {
                body.append(&label(&format!(
                    "also routed to {}",
                    row.other_recs.join(", ")
                )));
            }
            frame.set_child(Some(&body));
            self.rows.append(&frame);
        }
        let previously = self.was_confirmation.replace(confirming.is_some());
        let opened = confirming.is_some() && !previously;
        if !self.input.blocked() {
            let restore = if opened { Some(Key::Cancel) } else { focus };
            if let Some((_, b)) = self
                .buttons
                .borrow()
                .iter()
                .find(|(k, b)| Some(k) == restore.as_ref() && b.is_sensitive())
            {
                b.grab_focus();
            }
        }
    }
    fn button(
        self: &Rc<Self>,
        key: Key,
        text: &str,
        enabled: bool,
        action: impl Fn(&Rc<Self>) -> Result<(), Error> + 'static,
    ) -> gtk::Button {
        let b = gtk::Button::builder()
            .child(&label(text))
            .sensitive(enabled)
            .build();
        set_name(&b, text);
        let live = Rc::new(Cell::new(true));
        self.input.capture(&b, live.clone());
        let epoch = self.input.epoch();
        let scope = self.model.borrow().scope.clone();
        let weak = Rc::downgrade(self);
        let presentation = self.presentation.borrow().clone();
        b.connect_clicked(move |_| {
            let Some(s) = weak.upgrade() else { return };
            if !live.get()
                || s.input.blocked()
                || !Rc::ptr_eq(&presentation, &s.presentation.borrow())
                || !Rc::ptr_eq(&epoch, &s.input.epoch())
                || !Rc::ptr_eq(&scope, &s.model.borrow().scope)
                || s.offered.borrow().is_some()
            {
                return;
            }
            *s.last_error.borrow_mut() = None;
            if let Err(error) = action(&s) {
                *s.last_error.borrow_mut() = Some(error.to_string());
                s.render();
            }
        });
        self.buttons.borrow_mut().push((key, b.clone()));
        b
    }
    fn send(self: &Rc<Self>, result: Result<Event<O>, Error>) {
        let event = match result {
            Ok(e) => e,
            Err(e) => {
                *self.last_error.borrow_mut() = Some(e.to_string());
                self.render();
                return;
            }
        };
        let token = if let Event::History(intent) = &event {
            Some(intent.request.clone())
        } else {
            None
        };
        let callback = self.callback.borrow().clone();
        let Some(callback) = callback else {
            if let Some(token) = token {
                self.model
                    .borrow_mut()
                    .handoff_failed(&token, "Routing adapter not attached".into());
            }
            self.render();
            return;
        };
        self.render();
        if let Err(error) = callback(event) {
            if let Some(token) = token {
                self.model
                    .borrow_mut()
                    .handoff_failed(&token, error.clone());
            }
            *self.last_error.borrow_mut() = Some(error);
        }
        self.render();
    }
}
fn label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .max_width_chars(48)
        .build()
}
fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "x11-input-tests"))]
mod xtest;
