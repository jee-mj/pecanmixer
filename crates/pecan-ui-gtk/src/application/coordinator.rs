mod reset;
#[path = "volume.rs"]
pub(super) mod volume;
use super::{Endpoint, Graph};
use crate::controls::{Adapter, Controller};
use pecan_core::{
    backend::{Notice, Stamp},
    domain::history::{self, Action, Input},
    graph::Opdracht,
    history_backend::{bind_history_plan, HistoryResults, SequenceProgress, SequenceReport},
    inbox::{InboxBatch, LatestInbox},
    native_command::{NativeExecution, NativeProgress, NativeReceipt, NativeReport},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Disconnected,
    Closing,
    IdentityUnavailable,
    GestureActive,
    GestureFinishing,
    Ready,
    ExternalOperation,
    Queued,
    ExecutedLocally,
    WaitingForGraph,
    Observed { no_op: bool },
    Failed(String),
    Unconfirmed(String),
}
impl Status {
    pub fn message(&self) -> String {
        match self {
            Self::Closing => "Finalizing · new commands disabled".into(),
            Self::Disconnected => "Disconnected · commands unavailable".into(),
            Self::IdentityUnavailable => "Graph identity unavailable · commands paused".into(),
            Self::GestureActive => "Volume gesture active · proposals may be pending".into(),
            Self::GestureFinishing => {
                "Finishing volume gesture · waiting for observed final value".into()
            }
            Self::Ready => "Ready".into(),
            Self::ExternalOperation => "Bus configuration pending · local result required".into(),
            Self::Queued => "Pending · admitted to worker queue".into(),
            Self::ExecutedLocally => "Pending · waiting for server-fed observation".into(),
            Self::WaitingForGraph => "Observed · waiting for corresponding graph".into(),
            Self::Observed { no_op: true } => "Observed · already satisfied".into(),
            Self::Observed { no_op: false } => "Observed · postcondition present".into(),
            Self::Failed(e) => format!("Could not apply change: {e}"),
            Self::Unconfirmed(e) => format!("Unconfirmed · effects may be present: {e}"),
        }
    }
}
#[derive(Clone)]
enum Delivery {
    Native(NativeProgress),
    Sequence(SequenceProgress),
    Invalidated(String),
}
/// UI-local correlation for a request submitted by this session. Pointer
/// identity isolates old controller callbacks even if numeric tickets repeat in
/// a replacement session. Native receipts and their witnessed postconditions
/// supply completion; local dispatch/admission alone never succeeds.
#[derive(Clone)]
pub struct Request(Rc<RefCell<Delivery>>, Receipt, Weak<Inner>);
#[derive(Clone)]
enum Receipt {
    Native(NativeReceipt),
    Sequence(Rc<RefCell<reset::Sequence>>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RequestReport {
    Native(NativeReport),
    Sequence(SequenceReport),
}
impl RequestReport {
    fn ticket(&self) -> u64 {
        match self {
            Self::Native(r) => r.ticket,
            Self::Sequence(r) => r.ticket,
        }
    }
    fn source(&self) -> Stamp {
        match self {
            Self::Native(r) => r.source,
            Self::Sequence(r) => r.source,
        }
    }
    fn command_count(&self) -> usize {
        match self {
            Self::Native(r) => r.command_count,
            Self::Sequence(r) => r.command_count,
        }
    }
    fn failed(&self) -> bool {
        match self {
            Self::Native(r) => matches!(
                r.progress,
                NativeProgress::Rejected { .. } | NativeProgress::Unconfirmed { .. }
            ),
            Self::Sequence(r) => matches!(r.progress, SequenceProgress::Failed { .. }),
        }
    }
}
impl Request {
    /// Actual backend evidence, retained independently of presentation/history
    /// invalidation. Terminal status alone does not establish quiescence.
    pub(crate) fn native_report(&self) -> Option<NativeReport> {
        match &self.1 {
            Receipt::Native(r) => Some(r.report()),
            Receipt::Sequence(_) => None,
        }
    }
    pub(crate) fn report(&self) -> RequestReport {
        match &self.1 {
            Receipt::Native(r) => RequestReport::Native(r.report()),
            Receipt::Sequence(r) => RequestReport::Sequence(r.borrow().report()),
        }
    }
    fn execution(&self) -> NativeExecution {
        match &self.1 {
            Receipt::Native(r) => r.execution(),
            Receipt::Sequence(r) => r.borrow().execution(),
        }
    }
    pub(crate) fn same_request(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
struct Pending {
    ticket: u64,
    source: Stamp,
    commands: usize,
    receipt: NativeReceipt,
    request: Request,
    observed: Option<(Stamp, bool)>,
}
struct Uncertain {
    request: Request,
    ticket: u64,
    source: Stamp,
    commands: usize,
}
struct State {
    graph: Option<Graph>,
    witnessed: bool,
    frame: Option<pecan_core::history_identity::HistoryGraph>,
    volume: volume::Volumes,
    history: history::Controller,
    results: HistoryResults,
    pending: Option<Pending>,
    sequence: Option<Request>,
    uncertain: Option<Uncertain>,
    status: Status,
    notices: Vec<Notice>,
    admission_closed: bool,
    history_context: Rc<()>,
    external: Option<Rc<()>>,
}
struct Inner {
    inbox: Arc<LatestInbox>,
    endpoint: Rc<dyn Endpoint>,
    state: RefCell<State>,
}
/// Main-thread shared owner reference. All clones use one inbox consumer, one
/// history controller/bridge, and at most one outstanding ordered intent.
#[derive(Clone)]
pub struct Session(Rc<Inner>);
impl Session {
    pub(super) fn new(inbox: Arc<LatestInbox>, endpoint: Rc<dyn Endpoint>) -> Self {
        let mut history = history::Controller::default();
        history.buses = super::default_idle_configuration().buses;
        Self(Rc::new(Inner {
            inbox,
            endpoint,
            state: RefCell::new(State {
                graph: None,
                witnessed: false,
                frame: None,
                volume: Default::default(),
                history,
                results: HistoryResults::default(),
                pending: None,
                sequence: None,
                uncertain: None,
                status: Status::Disconnected,
                notices: vec![],
                admission_closed: false,
                history_context: Rc::new(()),
                external: None,
            }),
        }))
    }
    pub fn same_session(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn graph(&self) -> Option<Graph> {
        self.0.state.borrow().graph.clone()
    }
    /// Read the witness already accepted by this sole consumer. This neither
    /// drains the inbox nor obtains authority from the observer/DTO properties.
    pub(crate) fn witnessed_graph(&self) -> Option<pecan_core::history_identity::HistoryGraph> {
        let state = self.0.state.borrow();
        let frame = state.frame.as_ref()?;
        let graph = state.graph.as_ref()?;
        (state.witnessed
            && frame.graph().stamp == graph.stamp
            && Arc::ptr_eq(&frame.graph().value, &graph.value))
        .then(|| frame.clone())
    }
    pub fn status(&self) -> Status {
        let state = self.0.state.borrow();
        if state.admission_closed {
            Status::Closing
        } else if state.uncertain.is_some() {
            Status::Unconfirmed(
                "Native command closure is unconfirmed; new changes disabled.".into(),
            )
        } else if state.external.is_some() {
            Status::ExternalOperation
        } else {
            state.status.clone()
        }
    }
    /// Irreversible for this owner. Existing receipts continue polling; stopping
    /// new submissions is not cancellation, rollback or shutdown completion.
    pub fn close_admission(&self) {
        let mut state = self.0.state.borrow_mut();
        state.admission_closed = true;
        state.history_context = Rc::new(());
        state.abandon_volume("Application finalization started.");
    }
    /// UI-local presentation authority, not registry or backend command identity.
    pub(super) fn history_context(&self) -> Rc<()> {
        self.0.state.borrow().history_context.clone()
    }

    pub fn admission_closed(&self) -> bool {
        self.0.state.borrow().admission_closed
    }

    pub fn busy(&self) -> bool {
        let state = self.0.state.borrow();
        state.pending.is_some()
            || state.sequence.is_some()
            || state.uncertain.is_some()
            || state.volume.busy()
            || state.external.is_some()
    }
    pub fn has_receipt(&self) -> bool {
        let state = self.0.state.borrow();
        state.pending.is_some()
            || state.sequence.is_some()
            || state.uncertain.is_some()
            || state.volume.has_receipt()
            || state.external.is_some()
    }
    pub fn history_lengths(&self) -> (usize, usize) {
        self.0.state.borrow().history.lengths()
    }
    pub fn history_labels(&self) -> (Option<String>, Option<String>) {
        let state = self.0.state.borrow();
        let (undo, redo) = state.history.labels();
        (undo.map(str::to_owned), redo.map(str::to_owned))
    }
    pub(crate) fn request_pending(&self, request: &Request) -> bool {
        let state = self.0.state.borrow();
        state
            .pending
            .as_ref()
            .is_some_and(|p| Rc::ptr_eq(&p.request.0, &request.0))
            || state
                .sequence
                .as_ref()
                .is_some_and(|p| p.same_request(request))
            || state
                .uncertain
                .as_ref()
                .is_some_and(|p| Rc::ptr_eq(&p.request.0, &request.0))
    }
    /// Read-only prerequisite check; a frozen in-progress count is historical.
    pub(crate) fn recovery_ready(&self, request: &Request, graph: &Graph) -> Result<(), String> {
        if !request.2.upgrade().is_some_and(|s| Rc::ptr_eq(&s, &self.0)) {
            return Err("Recovery belongs to another application session.".into());
        }
        if request.execution() != NativeExecution::Closed {
            return Err("Local execution is still pending; retain the original receipt.".into());
        }
        self.0
            .state
            .borrow()
            .recovery_ready(request, graph, &request.report())
    }
    /// Called only after BusOwner inspected current authenticated configuration.
    /// Reopen local admission, never upgrade the report or record prefix undo.
    pub(crate) fn recover_request(
        &self,
        request: &Request,
        graph: &Graph,
        buses: &pecan_core::domain::buses::BusConfig,
    ) -> Result<(), String> {
        buses.definitions().map_err(|e| format!("{e:?}"))?;
        self.recovery_ready(request, graph)?;
        let frame = self
            .0
            .endpoint
            .recovery_witness(graph)
            .map_err(|e| format!("Current native graph inspection unavailable: {e:?}"))?;
        if frame.graph().stamp != graph.stamp || !Arc::ptr_eq(&frame.graph().value, &graph.value) {
            return Err("Recovery inspection returned a different native frame.".into());
        }
        let report = request.report();
        let mut state = self.0.state.borrow_mut();
        state.recovery_ready(request, graph, &report)?;
        state
            .history
            .reconcile_buses(&frame, buses.clone())
            .map_err(|e| format!("Recovery history reconciliation unavailable: {e:?}"))?;
        state.uncertain = None;
        state.history_context = Rc::new(());
        state.status = Status::Unconfirmed(
            "Local dispatch closed; current state inspected. The original routing outcome remains unconfirmed or partially applied; no rollback is established.".into(),
        );
        Ok(())
    }
    /// Called only with BusOwner's authenticated, accepted graph/configuration.
    /// Preserve the definitions captured by outstanding work until it releases
    /// the shared gate; do not replan that work using a new layout.
    pub(crate) fn adopt_buses(
        &self,
        origin: &Graph,
        buses: &pecan_core::domain::buses::BusConfig,
    ) -> Result<(), String> {
        buses.definitions().map_err(|e| format!("{e:?}"))?;
        let mut state = self.0.state.borrow_mut();
        if !state.witnessed
            || !state
                .graph
                .as_ref()
                .is_some_and(|g| g.stamp == origin.stamp && Arc::ptr_eq(&g.value, &origin.value))
        {
            return Err("Bus definitions require the exact native graph authority.".into());
        }
        if &state.history.buses == buses {
            return Ok(());
        }
        if state.pending.is_some()
            || state.sequence.is_some()
            || state.uncertain.is_some()
            || state.volume.busy()
            || state.external.is_some()
        {
            return Err("Bus definitions changed while shared work is pending.".into());
        }
        let frame = state
            .frame
            .as_ref()
            .filter(|frame| {
                frame.graph().stamp == origin.stamp
                    && Arc::ptr_eq(&frame.graph().value, &origin.value)
            })
            .cloned()
            .ok_or("Bus definitions require the retained current native witness.")?;
        state
            .history
            .reconcile_buses(&frame, buses.clone())
            .map_err(|e| format!("History layout reconciliation unavailable: {e:?}"))?;
        state.history_context = Rc::new(());
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        let s = self.0.state.borrow();
        !s.admission_closed
            && s.witnessed
            && s.pending.is_none()
            && s.sequence.is_none()
            && s.uncertain.is_none()
            && s.external.is_none()
            && !s.volume.busy()
            && s.graph.as_ref().is_some_and(|g| g.value.verbonden)
    }
    pub fn adapter(&self) -> SessionAdapter {
        SessionAdapter(self.clone())
    }
    /// Explicit full snapshots only. Refuse a copied graph or an old callback
    /// instead of rebinding its semantic action to the current object's origin.
    pub fn execute(
        &self,
        origin: &Graph,
        label: impl Into<String>,
        action: Action,
    ) -> Result<Option<Request>, String> {
        self.submit(
            origin,
            Input::Execute {
                label: label.into(),
                action,
            },
        )
    }
    pub fn undo(&self, origin: &Graph) -> Result<Option<Request>, String> {
        self.submit(origin, Input::Undo)
    }
    pub fn redo(&self, origin: &Graph) -> Result<Option<Request>, String> {
        self.submit(origin, Input::Redo)
    }
    pub(super) fn patch(&self, origin: Graph, input: Input) -> Result<(), String> {
        match input {
            Input::Execute { label, action } => self.execute(&origin, label, action).map(|_| ()),
            _ => Err(
                "Patch widgets may only hand off actions, not graph/result/history metadata."
                    .into(),
            ),
        }
    }
    fn submit(&self, origin: &Graph, input: Input) -> Result<Option<Request>, String> {
        let mut s = self.0.state.borrow_mut();
        if s.admission_closed {
            return Err("Application finalization has started; new commands are disabled.".into());
        }
        if s.pending.is_some()
            || s.sequence.is_some()
            || s.uncertain.is_some()
            || s.volume.busy()
            || s.external.is_some()
        {
            return Err("Another change or volume gesture is pending.".into());
        }
        if !s.witnessed {
            return Err("Native graph identity is unavailable.".into());
        }
        let current = s.graph.as_ref().ok_or("No native graph.")?;
        if !current.value.verbonden {
            return Err("Disconnected.".into());
        }
        if current.stamp != origin.stamp || !Arc::ptr_eq(&current.value, &origin.value) {
            return Err("The graph changed; start a fresh action.".into());
        }
        if let Input::Execute { action, .. } = &input {
            let commands = history::plan(action, &origin.value, &s.history.buses)
                .map_err(|e| format!("{e:?}"))?;
            if commands.is_empty() {
                return Ok(None);
            }
        }
        s.history_context = Rc::new(());
        let Some(plan) = s.history.reduce(input).map_err(|e| format!("{e:?}"))? else {
            // Pure no-command plans have no backend effects to acknowledge.
            // Nonempty commands (including backend no-ops) require fresh graph
            // progress and the actual receipt path below.
            s.status = Status::Ready;
            return Ok(None);
        };
        let ticket = plan.ticket;
        if plan.commands.len() > pecan_core::native_command::MAX_NATIVE_COMMANDS {
            let frame = s
                .frame
                .clone()
                .ok_or("Current native witness unavailable.")?;
            let sequence = match reset::Sequence::new(&s.history, &frame, plan) {
                Ok(sequence) => sequence,
                Err(error) => {
                    let _ = s.history.reduce(Input::CommandResult {
                        ticket,
                        result: Err(error.clone()),
                    });
                    return Err(error);
                }
            };
            let request = Request(
                Rc::new(RefCell::new(Delivery::Sequence(SequenceProgress::Running))),
                Receipt::Sequence(Rc::new(RefCell::new(sequence))),
                Rc::downgrade(&self.0),
            );
            s.sequence = Some(request.clone());
            s.status = Status::Queued;
            drop(s);
            self.drive_reset();
            return Ok(Some(request));
        }
        let intent = match bind_history_plan(origin.clone(), plan) {
            Ok(intent) => intent,
            Err(error) => {
                let message = format!("Cannot bind history plan: {error:?}");
                let _ = s.history.reduce(Input::CommandResult {
                    ticket,
                    result: Err(message.clone()),
                });
                s.status = Status::Failed(message.clone());
                return Err(message);
            }
        };
        let command_count = intent.commands.len();
        match self.0.endpoint.dispatch(intent) {
            Ok(receipt) => {
                let report = receipt.report();
                if report.ticket != ticket
                    || report.source != origin.stamp
                    || report.command_count != command_count
                {
                    let message = "Mismatched native receipt; effects may be present.".to_owned();
                    let _ = s.history.reduce(Input::CommandResult {
                        ticket,
                        result: Err(message.clone()),
                    });
                    s.status = Status::Unconfirmed(message.clone());
                    s.uncertain = Some(Uncertain {
                        request: Request(
                            Rc::new(RefCell::new(Delivery::Invalidated(message.clone()))),
                            Receipt::Native(receipt),
                            Rc::downgrade(&self.0),
                        ),
                        ticket,
                        source: origin.stamp,
                        commands: command_count,
                    });
                    return Err(message);
                }
                let request = Request(
                    Rc::new(RefCell::new(Delivery::Native(NativeProgress::Queued))),
                    Receipt::Native(receipt.clone()),
                    Rc::downgrade(&self.0),
                );
                s.pending = Some(Pending {
                    ticket,
                    source: origin.stamp,
                    commands: command_count,
                    receipt,
                    request: request.clone(),
                    observed: None,
                });
                s.status = Status::Queued;
                Ok(Some(request))
            }
            Err(error) => {
                if let Some(input) =
                    s.results
                        .admission_rejected(ticket, origin.stamp, error.clone())
                {
                    let _ = s.history.reduce(input);
                }
                let message = format!("Native admission rejected: {error:?}");
                s.status = Status::Failed(message.clone());
                Err(message)
            }
        }
    }
    /// Sole inbox drain and receipt poll. Call on the main context even without
    /// graph wakes: receipts can change without publishing a graph. No completion waits,
    /// joins, I/O, nested loops or GTK calls occur here. Returns the drained
    /// batch for the owning workspace's telemetry presentation; graph acceptance
    /// happens first. No second inbox consumer is created here.
    pub fn poll(&self) -> InboxBatch {
        let mut batch = self.0.inbox.drain();
        let mut s = self.0.state.borrow_mut();
        s.poll_uncertain();
        // Observe receipt first so a coalesced graph can satisfy its barrier.
        s.poll_receipt();
        s.notices = batch.notices.clone();
        if let Some(graph) = batch
            .graph
            .take()
            .filter(|g| s.graph.as_ref().is_none_or(|old| g.stamp > old.stamp))
        {
            let witness = self.0.endpoint.witness(&graph).ok().filter(|w| {
                w.graph().stamp == graph.stamp && Arc::ptr_eq(&w.graph().value, &graph.value)
            });
            s.history_context = Rc::new(());
            s.witnessed = witness.is_some();
            s.frame = witness.clone();
            let input = witness.map_or_else(|| Input::Graph(graph.clone()), Input::NativeGraph);
            let _ = s.history.reduce(input);
            batch.graph = Some(graph.clone());
            s.graph = Some(graph);
            if matches!(s.status, Status::Disconnected | Status::IdentityUnavailable) {
                s.status = Status::Ready;
            }
            if !s.history.busy() && s.pending.as_ref().is_some_and(|p| p.observed.is_none()) {
                s.invalidate("The graph owner, connection or request dependency changed.");
            }
        }
        s.finish();
        s.poll_volume(self.0.endpoint.as_ref());
        if s.pending.is_none() && !s.volume.busy() {
            if !s.graph.as_ref().is_some_and(|g| g.value.verbonden) {
                s.status = Status::Disconnected;
            } else if !s.witnessed {
                s.status = Status::IdentityUnavailable;
            }
        }
        drop(s);
        self.drive_reset();
        batch
    }
    pub fn notices(&self) -> Vec<Notice> {
        self.0.state.borrow().notices.clone()
    }
    /// Share the existing ticket sequence without adding these local protocols
    /// to graph history. A config receipt retains the lease until terminal.
    pub(crate) fn reserve_external(&self) -> Result<(u64, Rc<()>), String> {
        let mut state = self.0.state.borrow_mut();
        if state.admission_closed
            || state.pending.is_some()
            || state.sequence.is_some()
            || state.uncertain.is_some()
            || state.volume.busy()
            || state.external.is_some()
        {
            return Err("Another change is pending or finalization has started.".into());
        }
        let token = Rc::new(());
        let ticket = state.history.reserve_ticket();
        state.external = Some(token.clone());
        state.history_context = Rc::new(());
        Ok((ticket, token))
    }
    pub(crate) fn release_external(&self, token: &Rc<()>) {
        let mut state = self.0.state.borrow_mut();
        if state
            .external
            .as_ref()
            .is_some_and(|held| Rc::ptr_eq(held, token))
        {
            state.external = None;
            state.history_context = Rc::new(());
        }
    }
    /// Another sample under the same local protocol lease. The existing history
    /// allocator remains the sole ticket sequence; stale/closed leases cannot
    /// allocate or silently acquire a replacement lease.
    pub(crate) fn external_sample_ticket(&self, token: &Rc<()>) -> Result<u64, String> {
        let mut state = self.0.state.borrow_mut();
        if state.admission_closed
            || !state
                .external
                .as_ref()
                .is_some_and(|held| Rc::ptr_eq(held, token))
        {
            return Err("The original shared operation lease is unavailable.".into());
        }
        Ok(state.history.reserve_ticket())
    }
}
impl State {
    fn recovery_ready(
        &self,
        request: &Request,
        graph: &Graph,
        report: &RequestReport,
    ) -> Result<(), String> {
        if self.admission_closed
            || self.pending.is_some()
            || self.sequence.is_some()
            || self.history.busy()
            || self.volume.busy()
            || self.external.is_some()
        {
            return Err(
                "Release current history/gesture/configuration ownership before recovery.".into(),
            );
        }
        if !report.failed() {
            return Err("Recovery requires the original terminal failure outcome.".into());
        }
        if !self.witnessed
            || !graph.value.verbonden
            || !self
                .graph
                .as_ref()
                .is_some_and(|g| g.stamp == graph.stamp && Arc::ptr_eq(&g.value, &graph.value))
            || (graph.stamp.epoch, graph.stamp.revision)
                < (report.source().epoch, report.source().revision)
        {
            return Err(
                "Recovery requires the current authenticated connected native graph.".into(),
            );
        }
        if self.uncertain.as_ref().is_some_and(|held| {
            !held.request.same_request(request)
                || held.ticket != report.ticket()
                || held.source != report.source()
                || held.commands != report.command_count()
        }) {
            return Err("Another or mismatched receipt still owns the uncertainty hold.".into());
        }
        Ok(())
    }
    fn poll_uncertain(&mut self) {
        let Some(held) = self.uncertain.as_ref() else {
            return;
        };
        let Some(report) = held.request.native_report() else {
            return;
        };
        if report.ticket == held.ticket
            && report.source == held.source
            && report.command_count == held.commands
            && terminal_quiescent(&report)
        {
            self.uncertain = None;
            self.history_context = Rc::new(());
        }
    }
    fn poll_receipt(&mut self) {
        let Some(p) = self.pending.as_ref() else {
            return;
        };
        if p.observed.is_some() {
            return;
        }
        let report = p.receipt.report();
        if report.ticket != p.ticket
            || report.source != p.source
            || report.command_count != p.commands
        {
            self.invalidate("Receipt identity changed.");
            return;
        }
        if report.progress.is_terminal() && !terminal_quiescent(&report) {
            self.invalidate(
                "Terminal native status retains in-progress work; closure is unconfirmed.",
            );
            return;
        }
        match &report.progress {
            NativeProgress::Queued => {}
            NativeProgress::ExecutedLocally(_) => {
                *p.request.0.borrow_mut() = Delivery::Native(report.progress.clone());
                self.status = Status::ExecutedLocally;
            }
            NativeProgress::Observed { stamp, .. } | NativeProgress::ObservedNoOp { stamp, .. } => {
                if stamp.epoch != p.source.epoch || stamp.revision <= p.source.revision {
                    let ticket = p.ticket;
                    let _ = self.history.reduce(Input::CommandResult {
                        ticket,
                        result: Err("Invalid observation stamp.".into()),
                    });
                    self.invalidate("Invalid native observation stamp.");
                    return;
                }
                self.history_context = Rc::new(());
                if let Some(input) = self.results.terminal(&report) {
                    let _ = self.history.reduce(input);
                }
                // Deliver evidence without releasing the shared gate. Mixer
                // controllers retain their own graph barrier; patchbay and
                // history also remain busy until the witnessed frame arrives.
                *self.pending.as_ref().unwrap().request.0.borrow_mut() =
                    Delivery::Native(report.progress.clone());
                let no_op = matches!(report.progress, NativeProgress::ObservedNoOp { .. });
                self.pending.as_mut().unwrap().observed = Some((*stamp, no_op));
                self.status = Status::WaitingForGraph;
            }
            NativeProgress::Rejected { .. } | NativeProgress::Unconfirmed { .. } => {
                self.history_context = Rc::new(());
                if let Some(input) = self.results.terminal(&report) {
                    let _ = self.history.reduce(input);
                }
                let pending = self.pending.take().unwrap();
                *pending.request.0.borrow_mut() = Delivery::Native(report.progress.clone());
                self.status = if matches!(report.progress, NativeProgress::Rejected { .. }) {
                    Status::Failed(format!("{:?}", report.progress))
                } else {
                    Status::Unconfirmed(format!("{:?}", report.progress))
                };
            }
        }
    }
    fn invalidate(&mut self, reason: &str) {
        self.history_context = Rc::new(());
        if let Some(p) = self.pending.take() {
            let _ = self.history.reduce(Input::CommandResult {
                ticket: p.ticket,
                result: Err(reason.into()),
            });
            *p.request.0.borrow_mut() = Delivery::Invalidated(reason.into());
            let report = p.request.native_report().expect("native pending receipt");
            if report.ticket != p.ticket
                || report.source != p.source
                || report.command_count != p.commands
                || !terminal_quiescent(&report)
            {
                self.uncertain = Some(Uncertain {
                    request: p.request,
                    ticket: p.ticket,
                    source: p.source,
                    commands: p.commands,
                });
            }
        }
        self.status = Status::Unconfirmed(reason.into());
    }
    fn finish(&mut self) {
        let Some(p) = self.pending.as_ref() else {
            return;
        };
        let Some((stamp, no_op)) = p.observed else {
            return;
        };
        let connected = self
            .graph
            .as_ref()
            .is_some_and(|g| g.value.verbonden && g.stamp.epoch == stamp.epoch);
        if !connected || !self.witnessed {
            self.invalidate("Native ownership or connection changed before graph observation.");
            return;
        }
        if self.graph.as_ref().is_some_and(|g| g.stamp >= stamp) {
            self.history_context = Rc::new(());
            let pending = self.pending.take().unwrap();
            *pending.request.0.borrow_mut() = Delivery::Native(pending.receipt.report().progress);
            self.status = Status::Observed { no_op };
        }
    }
}
fn terminal_quiescent(report: &NativeReport) -> bool {
    match &report.progress {
        NativeProgress::Observed { counts, .. }
        | NativeProgress::ObservedNoOp { counts, .. }
        | NativeProgress::Rejected { counts, .. }
        | NativeProgress::Unconfirmed { counts, .. } => counts.in_progress == 0,
        _ => false,
    }
}

#[derive(Clone)]
pub struct SessionAdapter(Session);
impl Adapter for SessionAdapter {
    type Snapshot = Graph;
    type Request = Request;
    fn graph(snapshot: &Graph) -> &Graph {
        snapshot
    }
    fn same_request(a: &Request, b: &Request) -> bool {
        Rc::ptr_eq(&a.0, &b.0)
    }
    fn submit(&mut self, origin: &Graph, command: Opdracht) -> Result<Request, String> {
        let (label, action) = match command {
            Opdracht::ZetVolume { knoop, volume } => (
                "Volume",
                Action::Volume {
                    node: knoop,
                    values: volume,
                },
            ),
            Opdracht::ZetMute { knoop, mute } => (
                "Mute",
                Action::Mute {
                    node: knoop,
                    muted: mute,
                },
            ),
            _ => return Err("Unsupported mixer action.".into()),
        };
        self.0
            .execute(origin, label, action)?
            .ok_or_else(|| "No mixer command was planned.".into())
    }
}
impl Controller<SessionAdapter> {
    pub fn poll_session(&mut self) -> bool {
        let Some(request) = self.pending_request().cloned() else {
            return false;
        };
        let delivery = request.0.borrow().clone();
        match delivery {
            Delivery::Sequence(SequenceProgress::Running) => false,
            Delivery::Sequence(SequenceProgress::Observed { stamp, .. }) => {
                self.observed_result(&request, stamp)
            }
            Delivery::Sequence(SequenceProgress::Failed { message }) => {
                self.unconfirmed(&request, message)
            }
            Delivery::Native(NativeProgress::Queued) => false,
            Delivery::Native(NativeProgress::ExecutedLocally(_)) => self.executed_locally(&request),
            Delivery::Native(
                NativeProgress::Observed { stamp, .. } | NativeProgress::ObservedNoOp { stamp, .. },
            ) => self.observed_result(&request, stamp),
            Delivery::Native(NativeProgress::Rejected { reason, .. }) => {
                self.rejected(&request, format!("{reason:?}"))
            }
            Delivery::Native(NativeProgress::Unconfirmed { reason, .. }) => {
                self.unconfirmed(&request, format!("{reason:?}; effects may be present"))
            }
            Delivery::Invalidated(message) => self.unconfirmed(&request, message),
        }
    }
}
