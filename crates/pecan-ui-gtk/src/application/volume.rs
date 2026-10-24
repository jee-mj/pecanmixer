//! One active identity-bound volume/memory group, one in-flight exact native
//! receipt and one latest desired dB. Intermediate samples never record history.
use super::*;
use crate::controls::partial_mute::{self, Memory, Proposal};
use pecan_core::{
    domain::{
        levels::{MAX_DB, OFF_DB},
        mixer::{self, Mixer, Strip},
    },
    history_identity::RegistryBinding,
    native_command::NativeIntent,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripIdentity {
    registry: RegistryBinding,
    key: String,
    node: u32,
    serial: Option<u64>,
    ports: Vec<u32>,
    mapping: Vec<(u32, bool, Option<u32>, Option<String>)>,
    channels: Vec<usize>,
    vector_len: usize,
}
#[derive(Clone, Debug)]
pub struct GestureToken(Rc<()>);
impl PartialEq for GestureToken {
    fn eq(&self, o: &Self) -> bool {
        Rc::ptr_eq(&self.0, &o.0)
    }
}
impl Eq for GestureToken {}
struct Sample {
    ticket: u64,
    source: Stamp,
    receipt: NativeReceipt,
    sequence: u64,
    after: Vec<f32>,
    proposal: Option<Proposal<StripIdentity>>,
    observed: Option<Stamp>,
}
enum Desired {
    Db(f64),
    Toggle,
}
struct Group {
    token: GestureToken,
    identity: StripIdentity,
    before: Vec<f32>,
    desired: Option<(u64, Desired)>,
    sequence: u64,
    confirmed: Option<(u64, Vec<f32>)>,
    flight: Option<Sample>,
    ended: bool,
    abandoned: Option<String>,
}
pub(super) struct Volumes {
    group: Option<Group>,
    memory: Memory<StripIdentity>,
    remembered: BTreeMap<String, StripIdentity>,
}
impl Default for Volumes {
    fn default() -> Self {
        Self {
            group: None,
            memory: Memory::new(256),
            remembered: BTreeMap::new(),
        }
    }
}
impl Volumes {
    pub fn busy(&self) -> bool {
        self.group.is_some()
    }
    pub fn has_receipt(&self) -> bool {
        self.group.as_ref().is_some_and(|g| g.flight.is_some())
    }
    fn model(s: &State) -> Option<Mixer> {
        Some(Mixer::build(
            &s.graph.as_ref()?.value,
            &s.history.buses.definitions().ok()?,
        ))
    }
    fn strip(s: &State, key: &str) -> Option<Strip> {
        let m = Self::model(s)?;
        m.apps
            .iter()
            .chain(&m.hardware)
            .chain(&m.destinations)
            .find(|s| s.key == key)
            .cloned()
    }
    fn identity(s: &State, key: &str) -> Result<StripIdentity, String> {
        let strip = Self::strip(s, key).ok_or("Strip is unavailable.")?;
        if strip.bus.is_some() {
            return Err("Volume binding is unsupported.".into());
        }
        let volume = strip.node.volume.as_ref().ok_or("Volume unavailable.")?;
        if volume.iter().any(|v| !v.is_finite() || *v < 0.)
            || strip.channels.is_empty()
            || strip.channels.iter().any(|c| *c >= volume.len())
        {
            return Err("Invalid volume shape.".into());
        }
        let frame = s.frame.as_ref().ok_or("Native identity unavailable.")?;
        let registry = frame
            .bind_objects(
                std::iter::once(strip.node.id).chain(
                    frame
                        .graph()
                        .value
                        .poorten
                        .iter()
                        .filter(|p| {
                            p.knoop == strip.node.id
                                && p.soort == pecan_core::graph::Soort::Audio
                                && !p.monitor
                        })
                        .map(|p| p.id),
                ),
            )
            .ok_or("Registry binding unavailable.")?;
        let mut mapping: Vec<_> = frame
            .graph()
            .value
            .poorten
            .iter()
            .filter(|p| {
                p.knoop == strip.node.id && p.soort == pecan_core::graph::Soort::Audio && !p.monitor
            })
            .map(|p| (p.id, p.ingang, p.volgnummer, p.kanaal.clone()))
            .collect();
        mapping.sort();
        Ok(StripIdentity {
            registry,
            mapping,
            key: key.into(),
            node: strip.node.id,
            serial: strip.node.serial,
            ports: strip.ports.iter().map(|p| p.id).collect(),
            channels: strip.channels.clone(),
            vector_len: volume.len(),
        })
    }
    fn valid(s: &State, id: &StripIdentity) -> bool {
        Self::identity(s, &id.key).is_ok_and(|current| current == *id)
    }
    fn cleanup(&mut self, s: &State) {
        if self.has_receipt() {
            return;
        }
        let removed: Vec<_> = self
            .remembered
            .iter()
            .filter(|(_, id)| !Self::valid(s, id))
            .map(|(k, _)| k.clone())
            .collect();
        for key in removed {
            let _ = self.memory.invalidate_key(&key);
            self.remembered.remove(&key);
        }
    }
    fn abandon(&mut self, s: &mut State, reason: &str) {
        if let Some(group) = self.group.as_mut() {
            group.abandoned = Some(reason.into());
            group.ended = true;
            let _ = self.memory.invalidate_key(&group.identity.key);
            self.remembered.remove(&group.identity.key);
            if group.flight.is_none() {
                self.group = None;
            }
            s.status = Status::Unconfirmed(reason.into());
            s.history_context = Rc::new(());
        }
    }
    fn poll(&mut self, s: &mut State, endpoint: &dyn Endpoint) {
        if self
            .group
            .as_ref()
            .is_some_and(|g| !Self::valid(s, &g.identity))
        {
            self.abandon(s, "Volume binding changed; admitted effects may remain.");
        }
        if s.frame.is_none() {
            let _ = self.memory.clear();
            self.remembered.clear();
        }
        let Some(mut group) = self.group.take() else {
            self.cleanup(s);
            return;
        };
        if let Some(sample) = group.flight.as_mut() {
            let report = sample.receipt.report();
            if report.ticket != sample.ticket || report.source != sample.source {
                group.abandoned = Some("Mismatched volume receipt; effects may remain.".into());
                group.ended = true;
                let _ = self.memory.invalidate_key(&group.identity.key);
                self.remembered.remove(&group.identity.key);
            }
            match report.progress {
                NativeProgress::Queued => {}
                NativeProgress::ExecutedLocally(_) => {
                    s.status = Status::ExecutedLocally;
                }
                NativeProgress::Observed { stamp, .. }
                | NativeProgress::ObservedNoOp { stamp, .. } => {
                    if stamp.epoch != sample.source.epoch
                        || stamp.revision <= sample.source.revision
                    {
                        group.abandoned = Some("Invalid volume observation stamp.".into());
                        group.flight = None;
                        group.ended = true;
                        let _ = self.memory.invalidate_key(&group.identity.key);
                        self.remembered.remove(&group.identity.key);
                    } else {
                        sample.observed = Some(stamp);
                        s.status = Status::WaitingForGraph;
                    }
                }
                NativeProgress::Rejected { reason, .. } => {
                    s.status = Status::Failed(format!(
                        "Volume rejected: {reason:?}; earlier samples may remain."
                    ));
                    group.flight = None;
                    group.ended = true;
                    group.abandoned = Some("Volume request rejected.".into());
                }
                NativeProgress::Unconfirmed { reason, .. } => {
                    s.status = Status::Unconfirmed(format!(
                        "Volume unconfirmed: {reason:?}; effects may remain."
                    ));
                    group.flight = None;
                    group.ended = true;
                    group.abandoned = Some("Volume completion unconfirmed.".into());
                    let _ = self.memory.invalidate_key(&group.identity.key);
                    self.remembered.remove(&group.identity.key);
                }
            }
        }
        if let Some(reason) = &group.abandoned {
            if !matches!(s.status, Status::Failed(_)) {
                s.status = Status::Unconfirmed(reason.clone());
            }
            if group
                .flight
                .as_ref()
                .is_some_and(|f| f.receipt.report().progress.is_terminal())
            {
                group.flight = None;
            }
            if group.flight.is_some() {
                self.group = Some(group);
            } else {
                s.history_context = Rc::new(());
            }
            return;
        }
        let ready = group.flight.as_ref().is_some_and(|sample| {
            sample.observed.is_some_and(|stamp| {
                s.graph
                    .as_ref()
                    .is_some_and(|g| g.stamp.epoch == stamp.epoch && g.stamp >= stamp)
            })
        });
        if ready {
            let sample = group.flight.take().unwrap();
            if let Some(proposal) = sample.proposal {
                let strip = Self::strip(s, &group.identity.key).unwrap();
                let kind = proposal.kind();
                if let Err(error) = self.memory.apply_memory(proposal, &group.identity, &strip) {
                    s.status = Status::Unconfirmed(format!(
                        "Observed gain but memory could not commit: {error:?}"
                    ));
                    let _ = self.memory.invalidate_key(&group.identity.key);
                    self.remembered.remove(&group.identity.key);
                    s.history_context = Rc::new(());
                    return;
                }
                match kind {
                    partial_mute::Kind::Mute => {
                        self.remembered
                            .insert(group.identity.key.clone(), group.identity.clone());
                    }
                    _ => {
                        self.remembered.remove(&group.identity.key);
                    }
                }
            }
            group.confirmed = Some((sample.sequence, sample.after));
        }
        if group.flight.is_none() {
            self.cleanup(s);
            let needs_sample = group.desired.as_ref().is_some_and(|(seq, _)| {
                group.confirmed.as_ref().is_none_or(|(done, _)| done != seq)
            });
            if needs_sample {
                let (sequence, desired) = group.desired.as_ref().unwrap();
                let strip = Self::strip(s, &group.identity.key).unwrap();
                let partial = strip.channels.len() != group.identity.vector_len;
                let proposal = if partial {
                    Some(match desired {
                        Desired::Db(db) => {
                            self.memory
                                .propose_fader(group.identity.clone(), &strip, *db)
                        }
                        Desired::Toggle => {
                            self.memory.propose_toggle(group.identity.clone(), &strip)
                        }
                    })
                } else {
                    None
                };
                let proposal = match proposal.transpose() {
                    Ok(p) => p,
                    Err(e) => {
                        s.status = Status::Failed(format!("Partial gain proposal rejected: {e:?}"));
                        s.history_context = Rc::new(());
                        return;
                    }
                };
                let after = if let Some(p) = &proposal {
                    p.after().to_vec()
                } else {
                    match desired {
                        Desired::Db(db) => mixer::with_db(&strip, *db).unwrap(),
                        Desired::Toggle => unreachable!(),
                    }
                };
                let source = s.graph.as_ref().unwrap().clone();
                let ticket = s.history.reserve_ticket();
                match endpoint.dispatch(NativeIntent {
                    ticket,
                    origin: source.clone(),
                    commands: vec![Opdracht::ZetVolume {
                        knoop: group.identity.node,
                        volume: after.clone(),
                    }],
                }) {
                    Ok(receipt) => {
                        let report = receipt.report();
                        if report.ticket != ticket
                            || report.source != source.stamp
                            || report.command_count != 1
                        {
                            s.status = Status::Unconfirmed(
                                "Mismatched volume admission receipt; effects may remain.".into(),
                            );
                            let _ = self.memory.invalidate_key(&group.identity.key);
                            self.remembered.remove(&group.identity.key);
                            // Admission may already have effects. Retain even a
                            // malformed receipt until terminal, preventing a
                            // second group from overtaking uncertain work.
                            group.abandoned = Some(
                                "Mismatched volume admission receipt; effects may remain.".into(),
                            );
                            group.ended = true;
                            group.flight = Some(Sample {
                                ticket,
                                source: source.stamp,
                                receipt,
                                sequence: *sequence,
                                after,
                                proposal: None,
                                observed: None,
                            });
                            self.group = Some(group);
                            s.history_context = Rc::new(());
                            return;
                        }
                        group.flight = Some(Sample {
                            ticket,
                            source: source.stamp,
                            receipt,
                            sequence: *sequence,
                            after,
                            proposal,
                            observed: None,
                        });
                        s.status = Status::Queued;
                    }
                    Err(e) => {
                        s.status = Status::Failed(format!(
                            "Volume admission rejected: {e:?}; earlier samples may remain."
                        ));
                        s.history_context = Rc::new(());
                        return;
                    }
                }
            } else if group.ended {
                if let Some((_, after)) = group.confirmed {
                    if let Err(e) = s.history.record_volume_binding(
                        group.identity.node,
                        group.before,
                        after,
                        &group.identity.registry,
                    ) {
                        s.status = Status::Unconfirmed(format!(
                            "Observed gesture could not enter history: {e:?}"
                        ));
                        s.history_context = Rc::new(());
                        return;
                    }
                }
                s.status = Status::Observed {
                    no_op: group.desired.is_none(),
                };
                s.history_context = Rc::new(());
                return;
            } else {
                s.status = Status::GestureActive;
            }
        }
        if group.ended && group.flight.is_some() {
            s.status = Status::GestureFinishing;
        }
        self.group = Some(group);
    }
}
impl State {
    fn with_volume<R>(&mut self, f: impl FnOnce(&mut Volumes, &mut Self) -> R) -> R {
        let mut volume = std::mem::take(&mut self.volume);
        let result = f(&mut volume, self);
        self.volume = volume;
        result
    }
    pub(super) fn poll_volume(&mut self, endpoint: &dyn Endpoint) {
        self.with_volume(|v, s| v.poll(s, endpoint));
    }
    pub(super) fn abandon_volume(&mut self, reason: &str) {
        self.with_volume(|v, s| v.abandon(s, reason));
    }
}
impl Session {
    /// Opaque node authority for controls that do not require gain parameters.
    pub fn node_binding(&self, node: u32) -> Option<RegistryBinding> {
        let state = self.0.state.borrow();
        let frame = state.frame.as_ref()?;
        frame.graph().value.knopen.iter().find(|n| n.id == node)?;
        frame.bind_objects([node])
    }
    pub fn strip_identity(&self, key: &str) -> Result<StripIdentity, String> {
        Volumes::identity(&self.0.state.borrow(), key)
    }
    pub fn volume_model(&self) -> Mixer {
        Volumes::model(&self.0.state.borrow()).unwrap_or_default()
    }
    pub fn volume_active(&self, token: &GestureToken) -> bool {
        self.0
            .state
            .borrow()
            .volume
            .group
            .as_ref()
            .is_some_and(|g| g.token == *token && g.abandoned.is_none())
    }
    pub fn volume_input_available(&self, key: &str) -> bool {
        let s = self.0.state.borrow();
        !s.admission_closed
            && s.witnessed
            && s.pending.is_none()
            && Volumes::identity(&s, key).is_ok()
            && s.volume
                .group
                .as_ref()
                .is_none_or(|g| g.identity.key == key && !g.ended && g.abandoned.is_none())
    }
    pub fn begin_volume(&self, origin: &Graph, key: &str) -> Result<GestureToken, String> {
        let mut s = self.0.state.borrow_mut();
        if s.admission_closed || s.pending.is_some() || s.volume.busy() {
            return Err("Another change/gesture is pending or admission is closed.".into());
        }
        if !s
            .graph
            .as_ref()
            .is_some_and(|g| g.stamp == origin.stamp && Arc::ptr_eq(&g.value, &origin.value))
        {
            return Err("Stale presented volume origin.".into());
        }
        let identity = Volumes::identity(&s, key)?;
        let before = Volumes::strip(&s, key).unwrap().node.volume.unwrap();
        let token = GestureToken(Rc::new(()));
        s.volume.group = Some(Group {
            token: token.clone(),
            identity,
            before,
            desired: None,
            sequence: 0,
            confirmed: None,
            flight: None,
            ended: false,
            abandoned: None,
        });
        s.status = Status::GestureActive;
        s.history_context = Rc::new(());
        Ok(token)
    }
    pub fn update_volume(&self, token: &GestureToken, db: f64) -> Result<(), String> {
        if !db.is_finite() || !(OFF_DB..=MAX_DB).contains(&db) {
            return Err("Invalid volume dB.".into());
        }
        let mut s = self.0.state.borrow_mut();
        let group = s
            .volume
            .group
            .as_mut()
            .filter(|g| g.token == *token && !g.ended && g.abandoned.is_none())
            .ok_or("Gesture no longer accepts input.")?;
        group.sequence = group
            .sequence
            .checked_add(1)
            .ok_or("Volume sequence exhausted.")?;
        group.desired = Some((group.sequence, Desired::Db(db)));
        s.poll_volume(self.0.endpoint.as_ref());
        Ok(())
    }
    pub fn end_volume(&self, token: &GestureToken) -> Result<(), String> {
        let mut s = self.0.state.borrow_mut();
        let group = s
            .volume
            .group
            .as_mut()
            .filter(|g| g.token == *token && g.abandoned.is_none())
            .ok_or("Gesture unavailable.")?;
        group.ended = true;
        s.poll_volume(self.0.endpoint.as_ref());
        Ok(())
    }
    pub fn abandon_volume(&self, token: &GestureToken) {
        let mut s = self.0.state.borrow_mut();
        if s.volume.group.as_ref().is_some_and(|g| g.token == *token) {
            s.abandon_volume("Volume input invalidated; admitted effects may remain.");
        }
    }
    pub fn partial_muted(&self, key: &str) -> Result<bool, String> {
        let s = self.0.state.borrow();
        let strip = Volumes::strip(&s, key).ok_or("Strip missing.")?;
        let identity = Volumes::identity(&s, key)?;
        s.volume
            .memory
            .is_muted(&identity, &strip)
            .map_err(|e| format!("{e:?}"))
    }
    pub fn toggle_partial_mute(&self, origin: &Graph, key: &str) -> Result<GestureToken, String> {
        {
            let s = self.0.state.borrow();
            let strip = Volumes::strip(&s, key).ok_or("Strip unavailable.")?;
            if strip
                .node
                .volume
                .as_ref()
                .is_none_or(|v| strip.channels.len() == v.len())
            {
                return Err("Partial mute requires a partial strip.".into());
            }
        }
        let token = self.begin_volume(origin, key)?;
        let mut s = self.0.state.borrow_mut();
        let group = s.volume.group.as_mut().unwrap();
        group.sequence = 1;
        group.desired = Some((1, Desired::Toggle));
        group.ended = true;
        s.poll_volume(self.0.endpoint.as_ref());
        Ok(token)
    }
}

#[cfg(test)]
#[path = "volume/tests.rs"]
mod tests;
