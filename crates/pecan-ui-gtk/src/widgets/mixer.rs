use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
};

use gtk::prelude::*;

use super::accessibility::set_name;
use pecan_core::{backend::Notice, overview::Overview};

use super::strip::NodeStrip;

/// Read-only mixer-oriented shell, bound only to the pure core overview.
/// Retains node widgets by ID so ordinary graph/rate updates preserve expansion
/// and keyboard focus. Reconnect/disconnect models remove obsolete widgets.
pub struct MixerOverview {
    root: gtk::Box,
    status: gtk::Label,
    statistics: gtk::Label,
    notices: gtk::Label,
    empty: gtk::Box,
    empty_title: gtk::Label,
    empty_detail: gtk::Label,
    scroll: gtk::ScrolledWindow,
    strips: gtk::FlowBox,
    nodes: BTreeMap<u32, NodeStrip>,
    order: Vec<u32>,
    positions: Rc<RefCell<HashMap<gtk::FlowBoxChild, usize>>>,
}

impl Default for MixerOverview {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerOverview {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.set_margin_top(16);
        root.set_margin_bottom(16);
        root.set_margin_start(16);
        root.set_margin_end(16);
        let title = label("Pecan Mixer", false);
        title.add_css_class("title-1");
        let subtitle = label("Read-only GTK preview · observation only", true);
        let status = status_label("Disconnected · waiting for PipeWire");
        status.set_selectable(true);
        let statistics = label("Rate unknown · quantum unknown · 0 nodes · 0 ports", true);
        statistics.add_css_class("dim-label");
        let notices = status_label("");
        notices.set_selectable(true);
        notices.set_lines(3);
        notices.set_ellipsize(gtk::pango::EllipsizeMode::End);
        notices.set_visible(false);
        let scope = label(
            "Volume, mute, routing and meters are not available in this preview.",
            true,
        );
        scope.add_css_class("dim-label");
        let strips = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .activate_on_single_click(false)
            .homogeneous(true)
            .min_children_per_line(1)
            .max_children_per_line(64)
            .column_spacing(12)
            .row_spacing(12)
            .valign(gtk::Align::Start)
            .build();
        let positions = Rc::new(RefCell::new(HashMap::<gtk::FlowBoxChild, usize>::new()));
        let sort_positions = Rc::clone(&positions);
        strips.set_sort_func(move |a, b| {
            let positions = sort_positions.borrow();
            positions.get(a).cmp(&positions.get(b)).into()
        });
        set_name(&strips, "Observed node strips");
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&strips)
            .build();
        set_name(&scroll, "Mixer overview");
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.set_valign(gtk::Align::Center);
        empty.set_halign(gtk::Align::Center);
        empty.set_vexpand(true);
        let empty_title = label("Waiting for PipeWire", true);
        empty_title.add_css_class("title-2");
        let empty_detail = label(
            "Observed nodes and ports will appear when a connection is available.",
            true,
        );
        empty.append(&empty_title);
        empty.append(&empty_detail);
        for child in [&title, &subtitle, &status, &statistics, &scope, &notices] {
            root.append(child);
        }
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&empty);
        root.append(&scroll);
        let mut view = Self {
            root,
            status,
            statistics,
            notices,
            empty,
            empty_title,
            empty_detail,
            scroll,
            strips,
            nodes: BTreeMap::new(),
            order: vec![],
            positions,
        };
        view.update(&Overview::default());
        view
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn show_message(&self, message: &str) {
        self.notices.set_text(message);
        self.notices.set_visible(!message.is_empty());
    }

    /// The bounded inbox remains the owner of notice buffering. The widget keeps
    /// only the latest batch's human-readable notice text, never a growing log.
    pub fn show_notices(&self, notices: &[Notice]) {
        if notices.is_empty() {
            return;
        }
        let messages: Vec<_> = notices
            .iter()
            .map(|notice| match notice {
                Notice::Message(message) => message.clone(),
                Notice::InboxOverflow { dropped } => {
                    format!("{dropped} observer notices were dropped because the queue was full.")
                }
            })
            .collect();
        self.show_message(&messages.join("\n"));
    }

    pub fn update(&mut self, overview: &Overview) {
        // Capture focus before hiding/removing a disconnected strip. Surviving
        // strips retain their exact widgets; removed focus gets a useful target.
        let focused = self
            .nodes
            .iter()
            .find_map(|(id, strip)| strip.focused_widget().map(|widget| (*id, widget)));
        let order: Vec<_> = if overview.connected {
            overview.nodes.iter().map(|node| node.id).collect()
        } else {
            vec![]
        };
        if focused.as_ref().is_some_and(|(id, _)| !order.contains(id)) {
            if let Some(root) = self.root.root() {
                root.set_focus(gtk::Widget::NONE);
            }
        }
        self.status.set_text(if overview.connected {
            "Connected · observing PipeWire"
        } else {
            "Disconnected · waiting for PipeWire"
        });
        // Core's disconnected projection is empty. Guard the presentation too:
        // never retain stale node surfaces while showing disconnected status.
        let nodes = if overview.connected {
            overview.nodes.as_slice()
        } else {
            &[]
        };
        let rate = overview
            .rate
            .filter(|_| overview.connected)
            .map_or("unknown".into(), |rate| format!("{rate} Hz"));
        let quantum = overview
            .quantum
            .filter(|_| overview.connected)
            .map_or("unknown".into(), |quantum| quantum.to_string());
        let ports: usize = nodes.iter().map(|node| node.ports.len()).sum();
        self.statistics.set_text(&format!(
            "Rate {rate} · quantum {quantum} · {} nodes · {ports} ports",
            nodes.len()
        ));
        self.empty.set_visible(nodes.is_empty());
        self.scroll.set_visible(!nodes.is_empty());
        self.empty_title.set_text(if overview.connected {
            "No observed nodes"
        } else {
            "Waiting for PipeWire"
        });
        self.empty_detail.set_text(if overview.connected {
            "The connection is available; this graph currently contains no nodes."
        } else {
            "Observed nodes and ports will appear when a connection is available."
        });
        let wanted: BTreeSet<_> = nodes.iter().map(|node| node.id).collect();
        self.nodes.retain(|id, strip| {
            if wanted.contains(id) {
                true
            } else {
                self.strips.remove(&strip.slot);
                false
            }
        });
        for node in nodes {
            if let Some(strip) = self.nodes.get_mut(&node.id) {
                strip.update(node);
            } else {
                self.nodes.insert(node.id, NodeStrip::new(node));
            }
        }
        if self.order != order {
            // GTK sorts existing wrappers in place using the projection's exact
            // positions. No domain sorting or widget reparenting is necessary.
            {
                let mut positions = self.positions.borrow_mut();
                positions.clear();
                for (position, id) in order.iter().enumerate() {
                    positions.insert(self.nodes[id].slot.clone(), position);
                }
            }
            for id in &order {
                if self.nodes[id].slot.parent().is_none() {
                    self.strips.insert(&self.nodes[id].slot, -1);
                }
            }
            self.strips.invalidate_sort();
            self.order = order;
        }
        if let Some((id, widget)) = focused {
            if wanted.contains(&id) {
                widget.grab_focus();
            } else if let Some(id) = self.order.first() {
                self.nodes[id].ports.grab_focus();
            } else {
                self.status.grab_focus();
            }
        }
    }
}

fn label(text: &str, wrap: bool) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(wrap)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build()
}

fn status_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .accessible_role(gtk::AccessibleRole::Status)
        .build()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
