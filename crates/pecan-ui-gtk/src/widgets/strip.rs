use gtk::prelude::*;

use super::accessibility::set_name;
use pecan_core::{graph::Soort, overview::NodeOverview};

/// A reusable observation strip. It deliberately has no command sender, volume,
/// mute, route or meter control; those need integrated domain/worker validation.
pub(super) struct NodeStrip {
    pub(super) root: gtk::Frame,
    pub(super) slot: gtk::FlowBoxChild,
    title: gtk::Label,
    class: gtk::Label,
    identity: gtk::Label,
    pub(super) ports: gtk::Expander,
    groups: gtk::Box,
    model: NodeOverview,
}

impl NodeStrip {
    pub(super) fn new(node: &NodeOverview) -> Self {
        let title = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(26)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("heading");
        let class = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(26)
            .build();
        class.add_css_class("dim-label");
        let identity = gtk::Label::builder().xalign(0.0).build();
        identity.add_css_class("dim-label");
        let groups = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let port_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(200)
            .child(&groups)
            .build();
        set_name(&port_scroll, "Observed ports");
        let ports = gtk::Expander::builder()
            .expanded(true)
            .child(&port_scroll)
            .build();
        // Collapsed expanders unparent their content. Keep it explicitly hidden
        // too, so GTK focus traversal cannot enter an unrooted port scroller.
        ports.connect_expanded_notify(|ports| {
            if let Some(child) = ports.child() {
                child.set_visible(ports.is_expanded());
            }
        });
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_size_request(240, -1);
        content.append(&title);
        content.append(&class);
        content.append(&identity);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&ports);
        let root = gtk::Frame::builder()
            .child(&content)
            .valign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let slot = gtk::FlowBoxChild::builder()
            .child(&root)
            .focusable(false)
            .build();
        let strip = Self {
            root,
            slot,
            title,
            class,
            identity,
            ports,
            groups,
            model: node.clone(),
        };
        strip.populate(node);
        strip
    }

    pub(super) fn focused_widget(&self) -> Option<gtk::Widget> {
        self.root
            .root()
            .and_then(|root| root.focus())
            .filter(|focus| focus == &self.root || focus.is_ancestor(&self.root))
    }

    pub(super) fn update(&mut self, node: &NodeOverview) {
        if &self.model != node {
            self.populate(node);
            self.model = node.clone();
        }
    }

    fn populate(&self, node: &NodeOverview) {
        self.title.set_text(&node.label);
        self.title.set_tooltip_text(Some(&node.label));
        let class = node.class.as_deref().unwrap_or("Unknown media class");
        self.class.set_text(class);
        self.class.set_tooltip_text(Some(class));
        self.identity.set_text(&format!("Node {}", node.id));
        self.identity.set_tooltip_text(Some(&node.name));
        set_name(
            &self.root,
            &format!("{} · node {} · read-only", node.label, node.id),
        );
        self.ports
            .set_label(Some(&format!("Ports ({})", node.ports.len())));
        set_name(
            &self.ports,
            &format!("Ports for {} · node {}", node.label, node.id),
        );
        while let Some(child) = self.groups.first_child() {
            self.groups.remove(&child);
        }
        if node.ports.is_empty() {
            self.groups.append(&text("No observed ports"));
        }
        // Only partition by the DTO's explicit direction; no stereo pairing,
        // device classification, route inference or domain grouping here.
        for (input, heading) in [(true, "Inputs"), (false, "Outputs")] {
            let count = node.ports.iter().filter(|port| port.input == input).count();
            if count == 0 {
                continue;
            }
            let group = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let label = text(&format!("{heading} · {count}"));
            label.add_css_class("heading");
            group.append(&label);
            for port in node.ports.iter().filter(|port| port.input == input) {
                let kind = match port.kind {
                    Soort::Audio => "Audio",
                    Soort::Midi => "MIDI · unsupported",
                    Soort::Anders => "Other",
                };
                let detail = match &port.channel {
                    Some(channel) => format!("{kind} · {channel} · port {}", port.id),
                    None => format!("{kind} · port {}", port.id),
                };
                let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
                let name = text(&port.label);
                name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                name.set_max_width_chars(26);
                name.set_tooltip_text(Some(&format!("{} · {detail}", port.label)));
                set_name(
                    &name,
                    &format!(
                        "{} · {} · {detail}",
                        if input { "Input" } else { "Output" },
                        port.label
                    ),
                );
                let detail = text(&detail);
                detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
                detail.set_max_width_chars(26);
                detail.add_css_class("dim-label");
                row.append(&name);
                row.append(&detail);
                group.append(&row);
            }
            self.groups.append(&group);
        }
    }
}

fn text(value: &str) -> gtk::Label {
    gtk::Label::builder().label(value).xalign(0.0).build()
}
