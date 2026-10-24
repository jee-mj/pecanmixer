//! Main-context adoption of the reviewed accepted-batch presentation boundary.
//! The owning workspace passes its one Session drain; this module starts none.
use super::Session;
use crate::meters::{MeterKey, MeterSources, MeterSurface, TelemetryAdapter, TelemetryReport};
use gtk::prelude::*;
use pecan_core::{
    domain::audio::{self, Side},
    graph::Soort,
    history_identity::RegistryBinding,
    inbox::InboxBatch,
};
use std::collections::{BTreeMap, BTreeSet};

/// Registry identity plus the relevant DTO channel mapping, never numeric IDs
/// alone. RegistryBinding retains authenticated owner/epoch/object generations.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    registry: RegistryBinding,
    ports: Vec<(u32, bool, Option<u32>, Option<String>)>,
}
pub(crate) struct Telemetry {
    pub(crate) adapter: TelemetryAdapter<Identity>,
    modules: BTreeMap<String, (u32, bool)>,
    pub(crate) report: TelemetryReport,
}
impl Default for Telemetry {
    fn default() -> Self {
        Self {
            adapter: TelemetryAdapter::default(),
            modules: BTreeMap::new(),
            report: Default::default(),
        }
    }
}
impl Telemetry {
    pub(crate) fn apply(&mut self, session: &Session, batch: &InboxBatch) {
        if batch.graph.is_none() {
            // Authority was bound at the accepted graph. Peak/receipt-only
            // ticks do not rescan the graph, rebuild sources or reset samples.
            self.report = self.adapter.apply_batch(batch, None);
            return;
        }
        let frame = session.witnessed_graph();
        let graph = session.graph();
        let sources = graph.as_ref().zip(frame.as_ref()).filter(|(graph, frame)| {
            graph.stamp == frame.graph().stamp
                && std::sync::Arc::ptr_eq(&graph.value, &frame.graph().value)
        });
        let mut bindings = BTreeMap::new();
        if let Some((graph, frame)) = sources {
            for node in &graph.value.knopen {
                let mut ports: Vec<_> = graph
                    .value
                    .poorten
                    .iter()
                    .filter(|p| p.knoop == node.id && p.soort == Soort::Audio && !p.monitor)
                    .map(|p| (p.id, p.ingang, p.volgnummer, p.kanaal.clone()))
                    .collect();
                ports.sort();
                if let Some(registry) =
                    frame.bind_objects(std::iter::once(node.id).chain(ports.iter().map(|p| p.0)))
                {
                    bindings.insert(node.id, Identity { registry, ports });
                }
            }
        }
        let buses = super::default_idle_configuration()
            .buses
            .definitions()
            .expect("validated native defaults");
        self.report = self.adapter.apply_batch(
            batch,
            sources.map(|(graph, _)| MeterSources {
                graph,
                node_bindings: &bindings,
                buses: &buses,
            }),
        );
        if self.report.graph_accepted {
            self.modules.clear();
            if let Some(graph) = &batch.graph {
                for module in audio::layout(&graph.value)
                    .groups
                    .into_iter()
                    .flat_map(|g| g.modules)
                {
                    self.modules
                        .insert(module.key, (module.node, module.side == Side::Right));
                }
            }
        }
    }
    pub(crate) fn mixer_widgets(&self) -> Vec<(String, gtk::Box)> {
        self.adapter
            .meters()
            .filter(|(s, _)| s.key.surface == MeterSurface::Mixer)
            .map(|(s, w)| (s.key.item.clone(), w.widget().clone()))
            .collect()
    }
    pub(crate) fn patch_widgets(&self) -> Vec<(u32, bool, gtk::Box)> {
        self.adapter
            .meters()
            .filter(|(s, _)| s.key.surface == MeterSurface::Patchbay)
            .filter_map(|(s, w)| {
                self.modules
                    .get(&s.key.item)
                    .map(|(node, input)| (*node, *input, w.widget().clone()))
            })
            .collect()
    }
    pub(crate) fn visibility(&mut self, mixer: &gtk::Widget, patch: &gtk::Widget) {
        for (surface, root) in [
            (MeterSurface::Mixer, mixer),
            (MeterSurface::Patchbay, patch),
        ] {
            self.adapter.set_surface_active(surface, root.is_mapped());
            let visible: BTreeSet<MeterKey> = self
                .adapter
                .meters()
                .filter(|(s, w)| s.key.surface == surface && on_screen(w.widget()))
                .map(|(s, _)| s.key.clone())
                .collect();
            self.adapter.set_visible_keys(surface, &visible);
        }
    }
}
/// Retain/reparent the same MeterWidget wrappers across structural projection.
pub(crate) fn mount<'a>(slot: &gtk::Box, widgets: impl Iterator<Item = &'a gtk::Box>) {
    let widgets: Vec<_> = widgets.collect();
    let mut child = slot.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if !widgets
            .iter()
            .any(|w| w.upcast_ref::<gtk::Widget>() == &current)
        {
            slot.remove(&current);
        }
    }
    let mut previous: Option<&gtk::Box> = None;
    for widget in widgets {
        if widget.parent().as_ref() != Some(slot.upcast_ref()) {
            if let Some(parent) = widget.parent().and_downcast::<gtk::Box>() {
                parent.remove(widget);
            }
            slot.append(widget);
        }
        if widget.prev_sibling().as_ref() != previous.map(|w| w.upcast_ref::<gtk::Widget>()) {
            slot.reorder_child_after(widget, previous);
        }
        previous = Some(widget);
    }
}
pub(super) fn on_screen(widget: &gtk::Box) -> bool {
    if !widget.is_mapped() || widget.width() <= 0 || widget.height() <= 0 {
        return false;
    }
    let mut ancestor = widget.parent();
    while let Some(parent) = ancestor {
        if !parent.is_mapped() {
            return false;
        }
        let Some(bounds) = widget.compute_bounds(&parent) else {
            return false;
        };
        if bounds.x() + bounds.width() <= 0.
            || bounds.y() + bounds.height() <= 0.
            || bounds.x() >= parent.width() as f32
            || bounds.y() >= parent.height() as f32
        {
            return false;
        }
        ancestor = parent.parent();
    }
    true
}
