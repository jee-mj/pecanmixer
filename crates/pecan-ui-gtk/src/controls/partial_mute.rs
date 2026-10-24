//! Pure gain memory from `Mixer.tsx::Kaart`; see `partial_mute/README.md`.
//! Proposals and memory application never attest audio completion or history.
use pecan_core::domain::{
    levels::OFF_DB,
    mixer::{self, Strip},
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    WholeNodeOrBus,
    MissingVolume,
    InvalidChannels,
    InvalidGain,
    Capacity,
    StaleProposal,
    DifferentBinding,
    RevisionExhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mute,
    Restore,
    Fader,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding<T> {
    identity: T,
    key: String,
    node: u32,
    serial: Option<u64>,
    ports: Vec<u32>,
    channels: Vec<usize>,
    vector_len: usize,
}
struct Entry<T> {
    binding: Binding<T>,
    db: f64,
}
enum Change {
    Remember(f64),
    Forget,
}
/// A proposed full-vector gain write and local memory transition. Before keeps
/// every original channel, including unequal selected gains, for the owner's
/// observed/history integration. Preparing/dropping a proposal changes nothing.
pub struct Proposal<T> {
    authority: Rc<()>,
    revision: u64,
    binding: Binding<T>,
    kind: Kind,
    before: Vec<f32>,
    after: Vec<f32>,
    change: Change,
}
impl<T> Proposal<T> {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn before(&self) -> &[f32] {
        &self.before
    }
    pub fn after(&self) -> &[f32] {
        &self.after
    }
}
/// Explicitly bounded local memory. `T` is authoritative owner-supplied identity,
/// not a key/ID-derived generation or a completion token invented by this module.
/// Exact keys address entries; identity plus shape guard reuse across renders.
pub struct Memory<T> {
    authority: Rc<()>,
    revision: u64,
    capacity: usize,
    entries: BTreeMap<String, Entry<T>>,
}
impl<T: Clone + Eq> Memory<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            authority: Rc::new(()),
            revision: 0,
            capacity,
            entries: BTreeMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn remembered_db(&self, identity: &T, strip: &Strip) -> Result<Option<f64>, Error> {
        let binding = binding(identity.clone(), strip)?;
        Ok(self
            .entries
            .get(&binding.key)
            .filter(|e| e.binding == binding)
            .map(|e| e.db))
    }
    pub fn is_muted(&self, identity: &T, strip: &Strip) -> Result<bool, Error> {
        Ok(self.remembered_db(identity, strip)?.is_some()
            && mixer::strip_db(strip).ok_or(Error::InvalidGain)? <= OFF_DB)
    }
    pub fn propose_toggle(&self, identity: T, strip: &Strip) -> Result<Proposal<T>, Error> {
        let binding = binding(identity.clone(), strip)?;
        let db = mixer::strip_db(strip).ok_or(Error::InvalidGain)?;
        let (kind, target, change) = match self.remembered_db(&identity, strip)? {
            Some(remembered) if db <= OFF_DB => (Kind::Restore, remembered, Change::Forget),
            _ => (Kind::Mute, OFF_DB, Change::Remember(db)),
        };
        if matches!(change, Change::Remember(_))
            && !self.entries.contains_key(&binding.key)
            && self.entries.len() >= self.capacity
        {
            return Err(Error::Capacity);
        }
        self.proposal(binding, strip, target, kind, change)
    }
    /// The existing core helper preserves every unselected channel. A partial
    /// fader write forgets only this exact strip's remembered gain, even at off.
    pub fn propose_fader(&self, identity: T, strip: &Strip, db: f64) -> Result<Proposal<T>, Error> {
        let binding = binding(identity, strip)?;
        self.proposal(binding, strip, db, Kind::Fader, Change::Forget)
    }
    fn proposal(
        &self,
        binding: Binding<T>,
        strip: &Strip,
        db: f64,
        kind: Kind,
        change: Change,
    ) -> Result<Proposal<T>, Error> {
        // Reserve the possibility of committing before an owner dispatches it.
        self.next_revision()?;
        let after = mixer::with_db(strip, db).ok_or(Error::InvalidGain)?;
        Ok(Proposal {
            authority: self.authority.clone(),
            revision: self.revision,
            binding,
            kind,
            before: strip.node.volume.clone().ok_or(Error::MissingVolume)?,
            after,
            change,
        })
    }
    /// Apply only the local memory transition, after the owner validates actual
    /// receipt/graph evidence and supplies the still-identical binding. This API
    /// cannot authenticate a caller's identity/evidence or record undo itself.
    /// Dropping a rejected proposal leaves memory unchanged. Unconfirmed effects
    /// require explicit owner invalidation/reconciliation, not a guessed success.
    pub fn apply_memory(
        &mut self,
        proposal: Proposal<T>,
        identity: &T,
        observed_strip: &Strip,
    ) -> Result<(), Error> {
        if !Rc::ptr_eq(&self.authority, &proposal.authority) || proposal.revision != self.revision {
            return Err(Error::StaleProposal);
        }
        if proposal.binding != binding(identity.clone(), observed_strip)? {
            return Err(Error::DifferentBinding);
        }
        let next = self.next_revision()?;
        match proposal.change {
            Change::Remember(db) => {
                self.entries.insert(
                    proposal.binding.key.clone(),
                    Entry {
                        binding: proposal.binding,
                        db,
                    },
                );
            }
            Change::Forget => {
                self.entries.remove(&proposal.binding.key);
            }
        }
        self.revision = next;
        Ok(())
    }
    /// Identity loss/replacement clears all strips bound to that owner identity.
    /// Advance the local revision even with no entry to retire outstanding plans.
    pub fn invalidate_identity(&mut self, identity: &T) -> Result<(), Error> {
        let next = self.next_revision()?;
        self.entries.retain(|_, e| &e.binding.identity != identity);
        self.revision = next;
        Ok(())
    }
    pub fn invalidate_key(&mut self, key: &str) -> Result<(), Error> {
        let next = self.next_revision()?;
        self.entries.remove(key);
        self.revision = next;
        Ok(())
    }
    /// Disconnect/owner replacement hook. Hiding a strip alone need not clear
    /// session memory. This does not cancel or roll back backend work.
    pub fn clear(&mut self) -> Result<(), Error> {
        let next = self.next_revision()?;
        self.entries.clear();
        self.revision = next;
        Ok(())
    }
    fn next_revision(&self) -> Result<u64, Error> {
        self.revision.checked_add(1).ok_or(Error::RevisionExhausted)
    }
}
fn binding<T>(identity: T, strip: &Strip) -> Result<Binding<T>, Error> {
    let volume = strip.node.volume.as_ref().ok_or(Error::MissingVolume)?;
    if strip.bus.is_some() || strip.channels.len() == volume.len() {
        return Err(Error::WholeNodeOrBus);
    }
    if strip.channels.is_empty()
        || strip
            .channels
            .iter()
            .enumerate()
            .any(|(i, c)| *c >= volume.len() || strip.channels[..i].contains(c))
    {
        return Err(Error::InvalidChannels);
    }
    if volume.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(Error::InvalidGain);
    }
    Ok(Binding {
        identity,
        key: strip.key.clone(),
        node: strip.node.id,
        serial: strip.node.serial,
        ports: strip.ports.iter().map(|p| p.id).collect(),
        channels: strip.channels.clone(),
        vector_len: volume.len(),
    })
}

#[cfg(test)]
mod tests;
