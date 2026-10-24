use super::{Graph, Session, SessionAdapter, Status};
use crate::{
    controls::{Controller, MixerControls},
    PatchbayOverview,
};
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

/// Explicit workspace integration for a future owning shell. No worker, timer,
/// file access, recording, tray or automatic application startup is created.
/// Keep one workspace for a window; repeated GApplication activation should
/// present that window, while reusing ApplicationOwner for its whole lifetime.
pub struct NativeWorkspace {
    root: gtk::Box,
    mixer: MixerControls<SessionAdapter>,
    patchbay: RefCell<PatchbayOverview>,
    session: Session,
    undo: gtk::Button,
    redo: gtk::Button,
    status: gtk::Label,
    presented: RefCell<Option<Graph>>,
    error: RefCell<Option<String>>,
    patch_gate: RefCell<Option<(bool, Option<String>)>>,
}
impl NativeWorkspace {
    pub fn new(session: Session) -> Rc<Self> {
        let mixer = MixerControls::new(Controller::new(session.adapter(), Default::default()));
        let patchbay = PatchbayOverview::new();
        let shared = session.clone();
        patchbay.bind_history_actions(move |origin, input| shared.patch(origin, input));
        let undo = gtk::Button::with_label("Undo");
        let redo = gtk::Button::with_label("Redo");
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tools.append(&undo);
        tools.append(&redo);
        tools.append(&status);
        let pages = gtk::Stack::new();
        pages.add_titled(mixer.widget(), Some("mixer"), "Mixer");
        pages.add_titled(patchbay.widget(), Some("patchbay"), "Patchbay");
        pages.set_hexpand(true);
        pages.set_vexpand(true);
        let switcher = gtk::StackSwitcher::builder().stack(&pages).build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.append(&tools);
        root.append(&switcher);
        root.append(&pages);
        let view = Rc::new(Self {
            root,
            mixer,
            patchbay: RefCell::new(patchbay),
            session,
            undo,
            redo,
            status,
            presented: RefCell::new(None),
            error: RefCell::new(None),
            patch_gate: RefCell::new(None),
        });
        for (button, undo) in [(&view.undo, true), (&view.redo, false)] {
            let weak = Rc::downgrade(&view);
            button.connect_clicked(move |_| {
                let Some(view) = weak.upgrade() else { return };
                let origin = view.presented.borrow().clone();
                if let Some(origin) = origin {
                    let result = if undo {
                        view.session.undo(&origin)
                    } else {
                        view.session.redo(&origin)
                    };
                    *view.error.borrow_mut() = result.err();
                }
                view.refresh();
            });
        }
        view.refresh();
        view
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
    /// Drive from one main-context source. Receipts need polling even if there
    /// are no graph events. The lifetime adapter owns/removes that source; this
    /// view creates none, preventing per-activation consumer/timer duplication.
    pub fn poll(&self) {
        self.session.poll();
        self.refresh();
    }
    fn refresh(&self) {
        if let Some(graph) = self.session.graph() {
            *self.presented.borrow_mut() = Some(graph.clone());
            self.mixer.observe(graph.clone());
            self.patchbay.borrow_mut().update_graph(graph);
        }
        self.mixer.reconcile_request(|c| c.poll_session());
        let enabled = self.session.enabled();
        self.mixer.widget().set_sensitive(enabled);
        let status = self.session.status();
        let error = matches!(
            status,
            Status::Failed(_) | Status::Unconfirmed(_) | Status::IdentityUnavailable
        )
        .then(|| status.message());
        // Pending means a real submitted request. A missing witness uses widget
        // availability plus the coordinator's exact-origin rejection boundary.
        let patchbay = self.patchbay.borrow();
        patchbay
            .widget()
            .set_sensitive(enabled || self.session.busy());
        let gate = (self.session.busy(), error);
        if self.patch_gate.borrow().as_ref() != Some(&gate) {
            patchbay.set_actions_pending(gate.0, gate.1.as_deref());
            *self.patch_gate.borrow_mut() = Some(gate);
        }
        let (undo, redo) = self.session.history_labels();
        self.undo
            .set_label(&undo.as_ref().map_or("Undo".into(), |s| format!("Undo {s}")));
        self.redo
            .set_label(&redo.as_ref().map_or("Redo".into(), |s| format!("Redo {s}")));
        self.undo.set_sensitive(enabled && undo.is_some());
        self.redo.set_sensitive(enabled && redo.is_some());
        self.status
            .set_text(&self.error.borrow().as_ref().map_or_else(
                || status.message(),
                |e| format!("{} · {e}", status.message()),
            ));
    }
}
