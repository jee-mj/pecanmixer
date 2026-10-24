use std::sync::Arc;

use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{
        buses::BusConfig,
        levels::{MAX_DB, OFF_DB},
        mixer::{self, Mixer, Strip},
    },
    graph::{Momentopname, Opdracht},
};

/// UI-local boundary, not a second backend protocol. A production implementation
/// must forward the exact supplied snapshot origin to its reviewed dispatch API.
/// Submit returns admission only; confirmed/rejected callbacks arrive separately.
pub trait Adapter {
    type Snapshot: Clone;
    type Request: Clone;
    fn same_request(a: &Self::Request, b: &Self::Request) -> bool;
    fn graph(snapshot: &Self::Snapshot) -> &Stamped<Arc<Momentopname>>;
    fn writable(&self) -> bool {
        true
    }
    fn submit(
        &mut self,
        origin: &Self::Snapshot,
        command: Opdracht,
    ) -> Result<Self::Request, String>;
}

#[derive(Default)]
pub struct ReadOnly;
impl Adapter for ReadOnly {
    type Snapshot = Stamped<Arc<Momentopname>>;
    type Request = std::convert::Infallible;
    fn graph(snapshot: &Self::Snapshot) -> &Stamped<Arc<Momentopname>> {
        snapshot
    }
    fn same_request(a: &Self::Request, b: &Self::Request) -> bool {
        a == b
    }
    fn writable(&self) -> bool {
        false
    }
    fn submit(&mut self, _: &Self::Snapshot, _: Opdracht) -> Result<Self::Request, String> {
        Err("Mixer controls are not connected.".into())
    }
}

