//! Native request bookkeeping only. Shares the production worker/admission lock.
use super::*;
use pecan_core::native_command::{
    native_receipt, AdmissionError, Counts, NativeFailure, NativeIntent, NativeProgress,
    NativeReceipt, NativeReporter, MAX_NATIVE_COMMANDS, MAX_NATIVE_IN_FLIGHT,
    NATIVE_SOURCE_HISTORY,
};
use std::time::{Duration, Instant};

pub(super) const OBSERVATION_LIMIT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub(super) struct Origin {
    pub(super) graph: Stamped<Arc<Momentopname>>,
    pub(super) objects: BTreeMap<u32, u64>,
}

#[derive(Clone, Copy)]
pub(super) struct Tag {
    pub id: u64,
    pub index: usize,
}

#[derive(Clone, Copy)]
enum Stage {
    Queued,
    Claimed,
    Submitted,
    NoOp,
    ObservedSubmitted,
    ObservedNoOp,
    Rejected,
}

pub(super) struct Work {
    id: u64,
    source: Stamp,
    pub(super) commands: Vec<Opdracht>,
    objects: Vec<Vec<(u32, Option<u64>)>>,
    stages: Vec<Stage>,
    reporter: NativeReporter,
    deadline: Instant,
    last_execution: Stamp,
    barrier: bool,
}

#[derive(Debug)]
pub(crate) enum LocalExecution {
    /// Legacy callbacks do not have a completion contract. Never upgrade this
    /// to native submission or success implicitly.
    Unreported,
    Submitted,
    Rejected(String),
}

impl Work {
    fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for stage in &self.stages {
            if !matches!(stage, Stage::Queued) {
                counts.attempted += 1;
            }
            if matches!(stage, Stage::Claimed) {
                counts.in_progress += 1;
            }
            if matches!(stage, Stage::Submitted | Stage::ObservedSubmitted) {
                counts.submitted += 1;
            }
            if matches!(stage, Stage::NoOp | Stage::ObservedNoOp) {
                counts.no_ops += 1;
            }
            if matches!(stage, Stage::ObservedSubmitted | Stage::ObservedNoOp) {
                counts.observed += 1;
            }
        }
        counts
    }
    fn locally_done(&self) -> bool {
        self.stages
            .iter()
            .all(|s| !matches!(s, Stage::Queued | Stage::Claimed))
    }
    fn fail(&self, reason: NativeFailure) {
        let counts = self.counts();
        let uncertain =
            counts.submitted > 0 || self.stages.iter().any(|s| matches!(s, Stage::Claimed));
        self.reporter.set(if uncertain {
            NativeProgress::Unconfirmed { reason, counts }
        } else {
            NativeProgress::Rejected { reason, counts }
        });
    }
}

impl Commands {
    /// Copy only node/port generations from the same bounded publication cache
    /// used by native admission. No live state may replace the caller's origin.
    pub(crate) fn history_graph(
        &self,
        graph: &Stamped<Arc<Momentopname>>,
    ) -> Result<pecan_core::history_identity::HistoryGraph, AdmissionError> {
        let state = self.0.lock().unwrap();
        let origin = state
            .origins
            .iter()
            .find(|origin| {
                origin.graph.stamp == graph.stamp && Arc::ptr_eq(&origin.graph.value, &graph.value)
            })
            .ok_or(AdmissionError::OriginUnavailable)?;
        let generations = origin
            .graph
            .value
            .knopen
            .iter()
            .map(|n| n.id)
            .chain(origin.graph.value.poorten.iter().map(|p| p.id))
            .filter_map(|id| origin.objects.get(&id).map(|generation| (id, *generation)))
            .collect();
        Ok(pecan_core::history_identity::HistoryGraph::from_registry(
            origin.graph.clone(),
            state.history_authority.clone(),
            generations,
        ))
    }

