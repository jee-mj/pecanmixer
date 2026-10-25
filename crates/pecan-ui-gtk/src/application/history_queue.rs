//! Legacy history directions in FIFO order, selected only when their turn runs.
//! Reuses the workspace clock, Session/reducer and real receipts. No timer,
//! command cache, optimistic stack movement, rollback or automatic retry.
use super::{coordinator::RequestReport, BusOwner, Graph, Request, Session};
use pecan_core::{
    domain::history::HISTORY_LIMIT,
    history_backend::SequenceProgress,
    history_identity::HistoryGraph,
    native_command::{NativeExecution, NativeProgress},
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    time::{Duration, Instant},
};

const SPACING: Duration = Duration::from_millis(250);
pub(super) struct HistoryQueue {
    session: Session,
    state: RefCell<State>,
    driving: Cell<bool>,
}
struct State {
    directions: VecDeque<bool>,
    source: Option<HistoryGraph>,
    active: Option<Request>,
    recovery: Option<Request>,
    last: Option<Request>,
    next: Instant,
    message: Option<String>,
    canceled: bool,
}
impl HistoryQueue {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            state: RefCell::new(State {
                directions: VecDeque::new(),
                source: None,
                active: None,
                recovery: None,
                last: None,
                next: Instant::now(),
                message: None,
                canceled: false,
            }),
            driving: Cell::new(false),
        }
    }
    pub fn accepting(&self, origin: &Graph) -> bool {
        let q = self.state.borrow();
        q.recovery.is_none()
            && q.directions.len() < HISTORY_LIMIT
            && self.session.history_input(origin).is_ok()
    }
    pub fn has_work(&self) -> bool {
        let q = self.state.borrow();
        !q.directions.is_empty() || q.active.is_some()
    }
    pub fn message(&self) -> Option<String> {
        self.state.borrow().message.clone()
    }
    pub fn recovery(&self) -> Option<Request> {
        self.state.borrow().recovery.clone()
    }
    pub fn enqueue(&self, origin: &Graph, undo: bool) -> Result<(), String> {
        let (frame, barrier) = self.session.history_input(origin)?;
        {
            let mut q = self.state.borrow_mut();
            if q.recovery.is_some() {
                return Err("Inspect the retained failed history request before new input.".into());
            }
            if q.directions.len() == HISTORY_LIMIT {
                return Err("History input queue is full; this input was not queued.".into());
            }
            if q.source
                .as_ref()
                .is_some_and(|old| !old.same_connection(&frame))
            {
                q.directions.clear();
                q.canceled = true;
                q.message = Some("Queued history canceled: connection authority changed.".into());
                return Err(q.message.clone().unwrap());
            }
            if q.source.is_none() {
                q.source = Some(frame);
                q.active = barrier;
            }
            q.directions.push_back(undo);
            q.canceled = false;
            q.message = Some(format!(
                "{} history inputs queued; awaiting their turn",
                q.directions.len()
            ));
        }
        // An active receipt is processed by the normal workspace clock, after
        // Session has delivered its result/graph barrier. Input admission is
        // not another inbox/receipt consumer.
        if self.state.borrow().active.is_none() {
            self.tick();
        }
        Ok(())
    }
    pub fn tick(&self) {
        self.tick_at(Instant::now());
    }
    fn tick_at(&self, now: Instant) {
        if self.driving.replace(true) {
            return;
        }
        struct Driving<'a>(&'a Cell<bool>);
        impl Drop for Driving<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _driving = Driving(&self.driving);
        let frame = self.session.witnessed_graph();
        {
            let mut q = self.state.borrow_mut();
            if self.session.admission_closed()
                || q.source
                    .as_ref()
                    .is_some_and(|old| frame.as_ref().is_none_or(|new| !old.same_connection(new)))
            {
                q.directions.clear();
                q.canceled = true;
                q.message=Some("Queued history canceled: finalization or native connection authority changed. Admitted work is retained.".into());
            }
        }
        let active = self.state.borrow().active.clone();
        if let Some(request) = active {
            let report = request.report();
            let failed = match &report {
                RequestReport::Native(r) => matches!(
                    r.progress,
                    NativeProgress::Rejected { .. } | NativeProgress::Unconfirmed { .. }
                ),
                RequestReport::Sequence(r) => matches!(r.progress, SequenceProgress::Failed { .. }),
            };
            if failed {
                let recovery = self.session.hold_history_failure(&request);
                let mut q = self.state.borrow_mut();
                q.directions.clear();
                q.canceled = true;
                q.active = None;
                q.source = None;
                q.last = Some(request.clone());
                q.recovery = recovery.then_some(request);
                q.message = Some(if recovery {
                    "History outcome unconfirmed; remaining inputs canceled. Inspect current state before fresh input."
                } else {
                    "History request rejected; remaining queued inputs canceled without retry."
                }.into());
                return;
            }
            // A report alone cannot release an in-flight Session or graph
            // barrier. Local dispatch closure is separate from frozen counts.
            if self.session.request_pending(&request)
                || request.execution() != NativeExecution::Closed
            {
                return;
            }
            let observed = match &report {
                RequestReport::Native(r) => matches!(
                    r.progress,
                    NativeProgress::Observed { .. } | NativeProgress::ObservedNoOp { .. }
                ),
                RequestReport::Sequence(r) => {
                    matches!(r.progress, SequenceProgress::Observed { .. })
                }
            };
            if !observed {
                return;
            }
            let mut q = self.state.borrow_mut();
            q.active = None;
            q.last = Some(request);
        }
        if self.state.borrow().directions.is_empty() {
            let mut q = self.state.borrow_mut();
            q.source = None;
            if q.recovery.is_none() && !q.canceled {
                q.message = None;
            }
            return;
        }
        if !self.session.enabled() {
            return;
        }
        let Some(graph) = self.session.graph() else {
            return;
        };
        if self.session.history_input(&graph).is_err() {
            return;
        }
        let undo = {
            let mut q = self.state.borrow_mut();
            if now < q.next {
                return;
            }
            q.directions.pop_front().unwrap()
        };
        // Resolve current top and current endpoint IDs through the existing
        // reducer/registry guards, never a command captured when the key arrived.
        let result = if undo {
            self.session.undo(&graph)
        } else {
            self.session.redo(&graph)
        };
        let mut q = self.state.borrow_mut();
        // Legacy schedules its 250 ms delay after the attempted closure returns.
        q.next = now.max(Instant::now()) + SPACING;
        match result {
            Ok(request) => {
                q.active = request;
                q.message = Some(format!(
                    "History input running; {} queued",
                    q.directions.len()
                ));
            }
            Err(error) => {
                q.directions.clear();
                q.canceled = true;
                q.source = None;
                q.message = Some(format!(
                    "History admission failed; remaining inputs canceled: {error}"
                ));
            }
        }
    }
    pub fn recover(&self, request: &Request, buses: &BusOwner) -> Result<(), String> {
        if !self
            .state
            .borrow()
            .recovery
            .as_ref()
            .is_some_and(|old| old.same_request(request))
        {
            return Err("This recovery control belongs to a retired history request.".into());
        }
        let graph = self.session.graph().ok_or("History graph unavailable.")?;
        self.session.recovery_ready(request, &graph)?;
        let (_, graph, configuration) = buses.routing_recovery_capture()?;
        if !self
            .state
            .borrow()
            .recovery
            .as_ref()
            .is_some_and(|old| old.same_request(request))
        {
            return Err("History recovery changed during inspection.".into());
        }
        self.session
            .recover_request(request, &graph, &configuration)?;
        let mut q = self.state.borrow_mut();
        q.recovery = None;
        q.message=Some("History queue canceled; current state inspected. Original failed outcome retained; no rollback.".into());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
