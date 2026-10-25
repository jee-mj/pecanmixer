//! Typed presentation proposals. The existing owner owns receipts, tickets,
//! coalescing and identity; these values are neither native commands nor Undo.
use super::*;
use crate::controls::fader;

#[derive(Clone)]
pub struct Capture<O> {
    pub origin: O,
    pub bus: BusDef,
    pub configuration: BusConfig,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Db(f64),
    Mute(bool),
}
pub struct Intent<O> {
    pub capture: Capture<O>,
    pub change: Change,
    pub proposed_configuration: BusConfig,
}
/// Begin/Set return Tracked(owner_request). Update/End/Abandon return Accepted.
/// The same request spans a gesture and all its samples. The owner publishes it
/// as pending until End/Abandon and actual reconciliation, not each sample ack.
pub enum Event<O, R> {
    Begin {
        capture: Capture<O>,
        kind: fader::Kind,
    },
    Update {
        request: R,
        db: f64,
    },
    End {
        request: R,
        db: f64,
        reason: fader::End,
    },
    /// Local input invalidation, never rollback or receipt cancellation evidence.
    Abandon {
        request: R,
    },
    Set(Intent<O>),
}
pub enum Reply<R> {
    Tracked(R),
    Accepted,
}
impl<O: Clone, R, T: Clone> Model<O, R, T> {
    pub fn level_availability(&self, bus: &str) -> Result<Availability, Error> {
        use Availability::{Available, PendingCapability, PendingOperation, Unavailable as Off};
        let s = self.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        if !s.connected {
            return Ok(Off(Unavailable::Disconnected));
        }
        if s.closing {
            return Ok(Off(Unavailable::Closing));
        }
        if let Some(op) = &s.operation {
            match op.progress {
                Progress::Queued | Progress::Applying | Progress::AwaitingObservation => {
                    return Ok(PendingOperation)
                }
                Progress::PartiallyApplied(_) | Progress::Unconfirmed(_) => {
                    return Ok(Off(Unavailable::ReconciliationRequired))
                }
                _ => {}
            }
        }
        let Some(config) = s.configuration.as_ref() else {
            return Ok(Off(Unavailable::UnknownConfiguration));
        };
        if config.states.len() > 128 || config.states.keys().any(|k| k.len() > 256) {
            return Err(Error::BudgetExceeded);
        }
        if !config
            .definitions()
            .map_err(Error::Domain)?
            .iter()
            .any(|d| d.key == bus)
        {
            return Ok(Off(Unavailable::MissingBus));
        }
        // No active gain backend adapter has been reviewed. Layout/arming
        // support cannot accidentally enable this independent capability.
        match s.activity {
            Activity::Unknown => return Ok(Off(Unavailable::UnknownActivity)),
            Activity::Recording | Activity::Finalizing => {
                return Ok(PendingCapability(Dependency::ActiveLevels))
            }
            Activity::Idle => {}
        }
        let state = config.state(bus);
        if !state.db.is_finite() {
            return Err(Error::Domain(BusError::NonFiniteGain));
        }
        match &s.capabilities.idle_levels {
            Support::Available => Ok(Available),
            Support::Pending(d) => Ok(PendingCapability(*d)),
            Support::Unavailable(e) => Ok(Off(Unavailable::Adapter(e.clone()))),
        }
    }
    pub fn capture_level(&self, binding: &Binding<O>, bus: &str) -> Result<Capture<O>, Error> {
        let s = self.validate(binding)?;
        let availability = self.level_availability(bus)?;
        if availability != Availability::Available {
            return Err(Error::Disabled(availability));
        }
        let configuration = s.configuration.as_ref().ok_or(Error::NoSnapshot)?.clone();
        let definition = configuration
            .definitions()
            .map_err(Error::Domain)?
            .into_iter()
            .find(|d| d.key == bus)
            .ok_or(Error::Domain(BusError::InvalidIndex))?;
        Ok(Capture {
            origin: binding.origin.clone(),
            bus: definition,
            configuration,
        })
    }
}
impl<O> Capture<O> {
    pub fn propose(self, change: Change) -> Result<Intent<O>, BusError> {
        let proposed_configuration = match change {
            Change::Db(db) => self.configuration.with_db(&self.bus.key, db)?,
            Change::Mute(mute) => self.configuration.with_mute(&self.bus.key, mute)?,
        };
        Ok(Intent {
            capture: self,
            change,
            proposed_configuration,
        })
    }
}
