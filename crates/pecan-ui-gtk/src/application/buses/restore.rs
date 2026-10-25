//! Bounded automatic idle restore: existing channel count only, once per epoch.
//! The original witness is retained; local completion needs a later inspection
//! and accepted presentation. No settings writeback or graph history is emitted.
use super::*;

#[derive(Default)]
pub(super) struct State {
    attempted: Option<ConfigRevision>,
    pub(super) reports: VecDeque<ConfigReport>,
    pub(super) retired_completed: VecDeque<ConfigWork>,
}
pub(super) struct Work {
    binding: levels::Binding,
    desired: IdleBusConfig,
    last: Option<ConfigReport>,
    admission_error: Option<String>,
    pub(super) released: bool,
}
impl BusOwner {
    fn restore_message(&self, message: &str) {
        self.restore_status.set_text(message);
        self.restore_status.set_visible(true);
    }
    pub(crate) fn restore_preferences(
        &self,
        preferences: &super::super::preferences::PreferencesState,
    ) {
        let Some(service) = self.service.as_ref().filter(|s| s.supports_restore()) else {
            return;
        };
        if self.session.admission_closed() {
            return;
        }
        let Some(mut buses) = preferences.restorable_buses() else {
            self.restore_message("Saved Rec restore waits for valid initial preferences; protected input is never applied.");
            return;
        };
        // Stored MIDI/clock preferences survive in the document. Native
        // execution remains explicitly disabled and never starts recording.
        buses.midi = false;
        let desired = IdleBusConfig {
            buses,
            clock: None,
            start_stop: false,
        };
        let Some(origin) = self.origin() else { return };
        let Some(snapshot) = origin.0.config.as_ref().filter(|c| c.connected) else {
            return;
        };
        let revision = snapshot.revision;
        if self
            .restore
            .borrow()
            .attempted
            .is_some_and(|r| r.owner != revision.owner || r.epoch >= revision.epoch)
        {
            return;
        }
        if !origin.0.connected {
            return;
        }
        // Seeing activity in this epoch must not defer a settings effect until
        // the take ends. Preserve the existing skip policy before retirement.
        if self.recording_blocks_restore() {
            return;
        }
        let retired = self.retire_completed_for_reconnect(&origin);
        // Retirement calls injected receipt/service readers. Preserve recording
        // observed by those callbacks now, not on an optional later clock tick.
        if self.recording_blocks_restore() || retired {
            return; // Obtain a newly published origin/latest preferences next tick.
        }
        if self.data.borrow().operation.as_ref().is_some_and(|w| {
            w.expected.owner == revision.owner && w.expected.epoch < revision.epoch
        }) {
            // Old pending/uncertain work remains owned. Waiting for actual
            // closure is not an automatic attempt in the new epoch.
            return;
        }
        if self.validate(&origin).is_err() {
            return;
        }
        // Do not turn a delayed initial load into a later override of a user
        // operation, or apply/reload preferences after an active take ends.
        let superseded = {
            let d = self.data.borrow();
            d.operation.as_ref().is_some_and(|w| {
                w.restore.as_ref().is_none_or(|r| !r.released)
                    || !matches!(
                        w.progress,
                        ui::Progress::AppliedLocally | ui::Progress::NoOpLocally
                    )
            }) || d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| !t.status.complete)
                || snapshot.recording
        };
        if superseded {
            self.restore.borrow_mut().attempted = Some(revision);
            self.restore_message("Automatic Rec restore skipped for this epoch: a user operation or recording already owns the configuration.");
            return;
        }
        if self.idle().is_err() {
            return;
        }
        let Some(witness) = origin.0.witness.as_ref() else {
            return;
        };
        let Ok(binding) = levels::Binding::capture(witness, snapshot) else {
            self.restore_message("Saved Rec restore waits for complete authenticated Rec inputs.");
            return;
        };
        let desired_channels: usize = desired.buses.widths.iter().sum();
        let layout_changed = snapshot
            .configuration
            .as_ref()
            .is_none_or(|c| c.buses.widths != desired.buses.widths);
        // Other application instances can publish identically named Rec nodes.
        // Only cables touching the captured local filter constrain its layout.
        let owned_routed = snapshot
            .local
            .as_ref()
            .and_then(|l| l.node)
            .is_some_and(|node| {
                witness
                    .graph()
                    .value
                    .kabels
                    .iter()
                    .any(|c| c.in_knoop == node || c.uit_knoop == node)
            });
        if snapshot
            .local
            .as_ref()
            .is_none_or(|l| l.channels != desired_channels)
            || layout_changed && owned_routed
        {
            self.restore.borrow_mut().attempted = Some(revision);
            self.restore_message("Saved Rec layout needs a separate channel-count or routed-layout migration; automatic restore was not sent.");
            return;
        }
        if desired.buses.backend_config().is_err() {
            self.restore.borrow_mut().attempted = Some(revision);
            self.restore_message(
                "Saved Rec configuration is invalid; automatic restore was not sent.",
            );
            return;
        }
        let Ok((ticket, lease)) = self.session.reserve_external() else {
            return;
        };
        if self.validate(&origin).is_err() {
            self.session.release_external(&lease);
            return;
        }
        self.restore.borrow_mut().attempted = Some(revision);
        let request = Request(Rc::new(()));
        self.data.borrow_mut().operation = Some(ConfigWork {
            restore: Some(Work {
                binding,
                desired: desired.clone(),
                last: None,
                admission_error: None,
                released: false,
            }),
            levels: None,
            migration: None,
            arming: None,
            request: request.clone(),
            ticket,
            expected: revision,
            lease: lease.clone(),
            receipt: None,
            progress: ui::Progress::Queued,
            terminal: false,
            completion: None,
            reconcile_requested: false,
        });
        self.restore_message(
            "Restoring saved Rec preferences; waiting for local completion and fresh presentation.",
        );
        // No borrowed UI state across injected/native admission; use only the
        // original authenticated witness, with no weaker API or recapture.
        match service.configure_from(
            witness,
            IdleConfigIntent {
                ticket,
                expected: revision,
                configuration: desired,
            },
        ) {
            Ok(receipt) => {
                if let Some(w) = self
                    .data
                    .borrow_mut()
                    .operation
                    .as_mut()
                    .filter(|w| w.request == request)
                {
                    w.receipt = Some(receipt);
                }
            }
            Err(error) => {
                if let Some(w) = self
                    .data
                    .borrow_mut()
                    .operation
                    .as_mut()
                    .filter(|w| w.request == request)
                {
                    w.restore.as_mut().unwrap().admission_error = Some(error.to_string());
                }
                self.restore_message(&format!(
                    "Saved Rec restore admission rejected: {error}. No automatic retry."
                ));
            }
        }
        self.publish();
    }
    fn skip_recording_restore_epoch(&self, revision: ConfigRevision) {
        let mut state = self.restore.borrow_mut();
        if state
            .attempted
            .is_none_or(|old| old.owner == revision.owner && old.epoch <= revision.epoch)
        {
            state.attempted = Some(revision);
        }
        drop(state);
        self.restore_message("Automatic Rec restore skipped for this epoch: recording ownership is active or unresolved.");
    }
    fn recording_blocks_restore(&self) -> bool {
        let (occupied, scope) = {
            let d = self.data.borrow();
            let occupied = d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| !t.status.complete)
                || d.config.as_ref().is_some_and(|c| c.recording);
            let scope = d
                .config
                .as_ref()
                .filter(|c| {
                    d.current.as_ref().is_some_and(|o| {
                        o.0.connected
                            && o.0.config.as_ref().is_some_and(|shown| {
                                shown.revision.owner == c.revision.owner
                                    && shown.revision.epoch == c.revision.epoch
                            })
                    })
                })
                .map(|c| c.revision);
            (occupied, scope)
        };
        if occupied {
            if let Some(revision) = scope {
                self.skip_recording_restore_epoch(revision);
            }
        }
        occupied
    }
    fn retire_completed_for_reconnect(&self, origin: &Origin) -> bool {
        if self.session.busy()
            || self.validate_selection(origin).is_err()
            || self.view.awaited_request().is_some()
        {
            return false;
        }
        let Some(snapshot) = origin
            .0
            .config
            .as_ref()
            .filter(|c| c.connected && !c.recording)
        else {
            return false;
        };
        let Some(frame) = origin.0.witness.as_ref() else {
            return false;
        };
        if !origin.0.connected || levels::Binding::capture(frame, snapshot).is_err() {
            return false;
        }
        let candidate = {
            let d = self.data.borrow();
            if d.protocol_invalid
                || d.unknown_owned
                || !d.orphans.is_empty()
                || d.takes.iter().any(|t| !t.status.complete)
            {
                return false;
            }
            d.operation
                .as_ref()
                .filter(|w| closed_user_work(w, snapshot.revision))
                .map(|w| {
                    (
                        w.request.clone(),
                        w.ticket,
                        w.expected,
                        (w.restore.is_none()
                            && w.levels.is_none()
                            && w.migration.is_none()
                            && w.arming.is_none())
                        .then(|| w.receipt.clone())
                        .flatten(),
                    )
                })
        };
        let Some((request, ticket, expected, receipt)) = candidate else {
            return false;
        };
        // Generic configuration has no frozen report field. Inspect its original
        // receipt outside the UI borrow; a reentrant callback may replace the
        // presentation. Specialized protocols already retain their closed report.
        if let Some(receipt) = receipt {
            if !receipt
                .report()
                .as_ref()
                .is_some_and(|r| closed_config(r, ticket, expected))
            {
                return false;
            }
        } else if self.data.borrow().operation.as_ref().is_some_and(|w| {
            w.restore.is_none() && w.levels.is_none() && w.migration.is_none() && w.arming.is_none()
        }) {
            return false;
        }
        // A cached new-epoch snapshot is not a successful reconciliation read.
        // This is the existing read-only service, never another inbox consumer.
        let Some(fresh) = self.service.as_ref().and_then(|s| s.read()) else {
            return false;
        };
        if fresh.config.recording
            && fresh.config.revision.owner == snapshot.revision.owner
            && fresh.config.revision.epoch == snapshot.revision.epoch
        {
            self.skip_recording_restore_epoch(fresh.config.revision);
            return false;
        }
        if fresh.config != *snapshot || Some(fresh.revision) != origin.0.membership {
            return false;
        }
        if self.session.busy() || self.validate_selection(origin).is_err() {
            return false;
        }
        let mut d = self.data.borrow_mut();
        if d.protocol_invalid
            || d.unknown_owned
            || !d.orphans.is_empty()
            || d.takes.iter().any(|t| !t.status.complete)
            || !d
                .operation
                .as_ref()
                .is_some_and(|w| w.request == request && closed_user_work(w, snapshot.revision))
        {
            return false;
        }
        Self::remember_arming(&mut d);
        Self::remember_migration(&mut d);
        Self::remember_levels(&mut d);
        // Archive the complete original operation/receipt, not a rewritten
        // success or a new-epoch copy. Only proven closed successes enter here.
        let retired = d.operation.take().unwrap();
        d.publication = None;
        drop(d);
        let mut state = self.restore.borrow_mut();
        state.retired_completed.push_back(retired);
        while state.retired_completed.len() > 16 {
            state.retired_completed.pop_front();
        }
        drop(state);
        self.restore_message("Prior completed Rec work retained; preparing restoration from current-epoch authority and current preferences.");
        self.publish();
        true
    }
    pub(super) fn poll_restore(&self, inspected: Option<&ConfigSnapshot>) {
        let pending = {
            let d = self.data.borrow();
            d.operation.as_ref().and_then(|w| {
                let r = w.restore.as_ref().filter(|r| !r.released && !w.terminal)?;
                Some((
                    w.request.clone(),
                    w.ticket,
                    w.expected,
                    w.receipt.clone(),
                    r.last.as_ref().is_some_and(|r| {
                        matches!(
                            r.progress,
                            ConfigProgress::Completed(_) | ConfigProgress::Unconfirmed
                        )
                    }) || r.admission_error.is_some(),
                ))
            })
        };
        let Some((request, ticket, expected, receipt, terminal_before_inspection)) = pending else {
            return;
        };
        // Injected receipt polling may reenter the owner; no mutable UI borrow.
        let report = receipt.as_ref().and_then(|r| r.report());
        let frame = self.session.witnessed_graph();
        let mut d = self.data.borrow_mut();
        let Some(w) = d
            .operation
            .as_mut()
            .filter(|w| w.request == request && !w.terminal)
        else {
            return;
        };
        if let Some(report) = report {
            if report.ticket != ticket || report.expected != expected {
                d.protocol_invalid = true;
                self.restore_message("Saved Rec receipt identity changed; original lease retained until finalization.");
                return;
            }
            w.restore.as_mut().unwrap().last = Some(report);
        }
        let r = w.restore.as_ref().unwrap();
        if !terminal_before_inspection {
            w.progress = match r.last.as_ref().map(|r| &r.progress) {
                Some(ConfigProgress::Applying) => ui::Progress::Applying,
                _ => ui::Progress::Queued,
            };
            return;
        }
        // This inspection ran after the previous terminal delivery, including
        // same-revision NoOp/rejection. Cached pre-completion data never suffices.
        let Some(fresh) = inspected.filter(|c| c.revision.owner == expected.owner) else {
            return;
        };
        let original_identity = frame.as_ref().is_some_and(|f| r.binding.matches(f));
        let progress = if let Some(error) = &r.admission_error {
            ui::Progress::Rejected(error.clone())
        } else {
            let Some(report) = r.last.as_ref() else {
                return;
            };
            match &report.progress {
                ConfigProgress::Unconfirmed => ui::Progress::Unconfirmed(
                    "Saved Rec local completion is unconfirmed; no automatic replay.".into(),
                ),
                ConfigProgress::Completed(completed) => match &completed.outcome {
                    LocalConfigOutcome::Rejected(error) => {
                        ui::Progress::Rejected(format!("{error:?}"))
                    }
                    LocalConfigOutcome::PartiallyApplied(error) => {
                        ui::Progress::PartiallyApplied(format!("{error:?}"))
                    }
                    outcome => {
                        if original_identity
                            && fresh.connected
                            && fresh.revision.epoch == expected.epoch
                            && !revision_covers(fresh.revision, completed.snapshot.revision)
                        {
                            return;
                        }
                        if !original_identity
                            || !fresh.connected
                            || fresh.revision.epoch != expected.epoch
                            || fresh.configuration.as_ref() != Some(&r.desired)
                            || completed.snapshot.configuration.as_ref() != Some(&r.desired)
                            || fresh.local != completed.snapshot.local
                            || fresh.transport != completed.snapshot.transport
                        {
                            ui::Progress::Unconfirmed("Saved Rec completion was superseded or lost original authority; no automatic replay.".into())
                        } else if *outcome == LocalConfigOutcome::Applied {
                            ui::Progress::AppliedLocally
                        } else {
                            ui::Progress::NoOpLocally
                        }
                    }
                },
                _ => return,
            }
        };
        w.progress = progress;
        w.completion = Some(fresh.clone());
        w.terminal = true;
    }
    pub(super) fn finish_restore_publication(&self) -> bool {
        let pending = {
            let d = self.data.borrow();
            d.operation.as_ref().filter(|w| w.terminal).and_then(|w| {
                w.restore
                    .as_ref()
                    .filter(|r| !r.released)
                    .map(|_| (w.request.clone(), w.progress.clone(), w.lease.clone()))
            })
        };
        let Some((request, progress, lease)) = pending else {
            return false;
        };
        if self.view.awaited_request().is_some()
            || !self
                .view
                .inspect_model(|m| {
                    m.snapshot()
                        .and_then(|s| s.operation.as_ref())
                        .is_some_and(|o| o.request == request && o.progress == progress)
                })
                .unwrap_or(false)
        {
            return false;
        }
        let mut d = self.data.borrow_mut();
        let Some(w) = d.operation.as_mut().filter(|w| w.request == request) else {
            return false;
        };
        let r = w.restore.as_mut().unwrap();
        r.released = true;
        if let Some(report) = r.last.clone() {
            let mut state = self.restore.borrow_mut();
            state.reports.push_back(report);
            while state.reports.len() > 16 {
                state.reports.pop_front();
            }
        }
        if matches!(
            progress,
            ui::Progress::PartiallyApplied(_) | ui::Progress::Unconfirmed(_)
        ) {
            if d.past_failures.len() < 32 {
                d.past_failures
                    .push(format!("Saved Rec restore: {progress:?}"));
            }
        }
        drop(d);
        self.restore_message(&match &progress {
            ui::Progress::AppliedLocally | ui::Progress::NoOpLocally => "Saved Rec preferences match fresh local readback and authenticated input presentation; this is not an audio/server acknowledgement.".into(),
            _ => format!("Saved Rec restore retained {progress:?}; no automatic retry or rollback. Refresh inspects current state."),
        });
        self.session.release_external(&lease);
        true
    }
}

