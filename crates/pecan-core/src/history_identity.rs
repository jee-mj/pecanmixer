//! Registry-generation witnesses for history. This is owned native metadata,
//! not a serialized graph DTO. Only a backend registry can supply generations;
//! comparing visible port properties cannot detect a coalesced replacement.
use crate::{
    backend::Stamped,
    domain::{
        buses::BusConfig,
        history::{plan, Action},
        mixer::Mixer,
    },
    graph::{Momentopname, Opdracht},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// An exact native graph publication and its backend's registry generations.
/// Producers must reuse `authority` for one backend owner, assign a fresh
/// generation on every registry addition, and never infer it from DTO equality.
/// The production accessor validates the publication stamp AND owned graph Arc.
#[derive(Clone, Debug)]
pub struct HistoryGraph {
    graph: Stamped<Arc<Momentopname>>,
    authority: Arc<()>,
    generations: BTreeMap<u32, u64>,
}
impl HistoryGraph {
    pub fn from_registry(
        graph: Stamped<Arc<Momentopname>>,
        authority: Arc<()>,
        generations: BTreeMap<u32, u64>,
    ) -> Self {
        Self {
            graph,
            authority,
            generations,
        }
    }
    pub fn graph(&self) -> &Stamped<Arc<Momentopname>> {
        &self.graph
    }
    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authority, &other.authority)
    }
}

/// A step retains only its dependencies, not a graph or a bounded origin-cache
/// entry. Link generations deliberately do not belong here: replay resolves the
/// current link ID from the original, still-identical endpoint pair.
#[derive(Clone, Debug)]
pub(crate) struct Guard {
    authority: Arc<()>,
    epoch: u64,
    generations: BTreeMap<u32, u64>,
    targets: BTreeMap<String, BTreeSet<u32>>,
    routes: RouteBindings,
}
impl Guard {
    pub(crate) fn capture(
        frame: &HistoryGraph,
        actions: [&Action; 2],
        buses: &BusConfig,
    ) -> Option<Self> {
        let ids = dependencies(actions, &frame.graph.value, buses)?;
        let generations = ids
            .into_iter()
            .map(|id| Some((id, *frame.generations.get(&id)?)))
            .collect::<Option<_>>()?;
        Some(Self {
            authority: frame.authority.clone(),
            epoch: frame.graph.stamp.epoch,
            generations,
            targets: target_bindings(actions, &frame.graph.value),
            routes: route_bindings(actions, &frame.graph.value, buses)?,
        })
    }
    pub(crate) fn valid(
        &self,
        frame: &HistoryGraph,
        actions: [&Action; 2],
        buses: &BusConfig,
    ) -> bool {
        self.epoch == frame.graph.stamp.epoch
            && Arc::ptr_eq(&self.authority, &frame.authority)
            && self
                .generations
                .iter()
                .all(|(id, generation)| frame.generations.get(id) == Some(generation))
            && dependencies(actions, &frame.graph.value, buses)
                .is_some_and(|ids| ids.iter().all(|id| self.generations.contains_key(id)))
            && self.targets == target_bindings(actions, &frame.graph.value)
            && route_bindings(actions, &frame.graph.value, buses).as_ref() == Some(&self.routes)
    }
}

type RouteBindings = BTreeMap<(bool, String), (u32, Vec<u32>, Vec<usize>)>;
fn route_bindings(
    actions: [&Action; 2],
    graph: &Momentopname,
    buses: &BusConfig,
) -> Option<RouteBindings> {
    fn collect(
        action: &Action,
        graph: &Momentopname,
        buses: &BusConfig,
        bindings: &mut RouteBindings,
    ) -> Option<()> {
        match action {
            Action::Route {
                source,
                destination,
                ..
            } => {
                let model = Mixer::build(graph, &buses.definitions().ok()?);
                for (input, key, strip) in [
                    (false, source, model.source(source)?),
                    (true, destination, model.destination(destination)?),
                ] {
                    bindings.insert(
                        (input, key.clone()),
                        (
                            strip.node.id,
                            strip.ports.iter().map(|p| p.id).collect(),
                            strip.channels.clone(),
                        ),
                    );
                }
            }
            Action::Batch(actions) => {
                for action in actions {
                    collect(action, graph, buses, bindings)?;
                }
            }
            _ => {}
        }
        Some(())
    }
    let mut bindings = BTreeMap::new();
    for action in actions {
        collect(action, graph, buses, &mut bindings)?;
    }
    Some(bindings)
}

