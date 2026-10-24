//! Thin GTK binding: the core reducer remains the authority for accepted stamps.
use crate::{MixerOverview, PatchbayOverview};
use gtk::prelude::*;
use pecan_core::{
    inbox::InboxBatch,
    overview::{Overview, OverviewState},
};

pub struct GraphPresentation {
    root: gtk::Box,
    pages: gtk::Stack,
    mixer: MixerOverview,
    patchbay: PatchbayOverview,
    state: OverviewState,
}
impl Default for GraphPresentation {
    fn default() -> Self {
        Self::new()
    }
}
impl GraphPresentation {
    pub fn new() -> Self {
        let mixer = MixerOverview::new();
        let patchbay = PatchbayOverview::new();
        let pages = gtk::Stack::new();
        pages.set_vexpand(true);
        pages.add_titled(mixer.widget(), Some("mixer"), "Mixer overview");
        pages.add_titled(patchbay.widget(), Some("patchbay"), "Patchbay");
        let switcher = gtk::StackSwitcher::builder()
            .stack(&pages)
            .halign(gtk::Align::Start)
            .margin_start(16)
            .margin_top(12)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&switcher);
        root.append(&pages);
        Self {
            root,
            pages,
            mixer,
            patchbay,
            state: OverviewState::default(),
        }
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    pub fn show_patchbay(&self) {
        self.pages.set_visible_child_name("patchbay");
    }
    pub fn show_message(&self, message: &str) {
        self.mixer.show_message(message);
        self.patchbay.show_message(message);
    }
    pub fn apply(&mut self, batch: InboxBatch) -> Option<Overview> {
        self.mixer.show_notices(&batch.notices);
        if !batch.notices.is_empty() {
            let message = batch
                .notices
                .iter()
                .map(|notice| match notice {
                    pecan_core::backend::Notice::Message(message) => message.clone(),
                    pecan_core::backend::Notice::InboxOverflow { dropped } => format!(
                        "{dropped} observer notices were dropped because the queue was full."
                    ),
                })
                .collect::<Vec<_>>()
                .join("\n");
            self.patchbay.show_message(&message);
        }
        let graph = batch.graph.clone();
        let previous = self.state.stamp();
        self.state.apply(batch);
        // Link-only snapshots can leave Overview unchanged. Accepted stamp
        // advancement still updates the patchbay; stale frames never do.
        if self.state.stamp() == previous {
            return None;
        }
        let graph = graph?;
        let overview = self.state.overview();
        self.mixer.update(&overview);
        self.patchbay.update(&overview, &graph.value.kabels);
        Some(overview)
    }
}

#[cfg(test)]
mod tests;
