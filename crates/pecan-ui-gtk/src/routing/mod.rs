//! Native routing presentation. Core supplies every route/reset/history plan;
//! recording_buses::Model remains the sole Output/Rec selection store.
//! No application owner, backend dispatch or native origin factory lives here.
mod view;
pub use view::RoutingControls;

use crate::recording_buses::{self as buses, Destination};
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{
        history::{self, Action, Input},
        mixer::{Mixer, RouteState},
    },
    graph::Momentopname,
    native_command::{NativeProgress, NativeReport, MAX_NATIVE_COMMANDS},
};
use std::{rc::Rc, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub sources: usize,
    pub destinations: usize,
    pub graph_objects: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            sources: 512,
            destinations: 256,
            graph_objects: 16384,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NoSnapshot,
    BudgetExceeded,
    ProjectionMismatch,
    OldGraph,
    ConflictingStamp,
    StaleBinding,
    MissingStrip,
    Disconnected,
    Closing,
    UnknownConfiguration,
    Pending,
    ReconciliationRequired,
    AwaitingGraph,
    NoConfirmation,
    NothingToReset,
    PlanTooLarge,
    Domain(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoSnapshot => "Waiting for routing snapshot",
            Self::BudgetExceeded => "Routing view exceeds its display limit",
            Self::ProjectionMismatch => "Graph and mixer do not match; refresh routing",
            Self::OldGraph => "Older graph publication ignored",
            Self::ConflictingStamp => "Conflicting graph publication; refresh routing",
            Self::StaleBinding => "Routing view changed; start a fresh action",
            Self::MissingStrip => "Source or destination disappeared",
            Self::Disconnected => "Not connected",
            Self::Closing => "Application is closing",
            Self::UnknownConfiguration => "Bus configuration unavailable",
            Self::Pending => "Routing or configuration request pending",
            Self::ReconciliationRequired => "Refresh routing status before another change",
            Self::AwaitingGraph => "Waiting to display the current graph",
            Self::NoConfirmation => "Reset confirmation is no longer valid",
            Self::NothingToReset => "No manual routes to reset",
            Self::PlanTooLarge => {
                "Routing change exceeds the current action limit; no commands submitted"
            }
            Self::Domain(error) => return write!(f, "Cannot plan routing: {error}"),
        })
    }
}
impl std::error::Error for Error {}

