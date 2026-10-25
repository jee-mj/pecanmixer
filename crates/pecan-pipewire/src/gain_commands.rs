//! Same-layout native Rec levels. Never resize, change options or restart takes.
use super::*;
use crate::{opname, recording::RecordingReceipt};
use pecan_core::graph::Soort;
use std::sync::atomic::Ordering;

impl Commands {
    pub(crate) fn enqueue_gains(
        &self,
        intent: IdleConfigIntent,
    ) -> Result<ConfigReceipt, ConfigError> {
        let (mut work, receipt) = Prepared::new(intent)?;
        let mut state = self.0.try_lock().map_err(|_| ConfigError::Busy)?;
        state.gain_admission(&work)?;
        work.gain_binding = Some(state.gain_binding()?);
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

    pub(super) fn apply_gains(&self, work: Prepared, target: &mut impl ConfigTarget) {
        let old = {
            let state = self.0.lock().unwrap();
            if let Err(reason) = state
                .gain_admission(&work)
                .and_then(|_| state.check_gain_binding(&work))
            {
                state.reject_config(work, reason);
                return;
            }
            work.applying();
            state.config.snapshot.clone()
        };
        let before = target.readback();
        let options_match = target.options_match(&work.options);
        let locked = target.gains_locked();
        // These target callbacks are outside the command lock. Recheck their
        // authority and captured generations before the only mutating call.
        {
            let mut state = self.0.lock().unwrap();
            let error = state
                .gain_admission(&work)
                .and_then(|_| state.check_gain_binding(&work))
                .err()
                .or_else(|| locked.then_some(ConfigError::RecordingActive))
                .or_else(|| {
                    (before != old.local || !options_match).then_some(ConfigError::ReadbackMismatch)
                });
            if let Some(error) = error {
                if state.config.snapshot.revision == work.intent.expected
                    && state.check_gain_binding(&work).is_ok()
                    && (before != old.local || !options_match)
                {
                    state.config.refresh(before, old.recording, false);
                    // Options are independently mutable legacy state, even if
                    // the gain readback happens to be unchanged.
                    if !options_match && state.config.snapshot.configuration.is_some() {
                        state.config.snapshot.configuration = None;
                        state.config.snapshot.transport = None;
                        state.config.bump();
                    }
                }
                state.reject_config(work, error);
                return;
            }
            if old.configuration.as_ref() == Some(&work.intent.configuration) {
                work.finish(ConfigCompletion {
                    outcome: LocalConfigOutcome::NoOp,
                    snapshot: state.config.snapshot.clone(),
                    resize: None,
                    cancellation_requested: state.stopped,
                });
                return;
            }
        }
        target.gains(&work.gains);
        let after = target.readback();
        let options_match = target.options_match(&work.options);
        let recording = target.recording();
        let mut state = self.0.lock().unwrap();
        let expected_local = old.local.as_ref().map(|local| BusReadback {
            gains: work.gains.clone(),
            ..local.clone()
        });
        let authority = state.config.snapshot.revision == work.intent.expected;
        let binding = state.check_gain_binding(&work);
        let outcome = if !authority || binding.is_err() {
            // A claimed setter may already have changed the old filter. Never
            // overwrite a newer configuration/epoch with its stale readback.
            if authority {
                state.config.refresh(None, recording, false);
            }
            LocalConfigOutcome::PartiallyApplied(
                binding.err().unwrap_or(ConfigError::StaleRevision),
            )
        } else if after == expected_local && options_match {
            state.config.snapshot.local = after;
            state.config.snapshot.configuration = Some(work.intent.configuration.clone());
            state.config.snapshot.recording = recording;
            state.config.bump();
            LocalConfigOutcome::Applied
        } else if after == old.local && options_match {
            LocalConfigOutcome::Rejected(ConfigError::ReadbackMismatch)
        } else {
            state.config.snapshot.local = after;
            state.config.snapshot.configuration = None;
            if !options_match {
                state.config.snapshot.transport = None;
            }
            state.config.snapshot.recording = recording;
            state.config.bump();
            LocalConfigOutcome::PartiallyApplied(ConfigError::ReadbackMismatch)
        };
        work.finish(ConfigCompletion {
            outcome,
            snapshot: state.config.snapshot.clone(),
            resize: None,
            cancellation_requested: state.stopped,
        });
    }
}

impl State {
    fn gain_admission(&self, work: &Prepared) -> Result<(), ConfigError> {
        self.config_admission(work.intent.expected)?;
        if self.arming.is_some() {
            return Err(ConfigError::Busy);
        }
        let old = self
            .config
            .snapshot
            .configuration
            .as_ref()
            .ok_or(ConfigError::Unavailable)?;
        let next = &work.intent.configuration;
        if old.buses.widths != next.buses.widths
            || old.buses.midi != next.buses.midi
            || old.clock != next.clock
            || old.start_stop != next.start_stop
        {
            return Err(ConfigError::InvalidInput);
        }
        let defs = old
            .buses
            .definitions()
            .map_err(|_| ConfigError::InvalidInput)?;
        if defs
            .iter()
            .any(|d| old.buses.state(&d.key).armed != next.buses.state(&d.key).armed)
            || old.buses.states.iter().any(|(key, value)| {
                !defs.iter().any(|d| &d.key == key)
                    && !next.buses.states.get(key).is_some_and(|next| {
                        next.db.to_bits() == value.db.to_bits()
                            && next.mute == value.mute
                            && next.armed == value.armed
                    })
            })
            || next.buses.states.keys().any(|key| {
                !defs.iter().any(|d| &d.key == key) && !old.buses.states.contains_key(key)
            })
        {
            return Err(ConfigError::InvalidInput);
        }
        let (channels, gains, options) = validate_configuration(old)?;
        let local = self
            .config
            .snapshot
            .local
            .as_ref()
            .ok_or(ConfigError::Unavailable)?;
        if channels != local.channels
            || !same_gains(&gains, &local.gains)
            || self.config.snapshot.transport.as_ref() != Some(&options)
            || options != work.options
        {
            return Err(ConfigError::Unavailable);
        }
        // An opening start owns the old config identity. Avoid making it stale
        // midway through attachment; running/finalizing takes keep their maps.
        for request in &self.recording.active {
            if !request.started_once.load(Ordering::Acquire)
                && !RecordingReceipt(request.clone())
                    .try_report()
                    .ok_or(ConfigError::Busy)?
                    .is_complete()
            {
                return Err(ConfigError::Busy);
            }
            if (0..request.context.tracks.len()).any(|i| request.reserves(i)) {
                self.validate_recording(&request.context, false)
                    .map_err(|_| ConfigError::StaleRevision)?;
            }
        }
        Ok(())
    }