    pub(crate) fn enqueue_native(
        &self,
        intent: NativeIntent,
    ) -> Result<NativeReceipt, AdmissionError> {
        self.enqueue_native_at(intent, Instant::now())
    }
    pub(crate) fn enqueue_native_at(
        &self,
        intent: NativeIntent,
        now: Instant,
    ) -> Result<NativeReceipt, AdmissionError> {
        let mut state = self.0.lock().unwrap();
        if state.stopped {
            return Err(AdmissionError::Stopped);
        }
        if intent.origin.stamp.epoch != state.graph.stamp.epoch {
            return Err(AdmissionError::StaleEpoch);
        }
        if !state.graph.value.verbonden {
            return Err(AdmissionError::Disconnected);
        }
        if intent.commands.len() > MAX_NATIVE_COMMANDS {
            return Err(AdmissionError::TooManyCommands);
        }
        for command in &intent.commands {
            validate_input(command)?;
        }
        let origin = state
            .origins
            .iter()
            .find(|o| {
                o.graph.stamp == intent.origin.stamp
                    && Arc::ptr_eq(&o.graph.value, &intent.origin.value)
            })
            .cloned()
            .ok_or(AdmissionError::OriginUnavailable)?;
        let objects: Vec<Vec<_>> = intent
            .commands
            .iter()
            .map(|command| {
                native_references(command, &origin.graph.value)
                    .into_iter()
                    .map(|id| (id, origin.objects.get(&id).copied()))
                    .collect()
            })
            .collect();
        if intent
            .commands
            .iter()
            .zip(&objects)
            .any(|(c, refs)| !objects_match(c, refs, &state.objects))
        {
            return Err(AdmissionError::InvalidObject);
        }
        if state.native.len() >= MAX_NATIVE_IN_FLIGHT {
            return Err(AdmissionError::TooManyInFlight);
        }
        if state.pending.len() + intent.commands.len() > NORMAL_CAPACITY {
            return Err(AdmissionError::QueueFull);
        }
        state.next_native_id += 1;
        let id = state.next_native_id;
        let (receipt, reporter) =
            native_receipt(intent.ticket, intent.origin.stamp, intent.commands.len());
        for (index, (command, refs)) in intent.commands.iter().zip(&objects).enumerate() {
            state.pending.push_back(QueuedCommand {
                command: BackendCommand {
                    epoch: intent.origin.stamp.epoch,
                    command: command.clone(),
                },
                objects: refs.clone(),
                native: Some(Tag { id, index }),
                config: None,
                start: None,
                arming: None,
                migration: None,
            });
        }
        let count = intent.commands.len();
        state.native.push_back(Work {
            id,
            source: intent.origin.stamp,
            commands: intent.commands,
            objects,
            stages: vec![Stage::Queued; count],
            reporter,
            deadline: now + OBSERVATION_LIMIT,
            last_execution: intent.origin.stamp,
            barrier: count == 0,
        });
        Ok(receipt)
    }

    pub(crate) fn native_needs_barrier(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .native
            .iter()
            .any(|w| w.barrier && w.locally_done())
    }

    pub(crate) fn expire_native(&self, now: Instant) {
        let mut state = self.0.lock().unwrap();
        let expired: Vec<_> = state
            .native
            .iter()
            .filter(|w| now >= w.deadline)
            .map(|w| w.id)
            .collect();
        for id in expired {
            state.fail_native(id, NativeFailure::ObservationTimedOut);
        }
    }

    pub(super) fn native_known_rejection(&self, tag: Tag, reason: NativeFailure) {
        let mut state = self.0.lock().unwrap();
        if let Some(work) = state.native.iter_mut().find(|w| w.id == tag.id) {
            work.stages[tag.index] = Stage::Rejected;
        }
        state.fail_native(tag.id, reason);
    }

    pub(super) fn native_finished(&self, tag: Tag, execution: LocalExecution, no_op: bool) {
        let mut state = self.0.lock().unwrap();
        let stamp = state.graph.stamp;
        let Some(work) = state.native.iter_mut().find(|w| w.id == tag.id) else {
            return;
        };
        if no_op {
            work.stages[tag.index] = Stage::NoOp;
            work.barrier = true;
        } else {
            match execution {
                LocalExecution::Submitted => work.stages[tag.index] = Stage::Submitted,
                LocalExecution::Unreported => {
                    state.fail_native(
                        tag.id,
                        NativeFailure::LocalUnavailable(
                            "executor did not report local submission".into(),
                        ),
                    );
                    return;
                }
                LocalExecution::Rejected(error) => {
                    // This command made no RPC submission. Earlier commands
                    // in the same ordered batch can still have effects.
                    work.stages[tag.index] = Stage::Rejected;
                    state.fail_native(tag.id, NativeFailure::LocalUnavailable(error));
                    return;
                }
            }
        }
        work.last_execution = stamp;
        work.reporter
            .set(NativeProgress::ExecutedLocally(work.counts()));
    }
}

