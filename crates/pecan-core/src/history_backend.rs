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
#[derive(Default)]
pub struct HistoryResults {
    delivered: Option<(u64, Stamp)>,
}

impl HistoryResults {
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
        if self.delivered == Some(key) {
            return None;
        }
        self.delivered = Some(key);
        Some(Input::CommandResult { ticket, result })
    }
}
