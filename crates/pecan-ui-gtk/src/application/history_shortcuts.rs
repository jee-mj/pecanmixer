//! Standard window-scoped GTK activations matching src/App.tsx keydowns.
//!
//! Each delivered activation validates the current rendered context and enqueues
//! a direction on the one workspace history queue, as in JS. Execution selects
//! the then-current guarded history through Session, after receipt/graph barriers
//! and minimum spacing. Stale/blocked input is rejected, never retried. Pointer/
//! Space/Enter lifetime guards remain
//! separate. There is no global pressed-key tracking or platform input hook.
use gtk::{gdk, glib, prelude::*};
use std::{cell::RefCell, rc::Rc};

pub(super) struct HistoryShortcuts {
    controller: RefCell<Option<gtk::ShortcutController>>,
    activate: Box<dyn Fn(bool)>,
}
impl HistoryShortcuts {
    pub fn new(root: &impl IsA<gtk::Widget>, activate: impl Fn(bool) + 'static) -> Rc<Self> {
        let this = Rc::new(Self {
            controller: RefCell::default(),
            activate: Box::new(activate),
        });
        let weak = Rc::downgrade(&this);
        root.connect_map(move |root| {
            if let (Some(this), Some(window)) =
                (weak.upgrade(), root.root().and_downcast::<gtk::Window>())
            {
                this.attach(&window, root.as_ref());
            }
        });
        this
    }
    fn attach(self: &Rc<Self>, window: &gtk::Window, root: &gtk::Widget) {
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
        let controller = gtk::ShortcutController::new();
        controller.set_scope(gtk::ShortcutScope::Local);
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        for undo in [true, false] {
            let weak = Rc::downgrade(self);
            let root = root.downgrade();
            let window = window.downgrade();
            let action = gtk::CallbackAction::new(move |_, _| {
                let (Some(this), Some(window), Some(root)) =
                    (weak.upgrade(), window.upgrade(), root.upgrade())
                else {
                    return glib::Propagation::Proceed;
                };
                if !root.is_mapped() || editing(gtk::prelude::GtkWindowExt::focus(&window)) {
                    return glib::Propagation::Proceed;
                }
                (this.activate)(undo);
                glib::Propagation::Stop
            });
            for (key, modifiers, direction) in bindings().into_iter().filter(|b| b.2 == undo) {
                debug_assert_eq!(direction, undo);
                controller.add_shortcut(gtk::Shortcut::new(
                    Some(gtk::KeyvalTrigger::new(key, modifiers)),
                    Some(action.clone()),
                ));
            }
        }
        window.add_controller(controller.clone());
        *self.controller.borrow_mut() = Some(controller);
    }
}
fn bindings() -> Vec<(gdk::Key, gdk::ModifierType, bool)> {
    use gdk::ModifierType as M;
    // JS accepts Ctrl OR Meta (including Linux Super), ignores extra modifiers,
    // and lets Shift select Redo for Z. Exact GTK key triggers need these fixed
    // aliases; lock modifiers are ignored by GTK. 112 triggers, two actions.
    let flags = [
        M::CONTROL_MASK,
        M::META_MASK,
        M::SUPER_MASK,
        M::SHIFT_MASK,
        M::ALT_MASK,
        M::HYPER_MASK,
    ];
    let mut result = Vec::new();
    for bits in 0..64 {
        let modifiers = flags.iter().enumerate().fold(M::empty(), |all, (i, flag)| {
            if bits & (1 << i) != 0 {
                all | *flag
            } else {
                all
            }
        });
        if modifiers.intersects(M::CONTROL_MASK | M::META_MASK | M::SUPER_MASK) {
            result.push((gdk::Key::z, modifiers, !modifiers.contains(M::SHIFT_MASK)));
            result.push((gdk::Key::y, modifiers, false));
        }
    }
    result
}
#[allow(deprecated)] // ComboBox remains supported by our GTK 4.0 API floor.
fn editing(mut focus: Option<gtk::Widget>) -> bool {
    while let Some(widget) = focus {
        // Native equivalents of JS INPUT/SELECT/TEXTAREA, including internal
        // child widgets of composite input controls.
        if widget.is::<gtk::Editable>()
            || widget.is::<gtk::TextView>()
            || widget.is::<gtk::DropDown>()
            || widget.is::<gtk::ComboBox>()
            || widget.is::<gtk::Range>()
            || widget.is::<gtk::CheckButton>()
            || widget.is::<gtk::Switch>()
        {
            return true;
        }
        focus = widget.parent();
    }
    false
}
impl Drop for HistoryShortcuts {
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
    #[test]
    fn original_js_history_binding_table() {
        use gdk::ModifierType as M;
        let bindings = bindings();
        assert_eq!(bindings.len(), 112);
        for modifier in [
            M::CONTROL_MASK,
            M::META_MASK,
            M::SUPER_MASK,
            M::CONTROL_MASK | M::META_MASK,
        ] {
            assert!(bindings.contains(&(gdk::Key::z, modifier, true)));
            assert!(bindings.contains(&(gdk::Key::z, modifier | M::SHIFT_MASK, false)));
            assert!(bindings.contains(&(gdk::Key::y, modifier, false)));
            assert!(bindings.contains(&(gdk::Key::y, modifier | M::SHIFT_MASK, false)));
            assert!(bindings.contains(&(gdk::Key::z, modifier | M::ALT_MASK, true)));
        }
        assert!(!bindings
            .iter()
            .any(|b| b.1.is_empty() || b.1 == M::SHIFT_MASK));
    }
    #[gtk::test]
    #[allow(deprecated)]
    fn native_input_controls_keep_local_history() {
        assert!(editing(Some(gtk::Entry::new().upcast())));
        assert!(editing(Some(gtk::TextView::new().upcast())));
        assert!(editing(Some(
            gtk::DropDown::from_strings(&["Synthetic"]).upcast()
        )));
        assert!(editing(Some(gtk::ComboBoxText::new().upcast())));
        assert!(editing(Some(gtk::CheckButton::new().upcast())));
        assert!(editing(Some(gtk::Switch::new().upcast())));
        assert!(editing(Some(
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0., 1., 0.1).upcast()
        )));
        assert!(!editing(Some(
            gtk::Button::with_label("Synthetic focus").upcast()
        )));
        assert!(!editing(None));
    }
}
