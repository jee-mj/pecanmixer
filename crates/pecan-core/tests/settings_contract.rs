use pecan_core::{
    domain::buses::BusConfig,
    settings::{
        self, ClockPreferences, Decoder, HiddenKey, MigrationIssue, NativeLoadStatus, Preferences,
        RowWeights, SettingsV1, ValidationError, WebKey, WEB_BUSES, WEB_CLOCK, WEB_HIDDEN,
        WEB_ROWS, WEB_SPLIT,
    },
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
struct Json {
    calls: usize,
}
impl Decoder for Json {
    type Error = serde_json::Error;
    fn decode<T: DeserializeOwned>(&mut self, input: &str) -> Result<T, Self::Error> {
        self.calls += 1;
        serde_json::from_str(input)
    }
}
fn entries(input: &str) -> BTreeMap<String, String> {
    serde_json::from_str(input).unwrap()
}
fn current() -> BTreeMap<String, String> {
    entries(include_str!("../../../fixtures/settings/web-current.json"))
}
fn encode(settings: &SettingsV1) -> Value {
    serde_json::to_value(settings).unwrap()
}
fn import(entries: &BTreeMap<String, String>) -> settings::Migration {
    settings::migrate_web(entries, &mut Json::default())
}

#[test]
fn version_one_defaults_match_actual_legacy_defaults_and_round_trip() {
    let settings = SettingsV1::default();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/settings/native-default-v1.json"
    ))
    .unwrap();
    assert_eq!(encode(&settings), expected);
    assert_eq!(settings.schema_version(), 1);
    assert_eq!(
        serde_json::from_value::<SettingsV1>(expected).unwrap(),
        settings
    );
    assert_eq!(settings.preferences().buses, BusConfig::default());
    assert!(settings.preferences().buses.state("rec-1-2").armed);
    assert_eq!(settings.preferences().rows, RowWeights::default());
    assert_eq!(settings.preferences().clock, ClockPreferences::default());
    let missing = settings::load_native(None, &mut Json::default());
    assert_eq!(missing.settings, settings);
    assert_eq!(missing.status, NativeLoadStatus::Missing);
    let partial = settings::load_native(Some(r#"{"schema_version":1}"#), &mut Json::default());
    assert_eq!(partial.settings, settings);
    assert_eq!(partial.status, NativeLoadStatus::Loaded);
}

#[test]
fn current_web_fixture_migrates_to_hand_reviewed_native_document_without_clock_ids() {
    let web = current();
    let untouched = web.clone();
    let mut decoder = Json::default();
    let imported = settings::migrate_web(&web, &mut decoder);
    let expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/settings/native-current-expected-v1.json"
    ))
    .unwrap();
    assert_eq!(encode(&imported.settings), expected);
    assert_eq!(web, untouched);
    assert_eq!(decoder.calls, 5);
    assert_eq!(imported.issues, [MigrationIssue::ClockSelectionDiscarded]);
    assert!(imported.settings.preferences().validate().is_ok());
    let serialized = serde_json::to_string(&imported.settings).unwrap();
    assert!(!serialized.contains("987654321"));
    assert!(!serialized.contains("\"klok\""));
    let decoded = settings::load_native(Some(&serialized), &mut Json::default());
    assert_eq!(decoded.status, NativeLoadStatus::Loaded);
    assert_eq!(decoded.settings, imported.settings);
}

