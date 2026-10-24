//! Value-key lifetimes outlive widget focus. Watch releases on the containing
//! window, including when pending controls are disabled and none has focus.
use super::{
    gesture::{GestureOrigin, Input},
    Adapter,
};
use gtk::prelude::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy)]
pub(super) enum KeyboardControl {
    Fader,
    Mute,
}
impl KeyboardControl {
    pub(super) fn accepts(self, key: gtk::gdk::Key) -> bool {
        use gtk::gdk::Key;
        match self {
            Self::Fader => matches!(
                key,
                Key::Left
                    | Key::Right
                    | Key::Up
                    | Key::Down
                    | Key::Home
                    | Key::End
                    | Key::Page_Up
                    | Key::Page_Down
                    | Key::KP_Left
                    | Key::KP_Right
                    | Key::KP_Up
                    | Key::KP_Down
                    | Key::KP_Home
                    | Key::KP_End
                    | Key::KP_Page_Up
                    | Key::KP_Page_Down
            ),
            Self::Mute => matches!(key, Key::space | Key::Return | Key::KP_Enter),
        }
    }
}
pub(super) struct KeyboardRelease<A: Adapter> {
    held: RefCell<BTreeMap<u32, Weak<RefCell<GestureOrigin<A>>>>>,
    controller: RefCell<Option<gtk::EventControllerLegacy>>,
}
impl<A: Adapter + 'static> KeyboardRelease<A> {
    pub(super) fn new() -> Self {
        Self {
            held: RefCell::new(BTreeMap::new()),
            controller: RefCell::new(None),
        }
    }
    pub(super) fn begin(&self, key: u32, origin: &Rc<RefCell<GestureOrigin<A>>>) {
        let mut held = self.held.borrow_mut();
        if let Some(previous) = held.get(&key) {
            if previous
                .upgrade()
                .is_some_and(|previous| Rc::ptr_eq(&previous, origin))
            {
                origin.borrow_mut().begin(Input::Key(key));
            } else {
                // Repeated input cannot migrate to a different/replacement
                // control while the original physical key remains held.
                origin.borrow_mut().cancel();
            }
        } else {
            held.insert(key, Rc::downgrade(origin));
            origin.borrow_mut().begin(Input::Key(key));
        }
    }
    pub(super) fn end(&self, key: u32) {
        if let Some(origin) = self
            .held
            .borrow_mut()
            .remove(&key)
            .and_then(|origin| origin.upgrade())
        {
            origin.borrow_mut().end(&Input::Key(key));
        }
    }
    pub(super) fn observe_window(self: &Rc<Self>, root: &impl IsA<gtk::Widget>) {
        let weak = Rc::downgrade(self);
        root.connect_map(move |root| {
            if let (Some(this), Some(window)) =
                (weak.upgrade(), root.root().and_downcast::<gtk::Window>())
            {
                this.attach(&window);
            }
        });
    }
    fn attach(self: &Rc<Self>, window: &gtk::Window) {
        if self
            .controller
            .borrow()
            .as_ref()
            .is_some_and(|controller| controller.widget().as_ref() == Some(window.upcast_ref()))
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
            if event.event_type() == gtk::gdk::EventType::KeyRelease {
                if let (Some(this), Some(key)) =
                    (weak.upgrade(), event.downcast_ref::<gtk::gdk::KeyEvent>())
                {
                    this.end(key.keycode());
                }
            }
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(controller.clone());
        *self.controller.borrow_mut() = Some(controller);
    }
}
impl<A: Adapter> Drop for KeyboardRelease<A> {
    fn drop(&mut self) {
        if let Some(controller) = self.controller.get_mut().take() {
            if let Some(widget) = controller.widget() {
                widget.remove_controller(&controller);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::{
        tests::{app_binding, setup, Fake},
        ControlError,
    };
    #[test]
    fn release_elsewhere_ends_cancelled_key_and_allows_a_fresh_press() {
        let (mut c, _, mut next) = setup();
        let origin = Rc::new(RefCell::new(GestureOrigin::<Fake>::new(app_binding(&c))));
        let keys = KeyboardRelease::new();
        keys.begin(42, &origin);
        origin.borrow_mut().cancel();
        next.graph.stamp.revision += 1;
        c.observe(next);
        origin.borrow_mut().present(app_binding(&c));
        keys.begin(42, &origin);
        assert!(matches!(
            origin.borrow().binding(),
            Err(ControlError::StaleView)
        ));
        keys.end(42);
        keys.begin(42, &origin);
        assert!(origin.borrow().binding().is_ok());
    }
    #[test]
    fn held_key_cannot_migrate_to_another_or_replacement_control() {
        let (c, _, _) = setup();
        let first = Rc::new(RefCell::new(GestureOrigin::<Fake>::new(app_binding(&c))));
        let replacement = Rc::new(RefCell::new(GestureOrigin::<Fake>::new(app_binding(&c))));
        let keys = KeyboardRelease::new();
        keys.begin(42, &first);
        first.borrow_mut().retire();
        drop(first);
        keys.begin(42, &replacement);
        assert!(matches!(
            replacement.borrow().binding(),
            Err(ControlError::StaleView)
        ));
        keys.end(42);
        keys.begin(42, &replacement);
        assert!(replacement.borrow().binding().is_ok());
    }
    #[test]
    fn navigation_and_modifiers_never_become_value_gestures() {
        use gtk::gdk::Key;
        for key in [
            Key::Tab,
            Key::ISO_Left_Tab,
            Key::Shift_L,
            Key::Control_L,
            Key::Alt_L,
        ] {
            assert!(!KeyboardControl::Fader.accepts(key));
            assert!(!KeyboardControl::Mute.accepts(key));
        }
        assert!(KeyboardControl::Fader.accepts(Key::Right));
        assert!(KeyboardControl::Mute.accepts(Key::space));
    }
}
