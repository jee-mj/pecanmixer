//! Deterministic display geometry, not routing or device-grouping policy.
//! Node/port order and labels come from the core overview. Explicit DTO port
//! direction is the only partition: outputs on the left, inputs on the right.
use std::collections::BTreeMap;

use pecan_core::{graph::Kabel, overview::Overview};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Metrics {
    pub line: f64,
}
impl Default for Metrics {
    fn default() -> Self {
        Self { line: 20.0 }
    }
}
impl Metrics {
    pub fn row(self) -> f64 {
        self.line + 8.0
    }
    pub fn header(self) -> f64 {
        2.0 * self.line + 13.0
    }
}
pub(super) const WIDTH: f64 = 256.0;
pub(super) const LANE: f64 = 192.0;
pub(super) const MARGIN: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Module {
    pub node: u32,
    pub input: bool,
    pub origin: Point,
    pub height: f64,
    pub ports: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Cable {
    pub id: u32,
    pub from: Point,
    pub to: Point,
    pub control_from: Point,
    pub control_to: Point,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Port {
    pub id: u32,
    pub input: bool,
    pub point: Point,
    left: f64,
    top: f64,
    height: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Layout {
    pub modules: Vec<Module>,
    pub cables: Vec<Cable>,
    pub ports: Vec<Port>,
    pub undrawn: usize,
    pub width: f64,
    pub height: f64,
}
impl Layout {
    pub fn build(overview: &Overview, links: &[Kabel], metrics: Metrics) -> Self {
        let mut layout = Self {
            width: 2.0 * MARGIN + 2.0 * WIDTH + LANE,
            height: 2.0 * MARGIN,
            ..Default::default()
        };
        if !overview.connected {
            return layout;
        }
        let mut endpoints = BTreeMap::new();
        for input in [false, true] {
            let mut y = MARGIN;
            let x = MARGIN + if input { WIDTH + LANE } else { 0.0 };
            for node in &overview.nodes {
                let ports: Vec<_> = node.ports.iter().filter(|p| p.input == input).collect();
                if ports.is_empty() {
                    continue;
                }
                for (row, port) in ports.iter().enumerate() {
                    let point = Point {
                        x: x + if input { 0.0 } else { WIDTH },
                        y: y + metrics.header() + metrics.row() * (row as f64 + 0.5),
                    };
                    layout.ports.push(Port {
                        id: port.id,
                        input,
                        point,
                        left: x,
                        top: point.y - metrics.row() * 0.5,
                        height: metrics.row(),
                    });
                    endpoints.insert(port.id, (node.id, input, point));
                }
                let height = metrics.header() + metrics.row() * ports.len() as f64 + 13.0;
                layout.modules.push(Module {
                    node: node.id,
                    input,
                    origin: Point { x, y },
                    height,
                    ports: ports.iter().map(|p| p.id).collect(),
                });
                y += height + MARGIN;
            }
            layout.height = layout.height.max(y);
        }
        let mut ordered: Vec<_> = links.iter().collect();
        ordered.sort_by_key(|link| link.id);
        for link in ordered {
            let from = endpoints.get(&link.uit_poort);
            let to = endpoints.get(&link.in_poort);
            match (from, to) {
                (Some((owner_from, false, from)), Some((owner_to, true, to)))
                    if *owner_from == link.uit_knoop && *owner_to == link.in_knoop =>
                {
                    layout.cables.push(Cable {
                        id: link.id,
                        from: *from,
                        to: *to,
                        control_from: Point {
                            x: from.x + LANE * 0.5,
                            y: from.y,
                        },
                        control_to: Point {
                            x: to.x - LANE * 0.5,
                            y: to.y,
                        },
                    });
                }
                _ => layout.undrawn += 1,
            }
        }
        layout
    }
    pub fn port_at(&self, point: Point) -> Option<&Port> {
        self.ports.iter().find(|port| {
            point.x >= port.left
                && point.x <= port.left + WIDTH
                && point.y >= port.top
                && point.y < port.top + port.height
        })
    }
    pub fn cable_at(&self, point: Point) -> Option<u32> {
        // Fixed work per cable; reverse paint order resolves overlapping paths.
        self.cables.iter().rev().find_map(|cable| {
            let mut previous = cable.from;
            for step in 1..=48 {
                let next = cable.point(f64::from(step) / 48.0);
                let dx = next.x - previous.x;
                let dy = next.y - previous.y;
                let length = dx * dx + dy * dy;
                let t = if length == 0.0 {
                    0.0
                } else {
                    ((point.x - previous.x) * dx + (point.y - previous.y) * dy) / length
                }
                .clamp(0.0, 1.0);
                if (point.x - previous.x - t * dx).hypot(point.y - previous.y - t * dy) <= 8.0 {
                    return Some(cable.id);
                }
                previous = next;
            }
            None
        })
    }
}
impl Cable {
    fn point(&self, t: f64) -> Point {
        let a = 1.0 - t;
        let coordinate = |p0, p1, p2, p3| {
            a * a * a * p0 + 3.0 * a * a * t * p1 + 3.0 * a * t * t * p2 + t * t * t * p3
        };
        Point {
            x: coordinate(
                self.from.x,
                self.control_from.x,
                self.control_to.x,
                self.to.x,
            ),
            y: coordinate(
                self.from.y,
                self.control_from.y,
                self.control_to.y,
                self.to.y,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pecan_core::{graph::Momentopname, overview::project};
    fn graph() -> Momentopname {
        serde_json::from_str(include_str!(
            "../../../../../fixtures/graph/audio-basic.json"
        ))
        .unwrap()
    }
    #[test]
    fn geometry_is_stable_and_cables_join_exact_port_row_centers() {
        let graph = graph();
        let view = project(&graph);
        let metrics = Metrics::default();
        let layout = Layout::build(&view, &graph.kabels, metrics);
        assert_eq!(layout.modules.len(), 3);
        assert_eq!(layout.cables.len(), 2);
        assert_eq!(layout.undrawn, 0);
        assert_eq!(layout.modules[0].node, 20);
        assert!(!layout.modules[0].input);
        assert_eq!(layout.modules[1].node, 10);
        assert!(layout.modules[1].input);
        assert_eq!(
            layout.cables[0].from,
            Point {
                x: MARGIN + WIDTH,
                y: MARGIN + metrics.header() + metrics.row() / 2.0
            }
        );
        assert_eq!(layout.cables[0].to.y, layout.cables[0].from.y);
        assert_eq!(
            layout.cables[1].from.y - layout.cables[0].from.y,
            metrics.row()
        );
        let mut shuffled = graph.clone();
        shuffled.knopen.reverse();
        shuffled.poorten.reverse();
        shuffled.kabels.reverse();
        assert_eq!(
            layout,
            Layout::build(&project(&shuffled), &shuffled.kabels, metrics)
        );
        for input in [false, true] {
            let modules: Vec<_> = layout.modules.iter().filter(|m| m.input == input).collect();
            for pair in modules.windows(2) {
                assert!(pair[0].origin.y + pair[0].height < pair[1].origin.y);
            }
        }
    }
    #[test]
    fn missing_or_inconsistent_endpoints_are_counted_and_disconnect_clears_geometry() {
        let mut graph = graph();
        graph.kabels[0].in_poort = 9999;
        graph.kabels[1].uit_knoop = 9999;
        let layout = Layout::build(&project(&graph), &graph.kabels, Metrics::default());
        assert!(layout.cables.is_empty());
        assert_eq!(layout.undrawn, 2);
        graph.verbonden = false;
        let layout = Layout::build(&project(&graph), &graph.kabels, Metrics::default());
        assert!(layout.modules.is_empty() && layout.cables.is_empty());
        assert_eq!(layout.undrawn, 0);
    }
    #[test]
    fn larger_font_metrics_and_many_ports_remain_finite_within_canvas() {
        let mut view = project(&graph());
        let template = view.nodes[0].clone();
        view.nodes = (0..120)
            .map(|id| {
                let mut node = template.clone();
                node.id = 1000 + id;
                for port in &mut node.ports {
                    port.id += id * 10;
                }
                node
            })
            .collect();
        let layout = Layout::build(&view, &[], Metrics { line: 40.0 });
        assert_eq!(layout.modules.len(), 120);
        for module in &layout.modules {
            assert!(module.origin.x.is_finite() && module.origin.y.is_finite());
            assert!(module.origin.x + WIDTH <= layout.width);
            assert!(module.origin.y + module.height <= layout.height);
        }
        assert!(layout.height > 10_000.0);
    }

    #[test]
    fn hit_regions_follow_port_rows_and_cable_geometry_at_larger_metrics() {
        let graph = graph();
        for line in [20.0, 36.0] {
            let layout = Layout::build(&project(&graph), &graph.kabels, Metrics { line });
            for port in &layout.ports {
                let p = Point {
                    x: port.point.x + if port.input { 12.0 } else { -12.0 },
                    y: port.point.y,
                };
                assert_eq!(layout.port_at(p).map(|p| p.id), Some(port.id));
                assert!(layout
                    .port_at(Point {
                        x: p.x,
                        y: port.top - 0.1
                    })
                    .is_none_or(|p| p.id != port.id));
            }
            for cable in &layout.cables {
                assert_eq!(layout.cable_at(cable.point(0.5)), Some(cable.id));
            }
            assert!(layout
                .port_at(Point {
                    x: f64::NAN,
                    y: 0.0
                })
                .is_none());
            assert!(layout
                .cable_at(Point {
                    x: 350.0,
                    y: layout.height + 20.0
                })
                .is_none());
        }
    }
    #[test]
    fn overlapping_cable_hit_order_matches_the_last_painted_curve() {
        let graph = graph();
        let mut layout = Layout::build(&project(&graph), &graph.kabels, Metrics::default());
        let mut overlap = layout.cables[0].clone();
        overlap.id = 900;
        let point = overlap.point(0.5);
        layout.cables.push(overlap);
        assert_eq!(layout.cable_at(point), Some(900));
    }
}
