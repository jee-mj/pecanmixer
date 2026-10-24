//! Retired history controls cannot receive the tail of an earlier activation.
//! Observe releases on the window, including focus changes and disabled buttons.
use gtk::{glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
#[derive(Clone, PartialEq)]
enum Input {
    Key(u32),
    Pointer(u32),
    Touch(gtk::gdk::EventSequence),
}
struct Held {
    input: Input,
    owner: Weak<Cell<bool>>,
    cancelled: bool,
}
pub(super) struct HistoryInput {
    held: RefCell<Vec<Held>>,
    epoch: RefCell<Rc<()>>,
    controller: RefCell<Option<gtk::EventControllerLegacy>>,
    notify: RefCell<Option<Box<dyn Fn()>>>,
    scheduled: Cell<bool>,
}
impl HistoryInput {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            held: RefCell::new(vec![]),
            epoch: RefCell::new(Rc::new(())),
            controller: RefCell::new(None),
            notify: RefCell::new(None),
            scheduled: Cell::new(false),
        })
    }
    pub fn notify(&self, f: impl Fn() + 'static) {
        *self.notify.borrow_mut() = Some(Box::new(f));
    }
    pub fn epoch(&self) -> Rc<()> {
        self.epoch.borrow().clone()
    }
    pub fn held(&self) -> bool {
        !self.held.borrow().is_empty()
    }
    pub fn blocked(&self) -> bool {
        self.held.borrow().iter().any(|held| held.cancelled)
    }
    fn cancelled(&self, input: &Input) -> bool {
        self.held
            .borrow()
            .iter()
            .any(|held| &held.input == input && held.cancelled)
    }

    fn begin(&self, input: Input, owner: &Rc<Cell<bool>>) {
        let mut held = self.held.borrow_mut();
        if held.iter().any(|h| h.input == input) {
            return;
        }
        held.push(Held {
            input,
            owner: Rc::downgrade(owner),
            cancelled: false,
        });
    }
    fn schedule(self: &Rc<Self>) {
        if self.scheduled.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade() {
                this.scheduled.set(false);
                if let Some(notify) = this.notify.borrow().as_ref() {
                    notify();
                }
            }
        });
    }
    fn cancel(self: &Rc<Self>, owner: Option<&Rc<Cell<bool>>>, notify: bool) {
        let mut changed = false;
        for held in self.held.borrow_mut().iter_mut() {
            if owner.is_none_or(|owner| {
                held.owner
                    .upgrade()
                    .is_some_and(|previous| Rc::ptr_eq(&previous, owner))
            }) && !held.cancelled
            {
                held.cancelled = true;
                if let Some(owner) = held.owner.upgrade() {
                    owner.set(false);
                }
                changed = true;
            }
        }
        if changed {
            *self.epoch.borrow_mut() = Rc::new(());
            if notify {
                self.schedule();
            }
        }
    }
    /// Called before retiring/replacing buttons. A held input cannot transfer to
    /// the replacement, even if its label happens to be identical.
    pub fn retire_control(self: &Rc<Self>, owner: &Rc<Cell<bool>>) {
        self.cancel(Some(owner), false);
    }
    pub fn retire(self: &Rc<Self>) {
        self.cancel(None, false);
    }
    fn end(self: &Rc<Self>, input: Input) -> bool {
        let mut held = self.held.borrow_mut();
        let cancelled = held.iter().any(|h| h.input == input && h.cancelled);
        let previous = held.len();
        held.retain(|h| h.input != input);
        let changed = held.len() != previous;
        drop(held);
        if changed {
            self.schedule();
        }
        cancelled
    }
    pub fn capture(self: &Rc<Self>, button: &gtk::Button, live: Rc<Cell<bool>>) {
        let controller = gtk::EventControllerLegacy::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let owner = live.clone();
        controller.connect_event(move |_, event| {
            if let Some(this) = weak.upgrade() {
                use gtk::gdk::EventType;
                match event.event_type() {
                    EventType::KeyPress => {
                        if let Some(key) = event.downcast_ref::<gtk::gdk::KeyEvent>() {
                            if activation_key(key.keyval()) {
                                this.begin(Input::Key(key.keycode()), &owner);
                            }
                        }
                    }
                    EventType::ButtonPress => {
                        if let Some(pointer) = event.downcast_ref::<gtk::gdk::ButtonEvent>() {
                            if pointer.button() == 1 {
                                this.begin(Input::Pointer(1), &owner);
                            }
                        }
                    }
                    EventType::TouchBegin => {
                        this.begin(Input::Touch(event.event_sequence()), &owner)
                    }
                    EventType::GrabBroken | EventType::TouchCancel => {
                        this.cancel(Some(&owner), true)
                    }
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });
        button.add_controller(controller);
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(this) = weak.upgrade() {
                this.cancel(Some(&live), true);
            }
        });
        button.add_controller(focus);
    }
    pub fn observe(self: &Rc<Self>, root: &impl IsA<gtk::Widget>) {
        let weak = Rc::downgrade(self);
        root.connect_map(move |root| {
            if let (Some(this), Some(window)) =
                (weak.upgrade(), root.root().and_downcast::<gtk::Window>())
            {
                this.attach(&window);
            }
        });
        let weak = Rc::downgrade(self);
        root.connect_unmap(move |_| {
            if let Some(this) = weak.upgrade() {
                this.cancel(None, true);
            }
        });
    }
    fn attach(self: &Rc<Self>, window: &gtk::Window) {
        if self
            .controller
            .borrow()
            .as_ref()
            .is_some_and(|c| c.widget().as_ref() == Some(window.upcast_ref()))
        {
            return;
        }
        if let Some(old) = self.controller.borrow_mut().take() {
            if let Some(widget) = old.widget() {
                widget.remove_controller(&old);
            }
        }
        let controller = gtk::EventControllerLegacy::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        controller.connect_event(move |_, event| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            use gtk::gdk::EventType;
            let cancelled = match event.event_type() {
                EventType::KeyRelease => event
                    .downcast_ref::<gtk::gdk::KeyEvent>()
                    .is_some_and(|key| this.end(Input::Key(key.keycode()))),
                EventType::ButtonRelease => event
                    .downcast_ref::<gtk::gdk::ButtonEvent>()
                    .is_some_and(|button| this.end(Input::Pointer(button.button()))),
                EventType::TouchEnd => this.end(Input::Touch(event.event_sequence())),
                EventType::TouchCancel => {
                    let input = Input::Touch(event.event_sequence());
                    let owner = this
                        .held
                        .borrow()
                        .iter()
                        .find(|held| held.input == input)
                        .and_then(|held| held.owner.upgrade());
                    if let Some(owner) = owner {
                        this.cancel(Some(&owner), true);
                    }
                    this.end(input)
                }
                EventType::KeyPress => {
                    if event
                        .downcast_ref::<gtk::gdk::KeyEvent>()
                        .is_some_and(|k| k.keyval() == gtk::gdk::Key::Escape)
                    {
                        this.cancel(None, true);
                    }
                    event
                        .downcast_ref::<gtk::gdk::KeyEvent>()
                        .is_some_and(|key| this.cancelled(&Input::Key(key.keycode())))
                }
                EventType::ButtonPress => event
                    .downcast_ref::<gtk::gdk::ButtonEvent>()
                    .is_some_and(|button| this.cancelled(&Input::Pointer(button.button()))),
                _ => false,
            };
            if cancelled {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(controller.clone());
        *self.controller.borrow_mut() = Some(controller);
    }
}
fn activation_key(key: gtk::gdk::Key) -> bool {
    use gtk::gdk::Key;
    matches!(
        key,
        Key::space | Key::KP_Space | Key::Return | Key::KP_Enter | Key::ISO_Enter
    )
}
impl Drop for HistoryInput {
    fn drop(&mut self) {
        if let Some(controller) = self.controller.get_mut().take() {
            if let Some(widget) = controller.widget() {
                widget.remove_controller(&controller);
            }
        }
    }
}
