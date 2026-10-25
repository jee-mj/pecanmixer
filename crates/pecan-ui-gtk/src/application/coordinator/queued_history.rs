//! Narrow application history-input admission; no second reducer or dispatcher.
use super::*;
use pecan_core::history_identity::HistoryGraph;

impl Session {
    pub(crate) fn history_input(
        &self,
        origin: &Graph,
    ) -> Result<(HistoryGraph, Option<Request>), String> {
        let s = self.0.state.borrow();
        if s.admission_closed || s.uncertain.is_some() || s.external.is_some() || s.volume.busy() {
            return Err("History input is blocked by finalization, recovery, configuration or a volume gesture.".into());
        }
        if !s.witnessed
            || !origin.value.verbonden
            || !s
                .graph
                .as_ref()
                .is_some_and(|g| g.stamp == origin.stamp && Arc::ptr_eq(&g.value, &origin.value))
        {
            return Err("History input needs the current authenticated connected graph.".into());
        }
        let frame = s
            .frame
            .clone()
            .ok_or("History registry authority unavailable.")?;
        let pending = s
            .pending
            .as_ref()
            .map(|p| p.request.clone())
            .or(s.sequence.clone());
        Ok((frame, pending))
    }
    /// Preserve a queued replay's original partial/failure evidence under the
    /// existing explicit recovery gate. A zero-effect closed rejection can be
    /// retried by a fresh input; queued inputs are still canceled, never retried.
    pub(crate) fn hold_history_failure(&self, request: &Request) -> bool {
        if !request
            .2
            .upgrade()
            .is_some_and(|owner| Rc::ptr_eq(&owner, &self.0))
        {
            return false;
        }
        let report = request.report();
        let needs = match &report {
            RequestReport::Native(r) => match r.progress {
                NativeProgress::Unconfirmed { .. } => true,
                NativeProgress::Rejected { counts, .. } => {
                    counts.submitted > 0
                        || counts.in_progress > 0
                        || request.execution() != NativeExecution::Closed
                }
                _ => false,
            },
            RequestReport::Sequence(r) => matches!(r.progress, SequenceProgress::Failed { .. }),
        };
        if needs {
            let mut s = self.0.state.borrow_mut();
            if s.uncertain
                .as_ref()
                .is_none_or(|old| old.request.same_request(request))
            {
                s.uncertain = Some(Uncertain {
                    request: request.clone(),
                    ticket: report.ticket(),
                    source: report.source(),
                    commands: report.command_count(),
                });
                s.history_context = Rc::new(());
            }
        }
        needs
    }
}
