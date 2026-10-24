//! The production command boundary. No PipeWire objects cross this queue.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use pecan_core::backend::{
    validate_command, BackendCommand, BackendEvent, DispatchError, EventSequencer, Stamp, Stamped,
};
use pecan_core::graph::{Momentopname, Opdracht};

#[path = "native_commands.rs"]
mod native;
pub(crate) use native::LocalExecution;
use pecan_core::native_command::NativeFailure;

/// Bound both queued work and the number of commands dispatched per graph tick.
pub(crate) const CAPACITY: usize = 256;
/// Keep the final FIFO slot available for stop-all. It is appended in order;
/// ordinary work cannot crowd it out and existing work is never evicted.
pub(crate) const NORMAL_CAPACITY: usize = CAPACITY - 1;
pub(crate) const PER_TICK: usize = 32;

pub(crate) struct Commands(Mutex<State>);

struct State {
    sequence: EventSequencer,
    graph: Stamped<Arc<Momentopname>>,
    pending: VecDeque<QueuedCommand>,
    generation: u64,
    objects: BTreeMap<u32, u64>,
    published_objects: BTreeMap<u32, u64>,
    stopped: bool,
    origins: VecDeque<native::Origin>,
    native: VecDeque<native::Work>,
    next_native_id: u64,
}

struct QueuedCommand {
    command: BackendCommand,
    /// Captured under the admission lock; never rebound during execution.
    objects: Vec<(u32, Option<u64>)>,
    native: Option<native::Tag>,
}

impl Default for Commands {
    fn default() -> Self {
        Self(Mutex::new(State {
            sequence: EventSequencer::default(),
            graph: Stamped {
                stamp: Stamp::default(),
                value: Arc::default(),
            },
            pending: VecDeque::new(),
            generation: 0,
            objects: BTreeMap::new(),
            published_objects: BTreeMap::new(),
            stopped: false,
            origins: VecDeque::new(),
            native: VecDeque::new(),
            next_native_id: 0,
        }))
    }
}

impl Commands {
    /// Read-only registry metadata for worker-owned capture reconciliation.
    /// Called on structural graph updates, never from an audio process callback.
    pub(crate) fn registry_generations(&self) -> BTreeMap<u32, u64> {
        self.0.lock().unwrap().objects.clone()
    }

    pub(crate) fn current(&self) -> Stamped<Arc<Momentopname>> {
        self.0.lock().unwrap().graph.clone()
    }

    /// Called only by the worker, including on every disconnect. The epoch is
    /// shared by graph publication and command execution, across all sessions.
    pub(crate) fn publish(&self, graph: Momentopname) -> Stamped<Arc<Momentopname>> {
        let mut state = self.0.lock().unwrap();
        let BackendEvent::Graph(graph) = state.sequence.graph(graph) else {
            unreachable!()
        };
        if graph.stamp.epoch != state.graph.stamp.epoch {
            // Invalidate admission state and release old capacity under the
            // same lock. No producer can enqueue against a half-transition.
            state.fail_all_native(NativeFailure::Disconnected);
            state.pending.clear();
        }
        if !graph.value.verbonden {
            state.objects.clear();
        }
        state.published_objects = state.objects.clone();
        state.graph = graph.clone();
        state.remember_origin();
        state.observe_native();
        graph
    }

    pub(crate) fn enqueue(&self, command: BackendCommand) -> Result<(), DispatchError> {
        self.0.lock().unwrap().enqueue(command)
    }

    /// Registry lifecycle, on the same worker as drain. Even an object with
    /// identical properties and a reused ID belongs to a new generation.
    pub(crate) fn object_added(&self, id: u32) {
        let mut state = self.0.lock().unwrap();
        state.generation += 1;
        let generation = state.generation;
        state.objects.insert(id, generation);
    }

    pub(crate) fn object_removed(&self, id: u32) {
        self.0.lock().unwrap().objects.remove(&id);
    }

    /// The legacy IPC has no epoch field. Stamp at admission, atomically with
    /// checking connection state; never re-stamp pending work after reconnect.
    pub(crate) fn legacy(&self, command: Opdracht) -> Result<(), DispatchError> {
        let mut state = self.0.lock().unwrap();
        let epoch = state.graph.stamp.epoch;
        state.enqueue(BackendCommand { epoch, command })
    }

    pub(crate) fn stop(&self) {
        let mut state = self.0.lock().unwrap();
        state.stopped = true;
        state.fail_all_native(NativeFailure::Stopped);
        state.pending.clear();
    }

    /// Run on the PipeWire worker, immediately before the existing executor.
    /// `valid_objects` reads the live registry mirror, not the last UI snapshot.
    /// Session transitions also run on this worker, so cannot interleave with
    /// validation/execution. Producers never execute commands themselves.
    /// Cancellation discards queued work. One command already dequeued may
    /// still execute after cancellation; execution is not rolled back.
    #[cfg(test)]
    pub(crate) fn drain(
        &self,
        valid_objects: impl FnMut(&Opdracht) -> bool,
        mut execute: impl FnMut(Opdracht),
    ) -> Vec<DispatchError> {
        self.drain_with(
            valid_objects,
            |_| false,
            |command| {
                execute(command);
                LocalExecution::Unreported
            },
        )
    }

