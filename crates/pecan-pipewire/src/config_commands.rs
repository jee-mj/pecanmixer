//! Local recorder configuration bookkeeping on the existing command boundary.
use super::*;
use crate::bus_config::*;

impl Commands {
    pub(crate) fn config_snapshot(&self) -> Option<ConfigSnapshot> {
        self.0
            .try_lock()
            .ok()
            .map(|state| state.config.snapshot.clone())
    }
    pub(crate) fn enqueue_config(
        &self,
        intent: IdleConfigIntent,
    ) -> Result<ConfigReceipt, ConfigError> {
        let (work, receipt) = Prepared::new(intent)?;
        let mut state = self.0.try_lock().map_err(|_| ConfigError::Busy)?;
        state.config_admission(work.intent.expected)?;
        if state.pending.len() >= NORMAL_CAPACITY
            || state.pending.iter().filter(|q| q.config.is_some()).count() >= MAX_CONFIG_PENDING
        {
            return Err(ConfigError::QueueFull);
        }
        state.pending.push_back(QueuedCommand {
            command: BackendCommand {
                epoch: work.intent.expected.epoch,
                command: Opdracht::ZetBus {
                    kanalen: work.channels,
                    gain: work.gains.clone(),
                },
            },
            objects: vec![],
            native: None,
            config: Some(work),
            start: None,
            arming: None,
        });
        Ok(receipt)
    }
    pub(crate) fn refresh_config(&self, local: Option<BusReadback>, recording: bool, legacy: bool) {
        self.0
            .lock()
            .unwrap()
            .config
            .refresh(local, recording, legacy);
    }
    pub(crate) fn reject_config(&self, work: Prepared, reason: ConfigError) {
        self.0.lock().unwrap().reject_config(work, reason);
    }

    pub(crate) fn apply_config(&self, work: Prepared, target: &mut impl ConfigTarget) {
        let old = {
            let state = self.0.lock().unwrap();
            if let Err(reason) = state.config_admission(work.intent.expected) {
                state.reject_config(work, reason);
                return;
            }
            work.applying();
            state.config.snapshot.clone()
        };
        if target.config_locked() {
            self.refresh_config(target.readback(), target.recording(), false);
            self.reject_config(work, ConfigError::RecordingActive);
            return;
        }
        let Some(before) = target.readback() else {
            self.refresh_config(None, false, false);
            self.reject_config(work, ConfigError::Unavailable);
            return;
        };
        // Include local changes made outside this adapter before touching any
        // filter state, including a preceding legacy command in this FIFO.
        if old.local.as_ref() != Some(&before) {
            self.refresh_config(Some(before), false, false);
            self.reject_config(work, ConfigError::StaleRevision);
            return;
        }
        let layout_changed = before.channels != work.channels
            || old
                .configuration
                .as_ref()
                .is_some_and(|c| c.buses.widths != work.intent.configuration.buses.widths);
        // Shrink needs the separately staged route/port migration protocol,
        // even if no route is currently present in the asynchronous mirror.
        if work.channels < before.channels || (layout_changed && target.routed()) {
            self.reject_config(work, ConfigError::LayoutMigrationRequired);
            return;
        }
        if old.configuration.as_ref() == Some(&work.intent.configuration)
            && before.gains == work.gains
            && target.options_match(&work.options)
        {
            let state = self.0.lock().unwrap();
            work.finish(ConfigCompletion {
                outcome: LocalConfigOutcome::NoOp,
                snapshot: state.config.snapshot.clone(),
                resize: None,
                cancellation_requested: state.stopped,
            });
            return;
        }
        let resize = target.resize(work.channels);
        let mut error = (!resize.is_complete()).then_some(ConfigError::ResizeFailed);
        if error.is_none() {
            target.gains(&work.gains);
            target.options(work.options.clone());
        }
        let after = target.readback();
        if error.is_none()
            && !after
                .as_ref()
                .is_some_and(|r| r.channels == work.channels && r.gains == work.gains)
        {
            error = Some(ConfigError::ReadbackMismatch);
        }
        if error.is_none() && !target.options_match(&work.options) {
            error = Some(ConfigError::ReadbackMismatch);
        }
        let recording = target.recording();
        let mut state = self.0.lock().unwrap();
        let changed = after != old.local || error.is_none() || resize.is_complete();
        state.config.snapshot.local = after;
        state.config.snapshot.recording = recording;
        if changed {
            state.config.snapshot.configuration = if error.is_none() {
                Some(work.intent.configuration.clone())
            } else {
                None
            };
            state.config.snapshot.transport = if error.is_none() {
                Some(work.options.clone())
            } else {
                None
            };
            state.config.bump();
        }
        let outcome = match error {
            None => LocalConfigOutcome::Applied,
            Some(reason) if changed => LocalConfigOutcome::PartiallyApplied(reason),
            Some(reason) => LocalConfigOutcome::Rejected(reason),
        };
        work.finish(ConfigCompletion {
            outcome,
            snapshot: state.config.snapshot.clone(),
            resize: Some(resize),
            cancellation_requested: state.stopped,
        });
    }
}

impl State {
    fn config_admission(&self, expected: ConfigRevision) -> Result<(), ConfigError> {
        if self.stopped {
            return Err(ConfigError::Stopped);
        }
        if !self.graph.value.verbonden {
            return Err(ConfigError::Disconnected);
        }
        if self.config.exhausted || self.config.snapshot.revision.revision == u64::MAX {
            return Err(ConfigError::RevisionExhausted);
        }
        if expected != self.config.snapshot.revision {
            return Err(ConfigError::StaleRevision);
        }
        if self.config.snapshot.local.is_none() {
            return Err(ConfigError::Unavailable);
        }
        Ok(())
    }
    pub(super) fn reject_config(&self, work: Prepared, reason: ConfigError) {
        work.finish(ConfigCompletion {
            outcome: LocalConfigOutcome::Rejected(reason),
            snapshot: self.config.snapshot.clone(),
            resize: None,
            cancellation_requested: self.stopped,
        });
    }
    pub(super) fn fail_all_config(&mut self, reason: ConfigError) {
        let snapshot = self.config.snapshot.clone();
        for queued in &mut self.pending {
            if let Some(work) = queued.config.take() {
                work.finish(ConfigCompletion {
                    outcome: LocalConfigOutcome::Rejected(reason.clone()),
                    snapshot: snapshot.clone(),
                    resize: None,
                    cancellation_requested: self.stopped,
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
