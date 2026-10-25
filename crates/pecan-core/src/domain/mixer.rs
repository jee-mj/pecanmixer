//! Audio strip projection and declarative routing plans from mixer.ts.
use super::{
    audio::{app_name, device_title, is_bus, label_cmp, numbered, port_label},
    buses::BusDef,
    levels::{db_to_linear, OFF_DB},
};
use crate::graph::{Kabel, Knoop, Momentopname, Opdracht, Poort, Soort};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Row {
    Apps,
    Hardware,
    Destinations,
}
impl Row {
    fn key(self) -> &'static str {
        match self {
            Self::Apps => "apps",
            Self::Hardware => "hardware",
            Self::Destinations => "bestemmingen",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strip {
    pub key: String,
    pub node: Knoop,
    pub ports: Vec<Poort>,
    pub name: String,
    pub subtitle: String,
    pub row: Row,
    pub channels: Vec<usize>,
    pub targetable: bool,
    pub bus: Option<BusDef>,
    pub silent: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RouteState {
    Off,
    On { manual: bool },
    Custom { explanation: String },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub state: RouteState,
    pub links: Vec<Kabel>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualRoutes {
    pub links: Vec<u32>,
    pub targets: Vec<u32>,
    pub restore: Vec<[u32; 2]>,
    pub detached: usize,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mixer {
    pub apps: Vec<Strip>,
    pub hardware: Vec<Strip>,
    pub destinations: Vec<Strip>,
    pub manual: ManualRoutes,
}

fn strip(
    node: &Knoop,
    ports: Vec<Poort>,
    channels: Vec<usize>,
    row: Row,
    name: String,
    subtitle: String,
    targetable: bool,
    silent: bool,
) -> Strip {
    let key = format!(
        "{}|{}|{}",
        node.naam,
        row.key(),
        ports
            .iter()
            .map(|p| p.naam.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    Strip {
        key,
        node: node.clone(),
        ports,
        name,
        subtitle,
        row,
        channels,
        targetable,
        bus: None,
        silent,
    }
}
fn split(node: &Knoop, ports: Vec<Poort>, row: Row, silent: bool) -> Vec<Strip> {
    if ports.is_empty() {
        return vec![];
    }
    let title = device_title(node);
    let numbered = ports.len() > 2
        && ports.iter().all(|p| {
            p.kanaal
                .as_deref()
                .and_then(|c| numbered(c, "AUX"))
                .is_some()
        });
    if !numbered {
        let channels = (0..ports.len()).collect();
        return vec![strip(
            node,
            ports,
            channels,
            row,
            title,
            if row == Row::Hardware {
                "capture"
            } else {
                "playback"
            }
            .into(),
            true,
            silent,
        )];
    }
    ports
        .chunks(2)
        .enumerate()
        .map(|(pair, ports)| {
            let numbers = ports
                .iter()
                .map(|p| {
                    p.kanaal
                        .as_deref()
                        .and_then(|c| numbered_channel(c))
                        .unwrap_or(0)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("/");
            strip(
                node,
                ports.to_vec(),
                (pair * 2..pair * 2 + ports.len()).collect(),
                row,
                format!(
                    "{} {numbers}",
                    if row == Row::Hardware { "In" } else { "Out" }
                ),
                title.clone(),
                pair == 0,
                silent,
            )
        })
        .collect()
}
fn numbered_channel(channel: &str) -> Option<usize> {
    numbered(channel, "AUX")?.checked_add(1)
}
fn output_stream(node: &Knoop) -> bool {
    node.klasse
        .as_deref()
        .unwrap_or("")
        .starts_with("Stream/Output")
}
fn input_stream(node: &Knoop) -> bool {
    node.klasse
        .as_deref()
        .unwrap_or("")
        .starts_with("Stream/Input")
        && !is_bus(node)
}
fn audio(graph: &Momentopname) -> Vec<&Poort> {
    graph
        .poorten
        .iter()
        .filter(|p| p.soort == Soort::Audio && !p.monitor)
        .collect()
}
fn audio_links(graph: &Momentopname) -> Vec<&Kabel> {
    let ports: BTreeSet<_> = audio(graph).iter().map(|p| p.id).collect();
    graph
        .kabels
        .iter()
        .filter(|l| ports.contains(&l.uit_poort) && ports.contains(&l.in_poort))
        .collect()
}

impl Mixer {
    pub fn build(graph: &Momentopname, buses: &[BusDef]) -> Self {
        if !graph.verbonden {
            return Self::default();
        }
        let audio = audio(graph);
        let mut model = Self::default();
        for node in &graph.knopen {
            let ports: Vec<_> = audio
                .iter()
                .copied()
                .filter(|p| p.knoop == node.id)
                .collect();
            if ports.is_empty() {
                continue;
            }
            let class = node.klasse.as_deref().unwrap_or("");
            let silent = node.stand.as_deref() != Some("running");
            let mut outputs: Vec<_> = ports
                .iter()
                .filter(|p| !p.ingang)
                .map(|p| (*p).clone())
                .collect();
            let mut inputs: Vec<_> = ports
                .iter()
                .filter(|p| p.ingang)
                .map(|p| (*p).clone())
                .collect();
            // JS sort is stable when sort keys tie; retain input order on ties.
            outputs.sort_by_key(|p| p.volgnummer.unwrap_or(p.id));
            inputs.sort_by_key(|p| p.volgnummer.unwrap_or(p.id));
            if is_bus(node) {
                for bus in buses {
                    let selected: Option<Vec<_>> = bus
                        .channels
                        .iter()
                        .map(|c| {
                            inputs
                                .iter()
                                .find(|p| p.naam == format!("rec_{}", c + 1))
                                .cloned()
                        })
                        .collect();
                    let Some(selected) = selected else { continue };
                    let mut item = strip(
                        node,
                        selected,
                        bus.channels.clone(),
                        Row::Destinations,
                        bus.name.clone(),
                        "Pecan Mixer".into(),
                        false,
                        false,
                    );
                    item.key = format!("bus|{}", bus.key);
                    item.bus = Some(bus.clone());
                    model.destinations.push(item);
                }
            } else if class.starts_with("Stream/") {
                let name = app_name(node);
                if !outputs.is_empty() {
                    let subtitle = node
                        .media
                        .as_ref()
                        .filter(|m| !m.is_empty() && **m != name)
                        .cloned()
                        .unwrap_or_else(|| if silent { "silent" } else { "stereo" }.into());
                    let channels = (0..outputs.len()).collect();
                    model.apps.push(strip(
                        node,
                        outputs,
                        channels,
                        Row::Apps,
                        name.clone(),
                        subtitle,
                        true,
                        silent,
                    ));
                }
                if !inputs.is_empty() {
                    let channels = (0..inputs.len()).collect();
                    model.destinations.push(strip(
                        node,
                        inputs,
                        channels,
                        Row::Destinations,
                        name,
                        format!("app · {}", node.media.as_deref().unwrap_or("in")),
                        true,
                        silent,
                    ));
                }
            } else if class.starts_with("Audio/") {
                model
                    .hardware
                    .extend(split(node, outputs, Row::Hardware, silent));
                model
                    .destinations
                    .extend(split(node, inputs, Row::Destinations, silent));
            }
        }
        let by_name =
            |a: &Strip, b: &Strip| label_cmp(&a.name, &b.name).then(a.node.id.cmp(&b.node.id));
        let hardware = |a: &Strip, b: &Strip| {
            label_cmp(&a.subtitle, &b.subtitle)
                .then(a.node.id.cmp(&b.node.id))
                .then(a.channels.first().cmp(&b.channels.first()))
        };
        model.apps.sort_by(by_name);
        model.hardware.sort_by(hardware);
        let kind = |s: &Strip| {
            if is_bus(&s.node) {
                1
            } else if s
                .node
                .klasse
                .as_deref()
                .unwrap_or("")
                .starts_with("Stream/")
            {
                2
            } else {
                0
            }
        };
        model.destinations.sort_by(|a, b| {
            let (x, y) = (kind(a), kind(b));
            if x == 1 && y == 1 {
                return a.channels.first().cmp(&b.channels.first());
            }
            x.cmp(&y)
                .then_with(|| if x > 0 { by_name(a, b) } else { hardware(a, b) })
        });
        let mut seen = BTreeMap::<String, usize>::new();
        for item in model
            .apps
            .iter_mut()
            .chain(&mut model.hardware)
            .chain(&mut model.destinations)
        {
            let count = seen.entry(item.key.clone()).or_default();
            if *count > 0 {
                item.key = format!("{}#{count}", item.key)
            }
            *count += 1;
        }
        let bus_ids: BTreeSet<_> = graph
            .knopen
            .iter()
            .filter(|n| is_bus(n))
            .map(|n| n.id)
            .collect();
        let links = audio_links(graph);
        model.manual.links = links
            .iter()
            .filter(|l| !bus_ids.contains(&l.in_knoop) && !model.is_auto(graph, l))
            .map(|l| l.id)
            .collect();
        let streams: Vec<_> = graph
            .knopen
            .iter()
            .filter(|n| {
                (output_stream(n) || input_stream(n)) && audio.iter().any(|p| p.knoop == n.id)
            })
            .collect();
        model.manual.targets = streams
            .iter()
            .filter(|n| n.doel.is_some())
            .map(|n| n.id)
            .collect();
        for node in streams {
            if node.doel.is_some() {
                continue;
            }
            let output = output_stream(node);
            let remains = links.iter().any(|l| {
                !model.manual.links.contains(&l.id)
                    && !bus_ids.contains(&l.in_knoop)
                    && (if output { l.uit_knoop } else { l.in_knoop }) == node.id
            });
            if remains {
                continue;
            }
            let Some(target) = model.wp_strip(graph, node) else {
                continue;
            };
            let own = if output {
                model.apps.iter().find(|s| s.node.id == node.id)
            } else {
                model.destinations.iter().find(|s| s.node.id == node.id)
            };
            let Some(own) = own else { continue };
            let pairs = if output {
                expected_pairs(own, target)
            } else {
                expected_pairs(target, own)
            };
            model.manual.restore.extend(pairs);
            model.manual.detached += 1;
        }
        model
    }
    pub fn source(&self, key: &str) -> Option<&Strip> {
        self.apps
            .iter()
            .chain(&self.hardware)
            .find(|s| s.key == key)
    }
    pub fn destination(&self, key: &str) -> Option<&Strip> {
        self.destinations.iter().find(|s| s.key == key)
    }
    pub fn wp_strip(&self, graph: &Momentopname, stream: &Knoop) -> Option<&Strip> {
        let output = output_stream(stream);
        let target = if let Some(target) = stream.doel.as_ref().filter(|s| !s.is_empty()) {
            graph.knopen.iter().find(|n| {
                n.serial.map(|s| s.to_string()).as_ref() == Some(target) || &n.naam == target
            })
        } else {
            let name = if output {
                &graph.standaard_sink
            } else {
                &graph.standaard_bron
            };
            graph
                .knopen
                .iter()
                .find(|n| name.as_deref() == Some(&n.naam))
        }?;
        (if output {
            &self.destinations
        } else {
            &self.hardware
        })
        .iter()
        .find(|s| s.node.id == target.id && s.targetable)
    }
    fn is_auto(&self, graph: &Momentopname, link: &Kabel) -> bool {
        let Some(from) = graph.knopen.iter().find(|n| n.id == link.uit_knoop) else {
            return false;
        };
        let Some(to) = graph.knopen.iter().find(|n| n.id == link.in_knoop) else {
            return false;
        };
        if from.doel.is_some() || to.doel.is_some() {
            return false;
        }
        if output_stream(from) {
            return self
                .wp_strip(graph, from)
                .is_some_and(|s| s.ports.iter().any(|p| p.id == link.in_poort));
        }
        if input_stream(to) {
            return self
                .wp_strip(graph, to)
                .is_some_and(|s| s.ports.iter().any(|p| p.id == link.uit_poort));
        }
        link.client.is_some_and(|c| graph.wireplumber.contains(&c))
    }
    pub fn route(&self, graph: &Momentopname, source: &Strip, destination: &Strip) -> Route {
        let links: Vec<_> = audio_links(graph)
            .into_iter()
            .filter(|l| {
                source.ports.iter().any(|p| p.id == l.uit_poort)
                    && destination.ports.iter().any(|p| p.id == l.in_poort)
            })
            .cloned()
            .collect();
        if links.is_empty() {
            return Route {
                state: RouteState::Off,
                links,
            };
        }
        let expected = expected_pairs(source, destination);
        let same = links.len() == expected.len()
            && expected
                .iter()
                .all(|[s, d]| links.iter().any(|l| l.uit_poort == *s && l.in_poort == *d));
        let state = if same {
            RouteState::On {
                manual: !is_bus(&destination.node) && links.iter().any(|l| !self.is_auto(graph, l)),
            }
        } else {
            let label = |id| {
                graph
                    .poorten
                    .iter()
                    .find(|p| p.id == id)
                    .map_or("?".into(), port_label)
            };
            let description = links
                .iter()
                .map(|l| format!("{} → {}", label(l.uit_poort), label(l.in_poort)))
                .collect::<Vec<_>>()
                .join(", ");
            RouteState::Custom {
                explanation: if links.len() == 1 {
                    format!("only {description}")
                } else {
                    description
                },
            }
        };
        Route { state, links }
    }
    /// Green-button behavior. Custom routes remain the patchbay's responsibility.
    pub fn plan_toggle(
        &self,
        graph: &Momentopname,
        source: &Strip,
        destination: &Strip,
    ) -> Vec<Opdracht> {
        if !graph.verbonden {
            return vec![];
        }
        let route = self.route(graph, source, destination);
        match route.state {
            RouteState::Custom { .. } => return vec![],
            RouteState::On { .. } => {
                return route
                    .links
                    .into_iter()
                    .map(|l| Opdracht::Ontkoppel { kabel: l.id })
                    .collect()
            }
            RouteState::Off => {}
        }
        let bus_ids: BTreeSet<_> = graph
            .knopen
            .iter()
            .filter(|n| is_bus(n))
            .map(|n| n.id)
            .collect();
        let ports = |node: &Knoop, input: bool| {
            graph
                .poorten
                .iter()
                .filter(move |p| p.knoop == node.id && p.ingang == input && !p.monitor)
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
        };
        if output_stream(&source.node) {
            let outputs = ports(&source.node, false);
            let detached = !graph
                .kabels
                .iter()
                .any(|l| outputs.contains(&l.uit_poort) && !bus_ids.contains(&l.in_knoop));
            if detached
                && self.wp_strip(graph, &source.node).map(|s| &s.key) != Some(&destination.key)
                && destination.targetable
            {
                if let Some(serial) = destination.node.serial {
                    return vec![Opdracht::ZetDoel {
                        knoop: source.node.id,
                        doel: Some(serial.to_string()),
                    }];
                }
            }
        }
        if input_stream(&destination.node) {
            let inputs = ports(&destination.node, true);
            let detached = !graph.kabels.iter().any(|l| inputs.contains(&l.in_poort));
            if detached
                && self.wp_strip(graph, &destination.node).map(|s| &s.key) != Some(&source.key)
                && source.targetable
            {
                if let Some(serial) = source.node.serial {
                    return vec![Opdracht::ZetDoel {
                        knoop: destination.node.id,
                        doel: Some(serial.to_string()),
                    }];
                }
            }
        }
        expected_pairs(source, destination)
            .into_iter()
            .map(|[uit, naar]| Opdracht::Verbind { uit, naar })
            .collect()
    }
    pub fn manual_count(&self) -> usize {
        self.manual.links.len() + self.manual.targets.len() + self.manual.detached
    }
    pub fn plan_reset_auto(&self) -> Vec<Opdracht> {
        self.manual
            .links
            .iter()
            .map(|kabel| Opdracht::Ontkoppel { kabel: *kabel })
            .chain(self.manual.targets.iter().map(|knoop| Opdracht::ZetDoel {
                knoop: *knoop,
                doel: None,
            }))
            .chain(
                self.manual
                    .restore
                    .iter()
                    .map(|[uit, naar]| Opdracht::Verbind {
                        uit: *uit,
                        naar: *naar,
                    }),
            )
            .collect()
    }
}
/// Mono fan-in/out, then channel names (including matching absent names, as JS
/// does), falling back to driver order. Extra source channels are left alone.
pub fn expected_pairs(source: &Strip, destination: &Strip) -> Vec<[u32; 2]> {
    if source.ports.len() == 1 {
        return destination
            .ports
            .iter()
            .map(|p| [source.ports[0].id, p.id])
            .collect();
    }
    if destination.ports.len() == 1 {
        return source
            .ports
            .iter()
            .map(|p| [p.id, destination.ports[0].id])
            .collect();
    }
    source
        .ports
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            destination
                .ports
                .iter()
                .find(|q| q.kanaal == p.kanaal)
                .or_else(|| destination.ports.get(i))
                .map(|q| [p.id, q.id])
        })
        .collect()
}
pub fn strip_db(strip: &Strip) -> Option<f64> {
    let volume = strip.node.volume.as_ref()?;
    let values: Vec<_> = strip
        .channels
        .iter()
        .filter_map(|i| volume.get(*i))
        .copied()
        .collect();
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64;
    Some(if mean <= 0.0 {
        OFF_DB
    } else {
        (20.0 * mean.log10()).max(OFF_DB)
    })
}
pub fn with_db(strip: &Strip, db: f64) -> Option<Vec<f32>> {
    if !db.is_finite() {
        return None;
    }
    let mut volume = strip.node.volume.clone()?;
    let linear = db_to_linear(db);
    if !linear.is_finite() {
        return None;
    }
    for channel in &strip.channels {
        if let Some(value) = volume.get_mut(*channel) {
            *value = linear
        }
    }
    Some(volume)
}