impl State {
    pub(super) fn remember_origin(&mut self) {
        self.origins.push_back(Origin {
            graph: self.graph.clone(),
            objects: self.published_objects.clone(),
        });
        while self.origins.len() > NATIVE_SOURCE_HISTORY {
            self.origins.pop_front();
        }
    }
    pub(super) fn fail_native(&mut self, id: u64, reason: NativeFailure) {
        if let Some(index) = self.native.iter().position(|w| w.id == id) {
            self.native.remove(index).unwrap().fail(reason);
            self.pending
                .retain(|q| q.native.is_none_or(|tag| tag.id != id));
        }
    }
    pub(super) fn fail_all_native(&mut self, reason: NativeFailure) {
        while let Some(work) = self.native.pop_front() {
            work.fail(reason.clone());
        }
    }
    pub(super) fn claim_native(&mut self, tag: Tag, now: Instant) -> bool {
        let Some(work) = self.native.iter_mut().find(|w| w.id == tag.id) else {
            return false;
        };
        if now >= work.deadline {
            self.fail_native(tag.id, NativeFailure::ObservationTimedOut);
            return false;
        }
        work.stages[tag.index] = Stage::Claimed;
        work.reporter
            .set(NativeProgress::ExecutedLocally(work.counts()));
        true
    }
    pub(super) fn observe_native(&mut self) {
        let mut terminal = Vec::new();
        for work in &mut self.native {
            if !work.locally_done() {
                continue;
            }
            work.barrier = false;
            if self.graph.stamp.epoch != work.source.epoch {
                terminal.push((work.id, Some(NativeFailure::Disconnected)));
                continue;
            }
            if self.graph.stamp.revision <= work.last_execution.revision {
                continue;
            }
            let mut invalid = false;
            for (i, command) in work.commands.iter().enumerate() {
                if !objects_match(command, &work.objects[i], &self.objects) {
                    invalid = true;
                    break;
                }
                if satisfied(command, &self.graph.value) {
                    work.stages[i] = match work.stages[i] {
                        Stage::Submitted | Stage::ObservedSubmitted => Stage::ObservedSubmitted,
                        _ => Stage::ObservedNoOp,
                    };
                } else {
                    work.stages[i] = match work.stages[i] {
                        Stage::ObservedSubmitted => Stage::Submitted,
                        Stage::ObservedNoOp => Stage::NoOp,
                        other => other,
                    };
                }
            }
            if invalid {
                terminal.push((work.id, Some(NativeFailure::InvalidObject)));
                continue;
            }
            if work
                .stages
                .iter()
                .all(|s| matches!(s, Stage::ObservedNoOp | Stage::ObservedSubmitted))
            {
                let counts = work.counts();
                work.reporter.set(if counts.submitted == 0 {
                    NativeProgress::ObservedNoOp {
                        stamp: self.graph.stamp,
                        counts,
                    }
                } else {
                    NativeProgress::Observed {
                        stamp: self.graph.stamp,
                        counts,
                    }
                });
                terminal.push((work.id, None));
            }
        }
        for (id, failure) in terminal {
            if let Some(reason) = failure {
                self.fail_native(id, reason);
            } else if let Some(i) = self.native.iter().position(|w| w.id == id) {
                self.native.remove(i);
            }
        }
    }
}

pub(super) fn objects_match(
    command: &Opdracht,
    expected: &[(u32, Option<u64>)],
    live: &BTreeMap<u32, u64>,
) -> bool {
    expected.iter().all(|(id, generation)| {
        generation.is_some() && (live.get(id) == generation.as_ref()
            || matches!(command, Opdracht::Ontkoppel { kabel } if kabel == id && !live.contains_key(id)))
    })
}

fn native_references(command: &Opdracht, graph: &Momentopname) -> BTreeSet<u32> {
    let mut ids = referenced_objects(command, graph);
    if let Opdracht::ZetDoel {
        doel: Some(target), ..
    } = command
    {
        // Stable strings are still bound to any concrete target the caller
        // actually observed. Reusing its ID/name must not retarget old intent.
        ids.extend(
            graph
                .knopen
                .iter()
                .filter(|node| {
                    node.naam == *target
                        || node
                            .serial
                            .is_some_and(|serial| serial.to_string() == *target)
                })
                .map(|node| node.id),
        );
    }
    ids
}

fn validate_input(command: &Opdracht) -> Result<(), AdmissionError> {
    match command {
        Opdracht::Verbind { .. } | Opdracht::Ontkoppel { .. } | Opdracht::ZetMute { .. } => Ok(()),
        Opdracht::ZetDoel { doel, .. } => {
            if doel
                .as_ref()
                .is_some_and(|s| s.len() > 4096 || s.contains('\0'))
            {
                Err(AdmissionError::InvalidInput)
            } else {
                Ok(())
            }
        }
        Opdracht::ZetVolume { volume, .. } => {
            if volume.is_empty()
                || volume.len() > 256
                || volume
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=4.0).contains(v))
            {
                Err(AdmissionError::InvalidInput)
            } else {
                Ok(())
            }
        }
        _ => Err(AdmissionError::UnsupportedCommand),
    }
}

pub(crate) fn satisfied(command: &Opdracht, graph: &Momentopname) -> bool {
    match command {
        Opdracht::Verbind { uit, naar } => graph
            .kabels
            .iter()
            .any(|l| l.uit_poort == *uit && l.in_poort == *naar),
        Opdracht::Ontkoppel { kabel } => !graph.kabels.iter().any(|l| l.id == *kabel),
        Opdracht::ZetDoel { knoop, doel } => graph
            .knopen
            .iter()
            .any(|n| n.id == *knoop && n.doel == *doel),
        Opdracht::ZetVolume { knoop, volume } => graph
            .knopen
            .iter()
            .any(|n| n.id == *knoop && n.volume.as_ref() == Some(volume)),
        Opdracht::ZetMute { knoop, mute } => graph
            .knopen
            .iter()
            .any(|n| n.id == *knoop && n.mute == Some(*mute)),
        _ => false,
    }
}
