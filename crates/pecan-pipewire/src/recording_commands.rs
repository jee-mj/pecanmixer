//! Recording admission/execution on the production FIFO and session boundary.
use super::*;
use crate::{
    opname::{self, Spoor, TrackedWriter},
    recording::*,
    shutdown::Shutdown,
};
use std::{cell::RefCell, sync::atomic::Ordering, time::Instant};

impl Commands {
    pub(crate) fn enqueue_recording(
        &self,
        intent: RecordingStart,
    ) -> Result<RecordingReceipt, RecordingError> {
        match self.enqueue_recording_selection(intent, None)? {
            RecordingBusStartResult::Admitted(receipt) => Ok(receipt),
            RecordingBusStartResult::NoOp(_) => unreachable!("whole-take start remains idle-only"),
        }
    }
    pub(crate) fn enqueue_recording_bus(
        &self,
        intent: RecordingBusStart,
    ) -> Result<RecordingBusStartResult, RecordingError> {
        if intent.bus.is_empty() || intent.bus.len() > 256 {
            return Err(RecordingError::InvalidInput);
        }
        self.enqueue_recording_selection(
            intent.start,
            Some((intent.bus, intent.expected_recording)),
        )
    }
    pub(crate) fn recording_snapshot(&self) -> Option<RecordingSnapshot> {
        let mut state = self.0.try_lock().ok()?;
        state.recording.reap();
        Some(RecordingSnapshot {
            revision: RecordingRevision {
                owner: state.config.snapshot.revision.owner,
                revision: state.recording.revision,
            },
            configuration: state.config.snapshot.clone(),
            latest_started: state.recording.latest_started.clone().map(RecordingReceipt),
            takes: state
                .recording
                .active
                .iter()
                .cloned()
                .map(RecordingReceipt)
                .collect(),
            arming: state.arming.as_ref().map(|w| w.receipt.clone()),
        })
    }
    fn enqueue_recording_selection(
        &self,
        intent: RecordingStart,
        selection: Option<(String, RecordingRevision)>,
    ) -> Result<RecordingBusStartResult, RecordingError> {
        if intent.root.as_os_str().len() > 4096 {
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
        if state.recording.exhausted
            || state.recording.revision == u64::MAX
            || state.config.exhausted
        {
            return Err(RecordingError::RevisionExhausted);
        }
        if let Some((_, expected)) = &selection {
            if *expected
                != (RecordingRevision {
                    owner: state.config.snapshot.revision.owner,
                    revision: state.recording.revision,
                })
            {
                return Err(RecordingError::StaleRecording);
            }
        } else if !state.recording.active.is_empty() || state.config.snapshot.recording {
            return Err(RecordingError::AlreadyRecording);
        }
        if intent.expected_config != state.config.snapshot.revision {
            return Err(RecordingError::StaleConfig);
        }
        let selected = selection
            .as_ref()
            .map(|(key, _)| BTreeSet::from([key.clone()]));
        let context = state.recording_context(&intent, selected.as_ref())?;
        if selection.is_some() {
            for existing in &state.recording.active {
                for (index, track) in existing.context.tracks.iter().enumerate() {
                    if existing.reserves(index) && track.key == context.tracks[0].key {
                        let same = track.channels == context.tracks[0].channels
                            && track.ports == context.tracks[0].ports
                            && existing.context.node == context.node
                            && existing.context.serial == context.serial
                            && existing.context.rate == context.rate
                            && existing.context.source.epoch == context.source.epoch
                            && context
                                .objects
                                .iter()
                                .all(|entry| existing.context.objects.contains(entry));
                        return if same {
                            Ok(RecordingBusStartResult::NoOp(RecordingReceipt(
                                existing.clone(),
                            )))
                        } else {
                            Err(RecordingError::AlreadyRecording)
                        };
                    }
                }
            }
            // One opening transition at a time. Existing captures keep running;
            // stopped/finalizing takes do not prevent an independent new take.
            if state.recording.active.iter().any(|r| {
                !r.started_once.load(Ordering::Acquire)
                    && (0..r.context.tracks.len()).any(|i| r.reserves(i))
            }) {
                return Err(RecordingError::Busy);
            }
        }
        if state.recording.active.len() >= MAX_RECORDING_TAKES {
            return Err(RecordingError::Capacity);
        }
        if state.pending.len() >= NORMAL_CAPACITY {
            return Err(RecordingError::QueueFull);
        }
        let request = Request::new(context);
        state.pending.push_back(QueuedCommand {
            command: BackendCommand {
                epoch: request.context.source.epoch,
                command: Opdracht::Start,
            },
            objects: request.context.objects.clone(),
            native: None,
            config: None,
            // Retain the bounded path bytes, not caller-owned spare capacity.
            start: Some(PreparedStart::new(
                request.clone(),
                intent.root.as_path().to_path_buf(),
            )),
            arming: None,
            migration: None,
        });
        state.recording.active.push(request.clone());
        state.recording.refresh_revision();
        Ok(RecordingBusStartResult::Admitted(RecordingReceipt(request)))
    }
    pub(crate) fn stop_recording(
        &self,
        target: RecordingStop,
    ) -> Result<RecordingStopResult, RecordingError> {
        let mut state = self.0.try_lock().map_err(|_| RecordingError::Busy)?;
        if target == RecordingStop::All {
            let result = state.recording.stop_all(None, true);
            state.cancel_arming(RecordingError::Cancelled);
            state.cancel_migration(crate::bus_migration::MigrationError::Cancelled);
            // A stopped queued start must not retain ordinary FIFO capacity.
            state
                .pending
                .retain(|q| q.start.is_none() && q.arming.is_none());
            return Ok(result);
        }
        let take_id = match target {
            RecordingStop::Take(id) => id,
            RecordingStop::Track(id) => id.take(),
            RecordingStop::All => unreachable!(),
        };
        let request = state
            .recording
            .active
            .iter()
            .chain(state.recording.recent.iter())
            .find(|r| {
                r.installed
                    .get()
                    .is_some_and(|i| i.writer.receipt.take_id() == take_id)
            })
            .cloned()
            .ok_or(RecordingError::UnknownTake)?;
        let installed = request.installed.get().unwrap();
        let was_reserving = match target {
            RecordingStop::Take(_) => (0..installed.tracks.len()).any(|i| request.reserves(i)),
            RecordingStop::Track(id) => request.reserves(id.index()),
            RecordingStop::All => unreachable!(),
        };
        let was_stopped = request.stop.load(Ordering::Acquire);
        let old_tracks = request.track_stops.load(Ordering::Acquire);
        match target {
            RecordingStop::Take(_) => request.request_stop(),
            RecordingStop::Track(id) => {
                if installed.writer.receipt.track_id(id.index()) != Some(id) {
                    return Err(RecordingError::UnknownTrack);
                }
                request.request_track_stop(id.index());
            }
            _ => unreachable!(),
        }
        if was_reserving
            && (request.stop.load(Ordering::Acquire) != was_stopped
                || request.track_stops.load(Ordering::Acquire) != old_tracks)
        {
            state.cancel_arming(RecordingError::Cancelled);
            state.pending.retain(|q| q.arming.is_none());
        }
        state.recording.refresh_revision();
        Ok(RecordingStopResult::Retained(RecordingReceipt(request)))
    }
    pub(crate) fn recording_owned(&self) -> bool {
        let state = self.0.lock().unwrap();
        state.arming.as_ref().is_some_and(|w| w.claimed)
            || state.recording.active.iter().any(|r| r.owned())
    }
    pub(crate) fn legacy_recording_blocked(&self, command: &Opdracht) -> bool {
        matches!(
            command,
            Opdracht::ZetBus { .. }
                | Opdracht::ZetTransport(_)
                | Opdracht::Start
                | Opdracht::StartBus(_)
        ) && self.recording_owned()
    }
    pub(crate) fn recording_live(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .recording
            .active
            .iter()
            .any(|r| r.live())
    }
    pub(crate) fn legacy_recording_stop(&self, name: Option<&str>) {
        let mut state = self.0.lock().unwrap();
        if name.is_none() {
            state.recording.stop_all(None, false);
            if state.arming.as_ref().is_some_and(|w| w.claimed) {
                state.cancel_arming(RecordingError::Cancelled);
            }
            return;
        }
        let mut changed = false;
        for request in &state.recording.active {
            if !request.owned() {
                continue;
            }
            if let Some(installed) = request.installed.get() {
                for track in installed
                    .tracks
                    .iter()
                    .filter(|t| Some(t.name.as_str()) == name)
                {
                    changed |= request.reserves(track.id.index());
                    request.request_track_stop(track.id.index());
                }
            } else if request
                .context
                .tracks
                .iter()
                .any(|t| Some(t.name.as_str()) == name)
            {
                changed |= !request.stop.load(Ordering::Acquire);
                request.request_stop();
            }
        }
        if changed && state.arming.as_ref().is_some_and(|w| w.claimed) {
            state.cancel_arming(RecordingError::Cancelled);
        }
        state.recording.refresh_revision();
    }
    #[cfg(test)]
    pub(crate) fn reject_recording(&self, mut work: PreparedStart, reason: RecordingError) {
        work.request.reject(reason);
        work.disarm();
    }
    pub(crate) fn start_recording_with(
        &self,
        mut work: PreparedStart,
        completion: &Shutdown,
        legacy_active: bool,
        factory: impl FnOnce(
            &std::path::Path,
            Vec<Spoor>,
            u32,
            &Shutdown,
        ) -> std::io::Result<TrackedWriter>,
    ) {
        {
            let state = self.0.lock().unwrap();
            let result = if work.request.stop.load(Ordering::Acquire) {
                Err(RecordingError::CancelledBeforeStart)
            } else if legacy_active {
                Err(RecordingError::AlreadyRecording)
            } else {
                state.validate_recording(&work.request.context, true)
            };
            if let Err(reason) = result {
                work.request.reject(reason);
                work.disarm();
                return;
            }
            work.request.stage.store(OPENING, Ordering::Release);
        }
        let tracks = work
            .request
            .context
            .tracks
            .iter()
            .map(|t| Spoor {
                naam: t.name.clone(),
                kanalen: t.channels.len() as u16,
            })
            .collect();
        match factory(&work.root, tracks, work.request.context.rate, completion) {
            Err(error) => {
                work.request
                    .reject(RecordingError::Writer(crate::writer_status::error_text(
                        &error.to_string(),
                    )))
            }
            Ok(writer) => {
                let tracks = Arc::new(
                    work.request
                        .context
                        .tracks
                        .iter()
                        .enumerate()
                        .map(|(index, t)| RecordingTrack {
                            id: writer.receipt.track_id(index).unwrap(),
                            key: t.key.clone(),
                            name: t.name.clone(),
                            channels: t.channels.clone(),
                            ports: t.ports.clone(),
                        })
                        .collect(),
                );
                assert!(
                    work.request
                        .installed
                        .set(Installed { writer, tracks })
                        .is_ok(),
                    "one writer per admitted recording"
                );
                if work.request.stop.load(Ordering::Acquire) {
                    work.request.request_stop();
                }
            }
        }
        work.disarm();
    }
    pub(crate) fn pump_recording(&self, captures: &RefCell<Vec<Capture>>, legacy_active: bool) {
        let mut state = self.0.lock().unwrap();
        state.recording.reap();
        let mut captures = captures.borrow_mut();
        captures.clear();
        let mut latest_started = None;
        for request in &state.recording.active {
            if request.stage.load(Ordering::Acquire) == QUEUED {
                continue;
            }
            if request.stop.load(Ordering::Acquire) {
                request.attached.store(false, Ordering::Release);
                continue;
            }
            let validation = if legacy_active {
                Err(RecordingError::AlreadyRecording)
            } else {
                state.validate_recording(
                    &request.context,
                    !request.started_once.load(Ordering::Acquire),
                )
            };
            if let Err(error) = validation {
                request.fail(error);
                request.request_stop();
                continue;
            }
            let Some(installed) = request.installed.get() else {
                continue;
            };
            let Some(report) = installed.writer.receipt.try_report() else {
                // A busy status read must not detach an already attached
                // capture or silently skip its next sample block.
                if request.live() {
                    captures.push(Capture(request.clone()));
                }
                continue;
            };
            if report.phase != opname::WriterPhase::Ready || report.stop_requested {
                request.attached.store(false, Ordering::Release);
                continue;
            }
            let stops = request.track_stops.load(Ordering::Acquire);
            let mut failed = false;
            for track in installed
                .tracks
                .iter()
                .filter(|t| stops & (1_u64 << t.id.index()) != 0)
            {
                match installed.writer.sender.try_close(track.id) {
                    Ok(())
                    | Err(opname::WriterSendError::Busy | opname::WriterSendError::Closed) => {}
                    Err(_) => {
                        request.ingress_fault(6);
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                continue;
            }
            // The take becomes locally attached here, after writer readiness
            // and identity/format validation. Repeated pumps, no-op starts and
            // partial stops must never restart its monotonic origin.
            request.first_attachment.get_or_init(Instant::now);
            request.attached.store(true, Ordering::Release);
            if !request.started_once.swap(true, Ordering::AcqRel) {
                latest_started = Some(request.clone());
            }
            captures.push(Capture(request.clone()));
        }
        if let Some(request) = latest_started {
            state.recording.latest_started = Some(request);
        }
        state.recording.refresh_revision();
    }
}
impl State {
    pub(super) fn recording_context(
        &self,
        intent: &RecordingStart,
        keys: Option<&BTreeSet<String>>,
    ) -> Result<Context, RecordingError> {
        if intent.expected_config != self.config.snapshot.revision {
            return Err(RecordingError::StaleConfig);
        }
        let origin = self
            .origins
            .iter()
            .find(|o| {
                o.graph.stamp == intent.origin.stamp
                    && Arc::ptr_eq(&o.graph.value, &intent.origin.value)
            })
            .ok_or(RecordingError::OriginUnavailable)?;
        if origin.graph.stamp.epoch != self.graph.stamp.epoch {
            return Err(RecordingError::InvalidOrigin);
        }
        let config = self
            .config
            .snapshot
            .configuration
            .as_ref()
            .ok_or(RecordingError::ConfigurationUnknown)?;
        let local = self
            .config
            .snapshot
            .local
            .as_ref()
            .ok_or(RecordingError::Unavailable)?;
        let node = local.node.ok_or(RecordingError::Unavailable)?;
        let observed = origin
            .graph
            .value
            .knopen
            .iter()
            .find(|n| n.id == node && n.naam == opname::BUS_NAAM)
            .ok_or(RecordingError::InvalidOrigin)?;
        let rate = origin
            .graph
            .value
            .rate
            .filter(|r| *r != 0)
            .ok_or(RecordingError::Unavailable)?;
        let defs = if let Some(keys) = keys {
            let defs = config
                .buses
                .definitions()
                .map_err(|_| RecordingError::InvalidInput)?;
            if keys.iter().any(|k| !defs.iter().any(|d| &d.key == k)) {
                return Err(RecordingError::InvalidInput);
            }
            defs.into_iter().filter(|d| keys.contains(&d.key)).collect()
        } else {
            config
                .buses
                .armed()
                .map_err(|_| RecordingError::InvalidInput)?
        };
        if defs.is_empty() {
            return Err(RecordingError::NoArmedTracks);
        }
        let mut tracks = Vec::new();
        let mut references = BTreeSet::from([node]);
        for def in defs {
            let mut ports = Vec::new();
            for channel in &def.channels {
                let name = format!("rec_{}", channel + 1);
                let mut matching = origin.graph.value.poorten.iter().filter(|p| {
                    p.knoop == node
                        && p.ingang
                        && p.soort == pecan_core::graph::Soort::Audio
                        && p.naam == name
                });
                let port = matching
                    .next()
                    .filter(|_| *channel < local.channels)
                    .ok_or(RecordingError::InvalidOrigin)?;
                if matching.next().is_some() {
                    return Err(RecordingError::InvalidOrigin);
                }
                ports.push(port.id);
                references.insert(port.id);
            }
            tracks.push(TrackSpec {
                key: def.key,
                name: def.file,
                channels: def.channels,
                ports,
            });
        }
        let objects = references
            .into_iter()
            .map(|id| (id, origin.objects.get(&id).copied()))
            .collect();
        let context = Context {
            source: origin.graph.stamp,
            config: intent.expected_config,
            config_identity: self.config.identity.clone(),
            ticket: intent.ticket,
            node,
            serial: observed.serial,
            rate,
            objects,
            tracks,
        };
        self.validate_recording(&context, true)?;
        Ok(context)
    }

    pub(super) fn validate_recording(
        &self,
        context: &Context,
        config: bool,
    ) -> Result<(), RecordingError> {
        if self.stopped {
            return Err(RecordingError::Stopped);
        }
        if !self.graph.value.verbonden || self.graph.stamp.epoch != context.source.epoch {
            return Err(RecordingError::Disconnected);
        }
        if config && !Arc::ptr_eq(&self.config.identity, &context.config_identity) {
            return Err(RecordingError::StaleConfig);
        }
        if self.graph.value.rate != Some(context.rate) {
            return Err(RecordingError::FormatChanged);
        }
        if !context
            .objects
            .iter()
            .all(|(id, gen)| gen.is_some() && self.objects.get(id) == gen.as_ref())
        {
            return Err(RecordingError::InvalidOrigin);
        }
        if !self.graph.value.knopen.iter().any(|n| {
            n.id == context.node && n.serial == context.serial && n.naam == opname::BUS_NAAM
        }) {
            return Err(RecordingError::InvalidOrigin);
        }
        for track in &context.tracks {
            for (channel, id) in track.channels.iter().zip(&track.ports) {
                if !self.graph.value.poorten.iter().any(|p| {
                    p.id == *id
                        && p.knoop == context.node
                        && p.ingang
                        && p.soort == pecan_core::graph::Soort::Audio
                        && p.naam == format!("rec_{}", channel + 1)
                }) {
                    return Err(RecordingError::InvalidOrigin);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "recording_tests.rs"]
mod tests;
