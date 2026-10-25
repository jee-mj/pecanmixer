//! Latest-value high-rate slots and a bounded ordered low-rate notice queue.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::backend::{BackendEvent, Notice, Stamp, Stamped};
use crate::graph::{Activiteit, Momentopname, Pieken, TransportStand};

#[derive(Default)]
pub struct InboxBatch {
    pub graph: Option<Stamped<Arc<Momentopname>>>,
    pub peaks: Option<Stamped<Arc<Pieken>>>,
    pub midi: Option<Stamped<Arc<BTreeMap<u32, Activiteit>>>>,
    pub transport: Option<Stamped<Arc<TransportStand>>>,
    pub notices: Vec<Notice>,
}

#[derive(Default)]
struct Pending {
    latest: InboxBatch,
    notices: VecDeque<Notice>,
    dropped: u64,
    newest_graph: Option<Stamp>,
}

pub struct LatestInbox {
    pending: Mutex<Pending>,
    wake: async_channel::Sender<()>,
    notice_capacity: usize,
}

impl LatestInbox {
    pub fn new(notice_capacity: usize) -> (Arc<Self>, async_channel::Receiver<()>) {
        let (wake, receiver) = async_channel::bounded(1);
        (
            Arc::new(Self {
                pending: Mutex::new(Pending::default()),
                wake,
                notice_capacity,
            }),
            receiver,
        )
    }

    /// Never waits for the UI to receive an event. Every high-rate kind occupies
    /// at most one slot; an unread graph replaces the previous complete graph.
    pub fn publish(&self, event: BackendEvent) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            BackendEvent::Graph(value) => {
                if pending.newest_graph.is_none_or(|stamp| value.stamp > stamp) {
                    let epoch = value.stamp.epoch;
                    if pending.newest_graph.is_none_or(|stamp| epoch > stamp.epoch) {
                        pending.latest.peaks = None;
                        pending.latest.midi = None;
                        pending.latest.transport = None;
                    }
                    pending.newest_graph = Some(value.stamp);
                    pending.latest.graph = Some(value);
                }
            }
            BackendEvent::Peaks(value) => {
                if current_epoch(pending.newest_graph, value.stamp) {
                    replace_newer(&mut pending.latest.peaks, value);
                }
            }
            BackendEvent::Midi(value) => {
                if current_epoch(pending.newest_graph, value.stamp) {
                    replace_newer(&mut pending.latest.midi, value);
                }
            }
            BackendEvent::Transport(value) => {
                if current_epoch(pending.newest_graph, value.stamp) {
                    replace_newer(&mut pending.latest.transport, value);
                }
            }
            BackendEvent::Notice(notice) => {
                if pending.notices.len() < self.notice_capacity {
                    pending.notices.push_back(notice);
                } else {
                    pending.dropped = pending.dropped.saturating_add(1);
                }
            }
        }
        drop(pending);
        // Full already means a wake is pending. Closed means the UI has gone.
        let _ = self.wake.try_send(());
    }

    pub fn drain(&self) -> InboxBatch {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let mut batch = std::mem::take(&mut pending.latest);
        batch.notices.extend(pending.notices.drain(..));
        if pending.dropped > 0 {
            batch.notices.push(Notice::InboxOverflow {
                dropped: std::mem::take(&mut pending.dropped),
            });
        }
        batch
    }
}

fn current_epoch(graph: Option<Stamp>, incoming: Stamp) -> bool {
    graph.is_none_or(|stamp| incoming.epoch == stamp.epoch)
}

fn replace_newer<T>(slot: &mut Option<Stamped<T>>, incoming: Stamped<T>) {
    if slot
        .as_ref()
        .is_none_or(|current| incoming.stamp >= current.stamp)
    {
        *slot = Some(incoming);
    }
}
