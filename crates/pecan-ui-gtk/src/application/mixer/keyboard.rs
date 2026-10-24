//! Release observation outlives row focus, disabling and replacement. A held
//! physical key must never become a fresh gesture on a replacement row.
use super::*;
struct Held {
    row: Weak<Row>,
    cancelled: bool,
}
pub(super) struct Releases {
    held: RefCell<BTreeMap<u32, Held>>,
    controller: RefCell<Option<gtk::EventControllerLegacy>>,
}
impl Releases {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            held: RefCell::default(),
            controller: RefCell::default(),
        })
    }
    pub fn admit(&self, row: &Rc<Row>, code: u32) -> bool {
        let mut held = self.held.borrow_mut();
        if let Some(h) = held.get(&code) {
            return !h.cancelled && h.row.upgrade().is_some_and(|r| Rc::ptr_eq(&r, row));
        }
        held.insert(
            code,
            Held {
                row: Rc::downgrade(row),
                cancelled: false,
            },
        );
        true
    }
    pub fn cancel(&self, row: &Row) {
        for h in self.held.borrow_mut().values_mut() {
            if std::ptr::eq(h.row.as_ptr(), row) {
                h.cancelled = true;
            }
        }
    }
    pub fn observe(self: &Rc<Self>, root: &gtk::Box) {
        let weak = Rc::downgrade(self);
        root.connect_map(move |root| {
            if let (Some(s), Some(window)) =
                (weak.upgrade(), root.root().and_downcast::<gtk::Window>())
            {
                s.attach(&window);
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
        if let Some(c) = self.controller.borrow_mut().take() {
            if let Some(w) = c.widget() {
                w.remove_controller(&c);
            }
        }
        let c = gtk::EventControllerLegacy::new();
        c.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        c.connect_event(move |_, event| {
            if let Some(s) = weak.upgrade() {
                if event.event_type() == gdk::EventType::KeyRelease {
                    if let Some(key) = event.downcast_ref::<gdk::KeyEvent>() {
                        let h = s.held.borrow_mut().remove(&key.keycode());
                        if let Some(row) = h.and_then(|h| h.row.upgrade()) {
                            row.release_key(key.keyval());
                        }
                    }
                }
            }
            glib::Propagation::Proceed
        });
        window.add_controller(c.clone());
        *self.controller.borrow_mut() = Some(c);
    }
}
impl Drop for Releases {
    fn drop(&mut self) {
        if let Some(c) = self.controller.get_mut().take() {
            if let Some(w) = c.widget() {
                w.remove_controller(&c);
            }
        }
    }
}
