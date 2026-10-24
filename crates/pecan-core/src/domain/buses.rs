//! Recording-bus configuration policy from bussen.ts. Persistence is separate.
use super::{audio::is_bus, levels::db_to_linear};
use crate::graph::{Momentopname, Opdracht};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BusState {
    pub db: f64,
    pub mute: bool,
    #[serde(default)]
    pub armed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusConfig {
    pub widths: Vec<usize>,
    pub states: BTreeMap<String, BusState>,
    pub midi: bool,
}
impl Default for BusConfig {
    fn default() -> Self {
        Self {
            widths: vec![2],
            states: BTreeMap::from([(
                "rec-1-2".into(),
                BusState {
                    armed: true,
                    ..Default::default()
                },
            )]),
            midi: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Coupling {
    Split,
    Join,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusDef {
    pub key: String,
    pub name: String,
    pub file: String,
    pub channels: Vec<usize>,
    pub index: usize,
    pub coupling: Option<Coupling>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusError {
    Empty,
    InvalidWidth,
    InvalidIndex,
    NonFiniteGain,
    ChannelOverflow,
}

impl BusConfig {
    pub fn definitions(&self) -> Result<Vec<BusDef>, BusError> {
        if self.widths.is_empty() {
            return Err(BusError::Empty);
        }
        let mut definitions: Vec<BusDef> = Vec::new();
        let mut channel = 0_usize;
        for (index, &width) in self.widths.iter().enumerate() {
            if ![1, 2].contains(&width) {
                return Err(BusError::InvalidWidth);
            }
            let a = channel.checked_add(1).ok_or(BusError::ChannelOverflow)?;
            let end = channel
                .checked_add(width)
                .ok_or(BusError::ChannelOverflow)?;
            let (key, name, file, coupling) = if width == 2 {
                (
                    format!("rec-{a}-{end}"),
                    format!("Rec {a}/{end}"),
                    format!("Rec {a}-{end}"),
                    Some(Coupling::Split),
                )
            } else {
                let right = definitions
                    .last()
                    .is_some_and(|b| b.coupling == Some(Coupling::Join));
                (
                    format!("rec-{a}"),
                    format!("Rec {a}"),
                    format!("Rec {a}"),
                    (!right && self.widths.get(index + 1) == Some(&1)).then_some(Coupling::Join),
                )
            };
            definitions.push(BusDef {
                key,
                name,
                file,
                channels: (channel..end).collect(),
                index,
                coupling,
            });
            channel = end;
        }
        Ok(definitions)
    }
    pub fn state(&self, key: &str) -> BusState {
        self.states.get(key).cloned().unwrap_or_default()
    }
    pub fn armed(&self) -> Result<Vec<BusDef>, BusError> {
        Ok(self
            .definitions()?
            .into_iter()
            .filter(|b| self.state(&b.key).armed)
            .collect())
    }
    pub fn arm(&self, key: &str, additive: bool) -> Result<Self, BusError> {
        let all = self.definitions()?;
        if !all.iter().any(|b| b.key == key) {
            return Err(BusError::InvalidIndex);
        }
        let only = self.armed()?.len() == 1 && self.state(key).armed;
        let mut next = self.clone();
        for bus in all {
            let mut state = self.state(&bus.key);
            state.armed = if additive {
                if bus.key == key {
                    !state.armed
                } else {
                    state.armed
                }
            } else {
                bus.key == key && !only
            };
            next.states.insert(bus.key, state);
        }
        Ok(next)
    }
    pub fn backend_config(&self) -> Result<Opdracht, BusError> {
        let definitions = self.definitions()?;
        let count = definitions.iter().map(|b| b.channels.len()).sum();
        let mut gain = vec![1.0; count];
        for bus in definitions {
            let state = self.state(&bus.key);
            if !state.db.is_finite() {
                return Err(BusError::NonFiniteGain);
            }
            let value = if state.mute {
                0.0
            } else {
                db_to_linear(state.db)
            };
            if !value.is_finite() {
                return Err(BusError::NonFiniteGain);
            }
            for channel in bus.channels {
                gain[channel] = value;
            }
        }
        Ok(Opdracht::ZetBus {
            kanalen: count,
            gain,
        })
    }
    fn rearrange(&self, widths: Vec<usize>, origins: Vec<Option<usize>>) -> Result<Self, BusError> {
        let old = self.definitions()?;
        let mut next = Self {
            widths,
            states: BTreeMap::new(),
            midi: self.midi,
        };
        for bus in next.definitions()? {
            if let Some(state) = origins
                .get(bus.index)
                .and_then(|i| *i)
                .and_then(|i| old.get(i))
                .and_then(|b| self.states.get(&b.key))
            {
                next.states.insert(bus.key, state.clone());
            }
        }
        Ok(next)
    }
    pub fn change_coupling(&self, index: usize) -> Result<Self, BusError> {
        self.definitions()?;
        let mut widths = self.widths.clone();
        let width = *widths.get(index).ok_or(BusError::InvalidIndex)?;
        let origins = if width == 2 {
            widths.splice(index..=index, [1, 1]);
            (0..widths.len())
                .map(|p| {
                    Some(if p <= index {
                        p
                    } else if p == index + 1 {
                        index
                    } else {
                        p - 1
                    })
                })
                .collect()
        } else if widths.get(index + 1) == Some(&1) {
            widths.splice(index..=index + 1, [2]);
            (0..widths.len())
                .map(|p| Some(if p <= index { p } else { p + 1 }))
                .collect()
        } else {
            return Ok(self.clone());
        };
        self.rearrange(widths, origins)
    }
    pub fn add(&self) -> Result<Self, BusError> {
        self.definitions()?;
        let mut next = self.clone();
        next.widths.push(2);
        Ok(next)
    }
    /// New links, old links removed, then channel resize. No command is sent.
    pub fn remove(&self, index: usize, graph: &Momentopname) -> Result<BusEdit, BusError> {
        let defs = self.definitions()?;
        let bus = defs.get(index).ok_or(BusError::InvalidIndex)?;
        if defs.len() == 1 {
            return Ok(BusEdit {
                config: self.clone(),
                commands: vec![],
            });
        }
        let first = bus.channels[0];
        let width = bus.channels.len();
        let mut commands = Vec::new();
        let mut remove = Vec::new();
        if let Some(node) = graph.knopen.iter().find(|n| graph.verbonden && is_bus(n)) {
            let inputs: Vec<_> = graph
                .poorten
                .iter()
                .filter(|p| p.knoop == node.id && p.ingang)
                .collect();
            for link in graph.kabels.iter().filter(|l| l.in_knoop == node.id) {
                let channel = inputs
                    .iter()
                    .find(|p| p.id == link.in_poort)
                    .and_then(|p| p.naam.strip_prefix("rec_"))
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| n.checked_sub(1));
                let Some(channel) = channel.filter(|c| *c >= first) else {
                    continue;
                };
                remove.push(link.id);
                if channel >= first + width {
                    if let Some(port) = inputs
                        .iter()
                        .find(|p| p.naam == format!("rec_{}", channel - width + 1))
                    {
                        commands.push(Opdracht::Verbind {
                            uit: link.uit_poort,
                            naar: port.id,
                        });
                    }
                }
            }
        }
        commands.extend(
            remove
                .into_iter()
                .map(|kabel| Opdracht::Ontkoppel { kabel }),
        );
        let widths = self
            .widths
            .iter()
            .enumerate()
            .filter_map(|(p, &n)| (p != index).then_some(n))
            .collect::<Vec<_>>();
        let origins = (0..widths.len())
            .map(|p| Some(if p < index { p } else { p + 1 }))
            .collect();
        let config = self.rearrange(widths, origins)?;
        commands.push(config.backend_config()?);
        Ok(BusEdit { config, commands })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct BusEdit {
    pub config: BusConfig,
    pub commands: Vec<Opdracht>,
}