// A generation union alone is insufficient for string selectors in a batch:
// an originally unresolved target could later match another existing dependency.
fn target_bindings(actions: [&Action; 2], graph: &Momentopname) -> BTreeMap<String, BTreeSet<u32>> {
    fn collect(
        action: &Action,
        graph: &Momentopname,
        bindings: &mut BTreeMap<String, BTreeSet<u32>>,
    ) {
        match action {
            Action::Target {
                target: Some(target),
                ..
            } => {
                bindings.insert(
                    target.clone(),
                    graph
                        .knopen
                        .iter()
                        .filter(|n| {
                            n.naam == *target || n.serial.is_some_and(|s| s.to_string() == *target)
                        })
                        .map(|n| n.id)
                        .collect(),
                );
            }
            Action::Batch(actions) => {
                for action in actions {
                    collect(action, graph, bindings);
                }
            }
            _ => {}
        }
    }
    let mut bindings = BTreeMap::new();
    for action in actions {
        collect(action, graph, &mut bindings);
    }
    bindings
}

fn dependencies(
    actions: [&Action; 2],
    graph: &Momentopname,
    buses: &BusConfig,
) -> Option<BTreeSet<u32>> {
    let mut ids = BTreeSet::new();
    for action in actions {
        action_dependencies(action, graph, buses, &mut ids)?;
        // Route/reset planners can touch additional source/destination objects.
        // A later replay may not silently expand its original dependency set.
        for command in plan(action, graph, buses).ok()? {
            match command {
                Opdracht::Verbind { uit, naar } => pair([uit, naar], graph, &mut ids)?,
                Opdracht::Ontkoppel { kabel } => {
                    let link = graph.kabels.iter().find(|l| l.id == kabel)?;
                    pair([link.uit_poort, link.in_poort], graph, &mut ids)?;
                }
                Opdracht::ZetDoel { knoop, doel } => target(knoop, &doel, graph, &mut ids)?,
                Opdracht::ZetVolume { knoop, .. } | Opdracht::ZetMute { knoop, .. } => {
                    node(knoop, graph, &mut ids)?
                }
                _ => return None,
            }
        }
    }
    Some(ids)
}
fn node(id: u32, graph: &Momentopname, ids: &mut BTreeSet<u32>) -> Option<()> {
    graph.knopen.iter().find(|n| n.id == id)?;
    ids.insert(id);
    Some(())
}
fn pair(ports: [u32; 2], graph: &Momentopname, ids: &mut BTreeSet<u32>) -> Option<()> {
    for id in ports {
        let port = graph.poorten.iter().find(|p| p.id == id)?;
        node(port.knoop, graph, ids)?;
        ids.insert(id);
    }
    Some(())
}
fn target(
    id: u32,
    target: &Option<String>,
    graph: &Momentopname,
    ids: &mut BTreeSet<u32>,
) -> Option<()> {
    node(id, graph, ids)?;
    if let Some(target) = target {
        ids.extend(
            graph
                .knopen
                .iter()
                .filter(|n| n.naam == *target || n.serial.is_some_and(|s| s.to_string() == *target))
                .map(|n| n.id),
        );
    }
    Some(())
}
fn action_dependencies(
    action: &Action,
    graph: &Momentopname,
    buses: &BusConfig,
    ids: &mut BTreeSet<u32>,
) -> Option<()> {
    match action {
        Action::Connect(ports) => pair(*ports, graph, ids)?,
        Action::Disconnect(pairs) => {
            for ports in pairs {
                pair(*ports, graph, ids)?;
            }
        }
        Action::Move { old, new } => {
            pair(*old, graph, ids)?;
            pair(*new, graph, ids)?;
        }
        Action::Volume { node: id, .. } | Action::Mute { node: id, .. } => node(*id, graph, ids)?,
        Action::Target {
            node: id,
            target: value,
        } => target(*id, value, graph, ids)?,
        Action::Batch(actions) => {
            for action in actions {
                action_dependencies(action, graph, buses, ids)?;
            }
        }
        Action::Route {
            source,
            destination,
            ..
        } => {
            let model = Mixer::build(graph, &buses.definitions().ok()?);
            for strip in [model.source(source)?, model.destination(destination)?] {
                node(strip.node.id, graph, ids)?;
                for port in &strip.ports {
                    ids.insert(port.id);
                }
            }
        }
        Action::BackToAuto => {} // Concrete planner dependencies are added above.
    }
    Some(())
}
