//! Owned events, reconnect stamps and the command boundary.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::graph::{Activiteit, Momentopname, Opdracht, Pieken, TransportStand};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub epoch: u64,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stamped<T> {
    pub stamp: Stamp,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Message(String),
    /// Low-rate notices are bounded, too. Loss is visible on the next drain.
    InboxOverflow {
        dropped: u64,
    },
}

#[derive(Clone, Debug)]
pub enum BackendEvent {
    Graph(Stamped<Arc<Momentopname>>),
    Peaks(Stamped<Arc<Pieken>>),
    Midi(Stamped<Arc<BTreeMap<u32, Activiteit>>>),
    Transport(Stamped<Arc<TransportStand>>),
    Notice(Notice),
}

/// Used on the single backend worker. A connection-state transition invalidates
/// all previous session IDs; every graph event advances the revision.
#[derive(Default)]
pub struct EventSequencer {
    stamp: Stamp,
    connected: bool,
}

impl EventSequencer {
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    pub fn graph(&mut self, graph: Momentopname) -> BackendEvent {
        if self.connected != graph.verbonden {
            self.stamp.epoch += 1;
            self.connected = graph.verbonden;
        }
        self.stamp.revision += 1;
        BackendEvent::Graph(self.stamped(graph))
    }

    pub fn peaks(&self, peaks: Pieken) -> BackendEvent {
        BackendEvent::Peaks(self.stamped(peaks))
    }

    pub fn midi(&self, midi: BTreeMap<u32, Activiteit>) -> BackendEvent {
        BackendEvent::Midi(self.stamped(midi))
    }

    pub fn transport(&self, transport: TransportStand) -> BackendEvent {
        BackendEvent::Transport(self.stamped(transport))
    }

    fn stamped<T>(&self, value: T) -> Stamped<Arc<T>> {
        Stamped {
            stamp: self.stamp,
            value: Arc::new(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackendCommand {
    pub epoch: u64,
    pub command: Opdracht,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchError {
    Disconnected,
    StaleEpoch,
    Stopped,
}

/// Validate against worker state immediately before dispatch, not only when a UI
/// queues a command. A reconnect can occur between those two moments.
pub fn validate_command(
    command: &BackendCommand,
    current: &Stamped<Arc<Momentopname>>,
) -> Result<(), DispatchError> {
    if command.epoch != current.stamp.epoch {
        Err(DispatchError::StaleEpoch)
    } else if !current.value.verbonden {
        Err(DispatchError::Disconnected)
    } else {
        Ok(())
    }
}

/// Shells can use this seam without importing a toolkit or PipeWire proxies.
/// The initial native preview only observes events and does not dispatch.
pub trait Backend: Send + Sync {
    fn current_graph(&self) -> Stamped<Arc<Momentopname>>;
    fn dispatch(&self, command: BackendCommand) -> Result<(), DispatchError>;
}
