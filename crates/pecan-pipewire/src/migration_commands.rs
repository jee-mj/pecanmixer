//! Staged idle bus edits on the existing FIFO and server-fed graph mirror.
use super::*;
use crate::{
    bus_config::{self, ConfigTarget},
    bus_migration::*,
};
use pecan_core::{
    domain::{audio::is_bus, buses::Coupling},
    graph::Soort,
};
use std::time::Instant;

impl Commands {
    pub(crate) fn enqueue_migration(
        &self,
        intent: LayoutIntent,
    ) -> Result<MigrationReceipt, MigrationError> {
        self.enqueue_migration_at(intent, Instant::now())
    }
    pub(crate) fn enqueue_migration_at(
        &self,
        intent: LayoutIntent,
        now: Instant,
    ) -> Result<MigrationReceipt, MigrationError> {
        let mut state = self.0.try_lock().map_err(|_| MigrationError::Busy)?;
        if state.stopped {
            return Err(MigrationError::Stopped);
        }
        if !state.graph.value.verbonden {
            return Err(MigrationError::Disconnected);
        }
        if state.migration.is_some() || state.arming.is_some() {
            return Err(MigrationError::Busy);
        }
        state.recording.reap();
        if !state.recording.active.is_empty() || state.config.snapshot.recording {
            return Err(MigrationError::RecordingActive);
        }
        if state.config.exhausted {
            return Err(MigrationError::Configuration(
                bus_config::ConfigError::RevisionExhausted,
            ));
        }
        if intent.expected_config != state.config.snapshot.revision {
            return Err(MigrationError::StaleConfig);
        }
        if intent.expected_recording
            != (crate::recording::RecordingRevision {
                owner: state.config.snapshot.revision.owner,
                revision: state.recording.revision,
            })
        {
            return Err(MigrationError::StaleRecording);
        }
        if state.pending.len() >= NORMAL_CAPACITY {
            return Err(MigrationError::QueueFull);
        }
        let origin = state
            .origins
            .iter()
            .find(|o| {
                o.graph.stamp == intent.origin.stamp
                    && Arc::ptr_eq(&o.graph.value, &intent.origin.value)
            })
            .ok_or(MigrationError::OriginUnavailable)?;
        if origin.graph.stamp.epoch != state.graph.stamp.epoch {
            return Err(MigrationError::Disconnected);
        }
        let before =
            state
                .config
                .snapshot
                .configuration
                .as_ref()
                .ok_or(MigrationError::Configuration(
                    bus_config::ConfigError::Unavailable,
                ))?;
        let local = state
            .config
            .snapshot
            .local
            .as_ref()
            .ok_or(MigrationError::InvalidObject)?;
        let node = local.node.ok_or(MigrationError::InvalidObject)?;
        let owned = origin
            .graph
            .value
            .knopen
            .iter()
            .find(|n| n.id == node && is_bus(n))
            .ok_or(MigrationError::InvalidObject)?;
        let node_generation = *origin
            .objects
            .get(&node)
            .ok_or(MigrationError::InvalidObject)?;
        let definitions = before
            .buses
            .definitions()
            .map_err(|_| MigrationError::InvalidInput)?;
        let key = match &intent.action {
            LayoutAction::Split { bus }
            | LayoutAction::Join { bus }
            | LayoutAction::Remove { bus } => bus,
        };
        if key.len() > 256 {
            return Err(MigrationError::InvalidInput);
        }
        let definition = definitions
            .iter()
            .find(|d| &d.key == key)
            .ok_or(MigrationError::InvalidInput)?;
        let count: usize = definitions.iter().map(|d| d.channels.len()).sum();
        if count != local.channels {
            return Err(MigrationError::InvalidObject);
        }
        let mut ports = Vec::new();
        for i in 0..count {
            let name = format!("rec_{}", i + 1);
            let matching: Vec<_> = origin
                .graph
                .value
                .poorten
                .iter()
                .filter(|p| {
                    p.knoop == node && p.ingang && p.soort == Soort::Audio && p.naam == name
                })
                .take(2)
                .collect();
            if matching.len() != 1 {
                return Err(MigrationError::InvalidObject);
            }
            let p = matching[0];
            ports.push(Port {
                id: p.id,
                node,
                name: p.naam.clone(),
                input: true,
                kind: p.soort,
                generation: *origin
                    .objects
                    .get(&p.id)
                    .ok_or(MigrationError::InvalidObject)?,
            });
        }
        if origin
            .graph
            .value
            .poorten
            .iter()
            .filter(|p| p.knoop == node && p.ingang && p.soort == Soort::Audio)
            .count()
            != count
        {
            return Err(MigrationError::InvalidObject);
        }
        let incoming: Vec<_> = origin
            .graph
            .value
            .kabels
            .iter()
            .filter(|l| l.in_knoop == node && ports.iter().any(|p| p.id == l.in_poort))
            .take(MAX_MIGRATION_LINKS + 1)
            .collect();
        if incoming.len() > MAX_MIGRATION_LINKS {
            return Err(MigrationError::TooManyRoutes);
        }
        let mut proposed = before.clone();
        let mut desired = BTreeSet::new();
        let mut affected = BTreeSet::new();
        match intent.action {
            LayoutAction::Split { .. } if definition.coupling == Some(Coupling::Split) => {
                proposed.buses = before
                    .buses
                    .change_coupling(definition.index)
                    .map_err(|_| MigrationError::InvalidInput)?
            }
            LayoutAction::Join { .. } if definition.coupling == Some(Coupling::Join) => {
                proposed.buses = before
                    .buses
                    .change_coupling(definition.index)
                    .map_err(|_| MigrationError::InvalidInput)?
            }
            LayoutAction::Remove { .. } => {
                // The pure policy only sees the exact validated owner. It cannot select a
                // foreign same-name Rec instance earlier in the full snapshot.
                let scoped = Momentopname {
                    verbonden: true,
                    knopen: vec![owned.clone()],
                    poorten: origin
                        .graph
                        .value
                        .poorten
                        .iter()
                        .filter(|p| ports.iter().any(|owned| owned.id == p.id))
                        .cloned()
                        .collect(),
                    kabels: incoming.iter().map(|l| (*l).clone()).collect(),
                    ..Default::default()
                };
                let edit = before
                    .buses
                    .remove(definition.index, &scoped)
                    .map_err(|_| MigrationError::InvalidInput)?;
                proposed.buses = edit.config;
                if proposed.buses != before.buses {
                    affected.extend(ports.iter().skip(definition.channels[0]).map(|p| p.id));
                }
                for command in edit.commands {
                    if let Opdracht::Verbind { uit, naar } = command {
                        desired.insert((uit, naar));
                    }
                }
            }
            _ => return Err(MigrationError::InvalidInput),
        }
        let initial: BTreeSet<_> = incoming
            .iter()
            .filter(|l| affected.contains(&l.in_poort))
            .map(|l| (l.uit_poort, l.in_poort))
            .collect();
        let obsolete = initial.difference(&desired).copied().collect();
        let mut source_ports = Vec::new();
        let mut source_nodes = BTreeMap::new();
        for id in initial
            .iter()
            .chain(&desired)
            .map(|p| p.0)
            .collect::<BTreeSet<_>>()
        {
            let p = origin
                .graph
                .value
                .poorten
                .iter()
                .find(|p| p.id == id && !p.ingang && p.soort == Soort::Audio)
                .ok_or(MigrationError::InvalidObject)?;
            let n = origin
                .graph
                .value
                .knopen
                .iter()
                .find(|n| n.id == p.knoop)
                .ok_or(MigrationError::InvalidObject)?;
            source_ports.push(Port {
                id,
                node: p.knoop,
                name: p.naam.clone(),
                input: false,
                kind: p.soort,
                generation: *origin
                    .objects
                    .get(&id)
                    .ok_or(MigrationError::InvalidObject)?,
            });
            source_nodes.insert(
                n.id,
                (
                    *origin
                        .objects
                        .get(&n.id)
                        .ok_or(MigrationError::InvalidObject)?,
                    n.serial,
                ),
            );
        }
        if state.pending.iter().any(|q| {
            q.config.is_some()
                || q.start.is_some()
                || q.arming.is_some()
                || touches(
                    &q.command.command,
                    node,
                    &ports,
                    &source_nodes,
                    &origin.graph.value,
                )
        }) || state.native.iter().any(|w| {
            w.commands
                .iter()
                .any(|c| touches(c, node, &ports, &source_nodes, &origin.graph.value))
        }) {
            return Err(MigrationError::Busy);
        }
        let (channels, gains, options) =
            bus_config::validate_configuration(&proposed).map_err(MigrationError::Configuration)?;
        let receipt = MigrationReceipt(Arc::new(std::sync::Mutex::new(MigrationReport {
            ticket: intent.ticket,
            source: intent.origin.stamp,
            expected_config: intent.expected_config,
            expected_recording: intent.expected_recording,
            progress: MigrationProgress::Queued,
            configuration: state.config.snapshot.clone(),
            proposed: proposed.clone(),
            attempted: 0,
            submitted: 0,
            no_ops: 0,
            in_progress: 0,
            new_observed: None,
            old_observed: None,
            ports_observed: None,
            resize: None,
            local_readback: None,
            cancellation_requested: false,
            failure: None,
        })));
        let work = Work {
            receipt: receipt.clone(),
            source: intent.origin.stamp,
            node,
            serial: owned.serial,
            node_generation,
            ports,
            source_ports,
            source_nodes,
            affected,
            initial,
            desired,
            obsolete,
            connected: BTreeSet::new(),
            destroyed: BTreeSet::new(),
            proposed,
            gains,
            options,
            channels,
            expected_identity: state.config.identity.clone(),
            expected_recording: intent.expected_recording,
            phase: MigrationProgress::Queued,
            claimed: false,
            queued: true,
            in_progress: false,
            cancelled: None,
            local_applied: false,
            start: now,
            deadline: now + TOTAL_LIMIT,
            last_submission: None,
            finished: false,
        };
        state.validate_migration(&work)?;
        state.pending.push_back(QueuedCommand {
            command: BackendCommand {
                epoch: work.source.epoch,
                command: Opdracht::Start,
            },
            objects: references(&work, None)?,
            native: None,
            config: None,
            start: None,
            arming: None,
            migration: Some(work.tag(Kind::Begin)),
        });
        state.migration = Some(work);
        Ok(receipt)
    }
    pub(crate) fn drive_migration_at(&self, now: Instant) {
        let mut state = self.0.lock().unwrap();
        let Some(mut w) = state.migration.take() else {
            return;
        };
        if w.in_progress {
            state.migration = Some(w);
            return;
        }
        if let Err(error) = state.validate_migration(&w) {
            state.finish_migration(w, error);
            return;
        }
        if now.saturating_duration_since(w.start) >= TOTAL_LIMIT
            || (matches!(
                w.phase,
                MigrationProgress::AwaitingNew
                    | MigrationProgress::AwaitingOld
                    | MigrationProgress::AwaitingPorts
            ) && now >= w.deadline)
        {
            state.finish_migration(w, MigrationError::ObservationTimedOut);
            return;
        }
        if !w.claimed || w.queued {
            state.migration = Some(w);
            return;
        }
        let pairs = current_pairs(&w, &state.graph.value);
        let fresh = w
            .last_submission
            .is_none_or(|stamp| state.graph.stamp.revision > stamp.revision);
        let mut schedule = None;
        match w.phase {
            MigrationProgress::Connecting => {
                if let Some(pair) = w.desired.iter().find(|p| !w.connected.contains(p)).copied() {
                    schedule = Some((
                        Kind::Connect(pair),
                        Opdracht::Verbind {
                            uit: pair.0,
                            naar: pair.1,
                        },
                    ));
                } else {
                    w.phase = MigrationProgress::AwaitingNew;
                    w.deadline = now + OBSERVATION_LIMIT;
                }
            }
            MigrationProgress::AwaitingNew if fresh && w.desired.is_subset(&pairs) => {
                w.receipt.0.lock().unwrap().new_observed = Some(state.graph.stamp);
                w.phase = MigrationProgress::Disconnecting;
                w.last_submission = None;
            }
            MigrationProgress::Disconnecting | MigrationProgress::AwaitingOld => {
                let old = state
                    .graph
                    .value
                    .kabels
                    .iter()
                    .filter(|l| w.obsolete.contains(&(l.uit_poort, l.in_poort)))
                    .find(|l| {
                        state
                            .objects
                            .get(&l.id)
                            .is_none_or(|g| !w.destroyed.contains(&(l.id, *g)))
                    });
                if let Some(link) = old {
                    schedule = Some((
                        Kind::Disconnect {
                            pair: (link.uit_poort, link.in_poort),
                            id: link.id,
                            generation: match state.objects.get(&link.id) {
                                Some(g) => *g,
                                None => {
                                    state.finish_migration(w, MigrationError::InvalidObject);
                                    return;
                                }
                            },
                        },
                        Opdracht::Ontkoppel { kabel: link.id },
                    ));
                    w.phase = MigrationProgress::Disconnecting;
                } else if w.obsolete.is_disjoint(&pairs) && fresh {
                    w.receipt.0.lock().unwrap().old_observed = Some(state.graph.stamp);
                    w.phase = MigrationProgress::ApplyingLocal;
                } else if w.phase == MigrationProgress::Disconnecting {
                    w.phase = MigrationProgress::AwaitingOld;
                    w.deadline = now + OBSERVATION_LIMIT;
                }
            }
            MigrationProgress::ApplyingLocal => {
                schedule = Some((
                    Kind::Apply,
                    Opdracht::ZetBus {
                        kanalen: w.channels,
                        gain: w.gains.clone(),
                    },
                ))
            }
            MigrationProgress::AwaitingPorts if fresh && state.final_ports(&w) => {
                w.receipt.0.lock().unwrap().ports_observed = Some(state.graph.stamp);
                state.config.snapshot.configuration = Some(w.proposed.clone());
                state.config.snapshot.transport = Some(w.options.clone());
                state.config.bump();
                state.complete_migration(w, MigrationOutcome::Observed, None);
                return;
            }
            _ => {}
        }
        if let Some((kind, command)) = schedule {
            if state.pending.len() < NORMAL_CAPACITY {
                let refs = references(
                    &w,
                    if let Kind::Disconnect { id, .. } = kind {
                        Some((
                            id,
                            state
                                .objects
                                .get(&id)
                                .copied()
                                .ok_or(MigrationError::InvalidObject),
                        ))
                    } else {
                        None
                    },
                );
                // Missing global generations fail before dispatch rather than guessing IDs.
                let refs = match refs {
                    Ok(refs) => refs,
                    Err(error) => {
                        state.finish_migration(w, error);
                        return;
                    }
                };
                state.pending.push_back(QueuedCommand {
                    command: BackendCommand {
                        epoch: w.source.epoch,
                        command,
                    },
                    objects: refs,
                    native: None,
                    config: None,
                    start: None,
                    arming: None,
                    migration: Some(w.tag(kind)),
                });
                w.queued = true;
            }
        }
        state.publish_migration(&w);
        state.migration = Some(w);
    }
    pub(super) fn claim_migration(&self, tag: &Tag, command: &Opdracht, now: Instant) -> Claim {
        let mut state = self.0.lock().unwrap();
        let Some(mut w) = state.migration.take() else {
            return Claim::Skip;
        };
        if !Arc::ptr_eq(&tag.receipt.0, &w.receipt.0) {
            state.migration = Some(w);
            return Claim::Skip;
        }
        w.queued = false;
        if let Err(error) = state.validate_migration(&w) {
            state.finish_migration(w, error);
            return Claim::Skip;
        }
        if now.saturating_duration_since(w.start) >= TOTAL_LIMIT {
            state.finish_migration(w, MigrationError::ObservationTimedOut);
            return Claim::Skip;
        }
        let valid_tag = match tag.kind {
            Kind::Begin => w.phase == MigrationProgress::Queued,
            Kind::Connect(pair) => {
                w.phase == MigrationProgress::Connecting && w.desired.contains(&pair)
            }
            Kind::Disconnect { pair, id, .. } => {
                w.phase == MigrationProgress::Disconnecting
                    && w.obsolete.contains(&pair)
                    && state
                        .graph
                        .value
                        .kabels
                        .iter()
                        .find(|l| l.id == id)
                        .is_none_or(|l| (l.uit_poort, l.in_poort) == pair)
            }
            Kind::Apply => w.phase == MigrationProgress::ApplyingLocal,
        };
        if !valid_tag {
            state.finish_migration(w, MigrationError::InvalidObject);
            return Claim::Skip;
        }
        if matches!(tag.kind, Kind::Begin) {
            w.claimed = true;
            w.phase = if w.channels < w.ports.len() {
                MigrationProgress::Connecting
            } else {
                MigrationProgress::ApplyingLocal
            };
            state.publish_migration(&w);
            state.migration = Some(w);
            return Claim::Control;
        }
        if matches!(tag.kind, Kind::Apply) {
            state.migration = Some(w);
            return Claim::Control;
        }
        let mut report = w.receipt.0.lock().unwrap();
        if report.attempted >= MAX_MIGRATION_CALLS {
            drop(report);
            state.finish_migration(w, MigrationError::CallBudget);
            return Claim::Skip;
        }
        report.attempted += 1;
        let noop = match tag.kind {
            Kind::Connect(pair) => current_pairs(&w, &state.graph.value).contains(&pair),
            Kind::Disconnect { id, .. } => !state.graph.value.kabels.iter().any(|l| l.id == id),
            _ => false,
        };
        if noop {
            report.no_ops += 1;
            drop(report);
            if let Kind::Connect(pair) = tag.kind {
                w.connected.insert(pair);
            }
            state.publish_migration(&w);
            state.migration = Some(w);
            return Claim::Skip;
        }
        report.in_progress = 1;
        drop(report);
        w.in_progress = true;
        w.last_submission = Some(state.graph.stamp);
        state.publish_migration(&w);
        state.migration = Some(w);
        Claim::Execute(command.clone())
    }
    pub(super) fn migration_rpc_finished(&self, tag: &Tag, result: LocalExecution) {
        let mut state = self.0.lock().unwrap();
        let Some(mut w) = state.migration.take() else {
            return;
        };
        if !Arc::ptr_eq(&w.receipt.0, &tag.receipt.0) {
            state.migration = Some(w);
            return;
        }
        w.in_progress = false;
        w.receipt.0.lock().unwrap().in_progress = 0;
        match result {
            LocalExecution::Submitted => {
                w.receipt.0.lock().unwrap().submitted += 1;
                match tag.kind {
                    Kind::Connect(pair) => {
                        w.connected.insert(pair);
                    }
                    Kind::Disconnect { id, generation, .. } => {
                        w.destroyed.insert((id, generation));
                    }
                    _ => {}
                }
            }
            LocalExecution::Rejected(error) => {
                state.finish_migration(w, MigrationError::Local(error_text(&error)));
                return;
            }
            LocalExecution::Unreported => {
                state.complete_migration(
                    w,
                    MigrationOutcome::Unconfirmed,
                    Some(MigrationError::Local(
                        "executor did not confirm local submission".into(),
                    )),
                );
                return;
            }
        }
        if let Some(error) = w.cancelled.clone() {
            state.finish_migration(w, error);
            return;
        }
        state.publish_migration(&w);
        state.migration = Some(w);
    }
    pub(crate) fn apply_migration(
        &self,
        receipt: &MigrationReceipt,
        target: &mut impl ConfigTarget,
    ) {
        // These target methods can inspect Commands; never call them under its lock.
        let active = target.recording() || target.config_locked();
        let actual = target.readback();
        let Some(snapshot) = self.config_snapshot() else {
            // A busy read is not an options mismatch. The next bounded tick
            // will reschedule Apply, subject to the operation lifetime.
            return;
        };
        let expected_options = snapshot.transport;
        let options_match = expected_options
            .as_ref()
            .is_some_and(|o| target.options_match(o));
        let claimed = {
            let mut state = self.0.lock().unwrap();
            let Some(mut w) = state.migration.take() else {
                return;
            };
            if !Arc::ptr_eq(&w.receipt.0, &receipt.0) {
                state.migration = Some(w);
                return;
            }
            if let Err(error) = state.validate_migration(&w) {
                state.finish_migration(w, error);
                return;
            }
            if w.phase != MigrationProgress::ApplyingLocal {
                state.finish_migration(w, MigrationError::InvalidInput);
                return;
            }
            if active {
                w.receipt.0.lock().unwrap().local_readback = actual.clone();
                state.config.refresh(actual, active, false);
                state.finish_migration(w, MigrationError::RecordingActive);
                return;
            }
            let before = state.config.snapshot.clone();
            if actual != before.local || before.transport != expected_options || !options_match {
                w.receipt.0.lock().unwrap().local_readback = actual.clone();
                state.config.refresh(actual, active, false);
                if !options_match {
                    state.config.snapshot.configuration = None;
                    state.config.snapshot.transport = None;
                    state.config.bump();
                }
                state.finish_migration(
                    w,
                    MigrationError::Configuration(bus_config::ConfigError::ReadbackMismatch),
                );
                return;
            }
            w.in_progress = true;
            w.receipt.0.lock().unwrap().in_progress = 1;
            w.last_submission = Some(state.graph.stamp);
            let data = (w.channels, w.gains.clone(), w.options.clone(), before);
            state.publish_migration(&w);
            state.migration = Some(w);
            data
        };
        let (channels, gains, options, before) = claimed;
        let resize =
            (before.local.as_ref().unwrap().channels != channels).then(|| target.resize(channels));
        let mut error = resize
            .as_ref()
            .filter(|r| !r.is_complete())
            .map(|_| MigrationError::Configuration(bus_config::ConfigError::ResizeFailed));
        if error.is_none() {
            target.gains(&gains);
            target.options(options.clone());
        }
        let after = target.readback();
        let active_after = target.recording() || target.config_locked();
        if error.is_none() && active_after {
            error = Some(MigrationError::RecordingActive);
        }
        if error.is_none()
            && (!after.as_ref().is_some_and(|l| {
                l.node == before.local.as_ref().unwrap().node
                    && l.channels == channels
                    && l.gains == gains
            }) || !target.options_match(&options))
        {
            error = Some(MigrationError::Configuration(
                bus_config::ConfigError::ReadbackMismatch,
            ));
        }
        let mut state = self.0.lock().unwrap();
        let Some(mut w) = state.migration.take() else {
            return;
        };
        w.in_progress = false;
        {
            let mut report = w.receipt.0.lock().unwrap();
            report.in_progress = 0;
            report.resize = resize;
            report.local_readback = after.clone();
        }
        let changed = after != before.local
            || before.configuration.as_ref() != Some(&w.proposed)
            || error.is_some();
        w.local_applied = changed;
        if state.graph.stamp.epoch != w.source.epoch
            || !Arc::ptr_eq(&state.config.identity, &w.expected_identity)
        {
            let reason = w.cancelled.clone().unwrap_or(MigrationError::StaleConfig);
            state.finish_migration(w, reason);
            return;
        }
        // The local adapter runs outside the state lock. Epoch/config equality
        // alone cannot authenticate objects replaced during its callbacks.
        // Recheck the captured surviving graph scope after the last accessor,
        // before assigning its readback or publishing a local-only completion.
        if let Err(reason) = state.validate_migration_graph(&w) {
            w.local_applied = true;
            state.config.snapshot.local = None;
            state.config.snapshot.configuration = None;
            state.config.snapshot.transport = None;
            state.config.bump();
            state.finish_migration(w, reason);
            return;
        }
        state.config.snapshot.local = after;
        state.config.snapshot.recording = active_after;
        state.config.snapshot.configuration = if error.is_none() && w.channels == w.ports.len() {
            Some(w.proposed.clone())
        } else {
            None
        };
        state.config.snapshot.transport = if error.is_none() { Some(options) } else { None };
        if changed {
            state.config.bump();
        }
        w.expected_identity = state.config.identity.clone();
        if let Some(reason) = error.or(w.cancelled.clone()) {
            state.finish_migration(w, reason);
            return;
        }
        if w.channels == w.ports.len() {
            state.complete_migration(
                w,
                if changed {
                    MigrationOutcome::AppliedLocally
                } else {
                    MigrationOutcome::NoOpLocally
                },
                None,
            );
            return;
        }
        w.phase = MigrationProgress::AwaitingPorts;
        w.deadline = Instant::now() + OBSERVATION_LIMIT;
        state.publish_migration(&w);
        state.migration = Some(w);
    }
    pub(crate) fn cancel_migration_receipt(
        &self,
        receipt: &MigrationReceipt,
    ) -> Result<(), MigrationError> {
        let mut state = self.0.try_lock().map_err(|_| MigrationError::Busy)?;
        if receipt
            .0
            .try_lock()
            .map_err(|_| MigrationError::Busy)?
            .expected_config
            .owner
            != state.config.snapshot.revision.owner
        {
            return Err(MigrationError::InvalidObject);
        }
        if state
            .migration
            .as_ref()
            .is_some_and(|w| Arc::ptr_eq(&w.receipt.0, &receipt.0))
        {
            state.cancel_migration(MigrationError::Cancelled);
            return Ok(());
        }
        if receipt.try_report().is_some_and(|r| r.is_decided()) {
            Ok(())
        } else {
            Err(MigrationError::InvalidObject)
        }
    }
    #[cfg(test)]
    pub(super) fn reject_migration_apply(&self, receipt: &MigrationReceipt) {
        let mut state = self.0.lock().unwrap();
        if state
            .migration
            .as_ref()
            .is_some_and(|w| Arc::ptr_eq(&w.receipt.0, &receipt.0))
        {
            state.cancel_migration(MigrationError::Local(
                "local apply adapter unavailable".into(),
            ));
        }
    }
}

