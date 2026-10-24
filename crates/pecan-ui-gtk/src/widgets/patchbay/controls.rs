//! UI-only handoff of existing core history inputs. There is no backend sender.
use super::{
    super::accessibility::set_name,
    actions::{Editor, Operation},
};
use gtk::{glib, prelude::*};
use pecan_core::{
    backend::Stamped,
    domain::history::{Action, Input},
    graph::Momentopname,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

type Handler = Rc<dyn Fn(Stamped<Arc<Momentopname>>, Input) -> Result<(), String>>;
#[derive(Clone)]
pub(super) struct Controls(Rc<Inner>);
struct Inner {
    root: gtk::Box,
    pub model: RefCell<Editor>,
    outputs: gtk::StringList,
    inputs: gtk::StringList,
    source: gtk::DropDown,
    destination: gtk::DropDown,
    connect: gtk::Button,
    disconnect: gtk::Button,
    move_link: gtk::Button,
    cancel: gtk::Button,
    status: gtk::Label,
    handler: RefCell<Option<Handler>>,
    refreshing: Cell<bool>,
    pending: Cell<bool>,
    list: glib::WeakRef<gtk::ListBox>,
}
impl Controls {
    pub fn new() -> Self {
        let outputs = gtk::StringList::new(&["Choose an audio output"]);
        let inputs = gtk::StringList::new(&["Choose an audio input"]);
        let source = chooser(&outputs, "Audio output");
        let destination = chooser(&inputs, "Audio input");
        let connect = gtk::Button::with_label("Connect");
        let disconnect = gtk::Button::with_label("Disconnect selected");
        let move_link = gtk::Button::with_label("Move selected to input");
        let cancel = gtk::Button::with_label("Clear selection / cancel move");
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for (label, control) in [("Output", &source), ("Input", &destination)] {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.append(&gtk::Label::new(Some(label)));
            row.append(control);
            root.append(&row);
        }
        let buttons = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(4)
            .build();
        for button in [&connect, &disconnect, &move_link, &cancel] {
            buttons.insert(button, -1);
        }
        root.append(&buttons);
        root.append(&status);
        let controls = Self(Rc::new(Inner {
            root,
            model: RefCell::default(),
            outputs,
            inputs,
            source,
            destination,
            connect,
            disconnect,
            move_link,
            cancel,
            status,
            handler: RefCell::new(None),
            refreshing: Cell::new(false),
            pending: Cell::new(false),
            list: glib::WeakRef::new(),
        }));
        for output in [true, false] {
            let weak = Rc::downgrade(&controls.0);
            let chooser = if output {
                &controls.0.source
            } else {
                &controls.0.destination
            };
            chooser.connect_selected_notify(move |chooser| {
                let Some(inner) = weak.upgrade() else { return };
                if inner.refreshing.get() {
                    return;
                }
                let mut model = inner.model.borrow_mut();
                let selected = if output {
                    &model.outputs
                } else {
                    &model.inputs
                }
                .get(chooser.selected().wrapping_sub(1) as usize)
                .map(|p| p.id);
                if output {
                    model.source = selected;
                } else {
                    model.destination = selected;
                }
                let message = model
                    .prepare(Operation::Connect)
                    .map(|_| {
                        if model.moving.is_some() {
                            "Ready to move. Cancel keeps the original connection.".to_owned()
                        } else {
                            "Ready to connect the chosen ports.".to_owned()
                        }
                    })
                    .unwrap_or_else(|error| error);
                drop(model);
                if !inner.pending.get() {
                    inner.status.set_text(&message);
                }
                Controls(inner).refresh();
            });
        }
        for (button, operation) in [
            (&controls.0.connect, Operation::Connect),
            (&controls.0.disconnect, Operation::Disconnect),
        ] {
            let weak = Rc::downgrade(&controls.0);
            button.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    Controls(inner).request(operation);
                }
            });
        }
        let weak = Rc::downgrade(&controls.0);
        controls.0.move_link.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                let controls = Controls(inner);
                let result = controls.0.model.borrow_mut().begin_move();
                if let Err(error) = result {
                    controls.0.status.set_text(&error);
                } else {
                    controls.0.status.set_text(
                        "Choose a new input, then Move. Cancel keeps the original connection.",
                    );
                }
                controls.sync_choices();
                controls.refresh();
            }
        });
        let weak = Rc::downgrade(&controls.0);
        controls.0.cancel.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                Controls(inner).cancel();
            }
        });
        controls.refresh();
        controls
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    pub fn attach(&self, list: &gtk::ListBox) {
        self.0.list.set(Some(list));
        let weak = Rc::downgrade(&self.0);
        list.connect_selected_rows_changed(move |list| {
            if let Some(inner) = weak.upgrade() {
                Controls(inner).selected(
                    list.selected_rows()
                        .iter()
                        .filter_map(|r| r.widget_name().parse::<u32>().ok()),
                );
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&self.0);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(inner) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let controls = Controls(inner);
            if key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::BackSpace {
                controls.request(Operation::Disconnect);
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::Escape {
                controls.cancel();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        list.add_controller(keys);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&self.0);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                if let Some(inner) = weak.upgrade() {
                    Controls(inner).cancel();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.0.root.add_controller(keys);
    }
    pub fn bind(
        &self,
        handler: impl Fn(Stamped<Arc<Momentopname>>, Input) -> Result<(), String> + 'static,
    ) {
        *self.0.handler.borrow_mut() = Some(Rc::new(handler));
        self.refresh();
    }
    pub fn update(&self, graph: Stamped<Arc<Momentopname>>) -> bool {
        if !self.0.model.borrow_mut().update(graph) {
            return false;
        }
        self.0.refreshing.set(true);
        let model = self.0.model.borrow();
        let outputs: Vec<_> = std::iter::once("Choose an audio output")
            .chain(model.outputs.iter().map(|p| p.label.as_str()))
            .collect();
        let inputs: Vec<_> = std::iter::once("Choose an audio input")
            .chain(model.inputs.iter().map(|p| p.label.as_str()))
            .collect();
        self.0.outputs.splice(0, self.0.outputs.n_items(), &outputs);
        self.0.inputs.splice(0, self.0.inputs.n_items(), &inputs);
        drop(model);
        self.0.refreshing.set(false);
        self.sync_choices();
        if !self.0.pending.get() {
            self.0
                .status
                .set_text("Graph changed; choose endpoints or connections again.");
        }
        self.refresh();
        true
    }
    pub fn selected(&self, ids: impl IntoIterator<Item = u32>) {
        self.0.model.borrow_mut().selected = ids.into_iter().collect();
        if !self.0.pending.get() {
            let model = self.0.model.borrow();
            if !model.selected.is_empty() {
                let message=model.prepare(Operation::Disconnect).map(|_|format!("{} connections selected. Disconnect them, or select one connection to move.",model.selected.len())).unwrap_or_else(|error|error);
                self.0.status.set_text(&message);
            }
        }
        self.refresh();
    }
    pub fn cancel(&self) {
        self.0.model.borrow_mut().cancel();
        self.sync_choices();
        if let Some(list) = self.0.list.upgrade() {
            list.unselect_all();
        }
        self.0.status.set_text(if self.0.pending.get() {
            "Selection cleared. The submitted request is still pending."
        } else {
            "Selection cleared. No patch was sent."
        });
        self.refresh();
    }
    /// Presentation binding only. Clear pending only when the shared application
    /// controller has resolved its real completion/observation boundary.
    pub fn set_pending(&self, pending: bool, error: Option<&str>) {
        self.0.pending.set(pending);
        self.0.status.set_text(error.unwrap_or(if pending {
            "Patch request pending. Waiting for confirmed completion."
        } else {
            "Ready. Displayed connections come from the observed graph."
        }));
        self.refresh();
    }
    pub fn request(&self, operation: Operation) {
        if self.0.pending.get() {
            return;
        }
        let Some(handler) = self.0.handler.borrow().clone() else {
            self.refresh();
            return;
        };
        let operation =
            if matches!(operation, Operation::Connect) && self.0.model.borrow().moving.is_some() {
                Operation::Move
            } else {
                operation
            };
        let planned = {
            let model = self.0.model.borrow();
            model
                .prepare(operation)
                .map(|action| (model.graph.as_ref().unwrap().clone(), action))
        };
        let (origin, action) = match planned {
            Ok(value) => value,
            Err(error) => {
                self.0.status.set_text(&error);
                return;
            }
        };
        let label = match &action {
            Action::Connect(_) => "Connect".into(),
            Action::Disconnect(pairs) => {
                if pairs.len() == 1 {
                    "Disconnect".into()
                } else {
                    format!("Disconnect {}", pairs.len())
                }
            }
            Action::Move { .. } => "Move cable".into(),
            _ => unreachable!(),
        };
        self.set_pending(true, None);
        // Clone before calling out: the application can synchronously reject or
        // update presentation without reentering a RefCell borrow.
        if let Err(error) = handler(origin, Input::Execute { label, action }) {
            self.set_pending(false, Some(&error));
        }
    }
    fn sync_choices(&self) {
        self.0.refreshing.set(true);
        let model = self.0.model.borrow();
        self.0.source.set_selected(
            model
                .source
                .and_then(|id| model.outputs.iter().position(|p| p.id == id))
                .map_or(0, |i| i as u32 + 1),
        );
        self.0.destination.set_selected(
            model
                .destination
                .and_then(|id| model.inputs.iter().position(|p| p.id == id))
                .map_or(0, |i| i as u32 + 1),
        );
        self.0.refreshing.set(false);
    }
    fn refresh(&self) {
        let model = self.0.model.borrow();
        let enabled =
            self.0.handler.borrow().is_some() && model.connected() && !self.0.pending.get();
        self.0
            .source
            .set_sensitive(enabled && model.moving.is_none());
        self.0.destination.set_sensitive(enabled);
        self.0.connect.set_label(if model.moving.is_some() {
            "Move"
        } else {
            "Connect"
        });
        self.0
            .connect
            .set_sensitive(enabled && model.prepare(Operation::Connect).is_ok());
        self.0
            .disconnect
            .set_sensitive(enabled && model.prepare(Operation::Disconnect).is_ok());
        self.0.move_link.set_sensitive(
            enabled
                && model.selected.len() == 1
                && model
                    .selected
                    .first()
                    .is_some_and(|id| model.pair(*id).is_some()),
        );
        self.0.cancel.set_sensitive(
            model.source.is_some() || model.destination.is_some() || !model.selected.is_empty(),
        );
        if self.0.handler.borrow().is_none() {
            self.0
                .status
                .set_text("Editing is unavailable in this preview.");
        } else if !model.connected() && !self.0.pending.get() {
            self.0
                .status
                .set_text("Disconnected; editing is unavailable.");
        }
    }
}
fn chooser(model: &gtk::StringList, name: &str) -> gtk::DropDown {
    let dropdown = gtk::DropDown::new(Some(model.clone()), gtk::Expression::NONE);
    dropdown.set_hexpand(true);
    dropdown.set_enable_search(true);
    dropdown.set_selected(0);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            item.set_child(Some(
                &gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(28)
                    .build(),
            ));
        }
    });
    factory.connect_bind(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            if let (Some(value), Some(label)) = (
                item.item().and_downcast::<gtk::StringObject>(),
                item.child().and_downcast::<gtk::Label>(),
            ) {
                label.set_text(&value.string());
                label.set_tooltip_text(Some(&value.string()));
            }
        }
    });
    dropdown.set_factory(Some(&factory));
    set_name(&dropdown, name);
    dropdown
}

#[cfg(test)]
mod tests;