    fn gain_binding(&self) -> Result<GainBinding, ConfigError> {
        let local = self
            .config
            .snapshot
            .local
            .as_ref()
            .ok_or(ConfigError::Unavailable)?;
        let node = local.node.ok_or(ConfigError::Unavailable)?;
        let observed = self
            .graph
            .value
            .knopen
            .iter()
            .find(|n| n.id == node && n.naam == opname::BUS_NAAM)
            .ok_or(ConfigError::Unavailable)?;
        let generation = self
            .objects
            .get(&node)
            .copied()
            .ok_or(ConfigError::Unavailable)?;
        if self.published_objects.get(&node) != Some(&generation) {
            return Err(ConfigError::StaleRevision);
        }
        let ports: Vec<_> = self
            .graph
            .value
            .poorten
            .iter()
            .filter(|p| p.knoop == node && p.ingang && p.soort == Soort::Audio)
            .take(local.channels + 1)
            .collect();
        if ports.len() != local.channels {
            return Err(ConfigError::Unavailable);
        }
        let mut inputs = Vec::with_capacity(local.channels);
        for channel in 0..local.channels {
            let name = format!("rec_{}", channel + 1);
            let mut matching = ports.iter().filter(|p| p.naam == name);
            let p = matching.next().ok_or(ConfigError::Unavailable)?;
            if matching.next().is_some() {
                return Err(ConfigError::Unavailable);
            }
            let generation = self
                .objects
                .get(&p.id)
                .copied()
                .ok_or(ConfigError::Unavailable)?;
            if self.published_objects.get(&p.id) != Some(&generation) {
                return Err(ConfigError::StaleRevision);
            }
            inputs.push((p.id, generation));
        }
        Ok(GainBinding {
            epoch: self.graph.stamp.epoch,
            node,
            generation,
            serial: observed.serial,
            inputs,
        })
    }
    fn check_gain_binding(&self, work: &Prepared) -> Result<(), ConfigError> {
        if work.gain_binding.as_ref() != Some(&self.gain_binding()?) {
            return Err(ConfigError::StaleRevision);
        }
        Ok(())
    }
}
fn same_gains(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}
