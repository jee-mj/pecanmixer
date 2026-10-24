//! The production command boundary. No PipeWire objects cross this queue.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use pecan_core::backend::{
    validate_command, BackendCommand, BackendEvent, DispatchError, EventSequencer, Stamp, Stamped,
};
use pecan_core::graph::{Momentopname, Opdracht};

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
}

struct QueuedCommand {
    command: BackendCommand,
    /// Captured under the admission lock; never rebound during execution.
    objects: Vec<(u32, Option<u64>)>,
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
        }))
    }
}

impl Commands {
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
            state.pending.clear();
        }
        if !graph.value.verbonden {
            state.objects.clear();
        }
        state.published_objects = state.objects.clone();
        state.graph = graph.clone();
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
        state.pending.clear();
    }

    /// Run on the PipeWire worker, immediately before the existing executor.
    /// `valid_objects` reads the live registry mirror, not the last UI snapshot.
    /// Session transitions also run on this worker, so cannot interleave with
    /// validation/execution. Producers never execute commands themselves.
    /// Cancellation discards queued work. One command already dequeued may
    /// still execute after cancellation; execution is not rolled back.
    pub(crate) fn drain(
        &self,
        mut valid_objects: impl FnMut(&Opdracht) -> bool,
        mut execute: impl FnMut(Opdracht),
    ) -> Vec<DispatchError> {
        let mut rejected = Vec::new();
        for _ in 0..PER_TICK {
            let next = {
                let mut state = self.0.lock().unwrap();
                if state.stopped {
                    break;
                }
                state.pending.pop_front().map(|queued| {
                    validate_command(&queued.command, &state.graph)?;
                    if queued.objects.iter().any(|(id, expected)| {
                        expected.is_none() || state.objects.get(id) != expected.as_ref()
                    }) {
                        return Err(DispatchError::InvalidObject);
                    }
                    Ok(queued.command.command)
                })
            };
            match next {
                None => break,
                Some(Err(error)) => rejected.push(error),
                Some(Ok(command)) if !valid_objects(&command) => {
                    rejected.push(DispatchError::InvalidObject);
                }
                Some(Ok(command)) => execute(command),
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
        self.pending.push_back(QueuedCommand { command, objects });
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
