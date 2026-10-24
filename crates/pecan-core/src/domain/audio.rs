//! Supported audio grouping/labels from indeling.ts. MIDI remains unported.
use crate::graph::{Knoop, Momentopname, Poort, Soort};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub const REC_NODE: &str = "pecanmixer-opname";
pub fn is_bus(node: &Knoop) -> bool {
    node.naam == REC_NODE
}
pub fn app_name(node: &Knoop) -> String {
    if is_bus(node) {
        "Rec bus".into()
    } else {
        node.app
            .as_ref()
            .or(node.beschrijving.as_ref())
            .unwrap_or(&node.naam)
            .clone()
    }
}
pub fn device_title(node: &Knoop) -> String {
    let description = node.beschrijving.as_ref();
    if let Some(description) =
        description.filter(|s| s.encode_utf16().count() <= 28 && !s.is_empty())
    {
        return description.clone();
    }
    node.bijnaam
        .as_ref()
        .or(description)
        .unwrap_or(&node.naam)
        .clone()
}
pub fn numbered(value: &str, prefix: &str) -> Option<usize> {
    let digits = value.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
pub fn port_label(port: &Poort) -> String {
    if let Some(digits) = port.naam.strip_prefix("rec_") {
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return format!("Rec {digits}");
        }
    }
    if let Some(n) = port
        .kanaal
        .as_deref()
        .and_then(|c| numbered(c, "AUX"))
        .and_then(|n| n.checked_add(1))
    {
        return format!("{} {n}", if port.ingang { "Out" } else { "In" });
    }
    if let Some(channel) = &port.kanaal {
        return channel.clone();
    }
    for prefix in ["output_", "input_", "playback_", "capture_"] {
        if let Some(name) = port.naam.strip_prefix(prefix) {
            return name.into();
        }
    }
    port.naam.clone()
}
/// Locale-independent label ordering. The browser uses localeCompare; exact
/// locale-sensitive Unicode collation is a documented, unverified parity gap.
pub(crate) fn label_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRow {
    pub id: u32,
    pub label: String,
    pub connected: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioModule {
    pub key: String,
    pub side: Side,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub silent: bool,
    pub node: u32,
    pub meter_channels: Vec<Vec<usize>>,
    pub ports: Vec<PortRow>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub side: Side,
    pub title: String,
    pub modules: Vec<AudioModule>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibleLink {
    pub id: u32,
    pub from: u32,
    pub to: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioLayout {
    pub groups: Vec<Group>,
    pub links: Vec<VisibleLink>,
    pub hidden: Vec<String>,
}
const GROUPS: [&str; 8] = [
    "Apps · playing",
    "Hardware in",
    "Virtual in",
    "Other · out",
    "Hardware out",
    "Virtual out",
    "Apps · recording",
    "Other · in",
];
fn bars(n: usize) -> Vec<Vec<usize>> {
    if n < 2 {
        vec![(0..n).collect()]
    } else {
        vec![(0..n).step_by(2).collect(), (1..n).step_by(2).collect()]
    }
}
fn app_icon(title: &str, input: bool) -> String {
    let title = title.to_lowercase();
    let icon = if title == "opname" {
        "opname"
    } else if [
        "discord", "teams", "zoom", "slack", "signal", "webrtc", "skype",
    ]
    .iter()
    .any(|n| title.contains(n))
    {
        "praat"
    } else if [
        "firefox",
        "chrom",
        "brave",
        "vivaldi",
        "opera",
        "edge",
        "zen",
        "librewolf",
    ]
    .iter()
    .any(|n| title.contains(n))
    {
        "wereld"
    } else if [
        "mpv",
        "vlc",
        "totem",
        "celluloid",
        "haruna",
        "smplayer",
        "kodi",
    ]
    .iter()
    .any(|n| title.contains(n))
    {
        "speel"
    } else if input {
        "opname"
    } else {
        "noot"
    };
    icon.into()
}

pub fn layout(graph: &Momentopname) -> AudioLayout {
    if !graph.verbonden {
        return AudioLayout::default();
    }
    let connected: BTreeSet<_> = graph
        .kabels
        .iter()
        .flat_map(|l| [l.uit_poort, l.in_poort])
        .collect();
    // Map insertion follows first port occurrence, matching the JS Map. This
    // preserves duplicate-key suffix assignment even before display sorting.
    let mut nodes = Vec::new();
    for port in graph.poorten.iter().filter(|p| p.soort == Soort::Audio) {
        if !nodes.contains(&port.knoop) {
            nodes.push(port.knoop)
        }
    }
    let mut entries: Vec<(usize, AudioModule)> = Vec::new();
    let mut hidden = Vec::new();
    let mut visible = BTreeSet::new();
    let mut seen = BTreeMap::<String, usize>::new();
    for id in nodes {
        let Some(node) = graph.knopen.iter().find(|n| n.id == id) else {
            continue;
        };
        let ports: Vec<_> = graph
            .poorten
            .iter()
            .filter(|p| p.knoop == id && p.soort == Soort::Audio)
            .collect();
        if ports.iter().any(|p| p.monitor) && hidden.is_empty() {
            hidden.push("monitors".into())
        }
        let class = node.klasse.as_deref().unwrap_or("");
        let stream = class.starts_with("Stream/");
        let device = class.starts_with("Audio/");
        let physical = node.apparaat.is_some() || ports.iter().any(|p| p.fysiek);
        let silent = node.stand.as_deref() != Some("running");
        for input in [false, true] {
            let mut selected: Vec<_> = ports
                .iter()
                .copied()
                .filter(|p| p.ingang == input && (input || !p.monitor))
                .collect();
            if selected.is_empty() {
                continue;
            }
            selected.sort_by_key(|p| (p.volgnummer.unwrap_or(p.id), p.id));
            let (group, title, subtitle, icon, meter) = if stream {
                let title = app_name(node);
                let state = match (input, silent) {
                    (true, true) => "idle",
                    (true, false) => "recording",
                    (false, true) => "silent",
                    (false, false) => "playing",
                };
                let subtitle = node
                    .media
                    .as_deref()
                    .filter(|m| !m.is_empty() && *m != title)
                    .map_or_else(|| state.into(), |m| format!("{state} · {m}"));
                let icon = app_icon(
                    if is_bus(node) { "opname" } else { &title },
                    class.starts_with("Stream/Input"),
                );
                (if input { 6 } else { 0 }, title, subtitle, icon, !input)
            } else {
                let group = match (input, device, physical) {
                    (false, false, _) => 3,
                    (true, false, _) => 7,
                    (false, true, true) => 1,
                    (false, true, false) => 2,
                    (true, true, true) => 4,
                    (true, true, false) => 5,
                };
                let default = if input {
                    &graph.standaard_sink
                } else {
                    &graph.standaard_bron
                };
                let subtitle = format!(
                    "{}{}",
                    if input { "playback" } else { "capture" },
                    if default.as_deref() == Some(&node.naam) {
                        " · default"
                    } else {
                        ""
                    }
                );
                let name = node
                    .beschrijving
                    .as_ref()
                    .unwrap_or(&node.naam)
                    .to_lowercase();
                (
                    group,
                    device_title(node),
                    subtitle,
                    if name.contains("hdmi") || name.contains("displayport") {
                        "scherm".into()
                    } else {
                        "apparaat".into()
                    },
                    device,
                )
            };
            let key = format!("{}{}", node.naam, if input { "<" } else { ">" });
            let count = seen.entry(key.clone()).or_default();
            let key = if *count > 0 {
                format!("{key}#{count}")
            } else {
                key
            };
            *count += 1;
            let meter_channels = if meter { bars(selected.len()) } else { vec![] };
            let rows = selected
                .iter()
                .map(|p| {
                    visible.insert(p.id);
                    PortRow {
                        id: p.id,
                        label: port_label(p),
                        connected: connected.contains(&p.id),
                    }
                })
                .collect();
            entries.push((
                group,
                AudioModule {
                    key,
                    side: if input { Side::Right } else { Side::Left },
                    title,
                    subtitle,
                    icon,
                    silent,
                    node: id,
                    meter_channels,
                    ports: rows,
                },
            ));
        }
    }
    let mut groups = Vec::new();
    for (index, title) in GROUPS.iter().enumerate() {
        let mut modules: Vec<_> = entries
            .iter()
            .filter(|(g, _)| *g == index)
            .map(|(_, m)| m.clone())
            .collect();
        if modules.is_empty() {
            continue;
        }
        modules
            .sort_by(|a, b| label_cmp(&a.title, &b.title).then_with(|| label_cmp(&a.key, &b.key)));
        groups.push(Group {
            side: modules[0].side,
            title: (*title).into(),
            modules,
        });
    }
    let links = graph
        .kabels
        .iter()
        .filter(|l| visible.contains(&l.uit_poort) && visible.contains(&l.in_poort))
        .map(|l| VisibleLink {
            id: l.id,
            from: l.uit_poort,
            to: l.in_poort,
        })
        .collect();
    AudioLayout {
        groups,
        links,
        hidden,
    }
}
