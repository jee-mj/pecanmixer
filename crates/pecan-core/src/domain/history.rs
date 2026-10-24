//! Undoable audio intents from handelingen.ts / geschiedenis.ts, represented as
//! data instead of closures. Link IDs are re-resolved from port pairs on replay.
//! The caller supplies command acknowledgement stamps; no 250ms timer or I/O.
use super::{
    buses::BusConfig,
    mixer::{Mixer, RouteState},
};
use crate::{
    backend::{Stamp, Stamped},
    graph::{Momentopname, Opdracht, Soort},
};
use std::sync::Arc;

pub const HISTORY_LIMIT: usize = 200;
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Connect([u32; 2]),
    Disconnect(Vec<[u32; 2]>),
    Move {
        old: [u32; 2],
        new: [u32; 2],
    },
    Route {
        source: String,
        destination: String,
        enabled: bool,
    },
    BackToAuto,
    Volume {
        node: u32,
        values: Vec<f32>,
    },
    Mute {
        node: u32,
        muted: bool,
    },
    Batch(Vec<Action>),
    /// Captured target used by Back-to-auto undo; not a UI action by itself.
    Target {
        node: u32,
        target: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainError {
    Disconnected,
    Busy,
    UnknownTicket,
    InvalidAcknowledgement,
    NonFiniteVolume,
    InvalidBusConfig,
    Backend(String),
}

fn valid_pair(graph: &Momentopname, [from, to]: [u32; 2]) -> bool {
    graph
        .poorten
        .iter()
        .any(|p| p.id == from && p.soort == Soort::Audio && !p.ingang && !p.monitor)
        && graph
            .poorten
            .iter()
            .any(|p| p.id == to && p.soort == Soort::Audio && p.ingang && !p.monitor)
}
pub fn plan(
    action: &Action,
    graph: &Momentopname,
    buses: &BusConfig,
) -> Result<Vec<Opdracht>, DomainError> {
    if !graph.verbonden {
        return Err(DomainError::Disconnected);
    }
    let mut commands = Vec::new();
    match action {
        Action::Connect(pair) => {
            if valid_pair(graph, *pair)
                && !graph
                    .kabels
                    .iter()
                    .any(|l| [l.uit_poort, l.in_poort] == *pair)
            {
                commands.push(Opdracht::Verbind {
                    uit: pair[0],
                    naar: pair[1],
                })
            }
        }
        Action::Disconnect(pairs) => {
            for pair in pairs {
                commands.extend(
                    graph
                        .kabels
                        .iter()
                        .filter(|l| [l.uit_poort, l.in_poort] == *pair)
                        .map(|l| Opdracht::Ontkoppel { kabel: l.id }),
                );
            }
        }
        Action::Move { old, new } => {
            commands.extend(plan(&Action::Disconnect(vec![*old]), graph, buses)?);
            commands.extend(plan(&Action::Connect(*new), graph, buses)?);
        }
        Action::Route {
            source,
            destination,
            enabled,
        } => {
            let model = Mixer::build(
                graph,
                &buses
                    .definitions()
                    .map_err(|_| DomainError::InvalidBusConfig)?,
            );
            if let (Some(source), Some(destination)) =
                (model.source(source), model.destination(destination))
            {
                let route = model.route(graph, source, destination);
                if !matches!(route.state, RouteState::Custom { .. })
                    && matches!(route.state, RouteState::On { .. }) != *enabled
                {
                    commands = model.plan_toggle(graph, source, destination);
                }
            }
        }
        Action::BackToAuto => {
            commands = Mixer::build(
                graph,
                &buses
                    .definitions()
                    .map_err(|_| DomainError::InvalidBusConfig)?,
            )
            .plan_reset_auto()
        }
        Action::Volume { node, values } => {
            if values.iter().any(|v| !v.is_finite()) {
                return Err(DomainError::NonFiniteVolume);
            }
            if graph
                .knopen
                .iter()
                .any(|n| n.id == *node && n.volume.is_some())
            {
                commands.push(Opdracht::ZetVolume {
                    knoop: *node,
                    volume: values.clone(),
                })
            }
        }
        Action::Mute { node, muted } => {
            if graph.knopen.iter().any(|n| n.id == *node) {
                commands.push(Opdracht::ZetMute {
                    knoop: *node,
                    mute: *muted,
                })
            }
        }
        Action::Batch(actions) => {
            for action in actions {
                commands.extend(plan(action, graph, buses)?);
            }
        }
        Action::Target { node, target } => {
            if graph.knopen.iter().any(|n| n.id == *node) {
                commands.push(Opdracht::ZetDoel {
                    knoop: *node,
                    doel: target.clone(),
                })
            }
        }
    }
    Ok(commands)
}
fn inverse(
    action: &Action,
    graph: &Momentopname,
    buses: &BusConfig,
) -> Result<Action, DomainError> {
    Ok(match action {
        Action::Connect(pair) => Action::Disconnect(vec![*pair]),
        Action::Disconnect(pairs) => {
            Action::Batch(pairs.iter().map(|p| Action::Connect(*p)).collect())
        }
        Action::Move { old, new } => Action::Move {
            old: *new,
            new: *old,
        },
        Action::Route {
            source,
            destination,
            enabled,
        } => Action::Route {
            source: source.clone(),
            destination: destination.clone(),
            enabled: !*enabled,
        },
        Action::BackToAuto => {
            let model = Mixer::build(
                graph,
                &buses
                    .definitions()
                    .map_err(|_| DomainError::InvalidBusConfig)?,
            );
            let mut actions = vec![Action::Disconnect(model.manual.restore.clone())];
            actions.extend(
                model
                    .manual
                    .links
                    .iter()
                    .filter_map(|id| graph.kabels.iter().find(|l| l.id == *id))
                    .map(|l| Action::Connect([l.uit_poort, l.in_poort])),
            );
            // Targets are captured as data; unlike route intent replay these are
            // restored exactly, including an explicitly selected destination.
            actions.extend(
                model
                    .manual
                    .targets
                    .iter()
                    .filter_map(|id| graph.knopen.iter().find(|n| n.id == *id))
                    .map(|n| Action::Target {
                        node: n.id,
                        target: n.doel.clone(),
                    }),
            );
            Action::Batch(actions)
        }
        Action::Volume { node, .. } => Action::Volume {
            node: *node,
            values: graph
                .knopen
                .iter()
                .find(|n| n.id == *node)
                .and_then(|n| n.volume.clone())
                .unwrap_or_default(),
        },
        Action::Mute { node, muted } => Action::Mute {
            node: *node,
            muted: !*muted,
        },
        Action::Batch(actions) => Action::Batch(
            actions
                .iter()
                .rev()
                .map(|a| inverse(a, graph, buses))
                .collect::<Result<_, _>>()?,
        ),
        Action::Target { node, target: _ } => Action::Target {
            node: *node,
            target: graph
                .knopen
                .iter()
                .find(|n| n.id == *node)
                .and_then(|n| n.doel.clone()),
        },
    })
}

#[derive(Clone, Debug)]
struct Step {
    label: String,
    forward: Action,
    inverse: Action,
}
#[derive(Clone, Copy, Debug)]
enum Direction {
    Record,
    Undo,
    Redo,
}
#[derive(Clone, Debug)]
struct Pending {
    step: Step,
    direction: Direction,
    source: Stamp,
    ticket: u64,
    ack: Option<Stamp>,
    wait_for_graph: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandPlan {
    pub ticket: u64,
    pub epoch: u64,
    pub source_revision: u64,
    pub commands: Vec<Opdracht>,
}
pub enum Input {
    Graph(Stamped<Arc<Momentopname>>),
    Execute {
        label: String,
        action: Action,
    },
    /// A fader gesture already applied by the caller; capture only once.
    RecordVolume {
        node: u32,
        before: Vec<f32>,
        after: Vec<f32>,
    },
    Undo,
    Redo,
    /// A successful result must carry the backend revision at completion.
    /// History advances only after a graph at or after that stamp is observed.
    CommandResult {
        ticket: u64,
        result: Result<Stamp, String>,
    },
}
#[derive(Default)]
pub struct Controller {
    graph: Option<Stamped<Arc<Momentopname>>>,
    pub buses: BusConfig,
    undo: Vec<Step>,
    redo: Vec<Step>,
    pending: Option<Pending>,
    next_ticket: u64,
}
impl Controller {
    pub fn labels(&self) -> (Option<&str>, Option<&str>) {
        (
            self.undo.last().map(|s| s.label.as_str()),
            self.redo.last().map(|s| s.label.as_str()),
        )
    }
    pub fn lengths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn graph(&self) -> Option<&Stamped<Arc<Momentopname>>> {
        self.graph.as_ref()
    }
    pub fn reduce(&mut self, input: Input) -> Result<Option<CommandPlan>, DomainError> {
        match input {
            Input::Graph(graph) => {
                if self
                    .graph
                    .as_ref()
                    .is_some_and(|old| graph.stamp <= old.stamp)
                {
                    return Ok(None);
                }
                if !graph.value.verbonden
                    || self
                        .graph
                        .as_ref()
                        .is_some_and(|old| old.stamp.epoch != graph.stamp.epoch)
                {
                    self.undo.clear();
                    self.redo.clear();
                    self.pending = None;
                }
                self.graph = Some(graph);
                self.complete_if_observed();
                Ok(None)
            }
            Input::CommandResult { ticket, result } => {
                let pending = self
                    .pending
                    .as_ref()
                    .filter(|p| p.ticket == ticket)
                    .ok_or(DomainError::UnknownTicket)?;
                match result {
                    Err(error) => {
                        self.pending = None;
                        Err(DomainError::Backend(error))
                    }
                    Ok(stamp) => {
                        if stamp.epoch != pending.source.epoch
                            || stamp.revision < pending.source.revision
                        {
                            return Err(DomainError::InvalidAcknowledgement);
                        }
                        if let Some(pending) = self.pending.as_mut() {
                            pending.ack = Some(stamp)
                        }
                        self.complete_if_observed();
                        Ok(None)
                    }
                }
            }
            intent => {
                if self.pending.is_some() {
                    return Err(DomainError::Busy);
                }
                let graph = self
                    .graph
                    .as_ref()
                    .filter(|g| g.value.verbonden)
                    .ok_or(DomainError::Disconnected)?;
                if let Input::RecordVolume {
                    node,
                    before,
                    after,
                } = intent
                {
                    if before.iter().chain(&after).any(|v| !v.is_finite()) {
                        return Err(DomainError::NonFiniteVolume);
                    }
                    if before
                        .iter()
                        .enumerate()
                        .all(|(i, v)| (v - after.get(i).unwrap_or(v)).abs() < 1e-6)
                    {
                        return Ok(None);
                    }
                    self.record(Step {
                        label: "Volume".into(),
                        forward: Action::Volume {
                            node,
                            values: after,
                        },
                        inverse: Action::Volume {
                            node,
                            values: before,
                        },
                    });
                    return Ok(None);
                }
                let (step, direction) = match intent {
                    Input::Execute { label, action } => {
                        let inverse = inverse(&action, &graph.value, &self.buses)?;
                        (
                            Step {
                                label,
                                forward: action,
                                inverse,
                            },
                            Direction::Record,
                        )
                    }
                    Input::Undo => {
                        let Some(step) = self.undo.last().cloned() else {
                            return Ok(None);
                        };
                        (step, Direction::Undo)
                    }
                    Input::Redo => {
                        let Some(step) = self.redo.last().cloned() else {
                            return Ok(None);
                        };
                        (step, Direction::Redo)
                    }
                    _ => unreachable!("graph/results/record-volume handled above"),
                };
                let action = if matches!(direction, Direction::Undo) {
                    &step.inverse
                } else {
                    &step.forward
                };
                let commands = plan(action, &graph.value, &self.buses)?;
                self.next_ticket = self.next_ticket.wrapping_add(1);
                let ticket = self.next_ticket;
                let source = graph.stamp;
                self.pending = Some(Pending {
                    step,
                    direction,
                    source,
                    ticket,
                    ack: commands.is_empty().then_some(source),
                    wait_for_graph: !commands.is_empty(),
                });
                if commands.is_empty() {
                    self.complete_if_observed();
                    return Ok(None);
                }
                Ok(Some(CommandPlan {
                    ticket,
                    epoch: source.epoch,
                    source_revision: source.revision,
                    commands,
                }))
            }
        }
    }
    fn record(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    fn complete_if_observed(&mut self) {
        let ready = self.pending.as_ref().is_some_and(|p| {
            p.ack.is_some_and(|ack| {
                self.graph.as_ref().is_some_and(|g| {
                    g.value.verbonden
                        && g.stamp >= ack
                        && g.stamp.epoch == p.source.epoch
                        && (!p.wait_for_graph || g.stamp.revision > p.source.revision)
                })
            })
        });
        if !ready {
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending.direction {
            Direction::Record => self.record(pending.step),
            Direction::Undo => {
                self.undo.pop();
                self.redo.push(pending.step);
            }
            Direction::Redo => {
                self.redo.pop();
                self.undo.push(pending.step);
            }
        }
    }
}
