use super::widget::View;
use pecan_core::{
    backend::{Stamp, Stamped},
    graph::{Momentopname, Pieken, Soort},
    inbox::InboxBatch,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    serial: u64,
    name: String,
    class: Option<String>,
    ports: Vec<(u32, Option<u32>, bool, Option<String>)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    pub epoch: u64,
    pub node: u32,
    pub channels: Vec<usize>,
    identity: Identity,
}
#[derive(Default)]
pub(super) struct Frames {
    pub graph: Option<Stamped<Arc<Momentopname>>>,
    pub peaks: Option<Stamped<Arc<Pieken>>>,
    identities: BTreeMap<u32, Identity>,
}
impl Frames {
    fn observe(&mut self, graph: Stamped<Arc<Momentopname>>) -> bool {
        if self
            .graph
            .as_ref()
            .is_some_and(|old| old.stamp >= graph.stamp)
        {
            return false;
        }
        self.peaks = None;
        self.identities.clear();
        if graph.value.verbonden {
            let mut ports: BTreeMap<u32, Vec<_>> = BTreeMap::new();
            for port in graph
                .value
                .poorten
                .iter()
                .filter(|p| p.soort == Soort::Audio && !p.monitor)
            {
                ports.entry(port.knoop).or_default().push((
                    port.id,
                    port.volgnummer,
                    port.ingang,
                    port.kanaal.clone(),
                ));
            }
            for values in ports.values_mut() {
                values.sort_by_key(|p| p.0);
            }
            for node in &graph.value.knopen {
                if let Some(serial) = node.serial {
                    self.identities.insert(
                        node.id,
                        Identity {
                            serial,
                            name: node.naam.clone(),
                            class: node.klasse.clone(),
                            ports: ports.remove(&node.id).unwrap_or_default(),
                        },
                    );
                }
            }
        }
        self.graph = Some(graph);
        true
    }
    fn publish(&mut self, peaks: Stamped<Arc<Pieken>>) -> bool {
        if !self
            .graph
            .as_ref()
            .is_some_and(|graph| graph.value.verbonden && graph.stamp == peaks.stamp)
        {
            return false;
        }
        // Several successive peak frames normally share a structural stamp.
        // The owning inbox/adapter delivers them in producer order.
        self.peaks = Some(peaks);
        true
    }
    pub fn bind(&self, node: u32, channels: &[usize]) -> Option<Binding> {
        if channels.is_empty() {
            return None;
        }
        let graph = self.graph.as_ref().filter(|g| g.value.verbonden)?;
        Some(Binding {
            epoch: graph.stamp.epoch,
            node,
            channels: channels.to_vec(),
            identity: self.identities.get(&node)?.clone(),
        })
    }
    pub fn current(&self, binding: &Binding) -> bool {
        self.graph
            .as_ref()
            .is_some_and(|g| g.value.verbonden && g.stamp.epoch == binding.epoch)
            && self.identities.get(&binding.node) == Some(&binding.identity)
    }
    pub fn samples(&self, binding: &Binding) -> Option<Arc<Pieken>> {
        self.current(binding)
            .then(|| self.peaks.as_ref().map(|p| Arc::clone(&p.value)))
            .flatten()
    }
}

pub(super) struct Feed {
    pub frames: RefCell<Frames>,
    views: RefCell<Vec<Weak<View>>>,
}
/// Main-thread latest-frame adapter. Feed the accepted graph first, then peaks.
/// Clones share one graph and one replaceable peak frame, never a frame queue.
#[derive(Clone)]
pub struct MeterFeed(pub(super) Rc<Feed>);
impl Default for MeterFeed {
    fn default() -> Self {
        Self(Rc::new(Feed {
            frames: RefCell::default(),
            views: RefCell::default(),
        }))
    }
}
impl MeterFeed {
    pub fn observe_graph(&self, graph: Stamped<Arc<Momentopname>>) -> bool {
        if !self.0.frames.borrow_mut().observe(graph) {
            return false;
        }
        let views = self.0.live_views();
        for view in views {
            view.graph_changed();
        }
        true
    }
    /// Reject old/future graph revisions and other epochs. Equal stamps replace
    /// the current peak frame; the graph DTO has no finer frame sequence.
    pub fn publish_peaks(&self, peaks: Stamped<Arc<Pieken>>) -> bool {
        self.0.frames.borrow_mut().publish(peaks)
    }
    /// Explicitly stop displaying samples when capture is unavailable without
    /// fabricating silence or a disconnected graph. Sources remain bound.
    pub fn clear_peaks(&self) {
        self.0.frames.borrow_mut().peaks = None;
        for view in self.0.live_views() {
            view.reset();
        }
    }
    pub fn apply(&self, batch: &InboxBatch) {
        if let Some(graph) = &batch.graph {
            self.observe_graph(graph.clone());
        }
        if let Some(peaks) = &batch.peaks {
            self.publish_peaks(peaks.clone());
        }
    }
    pub fn stamp(&self) -> Option<Stamp> {
        self.0.frames.borrow().graph.as_ref().map(|g| g.stamp)
    }
    pub(super) fn register(&self, view: &Rc<View>) {
        let mut views = self.0.views.borrow_mut();
        views.retain(|v| v.strong_count() > 0);
        views.push(Rc::downgrade(view));
    }
}
impl Feed {
    fn live_views(&self) -> Vec<Rc<View>> {
        let mut views = self.views.borrow_mut();
        let live: Vec<_> = views.iter().filter_map(Weak::upgrade).collect();
        views.retain(|v| v.strong_count() > 0);
        live
    }
}