// Successful native completion is local closure, not server convergence. A
// cancelled/rejected/partial/unconfirmed operation still needs explicit recovery.
pub(super) fn closed_config(report: &ConfigReport, ticket: u64, expected: ConfigRevision) -> bool {
    report.ticket == ticket
        && report.expected == expected
        && matches!(&report.progress, ConfigProgress::Completed(c)
            if matches!(c.outcome, LocalConfigOutcome::Applied | LocalConfigOutcome::NoOp)
                && !c.cancellation_requested
                && c.snapshot.connected && c.snapshot.configuration.is_some()
                && c.snapshot.revision.owner == expected.owner
                && c.snapshot.revision.epoch == expected.epoch
                && c.resize.as_ref().is_none_or(|r| r.is_complete()))
}
fn closed_user_work(work: &ConfigWork, current: ConfigRevision) -> bool {
    work.terminal
        && work.expected.owner == current.owner
        && work.expected.epoch < current.epoch
        && matches!(
            work.progress,
            ui::Progress::AppliedLocally | ui::Progress::NoOpLocally | ui::Progress::Observed
        )
        && work.completion.as_ref().is_some_and(|c| {
            c.revision.owner == current.owner && c.revision.epoch == work.expected.epoch
        })
        && work.restore.as_ref().is_none_or(|r| {
            r.released
                && r.last
                    .as_ref()
                    .is_some_and(|r| closed_config(r, work.ticket, work.expected))
        })
        && work
            .levels
            .as_ref()
            .is_none_or(|l| l.closed_for_reconnect(work.ticket, work.expected))
        && work
            .migration
            .as_ref()
            .is_none_or(|m| m.closed_for_reconnect(work.ticket, work.expected))
        && work
            .arming
            .as_ref()
            .is_none_or(|a| a.closed_for_reconnect(work.ticket, work.expected))
}
