use super::*;
use crate::{recording_buses::input::ActivationInput, widgets::accessibility::set_name};
use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};

type Handler<O> = dyn Fn(FolderRequest<O>) -> Result<(), String>;
struct State<O> {
    model: RefCell<Model<O>>,
    root: gtk::Box,
    directory: gtk::Label,
    latest: gtk::Label,
    elapsed: gtk::Label,
    status: gtk::Label,
    actions: gtk::Box,
    folder: RefCell<Option<gtk::Button>>,
    folder_live: RefCell<Option<Rc<Cell<bool>>>>,
    handler: RefCell<Option<Rc<Handler<O>>>>,
    input: Rc<ActivationInput>,
    offered: RefCell<Option<Rc<Projection<O>>>>,
    scheduled: Cell<bool>,
    unavailable: RefCell<Option<String>>,
    now: Cell<Duration>,
    #[cfg(test)]
    renders: Cell<usize>,
}
pub struct TransportControls<O>(Rc<State<O>>);
impl<O> Clone for TransportControls<O> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<O: Clone + 'static> Default for TransportControls<O> {
    fn default() -> Self {
        Self::new()
    }
}
impl<O: Clone + 'static> TransportControls<O> {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        set_name(&root, "Recording folder and elapsed time");
        let label = |name, role| {
            let label = gtk::Label::builder()
                .xalign(0.)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .max_width_chars(60)
                .hexpand(true)
                .selectable(true)
                .accessible_role(role)
                .build();
            set_name(&label, name);
            root.append(&label);
            label
        };
        let directory = label("Recording directory", gtk::AccessibleRole::Label);
        let latest = label("Last take folder", gtk::AccessibleRole::Label);
        let elapsed = label("Recording time", gtk::AccessibleRole::Label);
        let status = label("Folder request status", gtk::AccessibleRole::Status);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.append(&actions);
        let input = ActivationInput::new();
        input.observe(&root);
        let state = Rc::new(State {
            model: RefCell::default(),
            root,
            directory,
            latest,
            elapsed,
            status,
            actions,
            folder: RefCell::default(),
            folder_live: RefCell::default(),
            handler: RefCell::default(),
            input: input.clone(),
            offered: RefCell::default(),
            scheduled: Cell::new(false),
            unavailable: RefCell::default(),
            now: Cell::new(Duration::ZERO),
            #[cfg(test)]
            renders: Cell::new(0),
        });
        let weak = Rc::downgrade(&state);
        input.notify(move || {
            if let Some(s) = weak.upgrade() {
                s.render();
            }
        });
        state.render();
        Self(state)
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    /// Err means no launch was admitted. After admission, retain the token and
    /// report the actual result with complete(); elapsed time is never success.
    pub fn bind_folder(&self, handler: impl Fn(FolderRequest<O>) -> Result<(), String> + 'static) {
        *self.0.handler.borrow_mut() = Some(Rc::new(handler));
        self.0.render();
    }
    pub fn observe(&self, snapshot: Snapshot<O>) -> Result<(), Error> {
        let p = Model::project(snapshot).map_err(|e| {
            self.unavailable(&e.to_string());
            e
        })?;
        self.0.check_time(p.observed_at)?;
        self.0.offered.borrow_mut().take();
        self.0.now.set(self.0.now.get().max(p.observed_at));
        self.0.model.borrow_mut().adopt(p)?;
        self.0.unavailable.borrow_mut().take();
        self.0.render();
        Ok(())
    }
    /// Retain only one bounded, already-projected publication and one weak wake.
    pub fn offer(&self, snapshot: Snapshot<O>) -> Result<(), Error> {
        let p = Model::project(snapshot).map_err(|e| {
            self.unavailable(&e.to_string());
            e
        })?;
        self.0.check_time(p.observed_at)?;
        *self.0.offered.borrow_mut() = Some(p);
        self.0.retire_folder();
        if let Some(button) = self.0.folder.borrow().as_ref() {
            button.set_sensitive(false);
        }
        if !self.0.scheduled.replace(true) {
            let weak = Rc::downgrade(&self.0);
            glib::idle_add_local_once(move || {
                if let Some(s) = weak.upgrade() {
                    s.scheduled.set(false);
                    let offered = s.offered.borrow_mut().take();
                    if let Some(p) = offered {
                        s.now.set(s.now.get().max(p.observed_at));
                        let result = s.model.borrow_mut().adopt(p);
                        if result.is_ok() {
                            s.unavailable.borrow_mut().take();
                        }
                        s.render();
                    }
                }
            });
        }
        Ok(())
    }
    pub fn unavailable(&self, message: &str) {
        self.0.offered.borrow_mut().take();
        *self.0.unavailable.borrow_mut() = Some(message.chars().take(1024).collect());
        self.0.render();
    }
    pub fn complete(&self, token: &RequestToken, result: Completion) -> bool {
        let accepted = self.0.model.borrow_mut().complete(token, result);
        if accepted {
            self.0.render();
        }
        accepted
    }
    pub fn pending(&self) -> Option<RequestToken> {
        self.0.model.borrow().pending()
    }
    /// Fresh timing from the same retained owner. Timing changes do not retire
    /// folder controls. Keep the folder action context stable between structural
    /// publications; it is independent of the timing report's identity.
    pub fn update_elapsed(
        &self,
        activity: Activity,
        takes: &[BusRecordingStatus],
        at: Duration,
    ) -> Result<(), Error> {
        if self.0.offered.borrow().is_some() {
            return Err(Error::Pending);
        }
        let result = self.0.model.borrow().update_elapsed(activity, takes, at);
        if result.is_ok() {
            self.0.now.set(self.0.now.get().max(at));
        }
        if let Err(error) = &result {
            if *error != Error::OldSnapshot {
                if let Some(p) = self.0.model.borrow().current.as_ref() {
                    p.clock.set(Clock::Unavailable);
                }
            }
        }
        if self.0.root.is_mapped() {
            self.0.render_time();
        }
        result
    }
    /// Call from the existing owner clock. No timer/receipt consumer is created.
    /// Hidden views do no GTK clock updates; resume evaluates only current time.
    pub fn tick(&self, now: Duration) {
        self.0.now.set(self.0.now.get().max(now));
        if self.0.root.is_mapped() && self.0.offered.borrow().is_none() {
            self.0.render_time();
        }
    }
}
impl<O: Clone + 'static> State<O> {
    fn retire_folder(&self) {
        // Input retirement only cancels held activations. Also revoke retained
        // idle/detached callbacks, even when the model binding is unchanged.
        if let Some(live) = self.folder_live.borrow_mut().take() {
            live.set(false);
        }
        self.input.retire();
    }
    fn check_time(&self, at: Duration) -> Result<(), Error> {
        let m = self.model.borrow();
        if m.current
            .as_ref()
            .is_some_and(|p| at < p.observed_at.max(p.timing_at.get()))
            || self
                .offered
                .borrow()
                .as_ref()
                .is_some_and(|p| at < p.observed_at.max(p.timing_at.get()))
        {
            return Err(Error::OldSnapshot);
        }
        Ok(())
    }
    fn render_time(&self) {
        let text = self
            .model
            .borrow()
            .elapsed(self.now.get())
            .map(elapsed_text)
            .unwrap_or_else(|| "Recording time unavailable".into());
        if self.elapsed.text() != text {
            self.elapsed.set_text(&text);
        }
    }
    fn render(self: &Rc<Self>) {
        #[cfg(test)]
        self.renders.set(self.renders.get() + 1);
        self.retire_folder();
        let restore = self.folder.borrow().as_ref().is_some_and(|b| b.has_focus());
        if let Some(window) = self.root.root().and_downcast::<gtk::Window>() {
            if GtkWindowExt::focus(&window).is_some_and(|w| w.is_ancestor(&self.actions)) {
                GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
            }
        }
        while let Some(child) = self.actions.first_child() {
            self.actions.remove(&child);
        }
        let m = self.model.borrow();
        let p = m.current.as_ref();
        self.directory.set_text(
            &p.and_then(|p| p.root.as_ref())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    "Recording directory unavailable · explicit root required".into()
                }),
        );
        self.latest.set_text(
            &p.and_then(|p| p.latest.as_ref())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    "No last take folder supplied · Folder uses the recording directory".into()
                }),
        );
        let handler = self.handler.borrow().clone();
        let attached = handler.is_some();
        let fallback = if !attached {
            "Folder action unavailable"
        } else if p.is_some_and(|p| p.closing) {
            "Application closing"
        } else if p.is_some_and(|p| p.paths.is_some()) {
            "Ready"
        } else {
            "Recording directory not set"
        };
        self.status.set_text(
            self.unavailable
                .borrow()
                .as_deref()
                .or(m.message())
                .unwrap_or(fallback),
        );
        let binding = m.bind().ok();
        let enabled = attached
            && self.unavailable.borrow().is_none()
            && self.offered.borrow().is_none()
            && !self.input.blocked()
            && m.pending.is_none()
            && p.is_some_and(|p| !p.closing && p.paths.is_some());
        drop(m);
        let button = gtk::Button::with_label("Folder");
        set_name(&button, "Open recording folder");
        button.set_tooltip_text(Some("Open the last take folder within the recording directory; otherwise open the recording directory"));
        button.set_sensitive(enabled);
        if let (Some(binding), Some(handler)) = (binding, handler) {
            let weak = Rc::downgrade(self);
            let live = Rc::new(Cell::new(true));
            *self.folder_live.borrow_mut() = Some(live.clone());
            self.input.capture(&button, live.clone());
            button.connect_clicked(move |button| {
                let Some(s) = weak.upgrade() else { return };
                if !live.get()
                    || !button.is_sensitive()
                    || s.input.blocked()
                    || s.offered.borrow().is_some()
                    || s.unavailable.borrow().is_some()
                {
                    return;
                }
                let result = s.model.borrow_mut().request_folder(&binding);
                let Ok(request) = result else { return };
                let token = request.token.clone();
                s.render();
                if let Err(error) = handler(request) {
                    let accepted = s
                        .model
                        .borrow_mut()
                        .complete(&token, Completion::Failed(error));
                    if accepted {
                        s.render();
                    }
                }
            });
        }
        self.actions.append(&button);
        if restore && enabled {
            button.grab_focus();
        }
        *self.folder.borrow_mut() = Some(button);
        self.render_time();
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
