//! The production command boundary. No PipeWire objects cross this queue.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pecan_core::backend::{
    validate_command, BackendCommand, BackendEvent, DispatchError, EventSequencer, Stamp, Stamped,
};
use pecan_core::graph::{Momentopname, Opdracht};

/// Bound both queued work and the number of commands dispatched per graph tick.
pub(crate) const CAPACITY: usize = 256;
pub(crate) const PER_TICK: usize = 32;

pub(crate) struct Commands(Mutex<State>);

struct State {
    sequence: EventSequencer,
    graph: Stamped<Arc<Momentopname>>,
    pending: VecDeque<BackendCommand>,
    stopped: bool,
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
        state.graph = graph.clone();
        graph
    }

    pub(crate) fn enqueue(&self, command: BackendCommand) -> Result<(), DispatchError> {
        self.0.lock().unwrap().enqueue(command)
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
                state.pending.pop_front().map(|command| {
                    validate_command(&command, &state.graph).map(|()| command.command)
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
        if self.pending.len() == CAPACITY {
            return Err(DispatchError::QueueFull);
        }
        self.pending.push_back(command);
        Ok(())
    }
}
