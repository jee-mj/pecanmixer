//! Pure recording-bus UI model. Core owns configuration/grouping/migration;
//! the application owns authenticated snapshots, receipts and recording identity.
//! See `recording_buses/README.md` for the pending adapter contract.
use pecan_core::domain::{
    buses::{BusConfig, BusDef, BusError, BusState, Coupling},
    mixer::{Mixer, Strip},
};
use std::{rc::Rc, sync::Arc, time::Duration};

pub(crate) mod input;
pub mod levels;
mod view;
pub use view::{BusControls, Destination, Event, Handoff, ViewError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dependency {
    IdleConfigurationAdapter,
    RoutedLayoutMigration,
    ChannelShrinkMigration,
    ActiveLayoutMigration,
    DynamicArming,
    RecordingReceipts,
    IdleLevels,
    ActiveLevels,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Support {
    Available,
    Pending(Dependency),
    Unavailable(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub idle_configuration: Support,
    /// Same-layout levels, independent of routing/structural capabilities.
    pub idle_levels: Support,
    pub active_levels: Support,
    pub routed_layout: Support,
    pub routed_add: Support,
    pub shrink_layout: Support,
    pub active_layout: Support,
    pub active_arming: Support,
}
impl Default for Capabilities {
    fn default() -> Self {
        Self {
            idle_configuration: Support::Pending(Dependency::IdleConfigurationAdapter),
            idle_levels: Support::Pending(Dependency::IdleLevels),
            active_levels: Support::Pending(Dependency::ActiveLevels),
            routed_layout: Support::Pending(Dependency::RoutedLayoutMigration),
            routed_add: Support::Pending(Dependency::RoutedLayoutMigration),
            shrink_layout: Support::Pending(Dependency::ChannelShrinkMigration),
            active_layout: Support::Pending(Dependency::ActiveLayoutMigration),
            active_arming: Support::Pending(Dependency::DynamicArming),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Unknown,
    Idle,
    Recording,
    Finalizing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackPhase {
    Starting,
    /// Writer readiness alone is not proof that capture started.
    WriterReady,
    Recording,
    Finalizing,
    /// Explicit finalization result, with thread joining still unproven.
    Finalized(Result<(), String>),
    NotOpened(String),
    Unconfirmed,
    Joined(Result<(), String>),
}
/// Adapter-projected status for one stable take/track identity. Multiple tracks
/// may share a bus while an older writer finalizes. Never infer identity from
/// a filename or Joined from an absent legacy transport entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track<T> {
    pub identity: T,
    pub bus_key: String,
    pub phase: TrackPhase,
    pub elapsed: Option<Duration>,
}
/// Presentation phases, not another backend receipt protocol. The adapter maps
/// exact ConfigProgress/LocalConfigOutcome values; local results are not graph
/// publication, writer readiness or joined recording completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    Queued,
    Applying,
    AwaitingObservation,
    Observed,
    AppliedLocally,
    NoOpLocally,
    Rejected(String),
    PartiallyApplied(String),
    Unconfirmed(String),
}
impl Progress {
    pub fn message(&self) -> String {
        match self {
            Self::Queued => "Configuration queued; not applied".into(),
            Self::Applying => "Applying local configuration; result pending".into(),
            Self::AwaitingObservation => {
                "Waiting for authenticated migration graph/configuration publication".into()
            }
            Self::Observed => {
                "Bus migration observed in the displayed graph and configuration".into()
            }
            Self::AppliedLocally => "Configuration applied locally".into(),
            Self::NoOpLocally => "Local configuration already matched".into(),
            Self::Rejected(error) => format!("Configuration rejected: {error}"),
            Self::PartiallyApplied(error) => {
                format!("Configuration partially applied; reconcile before retry: {error}")
            }
            Self::Unconfirmed(error) => format!("Configuration outcome unconfirmed: {error}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation<R> {
    pub request: R,
    pub progress: Progress,
}
pub struct Snapshot<O, R, T> {
    pub origin: O,
    pub connected: bool,
    pub closing: bool,
    /// None means logical metadata is unknown, not a default configuration.
    pub configuration: Option<BusConfig>,
    /// Full core projection, independent of hidden strips/display preferences.
    pub mixer: Arc<Mixer>,
    pub default_sink: Option<String>,
    pub has_bus_routes: bool,
    pub activity: Activity,
    pub tracks: Vec<Track<T>>,
    pub operation: Option<Operation<R>>,
    pub capabilities: Capabilities,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    AddStereo,
    Split { bus: String },
    Join { bus: String },
    Remove { bus: String },
    Arm { bus: String, additive: bool },
}
pub struct Binding<O> {
    authority: Rc<()>,
    context: Rc<()>,
    origin: O,
}
/// An owner request, never admission or applied state. Remove deliberately has
/// no UI-built migration/configuration: the owner must use core BusConfig::remove
/// with its exact graph and a reviewed staged migration executor.
pub struct Intent<O> {
    pub origin: O,
    pub action: Action,
    pub proposed_configuration: Option<BusConfig>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Disconnected,
    Closing,
    UnknownConfiguration,
    UnknownActivity,
    ReconciliationRequired,
    MissingBus,
    LastBus,
    WrongCoupling,
    RowBudget,
    Adapter(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    PendingOperation,
    PendingCapability(Dependency),
    Unavailable(Unavailable),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NoSnapshot,
    StaleBinding,
    MissingDestination,
    Domain(BusError),
    BudgetExceeded,
    Disabled(Availability),
}
#[derive(Clone, Copy)]
pub struct Budget {
    pub buses: usize,
    pub tracks: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            buses: 64,
            tracks: 128,
        }
    }
}
pub struct Row<T> {
    pub definition: BusDef,
    pub state: BusState,
    /// None keeps locally configured buses distinct from graph-observed ports.
    pub destination_key: Option<String>,
    pub tracks: Vec<Track<T>>,
    pub arm: Availability,
    pub coupling: Option<(Coupling, Availability)>,
    pub remove: Availability,
}
pub struct Model<O, R, T> {
    authority: Rc<()>,
    context: Rc<()>,
    budget: Budget,
    snapshot: Option<Snapshot<O, R, T>>,
    publication_error: Option<Error>,
    output_choice: Option<String>,
    rec_choice: Option<String>,
}
impl<O: Clone, R, T: Clone> Model<O, R, T> {
    pub fn new(budget: Budget) -> Self {
        Self {
            authority: Rc::new(()),
            context: Rc::new(()),
            budget,
            snapshot: None,
            publication_error: None,
            output_choice: None,
            rec_choice: None,
        }
    }
    /// Only authenticated, ordered owner publications belong here. In particular,
    /// a busy config try-read is not a new disconnected/default snapshot. Every
    /// accepted publication retires old UI bindings; it invents no backend epoch.
    pub fn observe(&mut self, snapshot: Snapshot<O, R, T>) -> Result<(), Error> {
        if snapshot
            .configuration
            .as_ref()
            .is_some_and(|c| c.widths.len() > self.budget.buses)
            || snapshot.tracks.len() > self.budget.tracks
        {
            return self.reject_publication(Error::BudgetExceeded);
        }
        let definitions = snapshot
            .configuration
            .as_ref()
            .map(|c| c.definitions())
            .transpose();
        if let Err(error) = definitions {
            return self.reject_publication(Error::Domain(error));
        }
        self.context = Rc::new(());
        self.publication_error = None;
        self.snapshot = Some(snapshot);
        Ok(())
    }
    fn reject_publication(&mut self, error: Error) -> Result<(), Error> {
        self.context = Rc::new(());
        self.snapshot = None;
        self.publication_error = Some(error.clone());
        Err(error)
    }
    pub fn publication_error(&self) -> Option<&Error> {
        self.publication_error.as_ref()
    }
    pub fn snapshot(&self) -> Option<&Snapshot<O, R, T>> {
        self.snapshot.as_ref()
    }
    pub fn bind(&self) -> Result<Binding<O>, Error> {
        Ok(Binding {
            authority: self.authority.clone(),
            context: self.context.clone(),
            origin: self
                .snapshot
                .as_ref()
                .ok_or(Error::NoSnapshot)?
                .origin
                .clone(),
        })
    }
    fn validate(&self, binding: &Binding<O>) -> Result<&Snapshot<O, R, T>, Error> {
        if !Rc::ptr_eq(&binding.authority, &self.authority)
            || !Rc::ptr_eq(&binding.context, &self.context)
        {
            return Err(Error::StaleBinding);
        }
        self.snapshot.as_ref().ok_or(Error::NoSnapshot)
    }
    pub fn select_destination(&mut self, binding: &Binding<O>, key: &str) -> Result<(), Error> {
        let snapshot = self.validate(binding)?;
        if !snapshot.connected {
            return Err(Error::Disabled(Availability::Unavailable(
                Unavailable::Disconnected,
            )));
        }
        let destination = snapshot
            .mixer
            .destination(key)
            .ok_or(Error::MissingDestination)?;
        if destination.bus.is_some() && snapshot.configuration.is_none() {
            return Err(Error::Disabled(Availability::Unavailable(
                Unavailable::UnknownConfiguration,
            )));
        }
        if let Some(bus) = &destination.bus {
            if !snapshot
                .configuration
                .as_ref()
                .unwrap()
                .definitions()
                .map_err(Error::Domain)?
                .iter()
                .any(|d| d.key == bus.key)
            {
                return Err(Error::MissingDestination);
            }
        }
        if destination.bus.is_some() {
            self.rec_choice = Some(key.into());
        } else {
            self.output_choice = Some(key.into());
        }
        Ok(())
    }
    pub fn selected_output(&self) -> Option<&Strip> {
        let s = self.snapshot.as_ref().filter(|s| s.connected)?;
        let outputs = || s.mixer.destinations.iter().filter(|d| d.bus.is_none());
        outputs()
            .find(|d| Some(d.key.as_str()) == self.output_choice.as_deref())
            .or_else(|| {
                outputs().find(|d| {
                    Some(d.node.naam.as_str()) == s.default_sink.as_deref() && d.targetable
                })
            })
            .or_else(|| outputs().next())
    }
    pub fn selected_rec(&self) -> Option<&Strip> {
        let s = self.snapshot.as_ref().filter(|s| s.connected)?;
        let definitions = s.configuration.as_ref()?.definitions().ok()?;
        let buses = || {
            s.mixer.destinations.iter().filter(|d| {
                d.bus
                    .as_ref()
                    .is_some_and(|b| definitions.iter().any(|d| d.key == b.key))
            })
        };
        buses()
            .find(|d| Some(d.key.as_str()) == self.rec_choice.as_deref())
            .or_else(|| buses().next())
    }
    pub fn availability(&self, action: &Action) -> Result<Availability, Error> {
        use Availability::{Available, PendingOperation, Unavailable as Off};
        let s = self.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        if !s.connected {
            return Ok(Off(Unavailable::Disconnected));
        }
        if s.closing {
            return Ok(Off(Unavailable::Closing));
        }
        if let Some(operation) = &s.operation {
            match operation.progress {
                Progress::Queued | Progress::Applying | Progress::AwaitingObservation => {
                    return Ok(PendingOperation)
                }
                Progress::PartiallyApplied(_) | Progress::Unconfirmed(_) => {
                    return Ok(Off(Unavailable::ReconciliationRequired))
                }
                _ => {}
            }
        }
        let Some(config) = &s.configuration else {
            return Ok(Off(Unavailable::UnknownConfiguration));
        };
        let definitions = config.definitions().map_err(Error::Domain)?;
        if let Some(key) = action_key(action) {
            let Some(bus) = definitions.iter().find(|d| d.key == key) else {
                return Ok(Off(Unavailable::MissingBus));
            };
            match action {
                Action::Split { .. } if bus.coupling != Some(Coupling::Split) => {
                    return Ok(Off(Unavailable::WrongCoupling))
                }
                Action::Join { .. } if bus.coupling != Some(Coupling::Join) => {
                    return Ok(Off(Unavailable::WrongCoupling))
                }
                Action::Remove { .. } if definitions.len() == 1 => {
                    return Ok(Off(Unavailable::LastBus))
                }
                _ => {}
            }
        }
        if matches!(action, Action::AddStereo | Action::Split { .. })
            && definitions.len() >= self.budget.buses
        {
            return Ok(Off(Unavailable::RowBudget));
        }
        let base = support(&s.capabilities.idle_configuration);
        if base != Available {
            return Ok(base);
        }
        match s.activity {
            Activity::Unknown => return Ok(Off(Unavailable::UnknownActivity)),
            Activity::Recording | Activity::Finalizing => {
                let active = support(if matches!(action, Action::Arm { .. }) {
                    &s.capabilities.active_arming
                } else {
                    &s.capabilities.active_layout
                });
                if active != Available {
                    return Ok(active);
                }
            }
            Activity::Idle => {}
        }
        if matches!(action, Action::Remove { .. }) {
            let shrink = support(&s.capabilities.shrink_layout);
            if shrink != Available {
                return Ok(shrink);
            }
        }
        if !matches!(action, Action::Arm { .. }) && s.has_bus_routes {
            return Ok(support(if matches!(action, Action::AddStereo) {
                &s.capabilities.routed_add
            } else {
                &s.capabilities.routed_layout
            }));
        }
        Ok(Available)
    }
    pub fn request(&self, binding: &Binding<O>, action: Action) -> Result<Intent<O>, Error> {
        let s = self.validate(binding)?;
        let availability = self.availability(&action)?;
        if availability != Availability::Available {
            return Err(Error::Disabled(availability));
        }
        let config = s.configuration.as_ref().ok_or(Error::NoSnapshot)?;
        let proposed = match &action {
            Action::AddStereo => Some(config.add().map_err(Error::Domain)?),
            Action::Split { bus } | Action::Join { bus } => {
                let definition = config
                    .definitions()
                    .map_err(Error::Domain)?
                    .into_iter()
                    .find(|d| &d.key == bus)
                    .ok_or(Error::Domain(BusError::InvalidIndex))?;
                Some(
                    config
                        .change_coupling(definition.index)
                        .map_err(Error::Domain)?,
                )
            }
            Action::Arm { bus, additive } => {
                Some(config.arm(bus, *additive).map_err(Error::Domain)?)
            }
            Action::Remove { .. } => None,
        };
        Ok(Intent {
            origin: binding.origin.clone(),
            action,
            proposed_configuration: proposed,
        })
    }
    pub fn rows(&self) -> Result<Vec<Row<T>>, Error> {
        let s = self.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        let Some(config) = &s.configuration else {
            return Ok(vec![]);
        };
        config
            .definitions()
            .map_err(Error::Domain)?
            .into_iter()
            .map(|d| {
                let coupling = d
                    .coupling
                    .map(|c| {
                        let action = match c {
                            Coupling::Split => Action::Split { bus: d.key.clone() },
                            Coupling::Join => Action::Join { bus: d.key.clone() },
                        };
                        self.availability(&action).map(|a| (c, a))
                    })
                    .transpose()?;
                Ok(Row {
                    state: config.state(&d.key),
                    destination_key: s
                        .connected
                        .then(|| {
                            s.mixer
                                .destinations
                                .iter()
                                .find(|strip| strip.bus.as_ref().is_some_and(|b| b.key == d.key))
                                .map(|s| s.key.clone())
                        })
                        .flatten(),
                    tracks: s
                        .tracks
                        .iter()
                        .filter(|t| t.bus_key == d.key)
                        .cloned()
                        .collect(),
                    arm: self.availability(&Action::Arm {
                        bus: d.key.clone(),
                        additive: false,
                    })?,
                    remove: self.availability(&Action::Remove { bus: d.key.clone() })?,
                    coupling,
                    definition: d,
                })
            })
            .collect()
    }
    pub fn unmatched_tracks(&self) -> Result<Vec<Track<T>>, Error> {
        let s = self.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        let definitions = s
            .configuration
            .as_ref()
            .map(|c| c.definitions())
            .transpose()
            .map_err(Error::Domain)?
            .unwrap_or_default();
        Ok(s.tracks
            .iter()
            .filter(|t| !definitions.iter().any(|d| d.key == t.bus_key))
            .cloned()
            .collect())
    }
    pub fn recording_control(&self) -> Availability {
        Availability::PendingCapability(Dependency::RecordingReceipts)
    }
}
fn support(support: &Support) -> Availability {
    match support {
        Support::Available => Availability::Available,
        Support::Pending(d) => Availability::PendingCapability(*d),
        Support::Unavailable(message) => {
            Availability::Unavailable(Unavailable::Adapter(message.clone()))
        }
    }
}
fn action_key(action: &Action) -> Option<&str> {
    match action {
        Action::AddStereo => None,
        Action::Split { bus }
        | Action::Join { bus }
        | Action::Remove { bus }
        | Action::Arm { bus, .. } => Some(bus),
    }
}

#[cfg(test)]
mod tests;
