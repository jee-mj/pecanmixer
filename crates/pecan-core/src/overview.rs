//! Deterministic read-only graph projection used by the GTK preview and fixtures.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::backend::{Stamp, Stamped};
use crate::graph::{Momentopname, Soort};
use crate::inbox::InboxBatch;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    pub connected: bool,
    pub rate: Option<u32>,
    pub quantum: Option<u32>,
    pub nodes: Vec<NodeOverview>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeOverview {
    pub id: u32,
    pub label: String,
    pub name: String,
    pub class: Option<String>,
    pub ports: Vec<PortOverview>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortOverview {
    pub id: u32,
    pub label: String,
    pub input: bool,
    pub kind: Soort,
    pub channel: Option<String>,
}

pub fn project(graph: &Momentopname) -> Overview {
    if !graph.verbonden {
        return Overview::default();
    }
    let mut nodes: Vec<_> = graph
        .knopen
        .iter()
        .map(|node| {
            let mut ports: Vec<_> = graph
                .poorten
                .iter()
                .filter(|port| port.knoop == node.id)
                .collect();
            ports.sort_by_key(|port| (port.volgnummer.unwrap_or(u32::MAX), port.id));
            NodeOverview {
                id: node.id,
                label: node
                    .beschrijving
                    .as_ref()
                    .or(node.bijnaam.as_ref())
                    .or(node.app.as_ref())
                    .unwrap_or(&node.naam)
                    .clone(),
                name: node.naam.clone(),
                class: node.klasse.clone(),
                ports: ports
                    .into_iter()
                    .map(|port| PortOverview {
                        id: port.id,
                        label: port.alias.as_ref().unwrap_or(&port.naam).clone(),
                        input: port.ingang,
                        kind: port.soort,
                        channel: port.kanaal.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    nodes.sort_by(|a, b| {
        (a.label.to_lowercase(), &a.name, a.id).cmp(&(b.label.to_lowercase(), &b.name, b.id))
    });
    Overview {
        connected: true,
        rate: graph.rate,
        quantum: graph.quantum,
        nodes,
    }
}

#[derive(Default)]
pub struct OverviewState {
    graph: Option<Stamped<Arc<Momentopname>>>,
}

impl OverviewState {
    /// Returns true only when the structural read-only view changed. Meter and
    /// transport events cannot force the node/port widgets to rebuild.
    pub fn apply(&mut self, batch: InboxBatch) -> bool {
        let Some(graph) = batch.graph else {
            return false;
        };
        if self
            .graph
            .as_ref()
            .is_some_and(|old| graph.stamp <= old.stamp)
        {
            return false;
        }
        let changed = self.overview() != project(&graph.value);
        self.graph = Some(graph);
        changed
    }

    pub fn stamp(&self) -> Option<Stamp> {
        self.graph.as_ref().map(|graph| graph.stamp)
    }

    pub fn overview(&self) -> Overview {
        self.graph
            .as_ref()
            .map_or_else(Overview::default, |graph| project(&graph.value))
    }
}