/// Bound when the UI presents/captures an interaction. Keeping the snapshot
/// prevents a delayed signal from silently adopting a newer object's origin.
#[derive(Clone)]
pub struct Binding<S> {
    pub(super) snapshot: S,
    pub(super) key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    ReadOnly,
    Disconnected,
    Ready,
    WaitingForCompletion,
    WaitingForObservation,
    WaitingForGraph,
    Failed(String),
    Unconfirmed(String),
}
impl Phase {
    pub fn message(&self) -> String {
        match self {
            Self::ReadOnly => "Read-only · mixer controls are not connected".into(),
            Self::Disconnected => "Disconnected · controls unavailable".into(),
            Self::Ready => "Mixer controls ready".into(),
            Self::WaitingForCompletion => "Pending · queued for the worker".into(),
            Self::WaitingForObservation => "Pending · waiting for server-fed observation".into(),
            Self::WaitingForGraph => "Observed · waiting for the graph frame".into(),
            Self::Failed(message) => format!("Could not apply change: {message}"),
            Self::Unconfirmed(message) => format!("Previous change is unconfirmed: {message}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlError {
    ReadOnly,
    Disconnected,
    Busy,
    StaleView,
    MissingStrip,
    Unsupported,
    InvalidValue,
    Rejected(String),
}

struct Pending<R> {
    request: R,
    source: Stamp,
    node: u32,
    serial: u64,
    name: String,
    observed: Option<Stamp>,
    executed_locally: bool,
}

pub struct Controller<A: Adapter> {
    adapter: A,
    snapshot: Option<A::Snapshot>,
    buses: BusConfig,
    model: Mixer,
    pending: Option<Pending<A::Request>>,
    status: Option<Phase>,
}
impl Default for Controller<ReadOnly> {
    fn default() -> Self {
        Self::new(ReadOnly, BusConfig::default())
    }
}
impl<A: Adapter> Controller<A> {
    pub fn new(adapter: A, buses: BusConfig) -> Self {
        Self {
            adapter,
            snapshot: None,
            buses,
            model: Mixer::default(),
            pending: None,
            status: None,
        }
    }
    pub fn model(&self) -> &Mixer {
        &self.model
    }
    pub fn phase(&self) -> Phase {
        if !self.adapter.writable() {
            return Phase::ReadOnly;
        }
        if !self
            .snapshot
            .as_ref()
            .is_some_and(|s| A::graph(s).value.verbonden)
        {
            return Phase::Disconnected;
        }
        if let Some(pending) = &self.pending {
            return if pending.observed.is_some() {
                Phase::WaitingForGraph
            } else if pending.executed_locally {
                Phase::WaitingForObservation
            } else {
                Phase::WaitingForCompletion
            };
        }
        self.status.clone().unwrap_or(Phase::Ready)
    }
    pub fn enabled(&self) -> bool {
        self.adapter.writable()
            && self.pending.is_none()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| A::graph(s).value.verbonden)
    }
    pub fn snapshot(&self) -> Option<&A::Snapshot> {
        self.snapshot.as_ref()
    }
    pub fn pending_request(&self) -> Option<&A::Request> {
        self.pending.as_ref().map(|p| &p.request)
    }
    fn strip(&self, key: &str) -> Option<&Strip> {
        self.model
            .apps
            .iter()
            .chain(&self.model.hardware)
            .chain(&self.model.destinations)
            .find(|s| s.key == key)
    }
    pub fn bind(&self, key: &str) -> Result<Binding<A::Snapshot>, ControlError> {
        if self.strip(key).is_none() {
            return Err(ControlError::MissingStrip);
        }
        Ok(Binding {
            snapshot: self.snapshot.clone().ok_or(ControlError::Disconnected)?,
            key: key.into(),
        })
    }
    pub fn volume_available(&self, strip: &Strip) -> bool {
        self.enabled()
            && strip.bus.is_none()
            && strip.node.serial.is_some()
            && mixer::strip_db(strip).is_some_and(|v| v.is_finite())
    }
    /// Whole-node mute only in this first slice. Channel-pair mute restoration
    /// and Rec bus gain/mute remain separate, explicitly unsupported intents.
    pub fn mute_available(&self, strip: &Strip) -> bool {
        self.volume_available(strip)
            && strip.node.mute.is_some()
            && strip
                .node
                .volume
                .as_ref()
                .is_some_and(|v| v.len() == strip.channels.len())
    }
    pub fn observe(&mut self, snapshot: A::Snapshot) -> bool {
        let graph = A::graph(&snapshot);
        if self
            .snapshot
            .as_ref()
            .is_some_and(|old| graph.stamp <= A::graph(old).stamp)
        {
            return false;
        }
        let invalidated = self.pending.as_ref().is_some_and(|p| {
            graph.stamp.epoch != p.source.epoch
                || !graph.value.verbonden
                || !graph
                    .value
                    .knopen
                    .iter()
                    .any(|n| n.id == p.node && n.serial == Some(p.serial) && n.naam == p.name)
        });
        if invalidated {
            self.pending = None;
            self.status = Some(Phase::Unconfirmed(
                "The connection or target changed.".into(),
            ));
        }
        let definitions = self.buses.definitions().unwrap_or_default();
        self.model = Mixer::build(&graph.value, &definitions);
        self.snapshot = Some(snapshot);
        self.finish_if_observed();
        true
    }
    fn validate_binding(&self, binding: &Binding<A::Snapshot>) -> Result<&Strip, ControlError> {
        if !self.adapter.writable() {
            return Err(ControlError::ReadOnly);
        }
        if self.pending.is_some() {
            return Err(ControlError::Busy);
        }
        let current = self.snapshot.as_ref().ok_or(ControlError::Disconnected)?;
        if !A::graph(current).value.verbonden {
            return Err(ControlError::Disconnected);
        }
        // Conservative first slice: old UI snapshots are rejected instead of
        // recomputing a full channel vector from another interaction's state.
        if A::graph(&binding.snapshot).stamp != A::graph(current).stamp {
            return Err(ControlError::StaleView);
        }
        self.strip(&binding.key).ok_or(ControlError::MissingStrip)
    }
    pub fn set_volume(
        &mut self,
        binding: &Binding<A::Snapshot>,
        db: f64,
    ) -> Result<(), ControlError> {
        if !db.is_finite() || !(OFF_DB..=MAX_DB).contains(&db) {
            return Err(ControlError::InvalidValue);
        }
        let strip = self.validate_binding(binding)?;
        if !self.volume_available(strip) {
            return Err(ControlError::Unsupported);
        }
        let values = mixer::with_db(strip, db).ok_or(ControlError::Unsupported)?;
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(ControlError::Unsupported);
        }
        let node = strip.node.id;
        self.admit(
            binding,
            node,
            Opdracht::ZetVolume {
                knoop: node,
                volume: values,
            },
        )
    }
    pub fn set_mute(
        &mut self,
        binding: &Binding<A::Snapshot>,
        muted: bool,
    ) -> Result<(), ControlError> {
        let strip = self.validate_binding(binding)?;
        if !self.mute_available(strip) {
            return Err(ControlError::Unsupported);
        }
        let node = strip.node.id;
        self.admit(
            binding,
            node,
            Opdracht::ZetMute {
                knoop: node,
                mute: muted,
            },
        )
    }
    fn admit(
        &mut self,
        binding: &Binding<A::Snapshot>,
        node: u32,
        command: Opdracht,
    ) -> Result<(), ControlError> {
        let graph = A::graph(&binding.snapshot);
        let owner = graph
            .value
            .knopen
            .iter()
            .find(|n| n.id == node)
            .ok_or(ControlError::MissingStrip)?;
        let serial = owner.serial.ok_or(ControlError::Unsupported)?;
        match self.adapter.submit(&binding.snapshot, command) {
            Ok(request) => {
                self.pending = Some(Pending {
                    request,
                    source: graph.stamp,
                    node,
                    serial,
                    name: owner.naam.clone(),
                    observed: None,
                    executed_locally: false,
                });
                self.status = None;
                Ok(())
            }
            Err(message) => {
                self.status = Some(Phase::Failed(message.clone()));
                Err(ControlError::Rejected(message))
            }
        }
    }
    /// Local execution is progress only. It cannot acknowledge history or
    /// update displayed graph values.
    pub fn executed_locally(&mut self, request: &A::Request) -> bool {
        let Some(pending) = self
            .pending
            .as_mut()
            .filter(|p| A::same_request(&p.request, request))
        else {
            return false;
        };
        if pending.executed_locally {
            return false;
        }
        pending.executed_locally = true;
        true
    }
    /// Supply the backend's observed postcondition stamp, never admission/local
    /// execution. The corresponding graph frame must reach this controller.
    /// Observation proves a postcondition, not a causal server transaction ack.
    pub fn observed_result(&mut self, request: &A::Request, stamp: Stamp) -> bool {
        let Some(pending) = self
            .pending
            .as_mut()
            .filter(|p| A::same_request(&p.request, request))
        else {
            return false;
        };
        if stamp.epoch != pending.source.epoch
            || stamp.revision <= pending.source.revision
            || pending.observed == Some(stamp)
        {
            return false;
        }
        pending.observed = Some(stamp);
        self.finish_if_observed();
        true
    }
    pub fn unconfirmed(&mut self, request: &A::Request, message: impl Into<String>) -> bool {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| A::same_request(&p.request, request))
        {
            return false;
        }
        self.pending = None;
        self.status = Some(Phase::Unconfirmed(message.into()));
        true
    }
    pub fn rejected(&mut self, request: &A::Request, message: impl Into<String>) -> bool {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| A::same_request(&p.request, request))
        {
            return false;
        }
        self.pending = None;
        self.status = Some(Phase::Failed(message.into()));
        true
    }
    fn finish_if_observed(&mut self) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let graph = A::graph(snapshot);
        let Some(observed) = pending.observed else {
            return;
        };
        if graph.stamp.epoch != observed.epoch || graph.stamp.revision < observed.revision {
            return;
        }
        let Some(node) = graph.value.knopen.iter().find(|n| {
            n.id == pending.node && n.serial == Some(pending.serial) && n.naam == pending.name
        }) else {
            return;
        };
        // The receipt already proves the postcondition at `observed`. A later
        // external change must not leave this request pending forever. Display
        // the latest actual graph, without attributing later changes to us.
        let _ = node;
        self.pending = None;
        self.status = None;
    }
}
