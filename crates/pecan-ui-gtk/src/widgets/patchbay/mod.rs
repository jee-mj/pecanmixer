//! Observed graph and isolated native patch intents. No backend sender or live
//! mutation is attached by preview startup.
mod actions;
mod controls;
mod layout;
mod pointer;
mod session;

use super::accessibility::set_name;
use gtk::prelude::*;
use layout::{Layout, Metrics, WIDTH};
use pecan_core::{
    backend::Stamped,
    domain::history::Input,
    graph::{Kabel, Momentopname, Soort},
    overview::Overview,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

pub struct PatchbayOverview {
    root: gtk::Box,
    actions: controls::Controls,
    pointer: pointer::Pointer,
    session: session::Session,
    #[cfg(test)]
    rearrange: gtk::Button,
    status: gtk::Label,
    notices: gtk::Label,
    scope: gtk::Label,
    empty: gtk::Label,
    paned: gtk::Paned,
    canvas: gtk::Overlay,
    #[cfg(test)]
    canvas_scroll: gtk::ScrolledWindow,
    drawing: gtk::DrawingArea,
    modules: gtk::Fixed,
    connection_title: gtk::Label,
    connections: gtk::ListBox,
    #[cfg(test)]
    connection_scroll: gtk::ScrolledWindow,
    geometry: Rc<RefCell<Layout>>,
    current: Option<(Overview, Vec<Kabel>)>,
    rows: BTreeMap<u32, gtk::ListBoxRow>,
}
impl Default for PatchbayOverview {
    fn default() -> Self {
        Self::new()
    }
}
impl PatchbayOverview {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.set_margin_top(16);
        root.set_margin_bottom(16);
        root.set_margin_start(16);
        root.set_margin_end(16);
        let title = text("Patchbay", false);
        title.add_css_class("title-1");
        let scope = text(
            "Editing is unavailable in this preview. Observed connections are listed below.",
            true,
        );
        scope.add_css_class("dim-label");
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let notices = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .lines(3)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        notices.set_visible(false);
        let empty = text("Disconnected · waiting for PipeWire", true);
        empty.set_vexpand(true);
        empty.set_valign(gtk::Align::Center);
        let drawing = gtk::DrawingArea::builder()
            .can_target(false)
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        let modules = gtk::Fixed::new();
        let canvas = gtk::Overlay::new();
        canvas.set_child(Some(&drawing));
        canvas.add_overlay(&modules);
        set_name(&canvas, "Observed patchbay graph");
        let canvas_scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&canvas)
            .build();
        set_name(
            &canvas_scroll,
            "Observed patchbay graph; outputs left, inputs right",
        );
        let geometry = Rc::new(RefCell::new(Layout::default()));
        let connection_title = text("Connections · 0", true);
        connection_title.add_css_class("heading");
        let connections = gtk::ListBox::new();
        connections.set_selection_mode(gtk::SelectionMode::Single);
        connections.set_activate_on_single_click(false);
        connections.set_sort_func(|a, b| {
            a.widget_name()
                .as_str()
                .parse::<u32>()
                .unwrap_or(0)
                .cmp(&b.widget_name().as_str().parse::<u32>().unwrap_or(0))
                .into()
        });
        set_name(
            &connections,
            "Read-only connections; use arrow keys to inspect rows",
        );
        let connection_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&connections)
            .build();
        set_name(&connection_scroll, "Connection list");
        let actions = controls::Controls::new();
        actions.attach(&connections);
        let pointer = pointer::Pointer::new(&actions, &geometry, &canvas, &drawing);
        let session = session::Session::new(
            &geometry,
            &pointer,
            &canvas,
            &modules,
            &canvas_scroll,
            &drawing,
        );
        let rearrange = gtk::Button::with_label("Rearrange");
        set_name(&rearrange, "Rearrange patchbay modules");
        rearrange.set_tooltip_text(Some("Reset this session's module positions"));
        session.bind_reset(&rearrange);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        list.append(actions.widget());
        list.append(&connection_title);
        list.append(&connection_scroll);
        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .start_child(&canvas_scroll)
            .end_child(&list)
            .position(240)
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(true)
            .shrink_end_child(true)
            .vexpand(true)
            .build();
        root.append(&rearrange);
        for label in [&title, &scope, &status, &notices, &empty] {
            root.append(label);
        }
        root.append(&paned);
        let mut view = Self {
            root,
            actions,
            pointer,
            session,
            #[cfg(test)]
            rearrange,
            status,
            notices,
            scope,
            empty,
            paned,
            canvas,
            #[cfg(test)]
            canvas_scroll,
            drawing,
            modules,
            connection_title,
            connections,
            #[cfg(test)]
            connection_scroll,
            geometry,
            current: None,
            rows: BTreeMap::new(),
        };
        view.update(&Overview::default(), &[]);
        view
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    /// Attach the shared application's history-intent handler, not a backend
    /// dispatcher. `Ok(())` acknowledges handoff only. The application must use
    /// the reviewed origin-bound API and drive `set_actions_pending` from real
    /// completion plus graph observation; this widget creates no request ticket.
    /// Match this exact snapshot to the backend’s retained opaque origin; do not mint
    /// identity from its numeric IDs. Reject handoff if the origin is unavailable.
    /// No handler is attached by the current preview startup.
    pub fn bind_history_actions(
        &self,
        handler: impl Fn(Stamped<Arc<Momentopname>>, Input) -> Result<(), String> + 'static,
    ) {
        self.scope.set_text("Drag an audio port to connect. Drag a connected input to move its selected or sole cable; release elsewhere to disconnect. Escape cancels. The controls and connection list also support keyboard editing.");
        set_name(&self.canvas, "Patchbay audio connections; drag ports to patch, Escape cancels, Delete disconnects selected cables");
        set_name(&self.connections, "Connections; arrows inspect rows, Shift extends selection, Delete disconnects selected cables");
        self.connections
            .set_selection_mode(gtk::SelectionMode::Multiple);
        self.actions.bind(handler);
    }
    pub fn set_actions_pending(&self, pending: bool, error: Option<&str>) {
        self.actions.set_pending(pending, error);
    }
    /// Editing requires the exact accepted full snapshot, not Overview alone.
    /// New stamps cancel local drafts/selections; submitted work stays pending
    /// until the application resolves it. Older/equal stamps are ignored.
    pub fn update_graph(&mut self, graph: Stamped<Arc<Momentopname>>) -> bool {
        if !self.actions.update(graph.clone()) {
            return false;
        }
        self.session.graph_changed(Some(&graph.value));
        self.render(
            &pecan_core::overview::project(&graph.value),
            &graph.value.kabels,
        );
        self.connections.unselect_all();
        self.session.flush();
        true
    }
    /// Reset session-only module positions; this sends no audio/history intent.
    pub fn rearrange(&self) {
        self.session.rearrange();
    }
    pub fn show_message(&self, message: &str) {
        self.notices.set_text(message);
        self.notices.set_visible(!message.is_empty());
    }
    /// Read-only projections cannot retain an editable full-graph origin. Use
    /// update_graph exclusively when binding actions; a newer accepted full
    /// snapshot is required to resume editing after this method.
    pub fn update(&mut self, overview: &Overview, links: &[Kabel]) -> bool {
        self.actions.suspend_origin();
        self.session.graph_changed(None);
        let changed = self.render(overview, links);
        self.session.flush();
        changed
    }
    /// Only structural changes replace widgets; motion/levels never rebuild.
    fn render(&mut self, overview: &Overview, links: &[Kabel]) -> bool {
        let links = if overview.connected {
            links.to_vec()
        } else {
            vec![]
        };
        if self
            .current
            .as_ref()
            .is_some_and(|(old, old_links)| old == overview && *old_links == links)
        {
            return false;
        }
        let selected: Vec<_> = self
            .connections
            .selected_rows()
            .iter()
            .filter_map(|row| row.widget_name().parse::<u32>().ok())
            .collect();
        let focused = self
            .root
            .root()
            .and_then(|r| r.focus())
            .is_some_and(|f| f.is_ancestor(&self.connections));
        if focused {
            if let Some(root) = self.root.root() {
                root.set_focus(gtk::Widget::NONE);
            }
        }
        self.status.set_text(&format!(
            "{} · {} nodes · {} ports · {} links",
            if overview.connected {
                "Connected"
            } else {
                "Disconnected"
            },
            overview.nodes.len(),
            overview.nodes.iter().map(|n| n.ports.len()).sum::<usize>(),
            links.len()
        ));
        self.empty.set_text(if overview.connected {
            "No observed ports. Connections, if present, are listed below."
        } else {
            "Disconnected · waiting for PipeWire"
        });
        self.empty
            .set_visible(!overview.connected || overview.nodes.iter().all(|n| n.ports.is_empty()));
        self.paned.set_visible(overview.connected);
        let measure = gtk::Label::new(Some("Ag"));
        let (_, height, _, _) = measure.measure(gtk::Orientation::Vertical, -1);
        let metrics = Metrics {
            line: f64::from(height.max(20)),
        };
        let layout = self
            .session
            .set_layout(Layout::build(overview, &links, metrics));
        self.canvas
            .set_size_request(layout.width as i32, layout.height as i32);
        self.drawing.set_content_width(layout.width as i32);
        self.drawing.set_content_height(layout.height as i32);
        self.modules
            .set_size_request(layout.width as i32, layout.height as i32);
        while let Some(child) = self.modules.first_child() {
            self.modules.remove(&child);
        }
        for module in &layout.modules {
            let Some(node) = overview.nodes.iter().find(|node| node.id == module.node) else {
                continue;
            };
            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.set_margin_top(12);
            content.set_margin_bottom(12);
            content.set_margin_start(12);
            content.set_margin_end(12);
            let title = text(&node.label, false);
            title.add_css_class("heading");
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_max_width_chars(24);
            title.set_height_request(metrics.line as i32);
            title.set_tooltip_text(Some(&node.label));
            let detail = text(
                &format!(
                    "{} · node {}",
                    if module.input { "Inputs" } else { "Outputs" },
                    node.id
                ),
                false,
            );
            detail.add_css_class("dim-label");
            detail.set_height_request(metrics.line as i32);
            content.append(&title);
            content.append(&detail);
            for id in &module.ports {
                if let Some(port) = node.ports.iter().find(|p| p.id == *id) {
                    let kind = match port.kind {
                        Soort::Audio => "audio",
                        Soort::Midi => "MIDI unsupported",
                        Soort::Anders => "other",
                    };
                    let full = format!("{} · {kind} · port {}", port.label, port.id);
                    let label = text(&full, false);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    label.set_max_width_chars(24);
                    label.set_height_request(metrics.row() as i32);
                    label.set_tooltip_text(Some(&full));
                    set_name(
                        &label,
                        &format!("{} · {full}", if module.input { "Input" } else { "Output" }),
                    );
                    content.append(&label);
                }
            }
            let frame = gtk::Frame::builder()
                .child(&content)
                .width_request(WIDTH as i32)
                .height_request(module.height as i32)
                .build();
            set_name(
                &frame,
                &format!(
                    "{} · {} · node {}",
                    node.label,
                    if module.input { "Inputs" } else { "Outputs" },
                    node.id
                ),
            );
            self.modules.put(&frame, module.origin.x, module.origin.y);
            self.session.register(module.node, module.input, &frame);
        }
        self.connection_title.set_text(&if layout.undrawn == 0 {
            format!("Connections · {}", links.len())
        } else {
            format!(
                "Connections · {} · {} not shown in diagram",
                links.len(),
                layout.undrawn
            )
        });
        while let Some(child) = self.connections.first_child() {
            self.connections.remove(&child);
        }
        self.rows.clear();
        for link in &links {
            let endpoint = |node_id, port_id| {
                let node = overview.nodes.iter().find(|n| n.id == node_id);
                let port = node.and_then(|n| n.ports.iter().find(|p| p.id == port_id));
                format!(
                    "{} [node {node_id}] · {} [port {port_id}]",
                    node.map_or("Unavailable node", |n| n.label.as_str()),
                    port.map_or("Unavailable port", |p| p.label.as_str())
                )
            };
            let full = format!(
                "Link {} · {} → {}",
                link.id,
                endpoint(link.uit_knoop, link.uit_poort),
                endpoint(link.in_knoop, link.in_poort)
            );
            let label = text(&full, true);
            label.set_lines(2);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_margin_top(8);
            label.set_margin_bottom(8);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_tooltip_text(Some(&full));
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .activatable(false)
                .selectable(true)
                .build();
            row.set_widget_name(&link.id.to_string());
            set_name(&row, &full);
            self.connections.insert(&row, -1);
            self.rows.insert(link.id, row);
        }
        if links.is_empty() {
            let label = text("No observed connections", true);
            label.set_margin_top(12);
            label.set_margin_bottom(12);
            let row = gtk::ListBoxRow::builder()
                .child(&label)
                .selectable(false)
                .activatable(false)
                .build();
            self.connections.append(&row);
        }
        let kept: Vec<_> = selected.iter().filter_map(|id| self.rows.get(id)).collect();
        for row in &kept {
            self.connections.select_row(Some(*row));
        }
        if let Some(row) = kept
            .first()
            .copied()
            .or_else(|| self.rows.first_key_value().map(|(_, r)| r))
        {
            self.connections.select_row(Some(row));
            if focused {
                row.grab_focus();
            }
        } else if focused {
            self.status.grab_focus();
        }
        *self.geometry.borrow_mut() = layout;
        self.drawing.queue_draw();
        self.current = Some((overview.clone(), links));
        true
    }
}
fn text(value: &str, wrap: bool) -> gtk::Label {
    gtk::Label::builder()
        .label(value)
        .xalign(0.0)
        .wrap(wrap)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build()
}

#[cfg(test)]
mod tests;

impl Drop for PatchbayOverview {
    fn drop(&mut self) {
        self.session.retire();
        self.pointer.retire();
    }
}

#[cfg(all(test, feature = "x11-input-tests"))]
mod xtest;