#[test]
fn historical_pair_layout_migration_uses_existing_source_rules() {
    let raw = entries(include_str!("../../../fixtures/settings/web-pairs.json"));
    let migrated = import(&raw);
    let buses = &migrated.settings.preferences().buses;
    assert_eq!(buses.widths, [2, 1, 1]);
    assert!(buses.midi);
    assert_eq!(buses.state("rec-1-2").db, -3.0);
    assert!(!buses.state("rec-1-2").armed);
    assert!(buses.state("rec-3").armed);
    assert!(migrated.issues.is_empty());
    // The old `paren` branch uses stand ?? {}, not BEGIN.stand. The current
    // `bussen` branch spreads BEGIN and keeps its armed stereo default instead.
    let old = import(&BTreeMap::from([(
        WEB_BUSES.into(),
        r#"{"paren":[{"gekoppeld":true}]}"#.into(),
    )]));
    assert!(old.settings.preferences().buses.states.is_empty());
    let old_null = import(&BTreeMap::from([(
        WEB_BUSES.into(),
        r#"{"paren":[{"gekoppeld":true}],"stand":null,"midi":null}"#.into(),
    )]));
    assert_eq!(old_null.settings, old.settings);
    let modern = import(&BTreeMap::from([(
        WEB_BUSES.into(),
        r#"{"bussen":[2]}"#.into(),
    )]));
    assert_eq!(modern.settings.preferences().buses, BusConfig::default());
    let precedence = import(&BTreeMap::from([(
        WEB_BUSES.into(),
        r#"{"bussen":[1,1],"paren":[{"gekoppeld":true}],"stand":{},"midi":false}"#.into(),
    )]));
    assert_eq!(precedence.settings.preferences().buses.widths, [1, 1]);
    assert!(!precedence.settings.preferences().buses.midi);
}

#[test]
fn corrupt_web_components_recover_independently_with_bounded_diagnostics() {
    let imported = import(&entries(include_str!(
        "../../../fixtures/settings/web-corrupt.json"
    )));
    let prefs = imported.settings.preferences();
    assert_eq!(prefs.buses, BusConfig::default());
    assert_eq!(prefs.rows, RowWeights::default());
    assert_eq!(prefs.destination_width_px, None);
    assert!(prefs.clock.start_stop);
    assert_eq!(
        prefs.hidden.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
        ["bus|rec-1-2", "rij|apps"]
    );
    assert_eq!(
        imported.issues,
        [
            MigrationIssue::Invalid(WebKey::Buses),
            MigrationIssue::Invalid(WebKey::Hidden),
            MigrationIssue::ClockSelectionDiscarded,
            MigrationIssue::Invalid(WebKey::Rows),
            MigrationIssue::Malformed(WebKey::Split)
        ]
    );
    assert!(prefs.validate().is_ok());
    let all_bad = BTreeMap::from([
        (WEB_BUSES.into(), "{".into()),
        (WEB_HIDDEN.into(), "{".into()),
        (WEB_CLOCK.into(), "{".into()),
        (WEB_ROWS.into(), "{".into()),
        (WEB_SPLIT.into(), "{".into()),
    ]);
    let migrated = import(&all_bad);
    assert_eq!(migrated.settings, SettingsV1::default());
    assert_eq!(migrated.issues.len(), 5);
}

