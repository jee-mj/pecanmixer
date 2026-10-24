use super::*;
use pecan_core::{
    backend::{Stamp, Stamped},
    domain::{
        buses::BusConfig,
        history::{Action, Controller, Input},
    },
    graph::Momentopname,
};
use std::sync::Arc;
#[test]
fn filtering_never_prunes_graph_origin_model_or_history() {
    let graph: Momentopname = serde_json::from_str(include_str!(
        "../../../../fixtures/domain/audio-policy.json"
    ))
    .unwrap();
    let origin = Stamped {
        stamp: Stamp {
            epoch: 2,
            revision: 1,
        },
        value: Arc::new(graph),
    };
    let original = origin.value.clone();
    let mixer = Mixer::build(&origin.value, &BusConfig::default().definitions().unwrap());
    let before = mixer.clone();
    let mut preferences = SettingsV1::default().into_preferences();
    preferences
        .hidden
        .insert(HiddenKey::new(mixer.apps[0].key.clone()).unwrap());
    preferences
        .hidden
        .insert(HiddenKey::new("rij|hardware").unwrap());
    let display = DisplayPreferences::from(&SettingsV1::new(preferences).unwrap());
    let keys = display.visible_keys(&mixer);
    assert!(!keys.contains(&mixer.apps[0].key));
    assert!(mixer
        .hardware
        .iter()
        .all(|strip| !keys.contains(&strip.key)));
    assert_eq!(mixer, before);
    assert!(Arc::ptr_eq(&origin.value, &original));
    let mut history = Controller::default();
    history.reduce(Input::Graph(origin.clone())).unwrap();
    let plan = history
        .reduce(Input::Execute {
            label: "Synthetic hidden-node mute".into(),
            action: Action::Mute {
                node: mixer.apps[0].node.id,
                muted: true,
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(plan.source_revision, origin.stamp.revision);
    assert!(Arc::ptr_eq(&history.graph().unwrap().value, &original));
    assert_eq!(display.fractions(), [0.5, 0.0, 0.5]);
}
#[test]
fn weights_empty_rows_and_render_width_do_not_rewrite_settings() {
    let mut preferences = SettingsV1::default().into_preferences();
    preferences.rows = RowWeights {
        apps: 2.0,
        hardware: 1.0,
        destinations: 1.0,
    };
    preferences.destination_width_px = Some(1200.0);
    let settings = SettingsV1::new(preferences).unwrap();
    let display = DisplayPreferences::from(&settings);
    assert_eq!(display.fractions(), [0.5, 0.25, 0.25]);
    assert!(display.row_visible(Row::Apps)); // even an empty Apps row remains
    assert_eq!(display.output_width(400, 240), 400);
    assert_eq!(display.output_width(-1, 240), 0);
    assert_eq!(settings.preferences().destination_width_px, Some(1200.0));
    assert_eq!(DisplayPreferences::default().output_width(400, 230), 230);
}
#[test]
fn audio_preferences_are_excluded_from_presentation_adapter() {
    let baseline = DisplayPreferences::default();
    let mut preferences = SettingsV1::default().into_preferences();
    preferences.clock.start_stop = true;
    preferences.buses.midi = false;
    let settings = SettingsV1::new(preferences).unwrap();
    assert_eq!(DisplayPreferences::from(&settings), baseline);
}