impl State {
    pub(super) fn migration_blocks(&self, command: &Opdracht) -> bool {
        self.migration.as_ref().is_some_and(|w| {
            touches(
                command,
                w.node,
                &w.ports,
                &w.source_nodes,
                &self.graph.value,
            )
        })
    }
    fn validate_migration(&self, w: &Work) -> Result<(), MigrationError> {
        if self.stopped {
            return Err(MigrationError::Stopped);
        }
        if !self.graph.value.verbonden || self.graph.stamp.epoch != w.source.epoch {
            return Err(MigrationError::Disconnected);
        }
        if w.cancelled.is_some() {
            return Err(w.cancelled.clone().unwrap());
        }
        if !Arc::ptr_eq(&self.config.identity, &w.expected_identity) {
            return Err(MigrationError::StaleConfig);
        }
        if self.config.exhausted {
            return Err(MigrationError::Configuration(
                bus_config::ConfigError::RevisionExhausted,
            ));
        }
        if self.config.snapshot.recording
            || !self.recording.active.is_empty()
            || self.arming.is_some()
        {
            return Err(MigrationError::RecordingActive);
        }
        if w.expected_recording
            != (crate::recording::RecordingRevision {
                owner: self.config.snapshot.revision.owner,
                revision: self.recording.revision,
            })
        {
            return Err(MigrationError::StaleRecording);
        }
        self.validate_migration_graph(w)
    }
    fn validate_migration_graph(&self, w: &Work) -> Result<(), MigrationError> {
        if self.objects.get(&w.node) != Some(&w.node_generation)
            || !self
                .graph
                .value
                .knopen
                .iter()
                .any(|n| n.id == w.node && n.serial == w.serial && is_bus(n))
        {
            return Err(MigrationError::InvalidObject);
        }
        for p in w
            .ports
            .iter()
            .take(if w.local_applied {
                w.channels
            } else {
                w.ports.len()
            })
            .chain(&w.source_ports)
        {
            if !port_matches(p, self) {
                return Err(MigrationError::InvalidObject);
            }
        }
        for (id, (generation, serial)) in &w.source_nodes {
            if self.objects.get(id) != Some(generation)
                || !self
                    .graph
                    .value
                    .knopen
                    .iter()
                    .any(|n| n.id == *id && n.serial == *serial)
            {
                return Err(MigrationError::InvalidObject);
            }
        }
        if self.graph.value.poorten.iter().any(|p| {
            p.knoop == w.node
                && p.ingang
                && p.soort == Soort::Audio
                && !w.ports.iter().any(|old| old.id == p.id)
        }) {
            return Err(MigrationError::ExternalConflict);
        }
        let pairs = current_pairs(w, &self.graph.value);
        let allowed: BTreeSet<_> = w.initial.union(&w.desired).copied().collect();
        if !pairs.is_subset(&allowed) {
            return Err(MigrationError::ExternalConflict);
        }
        if matches!(
            w.phase,
            MigrationProgress::Queued
                | MigrationProgress::Connecting
                | MigrationProgress::AwaitingNew
        ) && !w.initial.is_subset(&pairs)
        {
            return Err(MigrationError::ExternalConflict);
        }
        if matches!(
            w.phase,
            MigrationProgress::Disconnecting
                | MigrationProgress::AwaitingOld
                | MigrationProgress::ApplyingLocal
                | MigrationProgress::AwaitingPorts
        ) && !w.desired.is_subset(&pairs)
        {
            return Err(MigrationError::ExternalConflict);
        }
        if matches!(
            w.phase,
            MigrationProgress::ApplyingLocal | MigrationProgress::AwaitingPorts
        ) && !w.obsolete.is_disjoint(&pairs)
        {
            return Err(MigrationError::ExternalConflict);
        }
        Ok(())
    }
    fn final_ports(&self, w: &Work) -> bool {
        let actual: Vec<_> = self
            .graph
            .value
            .poorten
            .iter()
            .filter(|p| p.knoop == w.node && p.ingang && p.soort == Soort::Audio)
            .collect();
        actual.len() == w.channels
            && w.ports
                .iter()
                .take(w.channels)
                .all(|p| port_matches(p, self))
            && w.ports
                .iter()
                .skip(w.channels)
                .all(|p| self.objects.get(&p.id) != Some(&p.generation))
    }
    fn publish_migration(&self, w: &Work) {
        let mut report = w.receipt.0.lock().unwrap();
        report.progress = if w.cancelled.is_some() {
            MigrationProgress::Cancelling
        } else {
            w.phase
        };
        report.configuration = self.config.snapshot.clone();
        report.cancellation_requested = w.cancelled.is_some();
    }
    fn complete_migration(
        &mut self,
        mut w: Work,
        outcome: MigrationOutcome,
        failure: Option<MigrationError>,
    ) {
        self.publish_migration(&w);
        let mut report = w.receipt.0.lock().unwrap();
        report.progress = MigrationProgress::Completed(outcome);
        report.failure = failure;
        drop(report);
        w.finished = true;
        self.pending.retain(|q| {
            q.migration
                .as_ref()
                .is_none_or(|t| !Arc::ptr_eq(&t.receipt.0, &w.receipt.0))
        });
    }
    fn finish_migration(&mut self, w: Work, error: MigrationError) {
        let submitted = w.receipt.0.lock().unwrap().submitted;
        let outcome = if w.local_applied {
            MigrationOutcome::PartiallyApplied
        } else if submitted > 0 {
            MigrationOutcome::Unconfirmed
        } else {
            MigrationOutcome::Rejected
        };
        self.complete_migration(w, outcome, Some(error));
    }
    pub(super) fn cancel_migration(&mut self, error: MigrationError) {
        let Some(mut w) = self.migration.take() else {
            return;
        };
        w.cancelled = Some(error.clone());
        self.pending.retain(|q| {
            q.migration
                .as_ref()
                .is_none_or(|t| !Arc::ptr_eq(&t.receipt.0, &w.receipt.0))
        });
        if w.in_progress {
            self.publish_migration(&w);
            self.migration = Some(w);
        } else {
            self.finish_migration(w, error);
        }
    }
}
fn current_pairs(w: &Work, graph: &Momentopname) -> BTreeSet<Pair> {
    graph
        .kabels
        .iter()
        .filter(|l| l.in_knoop == w.node && w.affected.contains(&l.in_poort))
        .map(|l| (l.uit_poort, l.in_poort))
        .collect()
}
fn port_matches(p: &Port, state: &State) -> bool {
    state.objects.get(&p.id) == Some(&p.generation)
        && state.graph.value.poorten.iter().any(|q| {
            q.id == p.id
                && q.knoop == p.node
                && q.naam == p.name
                && q.ingang == p.input
                && q.soort == p.kind
        })
}
fn references(
    w: &Work,
    link: Option<(u32, Result<u64, MigrationError>)>,
) -> Result<Vec<(u32, Option<u64>)>, MigrationError> {
    let mut refs: BTreeMap<_, _> = w
        .ports
        .iter()
        .chain(&w.source_ports)
        .map(|p| (p.id, Some(p.generation)))
        .chain(w.source_nodes.iter().map(|(id, (g, _))| (*id, Some(*g))))
        .chain([(w.node, Some(w.node_generation))])
        .collect();
    if let Some((id, generation)) = link {
        refs.insert(id, Some(generation?));
    }
    Ok(refs.into_iter().collect())
}
fn touches(
    command: &Opdracht,
    node: u32,
    ports: &[Port],
    sources: &BTreeMap<u32, (u64, Option<u64>)>,
    graph: &Momentopname,
) -> bool {
    match command {
        Opdracht::ZetBus { .. }
        | Opdracht::ZetTransport(_)
        | Opdracht::Start
        | Opdracht::StartBus(_) => true,
        Opdracht::Verbind { uit, naar } => ports.iter().any(|p| p.id == *uit || p.id == *naar),
        Opdracht::Ontkoppel { kabel } => graph
            .kabels
            .iter()
            .any(|l| l.id == *kabel && (l.in_knoop == node || l.uit_knoop == node)),
        Opdracht::ZetDoel { knoop, .. } => *knoop == node || sources.contains_key(knoop),
        Opdracht::ZetVolume { knoop, .. } | Opdracht::ZetMute { knoop, .. } => *knoop == node,
        Opdracht::Stop | Opdracht::StopBus(_) => false,
    }
}
fn error_text(error: &str) -> String {
    crate::writer_status::error_text(error)
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
