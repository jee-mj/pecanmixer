use super::{history_input::HistoryInput, Graph, Session, Status};
use crate::PatchbayOverview;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
struct HistoryButtons {
    context: Rc<()>,
    input_epoch: Rc<()>,
    undo: gtk::Button,
    redo: gtk::Button,
    live: Rc<Cell<bool>>,
}

#[cfg(test)]
#[path = "telemetry/tests.rs"]
mod telemetry_tests;
/// Explicit workspace integration for the owning shell. No worker, timer,
/// file access, recording, tray or automatic application startup is created.
/// Keep one workspace for a window; repeated GApplication activation should
/// present that window, while reusing ApplicationOwner for its whole lifetime.
pub struct NativeWorkspace {
    root: gtk::Box,
    mixer: super::mixer::CoordinatedMixer,
    patchbay: RefCell<PatchbayOverview>,
    telemetry: RefCell<super::telemetry::Telemetry>,
    pages: gtk::Stack,
    buses: RefCell<Rc<super::BusOwner>>,
    #[cfg(test)]
    recording_page: gtk::Box,
    folders: RefCell<Rc<super::folders::Folders>>,
    now: Cell<std::time::Duration>,
    routes: Rc<super::routes::Routes>,
    session: Session,
    history_queue: super::history_queue::HistoryQueue,
    history_recovery: RefCell<Option<(super::Request, gtk::Button)>>,
    history: gtk::Box,
    buttons: RefCell<Option<HistoryButtons>>,
    history_input: Rc<HistoryInput>,
    _history_shortcuts: Rc<super::history_shortcuts::HistoryShortcuts>,
    status: gtk::Label,
    error: RefCell<Option<String>>,
    patch_gate: RefCell<Option<(bool, Option<String>)>>,
    weak: Weak<Self>,
}
impl NativeWorkspace {
    pub fn new(session: Session) -> Rc<Self> {
        Self::new_with_native(session, None)
    }
    pub(crate) fn new_with_native(
        session: Session,
        native: Option<Rc<pecan_pipewire::graaf::Graaf>>,
    ) -> Rc<Self> {
        let mixer = super::mixer::CoordinatedMixer::new(session.clone());
        let io = native.as_ref().map(|_| {
            std::sync::Arc::new(super::folders::NativeIo) as std::sync::Arc<dyn super::folders::Io>
        });
        let buses = super::BusOwner::native(session.clone(), native);
        let folders = super::folders::Folders::new(buses.clone(), io);
        let recording_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        recording_page.append(folders.widget());
        recording_page.append(buses.widget());
        let routes = super::routes::Routes::new(session.clone(), buses.clone());
        let patchbay = PatchbayOverview::new();
        let shared = session.clone();
        patchbay.bind_history_actions(move |origin, input| shared.patch(origin, input));
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let history = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tools.append(&history);
        tools.append(&status);
        let pages = gtk::Stack::new();
        pages.add_titled(mixer.widget(), Some("mixer"), "Mixer");
        pages.add_titled(patchbay.widget(), Some("patchbay"), "Patchbay");
        pages.add_titled(&recording_page, Some("buses"), "Buses and recording");
        pages.add_titled(routes.widget(), Some("routing"), "Routing");
        pages.set_hexpand(true);
        pages.set_vexpand(true);
        let switcher = gtk::StackSwitcher::builder().stack(&pages).build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.append(&tools);
        root.append(&switcher);
        root.append(&pages);
        let history_input = HistoryInput::new();
        history_input.observe(&root);
        let view = Rc::new_cyclic(|weak: &Weak<Self>| {
            let action = weak.clone();
            let shortcuts = super::history_shortcuts::HistoryShortcuts::new(&root, move |undo| {
                if let Some(view) = action.upgrade() {
                    view.history_shortcut(undo);
                }
            });
            Self {
                root,
                mixer,
                patchbay: RefCell::new(patchbay),
                telemetry: RefCell::default(),
                pages,
                buses: RefCell::new(buses),
                #[cfg(test)]
                recording_page,
                folders: RefCell::new(folders),
                now: Cell::new(std::time::Duration::ZERO),
                routes,
                history_queue: super::history_queue::HistoryQueue::new(session.clone()),
                history_recovery: RefCell::default(),
                session,
                history,
                buttons: RefCell::new(None),
                history_input,
                _history_shortcuts: shortcuts,
                status,
                error: RefCell::new(None),
                patch_gate: RefCell::new(None),
                weak: weak.clone(),
            }
        });
        let weak = Rc::downgrade(&view);
        view.routes.bind_navigation(move || {
            if let Some(view) = weak.upgrade() {
                view.show_patchbay();
            }
        });
        let weak = Rc::downgrade(&view);
        view.history_input.notify(move || {
            if let Some(view) = weak.upgrade() {
                view.refresh();
            }
        });
        view.refresh();
        view
    }
    pub(crate) fn bind_display_edits(
        &self,
        callback: Rc<dyn Fn(crate::display_settings::DisplayEdit)>,
    ) {
        self.mixer.bind_display_edits(callback);
    }
    pub(crate) fn apply_display(
        &self,
        preferences: crate::display_settings::DisplayPreferences,
        editable: bool,
    ) {
        self.mixer.set_display_preferences(preferences);
        self.mixer.set_display_editable(editable);
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    pub fn show_patchbay(&self) {
        self.pages.set_visible_child_name("patchbay");
    }
    pub fn bus_owner(&self) -> Rc<super::BusOwner> {
        self.buses.borrow().clone()
    }
    #[cfg(test)]
    pub(crate) fn attach_test_buses(&self, buses: Rc<super::BusOwner>) {
        assert!(self.session.same_session(&buses.session));
        self.recording_page.remove(self.buses.borrow().widget());
        self.recording_page.remove(self.folders.borrow().widget());
        let folders = super::folders::Folders::new(buses.clone(), None);
        self.recording_page.append(folders.widget());
        self.recording_page.append(buses.widget());
        *self.folders.borrow_mut() = folders;
        self.routes.attach_test_buses(buses.clone());
        *self.buses.borrow_mut() = buses;
    }
    /// Drive from one main-context source. Receipts need polling even if there
    /// are no graph events. The lifetime adapter owns/removes that source; this
    /// view creates none, preventing per-activation consumer/timer duplication.
    pub fn poll(&self) {
        self.poll_at(self.now.get());
    }
    pub(crate) fn poll_at(&self, now: std::time::Duration) {
        self.now.set(self.now.get().max(now));
        let batch = self.session.poll();
        let buses = self.buses.borrow().clone();
        buses.poll();
        buses.tick_levels(self.now.get());
        self.routes.poll();
        self.folders.borrow().poll(self.now.get());
        self.history_queue.tick();
        self.refresh();
        let mut telemetry = self.telemetry.borrow_mut();
        telemetry.apply(&self.session, &batch);
        self.mixer.mount_meters(&telemetry.mixer_widgets());
        self.patchbay
            .borrow()
            .mount_meters(&telemetry.patch_widgets());
        telemetry.visibility(
            self.mixer.widget().upcast_ref(),
            self.patchbay.borrow().widget().upcast_ref(),
        );
    }
    fn history_shortcut(&self, undo: bool) {
        // GTK/JS keydowns are direction intents (even at an empty stack), but
        // a stale rendered context or a held button cannot become fresh input.
        if self.history_input.held() || self.history_input.blocked() {
            return;
        }
        let current = self.buttons.borrow().as_ref().is_some_and(|buttons| {
            buttons.live.get() && Rc::ptr_eq(&buttons.context, &self.session.history_context())
        });
        if current {
            if let Some(graph) = self.session.graph() {
                *self.error.borrow_mut() = self.history_queue.enqueue(&graph, undo).err();
            }
        }
        self.refresh();
    }
    fn history_button(
        &self,
        undo: bool,
        label: Option<&str>,
        origin: Option<Graph>,
        context: Rc<()>,
        live: Rc<Cell<bool>>,
    ) -> gtk::Button {
        let verb = if undo { "Undo" } else { "Redo" };
        let button =
            gtk::Button::with_label(&label.map_or_else(|| verb.into(), |s| format!("{verb} {s}")));
        button.set_tooltip_text(Some(if undo {
            "Undo (Ctrl/Meta+Z outside input fields)"
        } else {
            "Redo (Ctrl/Meta+Shift+Z or Ctrl/Meta+Y outside input fields)"
        }));
        let input_live = Rc::new(Cell::new(true));
        self.history_input.capture(&button, input_live.clone());
        let weak = self.weak.clone();
        button.connect_clicked(move |button| {
            let Some(view) = weak.upgrade() else { return };
            // Immutable per-presentation origin and history authority. Neither
            // delayed GTK activation nor a retired callback may adopt a new top.
            if !live.get()
                || !input_live.get()
                || !button.is_sensitive()
                || !Rc::ptr_eq(&context, &view.session.history_context())
            {
                *view.error.borrow_mut() =
                    Some("History changed; start a fresh activation.".into());
            } else if let Some(origin) = &origin {
                let result = view.history_queue.enqueue(origin, undo);
                *view.error.borrow_mut() = result.err();
            }
            view.refresh();
        });
        button
    }
    fn refresh_history(&self) {
        let context = self.session.history_context();
        let input_epoch = self.history_input.epoch();
        let replace = self.buttons.borrow().as_ref().is_none_or(|b| {
            !b.live.get()
                || !Rc::ptr_eq(&b.context, &context)
                || !Rc::ptr_eq(&b.input_epoch, &input_epoch)
        });
        let (undo_label, redo_label) = self.session.history_labels();
        if replace {
            self.history_input.retire();
            let old = self.buttons.borrow_mut().take();
            let focus = old.as_ref().and_then(|b| {
                if b.undo.has_focus() {
                    Some(true)
                } else if b.redo.has_focus() {
                    Some(false)
                } else {
                    None
                }
            });
            if let Some(old) = old {
                old.live.set(false);
                self.history.remove(&old.undo);
                self.history.remove(&old.redo);
            }
            let live = Rc::new(Cell::new(true));
            let origin = self.session.graph();
            let undo = self.history_button(
                true,
                undo_label.as_deref(),
                origin.clone(),
                context.clone(),
                live.clone(),
            );
            let redo = self.history_button(
                false,
                redo_label.as_deref(),
                origin,
                context.clone(),
                live.clone(),
            );
            let enabled = self
                .session
                .graph()
                .as_ref()
                .is_some_and(|g| self.history_queue.accepting(g))
                && !self.history_input.blocked();
            undo.set_sensitive(enabled && (undo_label.is_some() || self.history_queue.has_work()));
            redo.set_sensitive(enabled && (redo_label.is_some() || self.history_queue.has_work()));
            self.history.append(&undo);
            self.history.append(&redo);
            if !self.history_input.held() {
                match focus {
                    Some(true) if undo.is_sensitive() => {
                        undo.grab_focus();
                    }
                    Some(false) if redo.is_sensitive() => {
                        redo.grab_focus();
                    }
                    _ => {}
                }
            }
            *self.buttons.borrow_mut() = Some(HistoryButtons {
                context,
                input_epoch: self.history_input.epoch(),
                undo,
                redo,
                live,
            });
        } else if let Some(buttons) = self.buttons.borrow().as_ref() {
            let enabled = self
                .session
                .graph()
                .as_ref()
                .is_some_and(|g| self.history_queue.accepting(g))
                && !self.history_input.blocked();
            buttons
                .undo
                .set_sensitive(enabled && (undo_label.is_some() || self.history_queue.has_work()));
            buttons
                .redo
                .set_sensitive(enabled && (redo_label.is_some() || self.history_queue.has_work()));
        }
        // One real failed request, inspected through the existing bus/config
        // authority. Retired callbacks cannot recover a newer failure.
        let recovery = self.history_queue.recovery();
        let replace = match (&*self.history_recovery.borrow(), &recovery) {
            (None, None) => false,
            (Some((old, _)), Some(new)) => !old.same_request(new),
            _ => true,
        };
        if replace {
            if let Some((_, button)) = self.history_recovery.borrow_mut().take() {
                self.history.remove(&button);
            }
            if let Some(request) = recovery {
                let button = gtk::Button::with_label("Inspect and refresh history status");
                button.set_widget_name("history-recovery");
                let weak = self.weak.clone();
                let captured = request.clone();
                button.connect_clicked(move |_| {
                    if let Some(view) = weak.upgrade() {
                        let buses = view.buses.borrow().clone();
                        *view.error.borrow_mut() =
                            view.history_queue.recover(&captured, &buses).err();
                        view.refresh();
                    }
                });
                self.history.append(&button);
                *self.history_recovery.borrow_mut() = Some((request, button));
            }
        }
    }
    fn refresh(&self) {
        if let Some(graph) = self.session.graph() {
            self.patchbay.borrow_mut().update_graph(graph);
        }
        self.mixer.poll();
        let enabled = self.session.enabled();
        let status = self.session.status();
        let error = matches!(
            status,
            Status::Failed(_) | Status::Unconfirmed(_) | Status::IdentityUnavailable
        )
        .then(|| status.message());
        let patchbay = self.patchbay.borrow();
        patchbay
            .widget()
            .set_sensitive(enabled || self.session.has_receipt());
        let gate = (self.session.has_receipt(), error);
        if self.patch_gate.borrow().as_ref() != Some(&gate) {
            patchbay.set_actions_pending(gate.0, gate.1.as_deref());
            *self.patch_gate.borrow_mut() = Some(gate);
        }
        self.refresh_history();
        self.status
            .set_text(&self.error.borrow().as_ref().map_or_else(
                || {
                    self.history_queue.message().map_or_else(
                        || status.message(),
                        |q| format!("{} · {q}", status.message()),
                    )
                },
                |e| format!("{} · {e}", status.message()),
            ));
    }
}
