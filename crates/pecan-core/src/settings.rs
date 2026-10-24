//! Versioned, validated preferences and a pure importer for the five existing
//! WebKit localStorage entries. No filesystem, browser, GTK or backend access.
//!
//! JSON decoding is supplied by the caller so serde_json stays a test-only
//! dependency of pecan-core. Serialize only a validated SettingsV1 document.
use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    buses::{BusConfig, BusState},
    levels::{MAX_DB, OFF_DB},
};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const WEB_BUSES: &str = "pecanmixer.bussen";
pub const WEB_HIDDEN: &str = "pecanmixer.verborgen";
pub const WEB_CLOCK: &str = "pecanmixer.klok";
pub const WEB_ROWS: &str = "pecanmixer.rijen";
pub const WEB_SPLIT: &str = "pecanmixer.deling";
// Existing recorder limit in opname.rs; this core contract does not import it.
const MAX_BUS_CHANNELS: usize = 64;
const MIN_SPLIT_WIDTH: f64 = 120.0;

/// Implement using the application's chosen serialization format. Tests use the
/// existing serde_json dev dependency; the core adds no runtime JSON parser.
pub trait Decoder {
    type Error;
    fn decode<T: DeserializeOwned>(&mut self, input: &str) -> Result<T, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct HiddenKey(String);
impl HiddenKey {
    pub fn new(key: impl Into<String>) -> Result<Self, ValidationError> {
        let key = key.into();
        if stable_hidden_key(&key) {
            Ok(Self(key))
        } else {
            Err(ValidationError::InvalidHiddenKey)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for HiddenKey {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(decoder)?).map_err(serde::de::Error::custom)
    }
}
fn bus_key(key: &str) -> bool {
    let Some(numbers) = key.strip_prefix("rec-") else {
        return false;
    };
    let parts: Vec<_> = numbers.split('-').collect();
    if ![1, 2].contains(&parts.len()) {
        return false;
    }
    let numbers: Option<Vec<usize>> = parts
        .iter()
        .map(|p| {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                p.parse().ok()
            }
        })
        .collect();
    let Some(numbers) = numbers else { return false };
    numbers.iter().all(|n| (1..=MAX_BUS_CHANNELS).contains(n))
        && (numbers.len() == 1 || numbers[1] == numbers[0] + 1)
}
fn stable_hidden_key(key: &str) -> bool {
    if key.contains('\0') {
        return false;
    }
    if matches!(key, "rij|apps" | "rij|hardware" | "rij|bestemmingen") {
        return true;
    }
    if let Some(key) = key.strip_prefix("bus|") {
        return bus_key(key);
    }
    if let Some(key) = key.strip_prefix("midi|") {
        return key
            .split_once('|')
            .is_some_and(|(node, port)| !node.is_empty() && !port.is_empty());
    }
    ["|apps|", "|hardware|", "|bestemmingen|"]
        .iter()
        .any(|separator| {
            key.split_once(separator)
                .is_some_and(|(node, ports)| !node.is_empty() && !ports.is_empty())
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClockPreferences {
    pub start_stop: bool,
    // Deliberately no clock source port. The legacy loader also discards it.
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RowWeights {
    pub apps: f64,
    pub hardware: f64,
    pub destinations: f64,
}
impl Default for RowWeights {
    fn default() -> Self {
        Self {
            apps: 1.0,
            hardware: 1.0,
            destinations: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preferences {
    pub buses: BusConfig,
    pub hidden: BTreeSet<HiddenKey>,
    pub clock: ClockPreferences,
    pub rows: RowWeights,
    pub destination_width_px: Option<f64>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            buses: BusConfig::default(),
            hidden: BTreeSet::new(),
            clock: ClockPreferences::default(),
            rows: RowWeights::default(),
            destination_width_px: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    InvalidBusLayout,
    InvalidBusStateKey,
    InvalidBusGain,
    InvalidHiddenKey,
    InvalidRowWeights,
    InvalidDestinationWidth,
    UnsupportedVersion(u32),
}
impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for ValidationError {}
impl Preferences {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_buses(&self.buses)?;
        if self
            .hidden
            .iter()
            .any(|key| !stable_hidden_key(key.as_str()))
        {
            return Err(ValidationError::InvalidHiddenKey);
        }
        validate_rows(self.rows)?;
        validate_width(self.destination_width_px)
    }
}
fn validate_buses(buses: &BusConfig) -> Result<(), ValidationError> {
    if buses.widths.is_empty()
        || buses.widths.len() > MAX_BUS_CHANNELS
        || buses.widths.iter().any(|n| ![1, 2].contains(n))
        || buses.widths.iter().sum::<usize>() > MAX_BUS_CHANNELS
    {
        return Err(ValidationError::InvalidBusLayout);
    }
    for (key, state) in &buses.states {
        if !bus_key(key) {
            return Err(ValidationError::InvalidBusStateKey);
        }
        if !state.db.is_finite() || !(OFF_DB..=MAX_DB).contains(&state.db) {
            return Err(ValidationError::InvalidBusGain);
        }
    }
    Ok(())
}
fn validate_rows(rows: RowWeights) -> Result<(), ValidationError> {
    let values = [rows.apps, rows.hardware, rows.destinations];
    if values.iter().any(|v| !v.is_finite() || *v <= 0.0) || !values.iter().sum::<f64>().is_finite()
    {
        Err(ValidationError::InvalidRowWeights)
    } else {
        Ok(())
    }
}
fn validate_width(width: Option<f64>) -> Result<(), ValidationError> {
    if width.is_some_and(|n| !n.is_finite() || n < MIN_SPLIT_WIDTH) {
        Err(ValidationError::InvalidDestinationWidth)
    } else {
        Ok(())
    }
}

/// Private validated storage prevents mutation into a non-finite/invalid
/// document after construction. Edit a cloned Preferences, then validate again.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsV1 {
    schema_version: u32,
    preferences: Preferences,
}
impl Default for SettingsV1 {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            preferences: Preferences::default(),
        }
    }
}
impl SettingsV1 {
    pub fn new(preferences: Preferences) -> Result<Self, ValidationError> {
        preferences.validate()?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            preferences,
        })
    }
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
    pub fn into_preferences(self) -> Preferences {
        self.preferences
    }
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeDocument {
    schema_version: u32,
    #[serde(default)]
    preferences: NativePreferences,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativePreferences {
    buses: NativeBuses,
    hidden: BTreeSet<HiddenKey>,
    clock: ClockPreferences,
    rows: NativeRows,
    destination_width_px: Option<f64>,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeBuses {
    widths: Vec<usize>,
    states: BTreeMap<String, NativeBusState>,
    midi: bool,
}
impl Default for NativeBuses {
    fn default() -> Self {
        let default = BusConfig::default();
        Self {
            widths: default.widths,
            states: default
                .states
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        NativeBusState {
                            db: value.db,
                            mute: value.mute,
                            armed: value.armed,
                        },
                    )
                })
                .collect(),
            midi: default.midi,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeBusState {
    db: f64,
    mute: bool,
    #[serde(default)]
    armed: bool,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeRows {
    apps: f64,
    hardware: f64,
    destinations: f64,
}
impl Default for NativeRows {
    fn default() -> Self {
        Self {
            apps: 1.0,
            hardware: 1.0,
            destinations: 1.0,
        }
    }
}
impl From<NativePreferences> for Preferences {
    fn from(value: NativePreferences) -> Self {
        Self {
            buses: BusConfig {
                widths: value.buses.widths,
                states: value
                    .buses
                    .states
                    .into_iter()
                    .map(|(key, state)| {
                        (
                            key,
                            BusState {
                                db: state.db,
                                mute: state.mute,
                                armed: state.armed,
                            },
                        )
                    })
                    .collect(),
                midi: value.buses.midi,
            },
            hidden: value.hidden,
            clock: value.clock,
            rows: RowWeights {
                apps: value.rows.apps,
                hardware: value.rows.hardware,
                destinations: value.rows.destinations,
            },
            destination_width_px: value.destination_width_px,
        }
    }
}
impl<'de> Deserialize<'de> for SettingsV1 {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let raw = NativeDocument::deserialize(decoder)?;
        if raw.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(
                ValidationError::UnsupportedVersion(raw.schema_version),
            ));
        }
        Self::new(raw.preferences.into()).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeLoadStatus {
    Missing,
    Loaded,
    /// Use defaults in memory, while retaining the original input for recovery.
    PreserveCorruptInput,
    /// Never silently reinterpret or overwrite another schema version.
    PreserveUnsupportedVersion(u32),
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeLoad {
    pub settings: SettingsV1,
    pub status: NativeLoadStatus,
}
#[derive(Deserialize)]
struct SchemaHeader {
    schema_version: u32,
}
pub fn load_native<D: Decoder>(input: Option<&str>, decoder: &mut D) -> NativeLoad {
    let Some(input) = input else {
        return NativeLoad {
            settings: SettingsV1::default(),
            status: NativeLoadStatus::Missing,
        };
    };
    let status = match decoder.decode::<SchemaHeader>(input) {
        Ok(header) if header.schema_version != SCHEMA_VERSION => {
            NativeLoadStatus::PreserveUnsupportedVersion(header.schema_version)
        }
        Ok(_) => match decoder.decode::<SettingsV1>(input) {
            Ok(settings) => {
                return NativeLoad {
                    settings,
                    status: NativeLoadStatus::Loaded,
                }
            }
            Err(_) => NativeLoadStatus::PreserveCorruptInput,
        },
        Err(_) => NativeLoadStatus::PreserveCorruptInput,
    };
    NativeLoad {
        settings: SettingsV1::default(),
        status,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebKey {
    Buses,
    Hidden,
    Clock,
    Rows,
    Split,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationIssue {
    Malformed(WebKey),
    Invalid(WebKey),
    ClockSelectionDiscarded,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Migration {
    pub settings: SettingsV1,
    pub issues: Vec<MigrationIssue>,
}
#[derive(Default)]
enum Presence<T> {
    #[default]
    Missing,
    Present(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Presence<T> {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        T::deserialize(decoder).map(Self::Present)
    }
}
#[derive(Deserialize)]
struct LegacyBuses {
    #[serde(default)]
    bussen: Option<Vec<usize>>,
    #[serde(default)]
    paren: Option<Vec<LegacyPair>>,
    #[serde(default)]
    stand: Presence<Option<BTreeMap<String, LegacyState>>>,
    #[serde(default)]
    midi: Presence<Option<bool>>,
}
#[derive(Deserialize)]
struct LegacyPair {
    gekoppeld: bool,
}
#[derive(Deserialize)]
struct LegacyState {
    db: f64,
    mute: bool,
    #[serde(default)]
    gewapend: bool,
}
fn migrate_buses(raw: Option<LegacyBuses>) -> Result<BusConfig, ()> {
    let Some(raw) = raw else {
        return Ok(BusConfig::default());
    };
    let current = raw.bussen.as_ref().is_some_and(|b| !b.is_empty());
    let widths = if current {
        raw.bussen.unwrap_or_default()
    } else if let Some(pairs) = raw.paren.filter(|p| !p.is_empty()) {
        pairs
            .into_iter()
            .flat_map(|p| if p.gekoppeld { vec![2] } else { vec![1, 1] })
            .collect()
    } else {
        return Ok(BusConfig::default());
    };
    let states = match raw.stand {
        Presence::Present(Some(states)) => states
            .into_iter()
            .map(|(key, state)| {
                (
                    key,
                    BusState {
                        db: state.db,
                        mute: state.mute,
                        armed: state.gewapend,
                    },
                )
            })
            .collect(),
        Presence::Missing if current => BusConfig::default().states,
        Presence::Present(None) if current => return Err(()),
        Presence::Missing | Presence::Present(None) => BTreeMap::new(),
    };
    let midi = match raw.midi {
        Presence::Present(Some(value)) => value,
        Presence::Present(None) if current => return Err(()),
        Presence::Missing | Presence::Present(None) => true,
    };
    let buses = BusConfig {
        widths,
        states,
        midi,
    };
    validate_buses(&buses).map_err(|_| ())?;
    Ok(buses)
}
#[derive(Deserialize)]
#[serde(untagged)]
enum LegacyHidden {
    Text(String),
    Other(serde::de::IgnoredAny),
}
#[derive(Default, Deserialize)]
struct LegacyClock {
    #[serde(default)]
    start_stop: bool,
    #[serde(default)]
    klok: Option<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
#[serde(default)]
struct LegacyRows {
    apps: f64,
    hardware: f64,
    bestemmingen: f64,
}
impl Default for LegacyRows {
    fn default() -> Self {
        Self {
            apps: 1.0,
            hardware: 1.0,
            bestemmingen: 1.0,
        }
    }
}

/// Import an explicitly supplied export of localStorage key -> JSON string.
/// Reads only known keys, never changes/deletes input, and does not touch files.
/// Malformed/invalid components fall back independently with visible diagnostics.
pub fn migrate_web<D: Decoder>(entries: &BTreeMap<String, String>, decoder: &mut D) -> Migration {
    let mut preferences = Preferences::default();
    let mut issues = Vec::new();
    if let Some(input) = entries.get(WEB_BUSES) {
        match decoder.decode::<Option<LegacyBuses>>(input) {
            Ok(raw) => match migrate_buses(raw) {
                Ok(buses) => preferences.buses = buses,
                Err(()) => issues.push(MigrationIssue::Invalid(WebKey::Buses)),
            },
            Err(_) => issues.push(MigrationIssue::Malformed(WebKey::Buses)),
        }
    }
    if let Some(input) = entries.get(WEB_HIDDEN) {
        match decoder.decode::<Vec<LegacyHidden>>(input) {
            Ok(raw) => {
                let mut invalid = false;
                for key in raw {
                    match key {
                        LegacyHidden::Text(key) => match HiddenKey::new(key) {
                            Ok(key) => {
                                preferences.hidden.insert(key);
                            }
                            Err(_) => invalid = true,
                        },
                        LegacyHidden::Other(_) => invalid = true,
                    }
                }
                if invalid {
                    issues.push(MigrationIssue::Invalid(WebKey::Hidden));
                }
            }
            Err(_) => issues.push(MigrationIssue::Malformed(WebKey::Hidden)),
        }
    }
    if let Some(input) = entries.get(WEB_CLOCK) {
        match decoder.decode::<LegacyClock>(input) {
            Ok(clock) => {
                preferences.clock.start_stop = clock.start_stop;
                if clock.klok.is_some() {
                    issues.push(MigrationIssue::ClockSelectionDiscarded);
                }
            }
            Err(_) => issues.push(MigrationIssue::Malformed(WebKey::Clock)),
        }
    }
    if let Some(input) = entries.get(WEB_ROWS) {
        match decoder.decode::<LegacyRows>(input) {
            Ok(rows) => {
                let rows = RowWeights {
                    apps: rows.apps,
                    hardware: rows.hardware,
                    destinations: rows.bestemmingen,
                };
                if validate_rows(rows).is_ok() {
                    preferences.rows = rows;
                } else {
                    issues.push(MigrationIssue::Invalid(WebKey::Rows));
                }
            }
            Err(_) => issues.push(MigrationIssue::Malformed(WebKey::Rows)),
        }
    }
    if let Some(input) = entries.get(WEB_SPLIT) {
        match decoder.decode::<Option<f64>>(input) {
            Ok(width) if validate_width(width).is_ok() => preferences.destination_width_px = width,
            Ok(_) => issues.push(MigrationIssue::Invalid(WebKey::Split)),
            Err(_) => issues.push(MigrationIssue::Malformed(WebKey::Split)),
        }
    }
    // All components above were independently validated or retained defaults.
    Migration {
        settings: SettingsV1 {
            schema_version: SCHEMA_VERSION,
            preferences,
        },
        issues,
    }
}
