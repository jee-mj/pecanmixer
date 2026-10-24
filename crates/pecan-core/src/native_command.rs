//! Native, origin-bound graph commands. No serialized legacy contract changes.
//!
//! Admission and local RPC submission are never server success. `Observed`
//! means the desired postcondition appeared in the server-fed graph mirror;
//! it is not a causal transaction acknowledgement from PipeWire/WirePlumber.
//! Batches are ordered, not atomic: a failed/unconfirmed batch may have effects
//! from earlier local submissions. Native GTK controls are not wired here.

use std::sync::{Arc, Mutex};

use crate::backend::{Stamp, Stamped};
use crate::graph::{Momentopname, Opdracht};

pub const MAX_NATIVE_COMMANDS: usize = 32;
pub const MAX_NATIVE_IN_FLIGHT: usize = 32;
pub const NATIVE_SOURCE_HISTORY: usize = 64;

#[derive(Clone, Debug)]
pub struct NativeIntent {
    /// Correlation local to the caller/history controller, not a global ID.
    pub ticket: u64,
    /// The exact owned snapshot the caller observed and planned against.
    /// Reconstructed/copied graphs are not equivalent origin witnesses.
    pub origin: Stamped<Arc<Momentopname>>,
    pub commands: Vec<Opdracht>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Stopped,
    Disconnected,
    StaleEpoch,
    OriginUnavailable,
    InvalidObject,
    QueueFull,
    TooManyInFlight,
    TooManyCommands,
    UnsupportedCommand,
    InvalidInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeFailure {
    Stopped,
    Disconnected,
    InvalidObject,
    LocalUnavailable(String),
    ObservationTimedOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Commands dequeued for local validation/execution. A claimed command
    /// can still execute after cancellation; cancellation is not rollback.
    pub attempted: usize,
    /// Already claimed local calls whose outcome was still unknown when this
    /// report was produced. They may finish after cancellation is reported.
    pub in_progress: usize,
    /// RPC calls submitted locally. No server acceptance is implied.
    pub submitted: usize,
    pub no_ops: usize,
    pub observed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeProgress {
    Queued,
    /// Local validation/execution progress only; inspect counts. This can
    /// include a currently claimed call, never server completion.
    ExecutedLocally(Counts),
    Observed {
        stamp: Stamp,
        counts: Counts,
    },
    ObservedNoOp {
        stamp: Stamp,
        counts: Counts,
    },
    Rejected {
        reason: NativeFailure,
        counts: Counts,
    },
    /// May have effects. A timeout/cancellation is not proof of rollback.
    Unconfirmed {
        reason: NativeFailure,
        counts: Counts,
    },
}

impl NativeProgress {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Queued | Self::ExecutedLocally(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReport {
    pub ticket: u64,
    pub source: Stamp,
    pub command_count: usize,
    pub progress: NativeProgress,
}

/// Owned, bounded per-request status. Reading admission/local execution never
/// yields a history acknowledgement. No GUI object or blocking wait is stored.
#[derive(Clone, Debug)]
pub struct NativeReceipt(Arc<Mutex<NativeReport>>);

impl NativeReceipt {
    pub fn report(&self) -> NativeReport {
        self.0.lock().unwrap().clone()
    }
}

/// Producer half for backend implementers; callers receive only NativeReceipt.
#[derive(Clone)]
pub struct NativeReporter(Arc<Mutex<NativeReport>>);

impl NativeReporter {
    pub fn set(&self, progress: NativeProgress) {
        let mut report = self.0.lock().unwrap();
        if !report.progress.is_terminal() {
            report.progress = progress;
        }
    }
}

pub fn native_receipt(
    ticket: u64,
    source: Stamp,
    command_count: usize,
) -> (NativeReceipt, NativeReporter) {
    let report = Arc::new(Mutex::new(NativeReport {
        ticket,
        source,
        command_count,
        progress: NativeProgress::Queued,
    }));
    (NativeReceipt(Arc::clone(&report)), NativeReporter(report))
}
