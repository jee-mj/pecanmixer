//! Local recorder configuration bookkeeping on the existing command boundary.
use super::*;
use crate::bus_config::*;
#[path = "gain_commands.rs"]
mod gains;

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
        self.enqueue_config_origin(None, intent)
    }
    pub(crate) fn enqueue_config_from(
        &self,
        origin: &pecan_core::history_identity::HistoryGraph,
        intent: IdleConfigIntent,
    ) -> Result<ConfigReceipt, ConfigError> {
        self.enqueue_config_origin(Some(origin), intent)
    }
    fn enqueue_config_origin(
        &self,
        origin: Option<&pecan_core::history_identity::HistoryGraph>,
        intent: IdleConfigIntent,
    ) -> Result<ConfigReceipt, ConfigError> {
        let (mut work, receipt) = Prepared::new(intent)?;
        let mut state = self.0.try_lock().map_err(|_| ConfigError::Busy)?;
        state.config_admission(work.intent.expected)?;
        if let Some(origin) = origin {
            work.full_binding = Some(state.rec_origin_binding(origin)?);
            state.config_work_admission(&work, false)?;
        }
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
            migration: None,
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
    #[cfg(test)]
    pub(crate) fn reject_config(&self, work: Prepared, reason: ConfigError) {
        self.0.lock().unwrap().reject_config(work, reason);
    }

    pub(crate) fn apply_config(&self, work: Prepared, target: &mut impl ConfigTarget) {
        if work.gain_binding.is_some() {
            return self.apply_gains(work, target);
        }
        let old = {
            let state = self.0.lock().unwrap();
            if let Err(reason) = state.config_work_admission(&work, false) {
                state.reject_config(work, reason);
                return;
            }
            work.applying();
            state.config.snapshot.clone()
        };
        let locked = target.config_locked();
        let before = target.readback();
        let recording = target.recording();
        let layout_changed = before.as_ref().is_some_and(|r| r.channels != work.channels)
            || old
                .configuration
                .as_ref()
                .is_some_and(|c| c.buses.widths != work.intent.configuration.buses.widths);
        let routed = layout_changed && target.routed();
        let no_op = old.configuration.as_ref() == Some(&work.intent.configuration)
            && before.as_ref().is_some_and(|r| r.gains == work.gains)
            && target.options_match(&work.options);
        {
            let mut state = self.0.lock().unwrap();
            // Target queries run outside the command lock. Retained native
            // requests must still name the original filter/input set afterward.
            if work.full_binding.is_some() {
                if let Err(reason) = state.config_work_admission(&work, false) {
                    state.reject_config(work, reason);
                    return;
                }
            }
            if locked {
                state.config.refresh(before, recording, false);
                state.reject_config(work, ConfigError::RecordingActive);
                return;
            }
            let Some(before) = before else {
                state.config.refresh(None, false, false);
                state.reject_config(work, ConfigError::Unavailable);
                return;
            };
            // Include preceding legacy/local changes before touching a filter.
            if old.local.as_ref() != Some(&before) {
                state.config.refresh(Some(before), false, false);
                state.reject_config(work, ConfigError::StaleRevision);
                return;
            }
            // Never infer safe shrink from an asynchronously empty route mirror.
            if work.channels < before.channels || routed {
                state.reject_config(work, ConfigError::LayoutMigrationRequired);
                return;
            }
            if no_op {
                work.finish(ConfigCompletion {
                    outcome: LocalConfigOutcome::NoOp,
                    snapshot: state.config.snapshot.clone(),
                    resize: None,
                    cancellation_requested: state.stopped,
                });
                return;
            }
        }
        let resize = target.resize(work.channels);
        let mut error = (!resize.is_complete()).then_some(ConfigError::ResizeFailed);
        if work.full_binding.is_some() {
            error = self
                .0
                .lock()
                .unwrap()
                .config_work_admission(&work, true)
                .err()
                .or(error);
        }
        if error.is_none() {
            target.gains(&work.gains);
            if work.full_binding.is_some() {
                error = self
                    .0
                    .lock()
                    .unwrap()
                    .config_work_admission(&work, true)
                    .err();
            }
        }
        if error.is_none() {
            target.options(work.options.clone());
        }
        let after = target.readback();
        if error.is_none()
            && !after.as_ref().is_some_and(|r| {
                r.channels == work.channels
                    && r.gains == work.gains
                    && (work.full_binding.is_none()
                        || r.node == old.local.as_ref().and_then(|r| r.node))
            })
        {
            error = Some(ConfigError::ReadbackMismatch);
        }
        if error.is_none() && !target.options_match(&work.options) {
            error = Some(ConfigError::ReadbackMismatch);
        }
        let recording = target.recording();
        let mut state = self.0.lock().unwrap();
        if work.full_binding.is_some() {
            if let Err(reason) = state.config_work_admission(&work, true) {
                // A local call has been attempted. Its effects cannot be
                // rolled back or attributed to a replacement's readback.
                // Preserve any newer configuration/epoch in full.
                if state.config.snapshot.revision == work.intent.expected {
                    state.config.refresh(None, recording, false);
                }
                work.finish(ConfigCompletion {
                    outcome: LocalConfigOutcome::PartiallyApplied(reason),
                    snapshot: state.config.snapshot.clone(),
                    resize: Some(resize),
                    cancellation_requested: state.stopped,
                });
                return;
            }
        }
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
    fn config_work_admission(&self, work: &Prepared, resized: bool) -> Result<(), ConfigError> {
        self.config_admission(work.intent.expected)?;
        let Some(binding) = &work.full_binding else {
            return Ok(());
        };
        if self.arming.is_some() {
            return Err(ConfigError::Busy);
        }
        if self.recording.active.iter().any(|r| r.owned()) {
            return Err(ConfigError::RecordingActive);
        }
        let max_inputs = if resized {
            work.channels
        } else {
            binding.inputs.len()
        };
        let graph = &self.graph.value;
        if self.graph.stamp.epoch != binding.epoch
            || self.config.snapshot.local.as_ref().and_then(|r| r.node) != Some(binding.node)
            || self.objects.get(&binding.node) != Some(&binding.generation)
            || self.published_objects.get(&binding.node) != Some(&binding.generation)
            || !graph.knopen.iter().any(|n| {
                n.id == binding.node
                    && n.serial == binding.serial
                    && n.naam == crate::opname::BUS_NAAM
            })
            || binding
                .inputs
                .iter()
                .enumerate()
                .any(|(channel, (id, generation))| {
                    self.objects.get(id) != Some(generation)
                        || self.published_objects.get(id) != Some(generation)
                        || !graph.poorten.iter().any(|p| {
                            p.id == *id
                                && p.knoop == binding.node
                                && p.ingang
                                && p.soort == pecan_core::graph::Soort::Audio
                                && p.naam == format!("rec_{}", channel + 1)
                        })
                })
        {
            return Err(ConfigError::StaleRevision);
        }
        let layout_changed = work.channels != binding.inputs.len()
            || self
                .config
                .snapshot
                .configuration
                .as_ref()
                .is_some_and(|old| old.buses.widths != work.intent.configuration.buses.widths);
        if layout_changed
            && graph
                .kabels
                .iter()
                .any(|link| link.in_knoop == binding.node || link.uit_knoop == binding.node)
        {
            // Supplement the synchronous target query with any newer route
            // already observed by the command mirror. Neither can authorize
            // routed growth or grouping without migration barriers.
            return Err(ConfigError::LayoutMigrationRequired);
        }
        // Growth need not be published yet. Any ports that do appear must be
        // unique channels in the permitted extended prefix; the original
        // prefix remains fixed. Shrink never reaches this phase.
        let mut names = BTreeSet::new();
        for port in graph.poorten.iter().filter(|p| {
            p.knoop == binding.node && p.ingang && p.soort == pecan_core::graph::Soort::Audio
        }) {
            if !names.insert(&port.naam)
                || !(1..=max_inputs).any(|i| port.naam == format!("rec_{i}"))
                || self.objects.get(&port.id).is_none()
                || self.objects.get(&port.id) != self.published_objects.get(&port.id)
            {
                return Err(ConfigError::StaleRevision);
            }
        }
        Ok(())
    }

    fn config_admission(&self, expected: ConfigRevision) -> Result<(), ConfigError> {
        if self.migration.is_some() {
            return Err(ConfigError::Busy);
        }
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
