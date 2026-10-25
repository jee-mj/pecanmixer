//! Narrow history completion bridge. No planning, GUI or backend I/O.
//!
//! Deliver terminal results once per pending history ticket. Local submission
//! never advances history. Unconfirmed/partial effects clear busy with an
//! explicit error; this bridge does not invent rollback or an atomic batch.

use crate::backend::{Stamp, Stamped};
use crate::domain::history::{CommandPlan, Input};
use crate::graph::Momentopname;
use crate::native_command::{AdmissionError, NativeIntent, NativeProgress, NativeReport};
use std::sync::Arc;

/// Matches the native routing capture's bounded graph budget. This does not
/// raise the per-native-intent limit or make an ordered sequence atomic.
pub const MAX_HISTORY_SEQUENCE_COMMANDS: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequenceProgress {
    Running,
    Observed { stamp: Stamp, no_op: bool },
    Failed { message: String },
}
/// Owner evidence for one history operation, explicitly distinct from a
/// NativeReport. Children are real unmodified backend reports; completion is
/// supplied only after their barriers and the whole intent's postcondition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequenceReport {
    pub ticket: u64,
    pub source: Stamp,
    pub command_count: usize,
    pub completed_commands: usize,
    pub progress: SequenceProgress,
    pub children: Vec<NativeReport>,
}

pub fn bind_history_plan(
    origin: Stamped<Arc<Momentopname>>,
    plan: CommandPlan,
) -> Result<NativeIntent, AdmissionError> {
    if origin.stamp.epoch != plan.epoch || origin.stamp.revision != plan.source_revision {
        return Err(AdmissionError::OriginUnavailable);
    }
    Ok(NativeIntent {
        ticket: plan.ticket,
        origin,
        commands: plan.commands,
    })
}

/// One bridge per history controller, which has at most one pending ticket.
/// Keep a constant-size delivery watermark, not a cache of recent receipts:
/// an arbitrarily delayed older receipt must remain suppressed. Source stamps
/// advance monotonically; tickets at the same source use wrapping u64 order
/// (a forward distance below 2^63). Recreate the bridge with a new controller.
#[derive(Default)]
pub struct HistoryResults {
    delivered: Option<(u64, Stamp)>,
}

impl HistoryResults {
    pub fn sequence_terminal(&mut self, report: &SequenceReport) -> Option<Input> {
        let result = match &report.progress {
            SequenceProgress::Running => return None,
            SequenceProgress::Observed { stamp, .. } => Ok(*stamp),
            SequenceProgress::Failed { message } => Err(message.clone()),
        };
        self.once(report.ticket, report.source, result)
    }
    pub fn admission_rejected(
        &mut self,
        ticket: u64,
        source: Stamp,
        reason: AdmissionError,
    ) -> Option<Input> {
        self.once(
            ticket,
            source,
            Err(format!("native admission rejected: {reason:?}")),
        )
    }

    pub fn terminal(&mut self, report: &NativeReport) -> Option<Input> {
        let result = match &report.progress {
            NativeProgress::Queued | NativeProgress::ExecutedLocally(_) => return None,
            NativeProgress::Observed { stamp, .. } | NativeProgress::ObservedNoOp { stamp, .. } => Ok(*stamp),
            NativeProgress::Rejected { reason, counts } => Err(format!("native command rejected: {reason:?}; local counts: {counts:?}")),
            NativeProgress::Unconfirmed { reason, counts } => Err(format!("native completion unconfirmed: {reason:?}; effects may be present; local counts: {counts:?}")),
        };
        self.once(report.ticket, report.source, result)
    }

    fn once(&mut self, ticket: u64, source: Stamp, result: Result<Stamp, String>) -> Option<Input> {
        let key = (ticket, source);
        if let Some((previous_ticket, previous_source)) = self.delivered {
            if source < previous_source {
                return None;
            }
            if source == previous_source {
                let forward = ticket.wrapping_sub(previous_ticket);
                if forward == 0 || forward >= (1u64 << 63) {
                    return None;
                }
            }
        }
        self.delivered = Some(key);
        Some(Input::CommandResult { ticket, result })
    }
}
