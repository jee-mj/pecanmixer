//! Temporary compatibility boundary for the existing web frontend.
//!
//! Backend DTOs retain their existing Dutch wire names. The native preview
//! consumes owned events directly and does not depend on this adapter.

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::graaf::{Graaf, Melding, Momentopname, Pieken, TransportStand};
use crate::midimeter::Activiteit;
use crate::tray;

/// Untagged serialization sends the original payload, without an enum wrapper.
#[derive(Clone, Copy, Serialize)]
#[serde(untagged)]
enum LegacyEvent<'a> {
    Graph(&'a Momentopname),
    Peaks(&'a Pieken),
    Midi(&'a BTreeMap<u32, Activiteit>),
    Transport(&'a TransportStand),
}

impl<'a> From<Melding<'a>> for LegacyEvent<'a> {
    fn from(event: Melding<'a>) -> Self {
        match event {
            Melding::Graaf(value) => Self::Graph(value),
            Melding::Pieken(value) => Self::Peaks(value),
            Melding::Midi(value) => Self::Midi(value),
            Melding::Transport(value) => Self::Transport(value),
        }
    }
}

impl LegacyEvent<'_> {
    fn name(&self) -> &'static str {
        match self {
            Self::Graph(_) => "graaf",
            Self::Peaks(_) => "pieken",
            Self::Midi(_) => "midi",
            Self::Transport(_) => "transport",
        }
    }
}

pub(crate) fn start(handle: AppHandle) -> Graaf {
    Graaf::start(Box::new(move |message| {
        let event = LegacyEvent::from(message);
        match event {
            LegacyEvent::Graph(graph) => {
                tray::werk_bij(&handle, |state| {
                    state.verbonden = graph.verbonden;
                    state.rate = graph.rate;
                });
            }
            LegacyEvent::Transport(transport) => {
                tray::werk_bij(&handle, |state| {
                    state.opname = transport.bezig;
                    state.bussen = transport.bussen.keys().cloned().collect();
                });
            }
            LegacyEvent::Peaks(_) | LegacyEvent::Midi(_) => {}
        }
        let _ = handle.emit(event.name(), event);
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn graph_event_preserves_the_legacy_fixture_payload() {
        let fixture = include_str!("../../fixtures/graph/audio-basic.json");
        let graph: Momentopname = serde_json::from_str(fixture).unwrap();
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let event = LegacyEvent::from(Melding::Graaf(&graph));

        assert_eq!(event.name(), "graaf");
        assert_eq!(serde_json::to_value(event).unwrap(), expected);
    }

    #[test]
    fn peaks_and_midi_events_preserve_numeric_id_maps() {
        let peaks = BTreeMap::from([(42, vec![0.25, 0.5])]);
        let event = LegacyEvent::from(Melding::Pieken(&peaks));
        assert_eq!(event.name(), "pieken");
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            json!({"42": [0.25, 0.5]})
        );

        let midi = BTreeMap::from([(
            61,
            Activiteit {
                events: 3,
                kanalen: 5,
                soorten: 1,
                laatste: Some("Note On E4".into()),
            },
        )]);
        let event = LegacyEvent::from(Melding::Midi(&midi));
        assert_eq!(event.name(), "midi");
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            json!({"61": {"events": 3, "kanalen": 5, "soorten": 1, "laatste": "Note On E4"}})
        );
    }

    #[test]
    fn transport_event_preserves_the_legacy_fixture_payload() {
        let fixture = include_str!("../../fixtures/transport.json");
        let transport: TransportStand = serde_json::from_str(fixture).unwrap();
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let event = LegacyEvent::from(Melding::Transport(&transport));

        assert_eq!(event.name(), "transport");
        assert_eq!(serde_json::to_value(event).unwrap(), expected);
    }
}