    pub(crate) fn drain_with(
        &self,
        mut valid_objects: impl FnMut(&Opdracht) -> bool,
        mut no_op: impl FnMut(&Opdracht) -> bool,
        mut execute: impl FnMut(Opdracht) -> LocalExecution,
    ) -> Vec<DispatchError> {
        let mut rejected = Vec::new();
        for _ in 0..PER_TICK {
            let queued = {
                let mut state = self.0.lock().unwrap();
                if state.stopped {
                    break;
                }
                let Some(queued) = state.pending.pop_front() else {
                    break;
                };
                let validation = validate_command(&queued.command, &state.graph);
                let identity_ok = if queued.native.is_some() {
                    native::objects_match(&queued.command.command, &queued.objects, &state.objects)
                } else {
                    queued.objects.iter().all(|(id, expected)| {
                        expected.is_some() && state.objects.get(id) == expected.as_ref()
                    })
                };
                if validation.is_err() || !identity_ok {
                    if let Some(tag) = queued.native {
                        state.fail_native(
                            tag.id,
                            if validation.is_err() {
                                NativeFailure::Disconnected
                            } else {
                                NativeFailure::InvalidObject
                            },
                        );
                    } else {
                        rejected.push(validation.err().unwrap_or(DispatchError::InvalidObject));
                    }
                    continue;
                }
                if let Some(tag) = queued.native {
                    if !state.claim_native(tag, std::time::Instant::now()) {
                        continue;
                    }
                }
                queued
            };
            let confirmed_no_op = queued.native.is_some() && no_op(&queued.command.command);
            let already_disconnected_link =
                confirmed_no_op && matches!(&queued.command.command, Opdracht::Ontkoppel { .. });
            if !already_disconnected_link && !valid_objects(&queued.command.command) {
                if let Some(tag) = queued.native {
                    self.native_known_rejection(tag, NativeFailure::InvalidObject);
                } else {
                    rejected.push(DispatchError::InvalidObject);
                }
                continue;
            }
            if confirmed_no_op {
                self.native_finished(queued.native.unwrap(), LocalExecution::Unreported, true);
                continue;
            }
            let execution = execute(queued.command.command);
            if let Some(tag) = queued.native {
                self.native_finished(tag, execution, false);
            }
        }
        rejected
    }
}

impl State {
    fn enqueue(&mut self, command: BackendCommand) -> Result<(), DispatchError> {
        if self.stopped {
            return Err(DispatchError::Stopped);
        }
        validate_command(&command, &self.graph)?;
        let stop_all = matches!(&command.command, Opdracht::Stop);
        if stop_all
            && self
                .pending
                .back()
                .is_some_and(|queued| matches!(&queued.command.command, Opdracht::Stop))
        {
            // Adjacent stops have the same effect. Never coalesce across any
            // intervening work, e.g. Stop, Start, Stop must keep that order.
            return Ok(());
        }
        let capacity = if stop_all { CAPACITY } else { NORMAL_CAPACITY };
        if self.pending.len() >= capacity {
            return Err(DispatchError::QueueFull);
        }
        let objects = referenced_objects(&command.command, &self.graph.value)
            .into_iter()
            .map(|id| (id, self.published_objects.get(&id).copied()))
            .collect();
        self.pending.push_back(QueuedCommand {
            command,
            objects,
            native: None,
        });
        Ok(())
    }
}

/// Include port owners and link endpoints as well as primary command IDs.
/// A parent node can disappear before its ports' remove notifications arrive.
fn referenced_objects(command: &Opdracht, graph: &Momentopname) -> BTreeSet<u32> {
    let mut ids = BTreeSet::new();
    let mut port = |id| {
        ids.insert(id);
        if let Some(port) = graph.poorten.iter().find(|port| port.id == id) {
            ids.insert(port.knoop);
        }
    };
    match command {
        Opdracht::Verbind { uit, naar } => {
            port(*uit);
            port(*naar);
        }
        Opdracht::ZetTransport(options) => {
            if let Some(id) = options.klok {
                port(id);
            }
        }
        Opdracht::Ontkoppel { kabel } => {
            if let Some(link) = graph.kabels.iter().find(|link| link.id == *kabel) {
                port(link.uit_poort);
                port(link.in_poort);
                ids.insert(link.uit_knoop);
                ids.insert(link.in_knoop);
            }
            ids.insert(*kabel);
        }
        Opdracht::ZetDoel { knoop, .. }
        | Opdracht::ZetVolume { knoop, .. }
        | Opdracht::ZetMute { knoop, .. } => {
            ids.insert(*knoop);
        }
        Opdracht::ZetBus { .. }
        | Opdracht::Start
        | Opdracht::StartBus(_)
        | Opdracht::Stop
        | Opdracht::StopBus(_) => {}
    }
    ids
}

#[cfg(test)]
#[path = "native_command_tests.rs"]
mod native_command_tests;