/// Read-only projection of the existing bus model, never a second selection
/// store. `origin` must authenticate this EXACT graph and bus configuration in
/// the owner's adapter. A DTO/projection comparison cannot manufacture authority.
pub struct Capture<O> {
    graph: Stamped<Arc<Momentopname>>,
    origin: O,
    selection: Rc<buses::Binding<O>>,
    mixer: Arc<Mixer>,
    configuration: Option<pecan_core::domain::buses::BusConfig>,
    output: Option<String>,
    rec: Option<String>,
    closing: bool,
    configuration_gate: Option<Error>,
}
impl<O: Clone> Capture<O> {
    pub fn from_bus_model<R, T: Clone>(
        graph: Stamped<Arc<Momentopname>>,
        model: &buses::Model<O, R, T>,
        budget: Budget,
    ) -> Result<Self, Error> {
        let s = model.snapshot().ok_or(Error::NoSnapshot)?;
        if s.mixer.apps.len() + s.mixer.hardware.len() > budget.sources
            || s.mixer.destinations.len() > budget.destinations
            || graph
                .value
                .knopen
                .len()
                .saturating_add(graph.value.poorten.len())
                .saturating_add(graph.value.kabels.len())
                > budget.graph_objects
        {
            return Err(Error::BudgetExceeded);
        }
        if s.connected != graph.value.verbonden || s.default_sink != graph.value.standaard_sink {
            return Err(Error::ProjectionMismatch);
        }
        if let Some(config) = &s.configuration {
            let definitions = config
                .definitions()
                .map_err(|e| Error::Domain(format!("{e:?}")))?;
            if Mixer::build(&graph.value, &definitions) != *s.mixer {
                return Err(Error::ProjectionMismatch);
            }
        }
        Ok(Self {
            graph,
            origin: s.origin.clone(),
            selection: Rc::new(model.bind().map_err(|_| Error::NoSnapshot)?),
            mixer: s.mixer.clone(),
            configuration: s.configuration.clone(),
            output: model.selected_output().map(|s| s.key.clone()),
            rec: model.selected_rec().map(|s| s.key.clone()),
            closing: s.closing,
            configuration_gate: s.operation.as_ref().and_then(|op| match op.progress {
                buses::Progress::Queued
                | buses::Progress::Applying
                | buses::Progress::AwaitingObservation => Some(Error::Pending),
                buses::Progress::PartiallyApplied(_) | buses::Progress::Unconfirmed(_) => {
                    Some(Error::ReconciliationRequired)
                }
                _ => None,
            }),
        })
    }
    pub fn graph(&self) -> &Stamped<Arc<Momentopname>> {
        &self.graph
    }
}
impl<O: Clone + 'static> Capture<O> {
    /// Read the same model instance already owned by the bus widget. Its narrow
    /// local selection bridge consumes Event::Selection's exact binding.
    pub fn from_bus_controls<R: Clone + Eq + 'static, T: Clone + Ord + 'static>(
        graph: Stamped<Arc<Momentopname>>,
        controls: &buses::BusControls<O, R, T>,
        budget: Budget,
    ) -> Result<Self, Error> {
        let mut capture = controls
            .inspect_model(|model| Self::from_bus_model(graph, model, budget))
            .map_err(|_| Error::NoSnapshot)??;
        if controls.configuration_pending() {
            capture.configuration_gate = Some(Error::Pending);
        }
        Ok(capture)
    }
}
#[derive(Clone)]
pub struct Binding<O> {
    scope: Rc<()>,
    capture: Rc<Capture<O>>,
}
#[derive(Clone)]
pub struct Confirmation<O> {
    binding: Binding<O>,
    identity: Rc<()>,
}

