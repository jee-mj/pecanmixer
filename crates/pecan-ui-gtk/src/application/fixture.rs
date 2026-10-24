//! Deliberately synthetic endpoint. Reports and publications are controlled by
//! the fixture driver; this is not a PipeWire simulator or hardware acceptance.
use super::{Endpoint, Graph};
use pecan_core::{
    backend::{BackendEvent, EventSequencer},
    graph::Momentopname,
    history_identity::HistoryGraph,
    inbox::LatestInbox,
    native_command::{
        native_receipt, AdmissionError, NativeIntent, NativeReceipt, NativeReporter,
        MAX_NATIVE_COMMANDS, MAX_NATIVE_IN_FLIGHT, NATIVE_SOURCE_HISTORY,
    },
};
use pecan_pipewire::shutdown::ShutdownReport;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::Arc,
};

pub struct FixtureRequest {
    pub intent: NativeIntent,
    pub reporter: NativeReporter,
}
struct Fixture {
    inbox: Arc<LatestInbox>,
    authority: Arc<()>,
    sequencer: RefCell<EventSequencer>,
    frames: RefCell<VecDeque<HistoryGraph>>,
    requests: RefCell<VecDeque<FixtureRequest>>,
    shutdown: Arc<std::sync::Mutex<Option<ShutdownReport>>>,
    shutdown_requests: std::cell::Cell<usize>,
    shutdown_failure: RefCell<Option<String>>,
}
#[derive(Clone)]
pub struct FixtureDriver(Rc<Fixture>);
impl FixtureDriver {
    pub(super) fn new(inbox: Arc<LatestInbox>) -> Self {
        Self(Rc::new(Fixture {
            inbox,
            authority: Arc::new(()),
            sequencer: RefCell::default(),
            frames: RefCell::default(),
            requests: RefCell::default(),
            shutdown: Arc::default(),
            shutdown_requests: std::cell::Cell::new(0),
            shutdown_failure: RefCell::new(None),
        }))
    }
    /// Synthetic finalization only; no audio worker or file is involved.
    pub fn complete_shutdown(&self, report: ShutdownReport) {
        *self.0.shutdown.lock().unwrap() = Some(report);
    }
    pub fn shutdown_requests(&self) -> usize {
        self.0.shutdown_requests.get()
    }
    pub fn fail_next_shutdown(&self, message: impl Into<String>) {
        *self.0.shutdown_failure.borrow_mut() = Some(message.into());
    }
    pub(super) fn request_shutdown(
        &self,
    ) -> Result<Arc<std::sync::Mutex<Option<ShutdownReport>>>, String> {
        self.0
            .shutdown_requests
            .set(self.0.shutdown_requests.get() + 1);
        if let Some(error) = self.0.shutdown_failure.borrow_mut().take() {
            return Err(error);
        }
        Ok(self.0.shutdown.clone())
    }
    pub(super) fn endpoint(&self) -> Rc<dyn Endpoint> {
        self.0.clone()
    }
    /// Fixtures supply their synthetic registry generations explicitly. Never
    /// derive production generations from numeric IDs or visible DTO equality.
    pub fn publish(&self, graph: Momentopname, generations: BTreeMap<u32, u64>) -> Graph {
        let event = self.0.sequencer.borrow_mut().graph(graph);
        let BackendEvent::Graph(graph) = &event else {
            unreachable!()
        };
        let frame =
            HistoryGraph::from_registry(graph.clone(), self.0.authority.clone(), generations);
        let mut frames = self.0.frames.borrow_mut();
        frames.push_back(frame);
        if frames.len() > NATIVE_SOURCE_HISTORY {
            frames.pop_front();
        }
        let graph = graph.clone();
        drop(frames);
        self.0.inbox.publish(event);
        graph
    }
    pub fn take_requests(&self) -> Vec<FixtureRequest> {
        self.0.requests.borrow_mut().drain(..).collect()
    }
    /// Model an evicted/unavailable witness without a graph-content heuristic.
    pub fn forget_origins(&self) {
        self.0.frames.borrow_mut().clear();
    }
}
impl Endpoint for Fixture {
    fn witness(&self, graph: &Graph) -> Result<HistoryGraph, AdmissionError> {
        self.frames
            .borrow()
            .iter()
            .find(|f| f.graph().stamp == graph.stamp && Arc::ptr_eq(&f.graph().value, &graph.value))
            .cloned()
            .ok_or(AdmissionError::OriginUnavailable)
    }
    fn dispatch(&self, intent: NativeIntent) -> Result<NativeReceipt, AdmissionError> {
        self.witness(&intent.origin)?;
        if !intent.origin.value.verbonden {
            return Err(AdmissionError::Disconnected);
        }
        if intent.commands.len() > MAX_NATIVE_COMMANDS {
            return Err(AdmissionError::TooManyCommands);
        }
        let mut requests = self.requests.borrow_mut();
        if requests.len() == MAX_NATIVE_IN_FLIGHT {
            return Err(AdmissionError::TooManyInFlight);
        }
        let (receipt, reporter) =
            native_receipt(intent.ticket, intent.origin.stamp, intent.commands.len());
        requests.push_back(FixtureRequest { intent, reporter });
        Ok(receipt)
    }
}
