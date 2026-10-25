//! One preferences document, request-aware disk baseline, and one coalesced
//! unsent edit. No filesystem, GTK, path discovery or audio effects here.
mod rec;
use crate::{
    settings_storage::LoadedSettings,
    settings_worker::{
        SettingsEvent, SettingsOperation, SettingsWorker, SubmissionError, SubmissionFailure,
    },
};
use pecan_core::settings::{NativeLoadStatus, SettingsV1};
pub(crate) use rec::RecEdit;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
enum Request {
    Save(u64),
    Reload,
    Recover(u64),
}
#[derive(Clone, Debug)]
pub struct PreferencesState {
    document: SettingsV1,
    loaded: Option<LoadedSettings>,
    ready: bool,
    restore_allowed: bool,
    persistent: bool,
    edit: u64,
    view: u64,
    dirty: bool,
    baseline_request: u64,
    pending: BTreeMap<u64, Request>,
    blocked: Option<String>,
    paused: bool,
    queue_full: bool,
    draining: bool,
    closed: bool,
}
impl PreferencesState {
    pub(super) fn new(persistent: bool) -> Self {
        Self {
            document: SettingsV1::default(),
            loaded: None,
            ready: !persistent,
            restore_allowed: !persistent,
            persistent,
            edit: 0,
            view: 0,
            dirty: false,
            baseline_request: 0,
            pending: BTreeMap::new(),
            blocked: None,
            paused: false,
            queue_full: false,
            draining: false,
            closed: false,
        }
    }
    /// Automatic idle restore never consumes corrupt/future fallback defaults,
    /// pending reloads or a protected/paused disk baseline.
    pub(super) fn restorable_buses(&self) -> Option<pecan_core::domain::buses::BusConfig> {
        (self.restore_allowed
            && self.editing_allowed()
            && self.blocked.is_none()
            && !self.paused
            && !self.recovery_required()
            && !self
                .pending
                .values()
                .any(|r| matches!(r, Request::Reload | Request::Recover(_))))
        .then(|| self.document.preferences().buses.clone())
    }
    pub fn document(&self) -> &SettingsV1 {
        &self.document
    }
    pub fn revision(&self) -> u64 {
        self.edit
    }
    pub fn view_revision(&self) -> u64 {
        self.view
    }
    pub(super) fn baseline_revision(&self) -> u64 {
        self.baseline_request
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn loaded(&self) -> Option<&LoadedSettings> {
        self.loaded.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    pub fn editing_allowed(&self) -> bool {
        self.ready && !self.draining && !self.closed
    }
    pub fn recovery_required(&self) -> bool {
        self.loaded
            .as_ref()
            .is_some_and(LoadedSettings::recovery_required)
    }
    pub fn message(&self) -> String {
        if self.closed {
            return format!(
                "Settings worker joined. {}",
                if self.dirty {
                    "Local edits remain unsaved and retained."
                } else {
                    "All accepted results were drained."
                }
            );
        }
        if self.draining {
            return "Finishing settings saves…".into();
        }
        if !self.ready {
            return self
                .blocked
                .clone()
                .unwrap_or_else(|| "Loading preferences…".into());
        }
        if !self.persistent {
            return "Preferences are in memory; no settings file was supplied.".into();
        }
        if let Some(error) = &self.blocked {
            return format!(
                "Settings error: {error}. Reload before saving; local edits are retained."
            );
        }
        if self.recovery_required() {
            return format!("Settings input preserved ({:?}); review recovery before saving. Local edits stay in memory.",self.loaded.as_ref().unwrap().status());
        }
        if self.paused {
            return "Reloaded disk baseline; local edits retained. Choose Save local edits to publish them.".into();
        }
        if self.queue_full {
            return "Settings queue full; latest unsent preference edit retained.".into();
        }
        if !self.pending.is_empty() {
            return "Saving/loading preferences…".into();
        }
        if self.dirty {
            return "Preference changes waiting to save.".into();
        }
        if self
            .loaded
            .as_ref()
            .is_some_and(|s| matches!(s.status(), NativeLoadStatus::Missing))
        {
            "Settings file missing; defaults in memory until an edit is saved.".into()
        } else {
            "Preferences saved.".into()
        }
    }
    pub(super) fn edit_rec(&mut self, edit: RecEdit) -> Result<(), String> {
        let next = edit.apply(&self.document)?;
        self.edit(next)
    }
    pub(super) fn edit(&mut self, next: SettingsV1) -> Result<(), String> {
        if !self.editing_allowed() {
            return Err(
                "Wait for the initial settings load; finalization disables editing.".into(),
            );
        }
        if next != self.document {
            self.edit = self
                .edit
                .checked_add(1)
                .ok_or("Preference revision exhausted.")?;
            self.document = next;
            self.view = self.view.wrapping_add(1);
            self.dirty = self.persistent;
        }
        Ok(())
    }
    pub(super) fn resume(&mut self) -> Result<(), String> {
        if self.draining
            || self.closed
            || !self.ready
            || self.blocked.is_some()
            || self.recovery_required()
        {
            return Err("Reload/recover settings before saving local edits.".into());
        }
        self.paused = false;
        Ok(())
    }
    pub(super) fn reload_operation(&self) -> SettingsOperation {
        SettingsOperation::Reload
    }
    pub(super) fn recover_operation(&self) -> Result<SettingsOperation, String> {
        if self.blocked.is_some() {
            return Err("Reload the current input before reviewing recovery.".into());
        }
        let from = self
            .loaded
            .as_ref()
            .filter(|s| s.recovery_required())
            .ok_or("No corrupt/future input is awaiting recovery.")?;
        Ok(SettingsOperation::Recover {
            from: from.clone(),
            settings: self.document.clone(),
        })
    }
    pub(super) fn submit(
        &mut self,
        worker: &mut SettingsWorker,
        operation: SettingsOperation,
    ) -> Result<u64, SubmissionError> {
        self.submit_with(operation, |op| worker.submit(op))
    }
    fn submit_with(
        &mut self,
        operation: SettingsOperation,
        send: impl FnOnce(SettingsOperation) -> Result<u64, SubmissionError>,
    ) -> Result<u64, SubmissionError> {
        if matches!(&operation, SettingsOperation::Desktop(_)) {
            return Err(SubmissionError {
                reason: SubmissionFailure::NotReady,
                operation,
            });
        }
        let desired = match &operation {
            SettingsOperation::Save(doc) | SettingsOperation::Recover { settings: doc, .. } => {
                Some(doc.clone())
            }
            SettingsOperation::Reload => None,
            SettingsOperation::Desktop(_) => unreachable!(),
        };
        if desired.is_some()
            && (!self.ready
                || self.closed
                || self.draining && desired.as_ref() != Some(&self.document))
        {
            return Err(SubmissionError {
                reason: SubmissionFailure::NotReady,
                operation,
            });
        }
        let changed = desired.as_ref().is_some_and(|doc| doc != &self.document);
        if changed && self.edit == u64::MAX {
            return Err(SubmissionError {
                reason: SubmissionFailure::IdExhausted,
                operation,
            });
        }
        let kind = match &operation {
            SettingsOperation::Save(_) => 0,
            SettingsOperation::Reload => 1,
            SettingsOperation::Recover { .. } => 2,
            SettingsOperation::Desktop(_) => unreachable!(),
        };
        match send(operation) {
            Ok(id) => {
                if let Some(doc) = desired {
                    if changed {
                        self.edit += 1;
                        self.view = self.view.wrapping_add(1);
                        self.document = doc;
                    }
                    self.dirty = self.persistent;
                }
                self.pending.insert(
                    id,
                    match kind {
                        0 => Request::Save(self.edit),
                        1 => Request::Reload,
                        _ => Request::Recover(self.edit),
                    },
                );
                self.queue_full = false;
                Ok(id)
            }
            Err(error) => {
                self.queue_full = error.reason == SubmissionFailure::QueueFull;
                Err(error)
            }
        }
    }
    pub(super) fn event(&mut self, event: &SettingsEvent) {
        if matches!(event, SettingsEvent::Desktop { .. }) {
            return;
        }
        let request = event.request();
        let pending = self.pending.remove(&request);
        if request < self.baseline_request {
            return;
        }
        if let SettingsEvent::Loaded { request, snapshot } = event {
            self.restore_allowed = *request == 0
                && !self.ready
                && matches!(
                    snapshot.status(),
                    NativeLoadStatus::Loaded | NativeLoadStatus::Missing
                );
        }
        if matches!(event, SettingsEvent::Recovered { .. }) {
            self.restore_allowed = false;
        }
        match event {
            SettingsEvent::Desktop { .. } => unreachable!(),
            SettingsEvent::Loaded { snapshot, .. }
            | SettingsEvent::Saved { snapshot, .. }
            | SettingsEvent::Recovered { snapshot, .. } => {
                let initial = !self.ready;
                let is_reload = matches!(event, SettingsEvent::Loaded { .. }) && !initial;
                self.baseline_request = request;
                self.loaded = Some(snapshot.clone());
                self.ready = true;
                self.queue_full = false;
                if initial || (!self.dirty && (is_reload || pending.is_none())) {
                    self.document = snapshot.settings().clone();
                    self.view = self.view.wrapping_add(1);
                }
                let acknowledged = match pending {
                    Some(Request::Save(rev) | Request::Recover(rev)) => {
                        rev == self.edit && snapshot.settings() == &self.document
                    }
                    _ => false,
                };
                if acknowledged {
                    self.dirty = false;
                }
                if matches!(event, SettingsEvent::Loaded { .. }) {
                    self.blocked = None;
                    self.paused = !initial && self.dirty;
                }
                if matches!(event, SettingsEvent::Recovered { .. }) {
                    self.blocked = None;
                    self.paused = false;
                }
            }
            SettingsEvent::Failed { error, .. } | SettingsEvent::Blocked { error, .. } => {
                self.blocked = Some(error.to_string());
                self.paused = true;
                self.queue_full = false;
            }
        }
    }
    fn can_save(&self) -> bool {
        self.ready
            && !self.closed
            && self.persistent
            && self.dirty
            && !self.paused
            && self.blocked.is_none()
            && !self.recovery_required()
    }
    pub(super) fn pump(&mut self, worker: &mut SettingsWorker) {
        if self.can_save() && self.pending.is_empty() {
            let doc = self.document.clone();
            if let Err(error) = self.submit(worker, SettingsOperation::Save(doc)) {
                if error.reason != SubmissionFailure::QueueFull {
                    self.blocked = Some(format!("Settings admission stopped: {:?}", error.reason));
                    self.paused = true;
                }
            }
        }
    }
    pub(super) fn stopped(&mut self) {
        self.pending.clear();
        self.blocked = Some("Settings worker stopped before all matching results arrived.".into());
        self.paused = true;
    }
    pub(super) fn finish_drain(&mut self) {
        self.draining = false;
        self.closed = true;
    }
    pub(super) fn begin_drain(&mut self) {
        self.draining = true;
    }
    pub(super) fn needs_final_admission(&self) -> bool {
        self.can_save()
    }
    pub(super) fn unsaved_error(&self) -> Option<String> {
        self.dirty.then(|| {
            format!(
                "Final preference edits were not saved; local document retained. {}",
                self.blocked.clone().unwrap_or_else(|| {
                    "Explicit reload/recovery or save confirmation is required.".into()
                })
            )
        })
    }
}
#[cfg(test)]
mod tests;
