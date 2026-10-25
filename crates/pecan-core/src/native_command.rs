//! Native, origin-bound graph commands. No serialized legacy contract changes.
//!
//! Admission and local RPC submission are never server success. `Observed`
//! means the desired postcondition appeared in the server-fed graph mirror;
//! it is not a causal transaction acknowledgement from PipeWire/WirePlumber.
//! Batches are ordered, not atomic: a failed/unconfirmed batch may have effects
//! from earlier local submissions. Native GTK controls are not wired here.

use std::sync::atomic::{AtomicUsize, Ordering};
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
pub struct NativeReceipt {
    report: Arc<Mutex<NativeReport>>,
    execution: Arc<AtomicUsize>,
}

/// Receipt-local execution closure, independent of the frozen terminal report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeExecution {
    /// Queued calls remain possible, or a claimed call has not returned.
    Pending,
    /// No further dispatch-side validation/executor callbacks can occur for
    /// this receipt. Graph observation/reporting can continue afterward.
    /// Earlier submissions may still affect the server. This proves neither
    /// rollback, server convergence, observation nor recorder finalization.
    Closed,
}

// One sealed bit and a bounded lease count; no per-command completion backlog.
const SEALED: usize = 1 << (usize::BITS - 1);

impl NativeReceipt {
    pub fn report(&self) -> NativeReport {
        self.report.lock().unwrap().clone()
    }

    /// Nonblocking atomic poll. Closed is permanent and belongs to this exact
    /// receipt, even when another request reuses its caller-provided ticket.
    pub fn execution(&self) -> NativeExecution {
        if self.execution.load(Ordering::Acquire) == SEALED {
            NativeExecution::Closed
        } else {
            NativeExecution::Pending
        }
    }
}

/// Producer half for backend implementers; callers receive only NativeReceipt.
#[derive(Clone)]
pub struct NativeReporter {
    report: Arc<Mutex<NativeReport>>,
    execution: Arc<AtomicUsize>,
}

/// Backend-held permission for one claimed call. Keep it through validation,
/// execution and result handling. Dropping it releases that call on return or
/// unwind, independently of removal of backend work or freezing its report.
#[must_use]
pub struct NativeExecutionLease(Arc<AtomicUsize>);

impl Drop for NativeExecutionLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

impl NativeReporter {
    pub fn set(&self, progress: NativeProgress) {
        let mut report = self.report.lock().unwrap();
        if !report.progress.is_terminal() {
            report.progress = progress;
        }
    }

    /// Claim under the backend's queue/removal lock, before invoking any local
    /// callback. Sealed execution cannot be reopened. At most 32 simultaneous
    /// leases are permitted, matching the native command bound.
    pub fn claim_execution(&self) -> Option<NativeExecutionLease> {
        let mut state = self.execution.load(Ordering::Acquire);
        while state < MAX_NATIVE_COMMANDS {
            match self.execution.compare_exchange_weak(
                state,
                state + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(NativeExecutionLease(Arc::clone(&self.execution))),
                Err(current) => state = current,
            }
        }
        None
    }

    /// Revoke every future claim when queued work is removed or all commands
    /// have returned. Existing leases remain Pending until they are dropped.
    /// Publishing a terminal report alone does not establish execution closure.
    pub fn seal_execution(&self) {
        self.execution.fetch_or(SEALED, Ordering::Release);
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
    let execution = Arc::new(AtomicUsize::new(0));
    (
        NativeReceipt {
            report: Arc::clone(&report),
            execution: Arc::clone(&execution),
        },
        NativeReporter { report, execution },
    )
}
