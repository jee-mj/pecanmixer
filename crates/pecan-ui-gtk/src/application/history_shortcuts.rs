//! Window shortcuts only: history authority and dispatch stay in the existing
//! guarded buttons/Session. A physical key gets one attempt until its release,
//! including attempts begun in an editor, while busy, or before Ctrl is pressed.
//!
//! Bindings follow src/App.tsx's window keydown handler. Unlike the legacy JS
//! repeat queue, a held key never adopts another history entry after completion
//! or replacement; release and a fresh press are required. Reparenting/unmapping
//! also retain this input latch. No new history state, timer or backend consumer.
use gtk::{gdk, glib, prelude::*};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

pub(super) struct HistoryShortcuts {
    held: RefCell<BTreeSet<u32>>,
    controller: RefCell<Option<gtk::EventControllerLegacy>>,
    activate: Box<dyn Fn(bool)>,
}
impl HistoryShortcuts {
    pub fn new(root: &impl IsA<gtk::Widget>, activate: impl Fn(bool) + 'static) -> Rc<Self> {
        let this = Rc::new(Self {
            held: RefCell::default(),
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
        let controller = gtk::EventControllerLegacy::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let root = root.downgrade();
        let weak_window = window.downgrade();
        controller.connect_event(move |_, event| {
            let (Some(this), Some(window), Some(root)) =
                (weak.upgrade(), weak_window.upgrade(), root.upgrade())
            else {
                return glib::Propagation::Proceed;
            };
            let Some(key) = event.downcast_ref::<gdk::KeyEvent>() else {
                return glib::Propagation::Proceed;
            };
            match event.event_type() {
                gdk::EventType::KeyRelease => {
                    this.held.borrow_mut().remove(&key.keycode());
                }
                gdk::EventType::KeyPress if root.is_mapped() => {
                    let letter = key.keyval().to_unicode().map(|c| c.to_ascii_lowercase());
                    if !matches!(letter, Some('z' | 'y')) {
                        return glib::Propagation::Proceed;
                    }
                    let fresh = this.held.borrow_mut().insert(key.keycode());
                    if let Some(undo) = binding(letter, key.modifier_state()) {
                        if !editing(gtk::prelude::GtkWindowExt::focus(&window)) {
                            // No delayed activation survives focus loss, refresh,
                            // replacement or a change in modifiers. Do not clear
                            // held keys on unmap/refocus: only release rearms them.
                            if fresh {
                                (this.activate)(undo);
                            }
                            return glib::Propagation::Stop;
                        }
                    }
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
        window.add_controller(controller.clone());
        *self.controller.borrow_mut() = Some(controller);
    }
}
fn binding(letter: Option<char>, modifiers: gdk::ModifierType) -> Option<bool> {
    // Browser metaKey includes the Linux Super/Windows key as well as Meta.
    if !modifiers.intersects(
        gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::SUPER_MASK,
    ) {
        return None;
    }
    match letter {
        Some('z') => Some(!modifiers.contains(gdk::ModifierType::SHIFT_MASK)),
        Some('y') => Some(false),
        _ => None,
    }
}
#[allow(deprecated)] // ComboBox remains supported by our GTK 4.0 API floor.
fn editing(mut focus: Option<gtk::Widget>) -> bool {
    while let Some(widget) = focus {
        // Native equivalents of the JS INPUT/SELECT/TEXTAREA exclusion,
        // including internal child widgets of composite input controls.
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
        for modifier in [M::CONTROL_MASK, M::META_MASK, M::SUPER_MASK] {
            assert_eq!(binding(Some('z'), modifier), Some(true));
            assert_eq!(binding(Some('z'), modifier | M::SHIFT_MASK), Some(false));
            assert_eq!(binding(Some('y'), modifier), Some(false));
            assert_eq!(binding(Some('y'), modifier | M::SHIFT_MASK), Some(false));
            assert_eq!(binding(Some('x'), modifier), None);
        }
        assert_eq!(binding(Some('z'), M::empty()), None);
        assert_eq!(binding(Some('y'), M::SHIFT_MASK), None);
    }
    #[gtk::test]
    #[allow(deprecated)] // Exercise the select control at the GTK 4.0 API floor.
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
