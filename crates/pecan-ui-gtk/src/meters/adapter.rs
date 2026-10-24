//! Accepted-batch presentation bridge. No inbox drain, capture or backend owner.
use super::{MeterFeed, MeterMode, MeterWidget};
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{audio, buses::BusDef, mixer::Mixer},
    graph::Momentopname,
    inbox::InboxBatch,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

type Graph = Stamped<Arc<Momentopname>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeterSurface {
    Mixer,
    Patchbay,
}
impl MeterSurface {
    fn index(self) -> usize {
        match self {
            Self::Mixer => 0,
            Self::Patchbay => 1,
        }
    }
}
/// A presentation address, never registry authority or a command target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MeterKey {
    pub surface: MeterSurface,
    pub node: u32,
    pub item: String,
    /// Strip channel index or module meter-group ordinal, respectively.
    pub bar: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeterSpec {
    pub key: MeterKey,
    pub channels: Vec<usize>,
    pub label: String,
}
/// Caller-authenticated metadata for exactly this accepted native publication.
/// Each opaque I must bind the owner, epoch, node and relevant audio-port
/// generations/mapping. Numeric IDs, DTO equality or serials alone are not I.
pub struct MeterSources<'a, I> {
    pub graph: &'a Graph,
    pub node_bindings: &'a BTreeMap<u32, I>,
    /// Obtain these from the existing validated BusConfig::definitions policy.
    pub buses: &'a [BusDef],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelemetryError {
    MissingSources,
    DifferentOrigin,
    TooManyMeters { limit: usize },
    DuplicateKey,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TelemetryReport {
    pub graph_accepted: bool,
    pub peaks_accepted: bool,
    /// Projection/authority failure, not a claim about capture availability.
    pub projection_error: Option<TelemetryError>,
}
struct Entry<I> {
    spec: MeterSpec,
    widget: MeterWidget,
    identity: Option<I>,
    bound: bool,
    on_screen: bool,
}
impl<I> Entry<I> {
    fn retire(&mut self) {
        self.widget.set_active(false);
        self.widget.bind(self.spec.key.node, &[]);
        self.identity = None;
        self.bound = false;
    }
    fn activate(&self, surfaces: &[bool; 2]) {
        self.widget
            .set_active(self.bound && self.on_screen && surfaces[self.spec.key.surface.index()]);
    }
}
/// One feed, bounded retained widgets, and no frame queue or extra consumer.
/// All methods run on the GTK thread. New meters/surfaces start inactive.
pub struct TelemetryAdapter<I> {
    feed: MeterFeed,
    graph: Option<Graph>,
    entries: BTreeMap<MeterKey, Entry<I>>,
    surfaces: [bool; 2],
    limit: usize,
    error: Option<TelemetryError>,
    #[cfg(test)]
    projections: usize,
}
impl<I: Clone + Eq> Default for TelemetryAdapter<I> {
    fn default() -> Self {
        Self::new(512)
    }
}
impl<I: Clone + Eq> TelemetryAdapter<I> {
    pub fn new(widget_limit: usize) -> Self {
        Self {
            feed: MeterFeed::default(),
            graph: None,
            entries: BTreeMap::new(),
            surfaces: [false; 2],
            limit: widget_limit,
            error: None,
            #[cfg(test)]
            projections: 0,
        }
    }
    /// Consume neither the batch nor an inbox: the Session/owner remains its
    /// sole consumer. Source metadata is required only for an accepted graph.
    /// Equal-stamp peak frames replace the one slot in caller delivery order.
    pub fn apply_batch(
        &mut self,
        batch: &InboxBatch,
        sources: Option<MeterSources<'_, I>>,
    ) -> TelemetryReport {
        let mut report = TelemetryReport::default();
        if let Some(graph) = &batch.graph {
            if self.feed.observe_graph(graph.clone()) {
                self.graph = Some(graph.clone());
                report.graph_accepted = true;
                if graph.value.verbonden {
                    self.project(sources);
                } else {
                    for entry in self.entries.values_mut() {
                        entry.retire();
                    }
                    self.entries.clear();
                    self.error = None;
                }
            }
        }
        if let Some(peaks) = &batch.peaks {
            report.peaks_accepted = self.feed.publish_peaks(peaks.clone());
        }
        report.projection_error = self.error.clone();
        report
    }
    pub fn stamp(&self) -> Option<Stamp> {
        self.feed.stamp()
    }
    pub fn meters(&self) -> impl Iterator<Item = (&MeterSpec, &MeterWidget)> {
        self.entries.values().map(|e| (&e.spec, &e.widget))
    }
    pub fn meter(&self, key: &MeterKey) -> Option<&MeterWidget> {
        self.entries.get(key).map(|e| &e.widget)
    }
    /// Tab/window visibility is independent of viewport visibility. Hidden
    /// surfaces retain their widgets and last viewport, but register no ticks.
    pub fn set_surface_active(&mut self, surface: MeterSurface, active: bool) {
        self.surfaces[surface.index()] = active;
        for entry in self
            .entries
            .values()
            .filter(|e| e.spec.key.surface == surface)
        {
            entry.activate(&self.surfaces);
        }
    }
    /// The shell supplies actual visible rows/modules after layout/scrolling.
    /// Unknown keys are ignored; no caller-sized visibility set is retained.
    pub fn set_visible_keys(&mut self, surface: MeterSurface, visible: &BTreeSet<MeterKey>) {
        for entry in self
            .entries
            .values_mut()
            .filter(|e| e.spec.key.surface == surface)
        {
            entry.on_screen = visible.contains(&entry.spec.key);
            entry.activate(&self.surfaces);
        }
    }
    /// Explicit caller reset: DTOs do not encode capture age/unavailability.
    /// This clears samples/hold/clip without inventing silence or a timeout.
    pub fn clear_samples(&self) {
        self.feed.clear_peaks();
    }
    /// Explicit authority/configuration reconciliation against the retained
    /// exact frame. Clear old samples first, even when its stamp is unchanged.
    pub fn rebind_current(&mut self, sources: MeterSources<'_, I>) -> Option<TelemetryError> {
        self.feed.clear_peaks();
        self.project(Some(sources));
        self.error.clone()
    }
    /// Stop trusting all old bindings while the caller obtains a new witness.
    /// Existing widgets remain unavailable until explicit rebind/new graph.
    pub fn reset_sources(&mut self) {
        self.feed.clear_peaks();
        self.invalidate(TelemetryError::MissingSources);
    }
    fn invalidate(&mut self, error: TelemetryError) {
        for entry in self.entries.values_mut() {
            entry.retire();
        }
        self.error = Some(error);
    }
    fn project(&mut self, sources: Option<MeterSources<'_, I>>) {
        let Some(sources) = sources else {
            self.invalidate(TelemetryError::MissingSources);
            return;
        };
        if !self.graph.as_ref().is_some_and(|g| {
            g.stamp == sources.graph.stamp && Arc::ptr_eq(&g.value, &sources.graph.value)
        }) {
            self.invalidate(TelemetryError::DifferentOrigin);
            return;
        }
        let specs = match specs(&sources.graph.value, sources.buses, self.limit) {
            Ok(specs) => specs,
            Err(e) => {
                self.invalidate(e);
                return;
            }
        };
        #[cfg(test)]
        {
            self.projections += 1;
        }
        self.entries.retain(|key, entry| {
            if specs.contains_key(key) {
                true
            } else {
                entry.retire();
                false
            }
        });
        for (key, spec) in specs {
            let identity = sources.node_bindings.get(&key.node).cloned();
            let entry = self.entries.entry(key.clone()).or_insert_with(|| {
                let mode = match key.surface {
                    MeterSurface::Mixer => MeterMode::Channel,
                    MeterSurface::Patchbay => MeterMode::Mini,
                };
                let widget = MeterWidget::new(&self.feed, &spec.label, mode);
                widget.set_active(false);
                Entry {
                    spec: spec.clone(),
                    widget,
                    identity: None,
                    bound: false,
                    on_screen: false,
                }
            });
            if entry.identity != identity {
                // Force a reset even if a replacement has byte-identical DTOs.
                entry.retire();
            }
            entry.widget.set_label(&spec.label);
            entry.spec = spec;
            entry.identity = identity;
            entry.bound = if entry.identity.is_some() {
                entry.widget.bind(key.node, &entry.spec.channels)
            } else {
                entry.widget.bind(key.node, &[]);
                false
            };
            entry.activate(&self.surfaces);
        }
        self.error = None;
    }
}
impl<I> Drop for TelemetryAdapter<I> {
    fn drop(&mut self) {
        for entry in self.entries.values_mut() {
            entry.retire();
        }
    }
}
fn specs(
    graph: &Momentopname,
    buses: &[BusDef],
    limit: usize,
) -> Result<BTreeMap<MeterKey, MeterSpec>, TelemetryError> {
    let mixer = Mixer::build(graph, buses);
    let modules = audio::layout(graph);
    let mut result = BTreeMap::new();
    let mut insert = |key: MeterKey, channels: Vec<usize>, label: String| {
        if result.contains_key(&key) {
            return Err(TelemetryError::DuplicateKey);
        }
        if result.len() >= limit {
            return Err(TelemetryError::TooManyMeters { limit });
        }
        result.insert(
            key.clone(),
            MeterSpec {
                key,
                channels,
                label,
            },
        );
        Ok(())
    };
    for strip in mixer
        .apps
        .iter()
        .chain(&mixer.hardware)
        .chain(&mixer.destinations)
    {
        for channel in &strip.channels {
            insert(
                MeterKey {
                    surface: MeterSurface::Mixer,
                    node: strip.node.id,
                    item: strip.key.clone(),
                    bar: *channel,
                },
                vec![*channel],
                format!(
                    "{} · {} · channel {}",
                    strip.name,
                    strip.subtitle,
                    channel + 1
                ),
            )?;
        }
    }
    for module in modules.groups.iter().flat_map(|g| &g.modules) {
        for (bar, channels) in module
            .meter_channels
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_empty())
        {
            insert(
                MeterKey {
                    surface: MeterSurface::Patchbay,
                    node: module.node,
                    item: module.key.clone(),
                    bar,
                },
                channels.clone(),
                format!("{} · {} · meter {}", module.title, module.subtitle, bar + 1),
            )?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
