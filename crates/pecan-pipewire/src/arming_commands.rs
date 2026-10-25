//! One bounded asynchronous arming operation on the production worker.
use super::*;
use crate::{
    bus_config::{ConfigError, ConfigTarget, IdleBusConfig},
    opname::{self, TrackedWriter},
    recording::*,
    recording_arming::*,
    shutdown::Shutdown,
};
use std::sync::atomic::Ordering;

impl Commands {
    pub(crate) fn enqueue_arming(
        &self,
        intent: RecordingArming,
    ) -> Result<ArmingReceipt, RecordingError> {
        if intent.root.as_os_str().len() > 4096
            || intent.armed.len() > 64
            || intent.armed.iter().any(|s| s.len() > 256)
        {
            return Err(RecordingError::InvalidInput);
        }
        let desired: BTreeSet<_> = intent.armed.iter().cloned().collect();
        if desired.len() != intent.armed.len() {
            return Err(RecordingError::InvalidInput);
        }
        let mut state = self.0.try_lock().map_err(|_| RecordingError::Busy)?;
        if state.stopped {
            return Err(RecordingError::Stopped);
        }
        if !state.graph.value.verbonden {
            return Err(RecordingError::Disconnected);
        }
        state.recording.reap();
        if state.arming.is_some() || state.migration.is_some() {
            return Err(RecordingError::Busy);
        }
        if state.config.exhausted
            || state.recording.exhausted
            || state.recording.revision == u64::MAX
        {
            return Err(RecordingError::RevisionExhausted);
        }
        if intent.expected_config != state.config.snapshot.revision {
            return Err(RecordingError::StaleConfig);
        }
        if intent.expected_recording != state.recording_revision() {
            return Err(RecordingError::StaleRecording);
        }
        if state.pending.len() >= NORMAL_CAPACITY {
            return Err(RecordingError::QueueFull);
        }
        if state.recording.active.iter().any(|r| {
            !r.started_once.load(Ordering::Acquire)
                && (0..r.context.tracks.len()).any(|i| r.reserves(i))
        }) {
            return Err(RecordingError::Busy);
        }
        let before = state
            .config
            .snapshot
            .configuration
            .clone()
            .ok_or(RecordingError::ConfigurationUnknown)?;
        let definitions = before
            .buses
            .definitions()
            .map_err(|_| RecordingError::InvalidInput)?;
        if desired
            .iter()
            .any(|k| !definitions.iter().any(|d| &d.key == k))
        {
            return Err(RecordingError::InvalidInput);
        }
        let armed: BTreeSet<_> = before
            .buses
            .armed()
            .map_err(|_| RecordingError::InvalidInput)?
            .into_iter()
            .map(|d| d.key)
            .collect();
        let mut baseline = Vec::new();
        for request in &state.recording.active {
            if request.live() {
                state.validate_recording(&request.context, false)?;
                for track in request
                    .installed
                    .get()
                    .unwrap()
                    .tracks
                    .iter()
                    .filter(|t| request.reserves(t.id.index()))
                {
                    baseline.push(BoundTrack {
                        request: request.clone(),
                        id: track.id,
                        key: track.key.clone(),
                    });
                }
            }
        }
        if baseline.is_empty() {
            return Err(RecordingError::Unavailable);
        }
        let required = armed
            .union(&desired)
            .cloned()
            .chain(baseline.iter().map(|t| t.key.clone()))
            .collect();
        let start = RecordingStart {
            ticket: intent.ticket,
            origin: intent.origin,
            expected_config: intent.expected_config,
            root: intent.root,
        };
        let context = state.recording_context(&start, Some(&required))?;
        let removals = baseline
            .iter()
            .enumerate()
            .filter(|(_, t)| armed.contains(&t.key) && !desired.contains(&t.key))
            .map(|(i, _)| i)
            .collect();
        let mut additions = Vec::new();
        for track in context
            .tracks
            .iter()
            .filter(|t| desired.contains(&t.key) && !armed.contains(&t.key))
        {
            let ids: BTreeSet<_> = track.ports.iter().copied().chain([context.node]).collect();
            let mut child = context.clone();
            child.tracks = vec![track.clone()];
            child.objects.retain(|(id, _)| ids.contains(id));
            let existing = baseline
                .iter()
                .find(|t| t.key == track.key)
                .map(|t| t.request.clone());
            additions.push(Addition {
                context: child,
                existing,
            });
        }
        let receipt = ArmingReceipt(Arc::new(ArmingRequest {
            report: std::sync::Mutex::new(ArmingReport {
                ticket: start.ticket,
                source: context.source,
                expected_config: start.expected_config,
                expected_recording: intent.expected_recording,
                progress: ArmingProgress::Queued,
                additions: Vec::new(),
                removed: Vec::new(),
                withheld: Vec::new(),
                failure: None,
                configuration: state.config.snapshot.clone(),
                recording: state.recording_revision(),
            }),
        }));
        state.recording.bump();
        receipt.0.report.lock().unwrap().recording = state.recording_revision();
        let work = Work {
            receipt: receipt.clone(),
            context,
            before,
            desired,
            baseline,
            removals,
            additions,
            attempted: Vec::new(),
            root: start.root.as_path().to_path_buf(),
            claimed: false,
            claim_revision: state.recording_revision(),
            finished: false,
        };
        state.pending.push_back(QueuedCommand {
            command: BackendCommand {
                epoch: work.context.source.epoch,
                command: Opdracht::Start,
            },
            objects: work.context.objects.clone(),
            native: None,
            config: None,
            start: None,
            arming: Some(receipt.clone()),
            migration: None,
        });
        state.arming = Some(work);
        Ok(receipt)
    }
    pub(super) fn claim_arming(&self, receipt: &ArmingReceipt) {
        let mut state = self.0.lock().unwrap();
        let Some(mut work) = state.arming.take() else {
            return;
        };
        if !Arc::ptr_eq(&work.receipt.0, &receipt.0) {
            state.arming = Some(work);
            return;
        }
        state.recording.reap();
        let validation = if state.recording_revision() != work.claim_revision {
            Err(RecordingError::StaleRecording)
        } else {
            state.validate_arming(&work)
        };
        if let Err(error) = validation {
            state.finish_arming(work, ArmingOutcome::Rejected, Some(error), Vec::new());
            return;
        }
        work.claimed = true;
        state.publish_arming(&work, ArmingProgress::Adding, None, Vec::new());
        state.arming = Some(work);
    }
    /// At most one child writer admission per worker tick, no filesystem wait.
    /// Target readback/options operations are bounded local accessors: never
    /// perform I/O or reenter Commands while the short state lock is held.
    pub(crate) fn drive_arming(
        &self,
        target: &mut impl ConfigTarget,
        completion: &Shutdown,
        legacy_active: bool,
        factory: impl FnOnce(
            &std::path::Path,
            Vec<opname::Spoor>,
            u32,
            &Shutdown,
        ) -> std::io::Result<TrackedWriter>,
    ) {
        let child = {
            let mut state = self.0.lock().unwrap();
            let Some(mut work) = state.arming.take() else {
                return;
            };
            if !work.claimed {
                state.arming = Some(work);
                return;
            }
            state.recording.reap();
            let validation = if legacy_active {
                Err(RecordingError::AlreadyRecording)
            } else {
                state.validate_arming(&work)
            };
            if let Err(error) = validation {
                work.stop_pending();
                state.finish_arming_failure(work, error, false, target);
                return;
            }
            let mut failure = None;
            for addition in &work.attempted {
                if addition.is_live() {
                    continue;
                }
                let Some(report) = addition.receipt.try_report() else {
                    state.arming = Some(work);
                    return;
                };
                if report.failure.is_some()
                    || report.stop_requested
                    || report.is_complete()
                    || report.progress == RecordingProgress::Unconfirmed
                {
                    failure = Some(report.failure.unwrap_or(RecordingError::CaptureUnavailable));
                    break;
                }
                state.publish_arming(&work, ArmingProgress::Adding, None, Vec::new());
                state.arming = Some(work);
                return;
            }
            if let Some(error) = failure {
                work.stop_pending();
                state.finish_arming_failure(work, error, true, target);
                return;
            }
            if work.attempted.len() == work.additions.len() {
                state.complete_arming(work, target);
                return;
            }
            let addition = &work.additions[work.attempted.len()];
            if let Some(existing) = &addition.existing {
                work.attempted.push(ArmingAddition {
                    bus: addition.context.tracks[0].key.clone(),
                    receipt: RecordingReceipt(existing.clone()),
                    new_take: false,
                });
                state.publish_arming(&work, ArmingProgress::Adding, None, Vec::new());
                state.arming = Some(work);
                return;
            }
            if state.recording.active.len() >= MAX_RECORDING_TAKES
                || state.recording.exhausted
                || state.recording.revision == u64::MAX
            {
                state.finish_arming_failure(work, RecordingError::Capacity, true, target);
                return;
            }
            let request = Request::new(addition.context.clone());
            work.attempted.push(ArmingAddition {
                bus: addition.context.tracks[0].key.clone(),
                receipt: RecordingReceipt(request.clone()),
                new_take: true,
            });
            state.recording.active.push(request.clone());
            state.recording.refresh_revision();
            let child = PreparedStart::new(request, work.root.clone());
            state.publish_arming(&work, ArmingProgress::Adding, None, Vec::new());
            state.arming = Some(work);
            child
        };
        self.start_recording_with(child, completion, legacy_active, factory);
    }
}
impl State {
    fn recording_revision(&self) -> RecordingRevision {
        RecordingRevision {
            owner: self.config.snapshot.revision.owner,
            revision: self.recording.revision,
        }
    }
    fn validate_arming(&self, work: &Work) -> Result<(), RecordingError> {
        self.validate_recording(&work.context, true)?;
        if self.config.exhausted {
            return Err(RecordingError::RevisionExhausted);
        }
        if work
            .baseline
            .iter()
            .any(|t| !t.request.live() || !t.request.reserves(t.id.index()))
        {
            return Err(RecordingError::StaleRecording);
        }
        Ok(())
    }
    fn publish_arming(
        &self,
        work: &Work,
        progress: ArmingProgress,
        failure: Option<RecordingError>,
        removed: Vec<opname::TrackId>,
    ) {
        let mut report = work.receipt.0.report.lock().unwrap();
        report.progress = progress;
        report.additions = work.attempted.clone();
        report.withheld = work
            .removals
            .iter()
            .map(|i| work.baseline[*i].id)
            .filter(|id| !removed.contains(id))
            .collect();
        report.removed = removed;
        report.failure = failure;
        report.configuration = self.config.snapshot.clone();
        report.recording = self.recording_revision();
    }
    fn finish_arming(
        &mut self,
        mut work: Work,
        outcome: ArmingOutcome,
        failure: Option<RecordingError>,
        removed: Vec<opname::TrackId>,
    ) {
        self.recording.refresh_revision();
        self.recording.bump();
        self.publish_arming(&work, ArmingProgress::Completed(outcome), failure, removed);
        work.finished = true;
    }
    pub(super) fn cancel_arming(&mut self, error: RecordingError) {
        let Some(work) = self.arming.take() else {
            return;
        };
        work.stop_pending();
        let partial = work.attempted.iter().any(|a| {
            a.new_take
                && (a.receipt.0.installed.get().is_some()
                    || a.receipt.0.stage.load(Ordering::Acquire) == OPENING)
        });
        self.finish_arming(
            work,
            if partial {
                ArmingOutcome::PartiallyApplied
            } else {
                ArmingOutcome::Rejected
            },
            Some(error),
            Vec::new(),
        );
    }
    fn arming_config(work: &Work, armed: &BTreeSet<String>) -> IdleBusConfig {
        let mut config = work.before.clone();
        for definition in config.buses.definitions().expect("validated fixed layout") {
            let mut state = config.buses.state(&definition.key);
            let next = armed.contains(&definition.key);
            if state.armed != next {
                state.armed = next;
                config.buses.states.insert(definition.key, state);
            }
        }
        config
    }
    fn apply_arming_config(
        &mut self,
        next: IdleBusConfig,
        target: &mut impl ConfigTarget,
    ) -> Result<bool, RecordingError> {
        let before = self.config.snapshot.clone();
        if target.readback() != before.local
            || before
                .transport
                .as_ref()
                .is_none_or(|o| !target.options_match(o))
        {
            self.config.snapshot.local = target.readback();
            self.config.snapshot.configuration = None;
            self.config.snapshot.transport = None;
            self.config.bump();
            return Err(RecordingError::ConfigApply(ConfigError::ReadbackMismatch));
        }
        if before.configuration.as_ref() == Some(&next) {
            return Ok(false);
        }
        let options = pecan_core::graph::Opties {
            bussen: next
                .buses
                .armed()
                .expect("validated arming")
                .into_iter()
                .map(|d| pecan_core::graph::BusOpname {
                    naam: d.file,
                    kanalen: d.channels,
                })
                .collect(),
            midi: next.buses.midi,
            klok: next.clock,
            start_stop: next.start_stop,
        };
        target.options(options.clone());
        if !target.options_match(&options) || target.readback() != before.local {
            self.config.snapshot.local = target.readback();
            self.config.snapshot.configuration = None;
            self.config.snapshot.transport = None;
            self.config.bump();
            return Err(RecordingError::ConfigApply(ConfigError::ReadbackMismatch));
        }
        self.config.snapshot.configuration = Some(next);
        self.config.snapshot.transport = Some(options);
        self.config.bump();
        Ok(true)
    }
    fn finish_arming_failure(
        &mut self,
        work: Work,
        error: RecordingError,
        commit_started: bool,
        target: &mut impl ConfigTarget,
    ) {
        let successful: Vec<_> = work
            .attempted
            .iter()
            .filter(|a| a.is_live())
            .map(|a| a.bus.clone())
            .collect();
        let mut partial = work
            .attempted
            .iter()
            .any(|a| a.new_take && a.receipt.0.installed.get().is_some());
        let mut error = error;
        if commit_started {
            let armed = work
                .before
                .buses
                .armed()
                .expect("validated config")
                .into_iter()
                .map(|d| d.key)
                .chain(successful.iter().cloned())
                .collect();
            match self.apply_arming_config(Self::arming_config(&work, &armed), target) {
                Ok(changed) => partial |= changed,
                Err(e) => {
                    partial = true;
                    error = e;
                }
            }
            // A second failure during reconciliation is not a stable logical
            // readback. Leave actual captures untouched and require the owner
            // to reconcile, rather than publish stale successful-addition flags.
            let still_live: Vec<_> = work
                .attempted
                .iter()
                .filter(|a| a.is_live())
                .map(|a| a.bus.clone())
                .collect();
            if successful != still_live {
                self.config.snapshot.configuration = None;
                self.config.snapshot.transport = None;
                self.config.bump();
                partial = true;
            }
        }
        self.finish_arming(
            work,
            if partial {
                ArmingOutcome::PartiallyApplied
            } else {
                ArmingOutcome::Rejected
            },
            Some(error),
            Vec::new(),
        );
    }
    fn complete_arming(&mut self, work: Work, target: &mut impl ConfigTarget) {
        self.publish_arming(&work, ArmingProgress::ApplyingRemovals, None, Vec::new());
        let next = Self::arming_config(&work, &work.desired);
        match self.apply_arming_config(next, target) {
            Err(error) => self.finish_arming(
                work,
                ArmingOutcome::PartiallyApplied,
                Some(error),
                Vec::new(),
            ),
            Ok(changed) => {
                // Options/readback accessors run after the earlier Adding
                // check. A writer can fail on its own thread during them.
                // Inspect the actual attachment immediately before committing
                // any old-track stop; readiness/earlier liveness is insufficient.
                if let Some(addition) = work.attempted.iter().find(|a| !a.is_live()) {
                    let error = addition
                        .receipt
                        .try_report()
                        .and_then(|r| r.failure)
                        .unwrap_or(RecordingError::CaptureUnavailable);
                    self.finish_arming_failure(work, error, true, target);
                    return;
                }
                let removed = work
                    .removals
                    .iter()
                    .map(|i| {
                        let track = &work.baseline[*i];
                        track.request.request_track_stop(track.id.index());
                        track.id
                    })
                    .collect();
                let live = self.recording.active.iter().any(|r| r.live());
                self.config
                    .refresh(self.config.snapshot.local.clone(), live, false);
                self.finish_arming(
                    work,
                    if changed {
                        ArmingOutcome::AppliedLocally
                    } else {
                        ArmingOutcome::NoOpLocally
                    },
                    None,
                    removed,
                );
            }
        }
    }
}
