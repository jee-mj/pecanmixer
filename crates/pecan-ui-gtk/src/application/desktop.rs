//! One correlated desktop request on the existing settings worker. No I/O here.
use crate::{
    desktop_launch::{EntryFormat, EntryState, Snapshot},
    settings_worker::{
        DesktopOperation, DesktopResult, SettingsEvent, SettingsOperation, SettingsWorker,
        SubmissionFailure,
    },
    tray::AutostartMenu,
};
use gtk::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Inspect,
    Enable,
    Disable,
    Refresh,
}
#[derive(Clone)]
pub(super) struct DesktopState {
    available: bool,
    source: u64,
    pending: Option<u64>,
    snapshot: Option<Snapshot>,
    inspect_needed: bool,
    error: Option<String>,
    stopped: bool,
}
impl DesktopState {
    pub fn new(available: bool) -> Self {
        Self {
            available,
            source: 0,
            pending: None,
            snapshot: None,
            inspect_needed: available,
            error: None,
            stopped: false,
        }
    }
    fn writable(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| {
            matches!(
                s.state(),
                EntryState::Missing | EntryState::Managed(EntryFormat::Native)
            )
        })
    }
    pub fn request(
        &mut self,
        source: u64,
        action: Action,
        worker: &mut SettingsWorker,
    ) -> Result<(), String> {
        if !self.available || self.stopped || self.pending.is_some() || source != self.source {
            return Err("Desktop controls changed, are pending or unavailable; use the current presentation.".into());
        }
        let operation = match action {
            Action::Inspect => DesktopOperation::Inspect,
            Action::Enable | Action::Disable | Action::Refresh if self.writable() => {
                let snapshot = self.snapshot.as_ref().unwrap().clone();
                match action {
                    Action::Enable => DesktopOperation::Enable(snapshot),
                    Action::Disable => DesktopOperation::Disable(snapshot),
                    _ => DesktopOperation::Refresh(snapshot),
                }
            }
            _ => return Err("Reload and inspect the preserved entry before changing it.".into()),
        };
        match worker.submit(SettingsOperation::Desktop(operation)) {
            Ok(request) => {
                self.source = request;
                self.pending = Some(request);
                self.inspect_needed = false;
                self.error = None;
                Ok(())
            }
            Err(error) => {
                if error.reason != SubmissionFailure::QueueFull {
                    self.stopped = true;
                }
                let message = format!("Desktop request was not admitted: {:?}.", error.reason);
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }
    pub fn pump(&mut self, worker: &mut SettingsWorker) {
        if self.inspect_needed && self.pending.is_none() && !self.stopped {
            let _ = self.request(self.source, Action::Inspect, worker);
        }
    }
    pub fn event(&mut self, event: &SettingsEvent) {
        if let SettingsEvent::Desktop { request, result } = event {
            self.result(*request, result);
        }
    }
    pub fn result(&mut self, request: u64, result: &DesktopResult) {
        if self.pending != Some(request) {
            return;
        }
        self.pending = None;
        match result {
            Ok(reply) => {
                self.snapshot = Some(reply.snapshot.clone());
                self.error = None;
            }
            Err(error) => {
                self.snapshot = None;
                self.error = Some(format!("{error}. Reload before changing the entry."));
            }
        }
    }
    pub fn stopped(&mut self) {
        self.stopped = true;
    }
    pub fn menu(&self, allowed: bool) -> Option<AutostartMenu> {
        self.available.then(|| AutostartMenu {
            source: self.source,
            checked: if self.pending.is_some() {
                None
            } else {
                self.snapshot.as_ref().and_then(|s| match s.state() {
                    EntryState::Missing => Some(false),
                    EntryState::Managed(EntryFormat::Native) => Some(true),
                    _ => None,
                })
            },
            enabled: self.source <= (u64::MAX >> 2)
                && allowed
                && !self.stopped
                && self.pending.is_none()
                && self.writable(),
            status: self.message(),
        })
    }
    pub fn message(&self) -> String {
        if !self.available {
            return "Native login controls require explicit config-root and native-executable context.".into();
        }
        if self.pending.is_some() {
            return "Native login entry request pending; the file result is not yet known.".into();
        }
        if let Some(error) = &self.error {
            return error.clone();
        }
        if self.stopped {
            return "Desktop worker joined/stopped; retained entry state is historical.".into();
        }
        match self.snapshot.as_ref().map(Snapshot::state) {
            None => "Inspecting explicit personal native login entry…".into(),
            Some(EntryState::Missing) => "No native entry at the explicit personal location. Legacy and system entries are not inspected.".into(),
            Some(EntryState::Managed(EntryFormat::Native)) => "Recognized native personal entry. Login launch and desktop acceptance are untested.".into(),
            _ => "Protected legacy/custom entry at the native location; no replacement or removal is allowed.".into(),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct View {
    menu: Option<AutostartMenu>,
    reload: bool,
    status: String,
}
pub(super) struct DesktopPanel {
    root: gtk::Box,
    shown: std::cell::RefCell<Option<View>>,
}
impl DesktopPanel {
    pub fn new() -> Self {
        Self {
            root: gtk::Box::new(gtk::Orientation::Vertical, 4),
            shown: std::cell::RefCell::default(),
        }
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    pub fn render(
        &self,
        state: &DesktopState,
        allowed: bool,
        callback: std::rc::Rc<dyn Fn(u64, Action)>,
    ) {
        let view = View {
            menu: state.menu(allowed),
            reload: allowed && state.available && !state.stopped && state.pending.is_none(),
            status: state.message(),
        };
        if self.shown.borrow().as_ref() == Some(&view) {
            return;
        }
        self.root.set_visible(state.available);
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        let label = gtk::Label::builder()
            .label(&view.status)
            .wrap(true)
            .xalign(0.)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        self.root.append(&label);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let mutable = view.menu.as_ref().is_some_and(|m| m.enabled);
        for (text, action, enabled) in [
            ("Reload native login entry", Action::Inspect, view.reload),
            ("Enable/update native login", Action::Enable, mutable),
            ("Disable native login", Action::Disable, mutable),
            ("Refresh native executable", Action::Refresh, mutable),
        ] {
            let button = gtk::Button::with_label(text);
            button.set_sensitive(enabled);
            let callback = callback.clone();
            let source = state.source;
            button.connect_clicked(move |b| {
                if b.is_sensitive() {
                    callback(source, action);
                }
            });
            row.append(&button);
        }
        self.root.append(&row);
        *self.shown.borrow_mut() = Some(view);
    }
}

#[cfg(test)]
mod tests;
