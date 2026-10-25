//! Immutable mute semantics per button. A changed observation retires only the
//! mute control, keeping the same row/fader gesture and its registry authority.
use super::*;
#[derive(Clone, PartialEq)]
struct Semantic {
    muted: bool,
    partial: bool,
    volume: Option<Vec<f32>>,
}
impl Semantic {
    fn current(strip: &Strip, session: &Session) -> Self {
        let partial = strip
            .node
            .volume
            .as_ref()
            .is_some_and(|v| strip.channels.len() != v.len());
        Self {
            muted: if partial {
                session.partial_muted(&strip.key).unwrap_or(false)
            } else {
                strip.node.mute.unwrap_or(false)
            },
            partial,
            volume: if partial {
                strip.node.volume.clone()
            } else {
                None
            },
        }
    }
}
pub(super) struct Control {
    pub button: gtk::ToggleButton,
    pub live: Rc<Cell<bool>>,
    semantic: Semantic,
}
impl Control {
    pub fn new(strip: &Strip, session: &Session) -> Self {
        let semantic = Semantic::current(strip, session);
        let button = gtk::ToggleButton::with_label("Mute");
        button.set_active(semantic.muted);
        set_name(
            &button,
            &format!("Mute for {} · {}", strip.name, strip.subtitle),
        );
        Self {
            button,
            live: Rc::new(Cell::new(true)),
            semantic,
        }
    }
}
impl Row {
    pub(super) fn mute(&self) -> gtk::ToggleButton {
        self.mute.borrow().button.clone()
    }
    pub(super) fn connect_mute(self: &Rc<Self>) {
        let Some(s) = self.owner.upgrade() else {
            return;
        };
        let control = self.mute.borrow();
        s.mute_input
            .capture(control.button.upcast_ref(), control.live.clone());
        let live = control.live.clone();
        let semantic = control.semantic.clone();
        let weak = Rc::downgrade(self);
        control.button.connect_toggled(move |button| {
            let Some(row) = weak.upgrade() else { return };
            let Some(s) = row.state() else { return };
            if !live.get()
                || !button.is_sensitive()
                || row.node_binding != s.session.node_binding(row.node)
            {
                return;
            }
            if s.mute_input.blocked() {
                s.refresh();
                return;
            }
            // The node's lifetime remains captured by the row. Use the latest exact
            // origin only while this immutable action still has the same semantics.
            let result = if semantic.partial {
                s.session
                    .toggle_partial_mute(&row.origin.borrow(), &row.key)
                    .map(|_| ())
            } else {
                s.session
                    .execute(
                        &row.origin.borrow(),
                        "Mute",
                        Action::Mute {
                            node: row.node,
                            muted: !semantic.muted,
                        },
                    )
                    .map(|_| ())
            };
            if let Err(error) = result {
                row.error(error);
            }
            s.refresh();
        });
    }
    pub(super) fn refresh_mute(self: &Rc<Self>, strip: &Strip) {
        let Some(s) = self.owner.upgrade() else {
            return;
        };
        let semantic = Semantic::current(strip, &s.session);
        let replace = {
            let old = self.mute.borrow();
            !old.live.get() || old.semantic != semantic
        };
        if replace {
            let old = self.mute.replace(Control::new(strip, &s.session));
            s.mute_input.retire_control(&old.live);
            old.live.set(false);
            let focus = old.button.has_focus();
            self.root.remove(&old.button);
            self.connect_mute();
            let button = self.mute();
            self.root.append(&button);
            button.set_sensitive(
                s.session.enabled()
                    && self.node_binding.is_some()
                    && (!self.partial || self.identity.is_some()),
            );
            // Keep a valid focus receiver while the window guard consumes the
            // cancelled release. A window with no focused child can otherwise
            // lose that release and strand the ledger/fresh activation.
            if focus && button.is_sensitive() {
                button.grab_focus();
            }
        }
        let button = self.mute();
        button.set_active(semantic.muted);
        button.set_sensitive(
            s.session.enabled()
                && self.node_binding.is_some()
                && (!self.partial || self.identity.is_some()),
        );
        set_name(
            &button,
            &format!("Mute for {} · {}", strip.name, strip.subtitle),
        );
    }
}