#[test]
fn missing_keys_partial_rows_and_null_split_keep_source_defaults() {
    let imported = import(&BTreeMap::new());
    assert_eq!(imported.settings, SettingsV1::default());
    assert!(imported.issues.is_empty());
    let migrated = import(&BTreeMap::from([
        (WEB_ROWS.into(), r#"{"hardware":2.5}"#.into()),
        (WEB_SPLIT.into(), "null".into()),
        (WEB_CLOCK.into(), r#"{"klok":null}"#.into()),
    ]));
    assert_eq!(
        migrated.settings.preferences().rows,
        RowWeights {
            apps: 1.0,
            hardware: 2.5,
            destinations: 1.0
        }
    );
    assert_eq!(migrated.settings.preferences().destination_width_px, None);
    assert!(migrated.issues.is_empty());
}

#[test]
fn unknown_schema_and_corrupt_native_input_are_explicitly_preserved() {
    let future = include_str!("../../../fixtures/settings/native-future.json");
    let mut decoder = Json::default();
    let loaded = settings::load_native(Some(future), &mut decoder);
    assert_eq!(
        loaded.status,
        NativeLoadStatus::PreserveUnsupportedVersion(2)
    );
    assert_eq!(loaded.settings, SettingsV1::default());
    assert_eq!(
        decoder.calls, 1,
        "never reinterpret a future preferences shape"
    );
    for raw in [
        include_str!("../../../fixtures/settings/native-truncated.txt"),
        "null",
        "[]",
        r#"{"preferences":{}}"#,
        r#"{"schema_version":-1}"#,
        r#"{"schema_version":1,"preferences":{"rows":{"apps":0}}}"#,
    ] {
        let loaded = settings::load_native(Some(raw), &mut Json::default());
        assert_eq!(loaded.status, NativeLoadStatus::PreserveCorruptInput);
        assert_eq!(loaded.settings, SettingsV1::default());
    }
    let zero = settings::load_native(Some(r#"{"schema_version":0}"#), &mut Json::default());
    assert_eq!(zero.status, NativeLoadStatus::PreserveUnsupportedVersion(0));
}

#[test]
fn native_contract_rejects_transient_or_unknown_fields_at_every_storage_layer() {
    let default = encode(&SettingsV1::default());
    let injections = [
        (vec!["node_id"], json!(987654321)),
        (vec!["preferences", "routes"], json!([[100, 200]])),
        (vec!["preferences", "clock", "klok"], json!(987654321)),
        (vec!["preferences", "buses", "node_id"], json!(987654321)),
        (
            vec!["preferences", "buses", "states", "rec-1-2", "port_id"],
            json!(987654321),
        ),
        (vec!["preferences", "rows", "node_id"], json!(987654321)),
    ];
    for (path, value) in injections {
        let mut document = default.clone();
        let mut field = &mut document;
        for key in &path[..path.len() - 1] {
            field = &mut field[*key];
        }
        field[path[path.len() - 1]] = value;
        let raw = serde_json::to_string(&document).unwrap();
        assert!(
            serde_json::from_str::<SettingsV1>(&raw).is_err(),
            "accepted transient path {path:?}"
        );
        assert_eq!(
            settings::load_native(Some(&raw), &mut Json::default()).status,
            NativeLoadStatus::PreserveCorruptInput
        );
    }
}

#[test]
fn migration_reads_only_five_known_keys_and_drops_unstored_session_state() {
    let mut raw = current();
    raw.insert("pecanmixer.graph".into(), "deliberately not JSON".into());
    raw.insert(
        "pecanmixer.patches".into(),
        r#"{"selected_node":987654321,"history":[100,200],"positions":{"1":[10,20]}}"#.into(),
    );
    let mut decoder = Json::default();
    let migrated = settings::migrate_web(&raw, &mut decoder);
    assert_eq!(decoder.calls, 5);
    assert_eq!(migrated.settings, import(&current()).settings);
    let mut raw = current();
    raw.insert(
        WEB_CLOCK.into(),
        r#"{"klok":{"port_id":987654321},"start_stop":true,"rate":48000}"#.into(),
    );
    let migrated = import(&raw);
    assert!(migrated.settings.preferences().clock.start_stop);
    assert!(!serde_json::to_string(&migrated.settings)
        .unwrap()
        .contains("987654321"));
}

#[test]
fn stable_hidden_keys_are_deduplicated_and_raw_object_identifiers_are_not_a_contract() {
    for valid in [
        "rij|apps",
        "rij|hardware",
        "rij|bestemmingen",
        "bus|rec-2-3",
        "firefox|apps|output_FL,output_FR#1",
        "alsa_input.name|hardware|capture_FL",
        "midi|Midi-Bridge/Fixture|Fixture: MIDI 1",
    ] {
        assert_eq!(HiddenKey::new(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "42",
        "node:42",
        "port:12",
        "rij|unknown",
        "bus|rec-0",
        "bus|rec-1-3",
        "|apps|output_FL",
        "name|apps|",
        "name\0|apps|port",
    ] {
        assert!(HiddenKey::new(invalid).is_err(), "accepted {invalid}");
    }
    let mut raw = encode(&SettingsV1::default());
    raw["preferences"]["hidden"] = json!(["42"]);
    assert!(serde_json::from_value::<SettingsV1>(raw).is_err());
}

#[test]
fn validation_rejects_nonfinite_invalid_layout_gain_and_dimensions_without_clamping() {
    let mut preferences = Preferences::default();
    preferences.buses.widths = vec![2; 33];
    assert_eq!(
        SettingsV1::new(preferences).unwrap_err(),
        ValidationError::InvalidBusLayout
    );
    let mut preferences = Preferences::default();
    preferences.buses.widths = vec![usize::MAX];
    assert_eq!(
        SettingsV1::new(preferences).unwrap_err(),
        ValidationError::InvalidBusLayout
    );
    for db in [f64::NAN, f64::INFINITY, -91.0, 6.1] {
        let mut preferences = Preferences::default();
        preferences.buses.states.get_mut("rec-1-2").unwrap().db = db;
        assert_eq!(
            SettingsV1::new(preferences).unwrap_err(),
            ValidationError::InvalidBusGain
        );
    }
    for weight in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut preferences = Preferences::default();
        preferences.rows.apps = weight;
        assert_eq!(
            SettingsV1::new(preferences).unwrap_err(),
            ValidationError::InvalidRowWeights
        );
    }
    let mut preferences = Preferences::default();
    preferences.rows = RowWeights {
        apps: f64::MAX,
        hardware: f64::MAX,
        destinations: 1.0,
    };
    assert_eq!(
        SettingsV1::new(preferences).unwrap_err(),
        ValidationError::InvalidRowWeights
    );
    for width in [0.0, 119.0, f64::NAN, f64::INFINITY] {
        let mut preferences = Preferences::default();
        preferences.destination_width_px = Some(width);
        assert_eq!(
            SettingsV1::new(preferences).unwrap_err(),
            ValidationError::InvalidDestinationWidth
        );
    }
    let mut preferences = Preferences::default();
    preferences.destination_width_px = Some(120.0);
    preferences.buses.widths = vec![2; 32];
    assert!(SettingsV1::new(preferences).is_ok());
}

#[test]
fn unsupported_loose_legacy_shapes_are_reported_without_claimed_compatibility() {
    for raw in [
        r#"{"bussen":[2],"stand":null}"#,
        r#"{"bussen":[2],"midi":null}"#,
        r#"{"bussen":[3],"stand":{}}"#,
        r#"{"bussen":[2],"stand":{"rec-1-2":{"db":30,"mute":false}}}"#,
    ] {
        let migrated = import(&BTreeMap::from([(WEB_BUSES.into(), raw.into())]));
        assert_eq!(migrated.settings.preferences().buses, BusConfig::default());
        assert!(!migrated.issues.is_empty());
    }
    let migrated = import(&BTreeMap::from([(WEB_BUSES.into(), "null".into())]));
    assert_eq!(migrated.settings, SettingsV1::default());
    assert!(migrated.issues.is_empty());
}

#[test]
fn valid_inactive_bus_state_is_retained_instead_of_inventing_cleanup() {
    let raw = BTreeMap::from([(WEB_BUSES.into(), r#"{"bussen":[2],"stand":{"rec-1-2":{"db":0,"mute":false,"gewapend":true},"rec-5-6":{"db":-3,"mute":true,"gewapend":false}},"midi":true}"#.into())]);
    let migrated = import(&raw);
    assert!(migrated.issues.is_empty());
    let states = &migrated.settings.preferences().buses.states;
    assert_eq!(states.len(), 2);
    assert_eq!(states["rec-5-6"].db, -3.0);
    assert!(states["rec-5-6"].mute);
    let mut preferences = migrated.settings.clone().into_preferences();
    preferences
        .buses
        .states
        .insert("123".into(), states["rec-5-6"].clone());
    assert_eq!(
        SettingsV1::new(preferences).unwrap_err(),
        ValidationError::InvalidBusStateKey
    );
}
