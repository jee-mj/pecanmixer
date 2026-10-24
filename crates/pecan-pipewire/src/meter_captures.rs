//! Capture ownership on the graph worker. The factory is injected only to test
//! the same reconciliation path without creating PipeWire streams.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub serial: u64,
    pub sink: bool,
    pub channels: Vec<String>,
    /// Node first, then selected ports in capture-channel order. Even an
    /// identical remove/add cycle belongs to different registry objects.
    pub objects: Vec<(u32, u64)>,
}

pub(crate) struct Capture<T> {
    pub source: T,
    identity: Identity,
}

pub(crate) fn reconcile<T>(
    captures: &mut BTreeMap<u32, Capture<T>>,
    desired: BTreeMap<u32, Identity>,
    mut create: impl FnMut(u32, &Identity) -> Option<T>,
) {
    // Drop invalid sources before attempting replacements. A failed factory
    // must leave the node absent, never harvesting the previous object's peaks.
    captures.retain(|id, capture| desired.get(id) == Some(&capture.identity));
    for (id, identity) in desired {
        if let std::collections::btree_map::Entry::Vacant(entry) = captures.entry(id) {
            if let Some(source) = create(id, &identity) {
                entry.insert(Capture { source, identity });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity {
            serial: 1010,
            sink: false,
            channels: vec!["MONO".into()],
            objects: vec![(10, 1), (11, 2)],
        }
    }
    #[test]
    fn coalesced_same_id_channels_changed_serial_recreates_capture() {
        let mut captures = BTreeMap::new();
        let old = identity();
        reconcile(
            &mut captures,
            BTreeMap::from([(10, old.clone())]),
            |_, id| Some(id.clone()),
        );
        let mut replacement = old;
        replacement.serial = 2020;
        reconcile(
            &mut captures,
            BTreeMap::from([(10, replacement.clone())]),
            |_, id| Some(id.clone()),
        );
        assert_eq!(captures[&10].source, replacement);
    }
    #[test]
    fn coalesced_same_id_channels_changed_sink_direction_recreates_capture() {
        let mut captures = BTreeMap::new();
        let old = identity();
        reconcile(
            &mut captures,
            BTreeMap::from([(10, old.clone())]),
            |_, id| Some(id.clone()),
        );
        let mut replacement = old;
        replacement.sink = true;
        reconcile(
            &mut captures,
            BTreeMap::from([(10, replacement.clone())]),
            |_, id| Some(id.clone()),
        );
        assert_eq!(captures[&10].source, replacement);
    }
}