/// UI correlation only. Never pass this to a backend or use it as a native
/// ticket/origin. The owner associates it with its real shared history request.
#[derive(Clone, Debug)]
pub struct RequestToken(Rc<()>);
impl PartialEq for RequestToken {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for RequestToken {}
pub struct Intent<O> {
    pub origin: O,
    pub graph: Stamped<Arc<Momentopname>>,
    pub bus_configuration: pecan_core::domain::buses::BusConfig,
    pub history: Input,
    pub request: RequestToken,
}
pub enum Event<O> {
    /// Apply selection using the SAME Model::select_destination(&binding,key),
    /// then publish a new Capture. No local optimistic selection is performed.
    Selection {
        origin: O,
        graph: Stamped<Arc<Momentopname>>,
        binding: Rc<buses::Binding<O>>,
        kind: Destination,
        key: String,
    },
    History(Intent<O>),
    OpenPatchbay {
        origin: O,
        graph: Stamped<Arc<Momentopname>>,
        source: String,
        destination: String,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetCounts {
    pub links: usize,
    pub targets: usize,
    pub detached: usize,
}
impl ResetCounts {
    pub fn total(self) -> usize {
        self.links + self.targets + self.detached
    }
}
#[derive(Clone, Debug)]
pub struct RouteCell {
    pub destination: String,
    pub name: String,
    pub state: RouteState,
}
#[derive(Clone, Debug)]
pub struct SourceRow {
    pub key: String,
    pub name: String,
    pub subtitle: String,
    pub output: Option<RouteCell>,
    pub rec: Option<RouteCell>,
    pub elsewhere: Vec<String>,
    pub other_recs: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Choice {
    pub key: String,
    pub name: String,
    pub selected: bool,
    pub kind: Destination,
}
struct Pending {
    token: RequestToken,
    source: Stamp,
    commands: usize,
    ticket: Option<u64>,
    reported: bool,
}
pub struct Model<O> {
    capture: Option<Rc<Capture<O>>>,
    scope: Rc<()>,
    confirmation: Option<Confirmation<O>>,
    pending: Option<Pending>,
    barrier: Option<Stamp>,
    reconcile: bool,
    notice: Option<String>,
}
impl<O: Clone> Default for Model<O> {
    fn default() -> Self {
        Self {
            capture: None,
            scope: Rc::new(()),
            confirmation: None,
            pending: None,
            barrier: None,
            reconcile: false,
            notice: None,
        }
    }
}
impl<O: Clone> Model<O> {
    pub fn observe(&mut self, capture: Capture<O>) -> Result<(), Error> {
        if let Some(old) = &self.capture {
            if earlier(capture.graph.stamp, old.graph.stamp) {
                return Err(Error::OldGraph);
            }
            if capture.graph.stamp == old.graph.stamp
                && !Arc::ptr_eq(&capture.graph.value, &old.graph.value)
            {
                return Err(Error::ConflictingStamp);
            }
        }
        if matches!(
            capture.configuration_gate,
            Some(Error::ReconciliationRequired)
        ) {
            self.reconcile = true;
        }
        self.retire();
        self.capture = Some(Rc::new(capture));
        Ok(())
    }
    pub fn retire(&mut self) {
        self.scope = Rc::new(());
        self.confirmation = None;
    }
    pub fn bind(&self) -> Result<Binding<O>, Error> {
        Ok(Binding {
            scope: self.scope.clone(),
            capture: self.capture.clone().ok_or(Error::NoSnapshot)?,
        })
    }
    fn validate(&self, b: &Binding<O>) -> Result<(), Error> {
        if !Rc::ptr_eq(&b.scope, &self.scope)
            || !self
                .capture
                .as_ref()
                .is_some_and(|c| Rc::ptr_eq(c, &b.capture))
        {
            return Err(Error::StaleBinding);
        }
        Ok(())
    }
    pub fn availability(&self) -> Result<(), Error> {
        let c = self.capture.as_ref().ok_or(Error::NoSnapshot)?;
        if !c.graph.value.verbonden {
            return Err(Error::Disconnected);
        }
        if c.closing {
            return Err(Error::Closing);
        }
        if c.configuration.is_none() {
            return Err(Error::UnknownConfiguration);
        }
        if self.pending.is_some() {
            return Err(Error::Pending);
        }
        if let Some(error) = &c.configuration_gate {
            return Err(error.clone());
        }
        if self.reconcile {
            return Err(Error::ReconciliationRequired);
        }
        if self.barrier.is_some_and(|s| earlier(c.graph.stamp, s)) {
            return Err(Error::AwaitingGraph);
        }
        Ok(())
    }
    pub fn choices(&self) -> Vec<Choice> {
        self.capture.as_ref().map_or_else(Vec::new, |c| {
            c.mixer
                .destinations
                .iter()
                .map(|d| Choice {
                    key: d.key.clone(),
                    name: d.name.clone(),
                    selected: Some(d.key.as_str())
                        == if d.bus.is_some() {
                            c.rec.as_deref()
                        } else {
                            c.output.as_deref()
                        },
                    kind: if d.bus.is_some() {
                        Destination::Rec
                    } else {
                        Destination::Output
                    },
                })
                .collect()
        })
    }
    pub fn rows(&self) -> Vec<SourceRow> {
        self.capture.as_ref().map_or_else(Vec::new, |c| {
            c.mixer
                .apps
                .iter()
                .chain(&c.mixer.hardware)
                .map(|s| {
                    let cell = |key: Option<&str>| {
                        key.and_then(|k| c.mixer.destination(k)).map(|d| RouteCell {
                            destination: d.key.clone(),
                            name: d.name.clone(),
                            state: c.mixer.route(&c.graph.value, s, d).state,
                        })
                    };
                    let present = |d: &&pecan_core::domain::mixer::Strip| {
                        !matches!(c.mixer.route(&c.graph.value, s, d).state, RouteState::Off)
                    };
                    SourceRow {
                        key: s.key.clone(),
                        name: s.name.clone(),
                        subtitle: s.subtitle.clone(),
                        output: cell(c.output.as_deref()),
                        rec: cell(c.rec.as_deref()),
                        elsewhere: c
                            .mixer
                            .destinations
                            .iter()
                            .filter(|d| {
                                Some(d.key.as_str()) != c.output.as_deref()
                                    && Some(d.key.as_str()) != c.rec.as_deref()
                            })
                            .filter(present)
                            .map(|d| d.name.clone())
                            .collect(),
                        other_recs: c
                            .mixer
                            .destinations
                            .iter()
                            .filter(|d| d.bus.is_some() && Some(d.key.as_str()) != c.rec.as_deref())
                            .filter(present)
                            .map(|d| d.name.clone())
                            .collect(),
                    }
                })
                .collect()
        })
    }
    pub fn counts(&self) -> ResetCounts {
        self.capture
            .as_ref()
            .map_or(ResetCounts::default(), |c| ResetCounts {
                links: c.mixer.manual.links.len(),
                targets: c.mixer.manual.targets.len(),
                detached: c.mixer.manual.detached,
            })
    }
    pub fn confirmation(&self) -> Option<ResetCounts> {
        self.confirmation.as_ref().map(|_| self.counts())
    }
    pub fn confirmation_binding(&self) -> Option<Confirmation<O>> {
        self.confirmation.clone()
    }
    pub fn select(&self, b: &Binding<O>, key: &str) -> Result<Event<O>, Error> {
        self.validate(b)?;
        if !b.capture.graph.value.verbonden {
            return Err(Error::Disconnected);
        }
        if b.capture.closing {
            return Err(Error::Closing);
        }
        let d = b
            .capture
            .mixer
            .destination(key)
            .ok_or(Error::MissingStrip)?;
        Ok(Event::Selection {
            origin: b.capture.origin.clone(),
            graph: b.capture.graph.clone(),
            binding: b.capture.selection.clone(),
            kind: if d.bus.is_some() {
                Destination::Rec
            } else {
                Destination::Output
            },
            key: key.into(),
        })
    }
    pub fn route(
        &mut self,
        b: &Binding<O>,
        source: &str,
        destination: &str,
    ) -> Result<Event<O>, Error> {
        self.validate(b)?;
        let c = &b.capture;
        if !c.graph.value.verbonden {
            return Err(Error::Disconnected);
        }
        let s = c.mixer.source(source).ok_or(Error::MissingStrip)?;
        let d = c
            .mixer
            .destination(destination)
            .ok_or(Error::MissingStrip)?;
        if Some(destination) != c.output.as_deref() && Some(destination) != c.rec.as_deref() {
            return Err(Error::StaleBinding);
        }
        let state = c.mixer.route(&c.graph.value, s, d).state;
        if matches!(state, RouteState::Custom { .. }) {
            return Ok(Event::OpenPatchbay {
                origin: c.origin.clone(),
                graph: c.graph.clone(),
                source: source.into(),
                destination: destination.into(),
            });
        }
        self.availability()?;
        let enabled = !matches!(state, RouteState::On { .. });
        self.prepare(
            b,
            if enabled { "Route on" } else { "Route off" },
            Action::Route {
                source: source.into(),
                destination: destination.into(),
                enabled,
            },
        )
    }
    pub fn begin_reset(&mut self, b: &Binding<O>) -> Result<(), Error> {
        self.validate(b)?;
        self.availability()?;
        if self.counts().total() == 0 {
            return Err(Error::NothingToReset);
        }
        self.confirmation = Some(Confirmation {
            binding: b.clone(),
            identity: Rc::new(()),
        });
        Ok(())
    }
    pub fn cancel_reset(&mut self) {
        self.confirmation = None;
    }
    pub fn confirm_reset(&mut self, b: &Confirmation<O>) -> Result<Event<O>, Error> {
        self.validate(&b.binding)?;
        self.availability()?;
        if !self
            .confirmation
            .as_ref()
            .is_some_and(|c| Rc::ptr_eq(&c.identity, &b.identity))
        {
            return Err(Error::NoConfirmation);
        }
        let event = self.prepare(&b.binding, "Back to auto", Action::BackToAuto)?;
        self.confirmation = None;
        Ok(event)
    }
    fn prepare(&mut self, b: &Binding<O>, label: &str, action: Action) -> Result<Event<O>, Error> {
        let config = b
            .capture
            .configuration
            .as_ref()
            .ok_or(Error::UnknownConfiguration)?;
        let commands = history::plan(&action, &b.capture.graph.value, config)
            .map_err(|e| Error::Domain(format!("{e:?}")))?
            .len();
        if commands > MAX_NATIVE_COMMANDS {
            return Err(Error::PlanTooLarge);
        }
        let request = RequestToken(Rc::new(()));
        self.pending = Some(Pending {
            token: request.clone(),
            source: b.capture.graph.stamp,
            commands,
            ticket: None,
            reported: false,
        });
        self.notice = Some("Routing change pending".into());
        Ok(Event::History(Intent {
            origin: b.capture.origin.clone(),
            graph: b.capture.graph.clone(),
            bus_configuration: config.clone(),
            history: Input::Execute {
                label: label.into(),
                action,
            },
            request,
        }))
    }
    pub fn pending(&self) -> Option<RequestToken> {
        self.pending.as_ref().map(|p| p.token.clone())
    }
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
    pub fn handoff_failed(&mut self, token: &RequestToken, error: String) {
        if let Some(p) = &self.pending {
            if &p.token != token {
                return;
            }
            if p.reported {
                self.notice = Some(format!(
                    "Routing error after progress: {error}; result still pending"
                ));
                return;
            }
            self.pending = None;
            self.notice = Some(error);
        }
    }
    /// Non-coalesced delivery of the owner's real, matching native report.
    pub fn report(&mut self, token: &RequestToken, report: NativeReport) -> bool {
        let Some(p) = self.pending.as_mut().filter(|p| &p.token == token) else {
            return false;
        };
        if report.source != p.source
            || report.command_count != p.commands
            || p.ticket.is_some_and(|t| t != report.ticket)
        {
            return false;
        }
        if let NativeProgress::Observed { stamp, counts }
        | NativeProgress::ObservedNoOp { stamp, counts } = &report.progress
        {
            if stamp.epoch != p.source.epoch || earlier(*stamp, p.source) || counts.in_progress != 0
            {
                return false;
            }
        }
        p.ticket = Some(report.ticket);
        p.reported = true;
        self.notice = Some(match &report.progress {
            NativeProgress::Queued => "Routing queued; not applied".into(),
            NativeProgress::ExecutedLocally(_) => {
                "Routing submitted locally; awaiting graph observation".into()
            }
            NativeProgress::Observed { .. } | NativeProgress::ObservedNoOp { .. } => {
                "Routing change observed in the graph".into()
            }
            NativeProgress::Rejected { reason, .. } => format!("Routing rejected: {reason:?}"),
            NativeProgress::Unconfirmed { reason, .. } => {
                format!("Routing unconfirmed; reconcile before retry: {reason:?}")
            }
        });
        match &report.progress {
            NativeProgress::Observed { stamp, .. } | NativeProgress::ObservedNoOp { stamp, .. } => {
                self.barrier = Some(*stamp)
            }
            NativeProgress::Rejected { counts, .. } => {
                // A zero-effect route says nothing about uncertainty already
                // reported by configuration. Only owner reconciliation clears it.
                self.reconcile |= counts.submitted > 0 || counts.in_progress > 0
            }
            NativeProgress::Unconfirmed { .. } => self.reconcile = true,
            _ => {}
        }
        if report.progress.is_terminal() {
            self.pending = None;
        }
        true
    }
    /// Owner-only explicit reconciliation. A fresh graph does not by itself
    /// prove rollback, release history, or clear an in-progress receipt.
    pub fn reconciled(&mut self, capture: Capture<O>) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Pending);
        }
        if let Some(error) = &capture.configuration_gate {
            return Err(error.clone());
        }
        if !capture.graph.value.verbonden {
            return Err(Error::Disconnected);
        }
        if capture.closing {
            return Err(Error::Closing);
        }
        if capture.configuration.is_none() {
            return Err(Error::UnknownConfiguration);
        }
        self.observe(capture)?;
        self.reconcile = false;
        self.barrier = None;
        self.notice = Some("Routing status refreshed".into());
        Ok(())
    }
}
fn earlier(a: Stamp, b: Stamp) -> bool {
    (a.epoch, a.revision) < (b.epoch, b.revision)
}

#[cfg(test)]
mod tests;
