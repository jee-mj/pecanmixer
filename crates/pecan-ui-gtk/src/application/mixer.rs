//! Native Session adapter for the reviewed pure fader and partial-mute policies.
//! There is no worker, timer, filesystem access or optimistic history here.
use super::{GestureToken, Graph, Session, StripIdentity};
use crate::{
    controls::fader::{Fader, Output, PointerButton},
    display_settings::{DisplayPreferences, PreferenceLayout},
    widgets::accessibility::set_name,
};
use gtk::{gdk, glib, prelude::*};
use pecan_core::domain::{
    history::Action,
    levels::{position_to_db, OFF_DB},
    mixer::{self, Strip},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Input {
    Pointer,
    Wheel,
    Keys,
    Discrete,
}
struct Row {
    root: gtk::Box,
    title: gtk::Label,
    observed: gtk::Label,
    scale: gtk::Scale,
    mute: RefCell<mute::Control>,
    key: String,
    identity: Option<StripIdentity>,
    node_binding: Option<pecan_core::history_identity::RegistryBinding>,
    origin: RefCell<Graph>,
    node: u32,
    partial: bool,
    live: Cell<bool>,
    pointer_y: Cell<Option<f64>>,
    policy: RefCell<Fader<GestureToken>>,
    active: RefCell<Option<(Input, GestureToken)>>,
    keys: RefCell<Vec<gdk::Key>>,
    key_position: Cell<Option<f64>>,
    owner: Weak<State>,
}
struct State {
    root: gtk::Box,
    layout: PreferenceLayout,
    session: Session,
    rows: RefCell<BTreeMap<String, Rc<Row>>>,
    preferences: RefCell<DisplayPreferences>,
    applying: Cell<bool>,
    error: RefCell<Option<String>>,
    status: gtk::Label,
    clock: Instant,
    keyboard: Rc<keyboard::Releases>,
    mute_input: Rc<super::history_input::HistoryInput>,
}
/// Retain this alongside its Session and call poll from the owning application's
/// clock, including while hidden. Each row captures registry identity; refreshing
/// an ordinary graph updates its exact origin without retargeting an active input.
pub struct CoordinatedMixer(Rc<State>);
impl CoordinatedMixer {
    pub fn new(session: Session) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let layout = PreferenceLayout::new();
        let status = gtk::Label::builder()
            .xalign(0.)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        root.append(&status);
        root.append(layout.widget());
        let view = Self(Rc::new(State {
            root,
            layout,
            status,
            session,
            rows: RefCell::new(BTreeMap::new()),
            preferences: RefCell::default(),
            applying: Cell::new(false),
            error: RefCell::default(),
            clock: Instant::now(),
            keyboard: keyboard::Releases::new(),
            mute_input: super::history_input::HistoryInput::new(),
        }));
        view.0.keyboard.observe(&view.0.root);
        view.0.mute_input.observe(&view.0.root);
        let weak = Rc::downgrade(&view.0);
        view.0.mute_input.notify(move || {
            if let Some(state) = weak.upgrade() {
                state.refresh();
            }
        });
        view.refresh();
        view
    }
    pub(crate) fn bind_display_edits(
        &self,
        callback: Rc<dyn Fn(crate::display_settings::DisplayEdit)>,
    ) {
        self.0.layout.edit_handler(callback);
    }
    pub(crate) fn set_display_editable(&self, enabled: bool) {
        self.0.layout.set_editable(enabled);
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.root
    }
    pub fn set_display_preferences(&self, p: DisplayPreferences) {
        self.0.layout.set_preferences(p.clone());
        *self.0.preferences.borrow_mut() = p;
        self.refresh();
    }
    pub fn poll(&self) {
        self.advance(self.0.clock.elapsed());
    }
    fn advance(&self, now: Duration) {
        let rows: Vec<_> = self.0.rows.borrow().values().cloned().collect();
        for row in rows {
            let outputs = row.policy.borrow_mut().advance(now);
            match outputs {
                Ok(out) => row.deliver(out),
                Err(e) => row.error(format!("Fader clock: {e:?}")),
            }
        }
        self.refresh();
    }
    pub fn refresh(&self) {
        self.0.refresh();
    }
}
impl State {
    fn refresh(self: &Rc<Self>) {
        self.applying.set(true);
        let model = self.session.volume_model();
        let origin = self.session.graph();
        let preferences = self.preferences.borrow();
        let items: Vec<_> = model
            .apps
            .iter()
            .chain(&model.hardware)
            .chain(&model.destinations)
            .filter(|s| preferences.strip_visible(s))
            .collect();
        let mut rows = self.rows.borrow_mut();
        rows.retain(|key, row| {
            if items.iter().any(|s| {
                &s.key == key
                    && self.session.strip_identity(key).ok() == row.identity
                    && (if s.bus.is_none() {
                        self.session.node_binding(row.node)
                    } else {
                        None
                    }) == row.node_binding
            }) {
                true
            } else {
                row.retire();
                if let Some(parent) = row.root.parent().and_downcast::<gtk::Box>() {
                    parent.remove(&row.root);
                }
                false
            }
        });
        if let Some(origin) = origin {
            let mut previous: [Option<gtk::Widget>; 4] = std::array::from_fn(|_| None);
            for strip in items {
                let row = rows
                    .entry(strip.key.clone())
                    .or_insert_with(|| Row::new(self, strip, origin.clone()));
                *row.origin.borrow_mut() = origin.clone();
                row.title
                    .set_text(&format!("{} · {}", strip.name, strip.subtitle));
                let db = mixer::strip_db(strip);
                if let Some(db) = db {
                    let _ = row.policy.borrow_mut().observe(db);
                }
                if row
                    .active
                    .borrow()
                    .as_ref()
                    .is_some_and(|(_, t)| !self.session.volume_active(t))
                {
                    // The receipt ended/failed, not the physical key. Keep its
                    // ledger entry cancelled until an actual window release.
                    self.keyboard.cancel(row);
                    row.active.borrow_mut().take();
                    row.keys.borrow_mut().clear();
                    row.key_position.set(None);
                    let _ = row.policy.borrow_mut().invalidate();
                }
                row.observed.set_text(&match db {
                    Some(db) if db <= OFF_DB => "Observed: −∞ dB".into(),
                    Some(db) => format!("Observed: {db:.1} dB"),
                    None => "Observed gain unavailable".into(),
                });
                // The thumb is the legacy local relative-drag position. The separate label
                // always displays the observed graph, never a submitted target as completion.
                row.scale.set_value(
                    row.key_position
                        .get()
                        .unwrap_or_else(|| row.policy.borrow().position()),
                );
                row.scale.set_sensitive(
                    db.is_some()
                        && row.identity.is_some()
                        && self.session.volume_input_available(&strip.key),
                );
                row.refresh_mute(strip);
                let lane = PreferenceLayout::lane_index(strip);
                self.layout
                    .lane(lane)
                    .reorder_child_after(&row.root, previous[lane].as_ref());
                previous[lane] = Some(row.root.clone().upcast());
            }
        }
        self.layout.changed();
        self.status
            .set_text(&self.error.borrow().as_ref().map_or_else(
                || self.session.status().message(),
                |e| format!("{} · {e}", self.session.status().message()),
            ));
        self.applying.set(false);
    }
}
impl Drop for State {
    fn drop(&mut self) {
        for row in self.rows.get_mut().values() {
            // Weak<State> cannot upgrade once this destructor starts. Retire
            // through the retained Session explicitly, including a held input
            // that has not submitted its first sample yet.
            if let Some((_, token)) = row.active.borrow().as_ref() {
                self.session.abandon_volume(token);
            }
            row.retire();
        }
    }
}

impl Row {
    fn new(state: &Rc<State>, strip: &Strip, origin: Graph) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = gtk::Label::builder().xalign(0.).wrap(true).build();
        let observed = gtk::Label::builder().xalign(0.).build();
        let scale = gtk::Scale::with_range(gtk::Orientation::Vertical, 0., 1., 0.005);
        scale.set_inverted(true);
        scale.set_draw_value(false);
        scale.set_size_request(72, 160);
        scale.set_vexpand(true);
        scale.set_hexpand(false);
        let mute = mute::Control::new(strip, &state.session);
        root.append(&title);
        root.append(&observed);
        root.append(&scale);
        root.append(&mute.button);
        state
            .layout
            .lane(PreferenceLayout::lane_index(strip))
            .append(&root);
        set_name(
            &scale,
            &format!("Volume for {} · {}", strip.name, strip.subtitle),
        );
        set_name(
            &mute.button,
            &format!("Mute for {} · {}", strip.name, strip.subtitle),
        );
        let row = Rc::new(Self {
            root,
            title,
            observed,
            scale,
            mute: RefCell::new(mute),
            key: strip.key.clone(),
            identity: state.session.strip_identity(&strip.key).ok(),
            node_binding: if strip.bus.is_none() {
                state.session.node_binding(strip.node.id)
            } else {
                None
            },
            origin: RefCell::new(origin),
            node: strip.node.id,
            partial: strip
                .node
                .volume
                .as_ref()
                .is_some_and(|v| strip.channels.len() != v.len()),
            live: Cell::new(true),
            pointer_y: Cell::new(None),
            policy: RefCell::new(Fader::new(mixer::strip_db(strip).unwrap_or(OFF_DB)).unwrap()),
            active: RefCell::default(),
            keys: RefCell::default(),
            key_position: Cell::new(None),
            owner: Rc::downgrade(state),
        });
        row.connect();
        row.connect_mute();
        row
    }
    fn state(&self) -> Option<Rc<State>> {
        let s = self.owner.upgrade()?;
        (self.live.get() && !s.applying.get()).then_some(s)
    }
    fn error(&self, error: String) {
        if let Some(s) = self.owner.upgrade() {
            *s.error.borrow_mut() = Some(error);
        }
    }
    fn begin(&self, input: Input) -> Result<GestureToken, String> {
        let s = self.state().ok_or("Retired volume input.")?;
        if let Some((kind, t)) = self.active.borrow().as_ref() {
            return if *kind == input
                && matches!(input, Input::Wheel | Input::Keys)
                && s.session.volume_input_available(&self.key)
            {
                Ok(t.clone())
            } else {
                Err("Finish the current input before starting another gesture.".into())
            };
        }
        if !self.scale.is_sensitive()
            || self.identity.as_ref() != s.session.strip_identity(&self.key).ok().as_ref()
        {
            return Err("Volume binding is unavailable.".into());
        }
        let t = s.session.begin_volume(&self.origin.borrow(), &self.key)?;
        *self.active.borrow_mut() = Some((input, t.clone()));
        *s.error.borrow_mut() = None;
        Ok(t)
    }
    fn deliver(&self, out: Vec<Output<GestureToken>>) {
        let Some(s) = self.owner.upgrade() else {
            return;
        };
        for output in out {
            let result = match output {
                Output::Begin { .. } => Ok(()),
                Output::Update { token, db } => s.session.update_volume(&token, db),
                Output::End { token, .. } => s.session.end_volume(&token),
                Output::Abandon { token } => {
                    s.session.abandon_volume(&token);
                    Ok(())
                }
            };
            if let Err(e) = result {
                self.error(e);
            }
        }
    }
    fn pointer_down(&self, y: f64) {
        self.pointer_y.set(Some(y));
        match self.begin(Input::Pointer) {
            Ok(t) => {
                let out = self
                    .policy
                    .borrow_mut()
                    .pointer_down(t, PointerButton::Primary, y);
                if let Ok(out) = out {
                    self.deliver(out);
                }
            }
            Err(e) => self.error(e),
        }
    }
    fn pointer_move(&self, y: f64, shift: bool) {
        // GTK can emit an initial drag-update at the press coordinates. It is
        // not a legacy pointermove and must not flatten/round untouched gains.
        if self.pointer_y.replace(Some(y)) == Some(y) {
            return;
        }
        let out = self.policy.borrow_mut().pointer_move(
            y,
            Some(f64::from(self.scale.height().max(25))),
            shift,
        );
        match out {
            Ok(out) => self.deliver(out),
            Err(e) => self.error(format!("Fader input: {e:?}")),
        }
    }
    fn pointer_end(&self, cancel: bool) {
        self.pointer_y.set(None);
        let out = if cancel {
            self.policy.borrow_mut().pointer_cancel()
        } else {
            self.policy.borrow_mut().pointer_up()
        };
        self.deliver(out);
    }
    fn wheel(&self, dy: f64, shift: bool, now: Duration) {
        match self.begin(Input::Wheel) {
            Ok(t) => {
                let out = self.policy.borrow_mut().wheel(t, dy, shift, now);
                match out {
                    Ok(out) => self.deliver(out),
                    Err(e) => self.error(format!("Wheel input: {e:?}")),
                }
            }
            Err(e) => self.error(e),
        }
    }
    fn reset(&self) {
        self.pointer_end(false);
        if let Some(s) = self.owner.upgrade() {
            if self
                .active
                .borrow()
                .as_ref()
                .is_some_and(|(_, t)| !s.session.volume_active(t))
            {
                self.active.borrow_mut().take();
            }
        }
        match self.begin(Input::Discrete) {
            Ok(t) => {
                let out = self.policy.borrow().double_click(t);
                self.deliver(out)
            }
            Err(e) => self.error(e),
        }
    }
    fn discrete(&self, db: f64) {
        match self.begin(Input::Discrete) {
            Ok(t) => {
                self.deliver(vec![
                    Output::Update {
                        token: t.clone(),
                        db,
                    },
                    Output::End {
                        token: t,
                        db,
                        reason: crate::controls::fader::End::Released,
                    },
                ]);
            }
            Err(e) => self.error(e),
        }
    }
    fn key(&self, key: gdk::Key, shift: bool) -> bool {
        if !matches!(
            key,
            gdk::Key::Up
                | gdk::Key::Down
                | gdk::Key::Left
                | gdk::Key::Right
                | gdk::Key::Home
                | gdk::Key::End
        ) {
            return false;
        }
        match self.begin(Input::Keys) {
            Ok(t) => {
                if !self.keys.borrow().contains(&key) {
                    self.keys.borrow_mut().push(key);
                }
                let position = match key {
                    gdk::Key::Home => 0.,
                    gdk::Key::End => 1.,
                    _ => {
                        let sign = if matches!(key, gdk::Key::Up | gdk::Key::Right) {
                            1.
                        } else {
                            -1.
                        };
                        (self
                            .key_position
                            .get()
                            .unwrap_or_else(|| self.scale.value())
                            + sign * if shift { 0.001 } else { 0.01 })
                        .clamp(0., 1.)
                    }
                };
                self.key_position.set(Some(position));
                if let Some(s) = self.owner.upgrade() {
                    s.applying.set(true);
                    self.scale.set_value(position);
                    s.applying.set(false);
                }
                self.deliver(vec![Output::Update {
                    token: t,
                    db: position_to_db(position),
                }]);
            }
            Err(e) => self.error(e),
        }
        true
    }
    fn release_key(&self, key: gdk::Key) {
        self.keys.borrow_mut().retain(|k| *k != key);
        if self.keys.borrow().is_empty() {
            self.end_keys();
        }
    }
    fn end_keys(&self) {
        self.keys.borrow_mut().clear();
        if let Some((Input::Keys, t)) = self.active.borrow().as_ref() {
            if let Some(s) = self.owner.upgrade() {
                let _ = s.session.end_volume(t);
            }
        }
    }
    fn retire(&self) {
        if self.live.replace(false) {
            self.mute.borrow().live.set(false);
            if let Some(s) = self.owner.upgrade() {
                s.keyboard.cancel(self);
                s.mute_input.retire_control(&self.mute.borrow().live);
            }
            let out = self.policy.borrow_mut().invalidate();
            self.deliver(out);
            if let Some((_, t)) = self.active.borrow_mut().take() {
                if let Some(s) = self.owner.upgrade() {
                    s.session.abandon_volume(&t);
                }
            }
        }
    }
    fn connect(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |gesture, _, y| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                r.scale.grab_focus();
                r.pointer_down(y);
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |gesture, _, dy| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                let y = gesture.start_point().map_or(dy, |(_, y)| y + dy);
                r.pointer_move(
                    y,
                    gesture
                        .current_event_state()
                        .contains(gdk::ModifierType::SHIFT_MASK),
                );
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(r) = weak.upgrade() {
                r.pointer_end(false);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            if let Some(r) = weak.upgrade() {
                r.pointer_end(true);
            }
        });
        let click = gtk::GestureClick::new();
        click.group_with(&drag);
        self.scale.add_controller(drag);
        click.set_button(1);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, n, _, _| {
            if n == 2 {
                if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                    r.reset();
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
        self.scale.add_controller(click);
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |control, _, dy| {
            if let Some(r) = weak.upgrade() {
                if let Some(s) = r.state() {
                    r.wheel(
                        dy,
                        control
                            .current_event_state()
                            .contains(gdk::ModifierType::SHIFT_MASK),
                        s.clock.elapsed(),
                    );
                    s.refresh();
                }
            }
            glib::Propagation::Stop
        });
        self.scale.add_controller(scroll);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, code, mods| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                if !matches!(
                    key,
                    gdk::Key::Up
                        | gdk::Key::Down
                        | gdk::Key::Left
                        | gdk::Key::Right
                        | gdk::Key::Home
                        | gdk::Key::End
                ) {
                    return glib::Propagation::Proceed;
                }
                if let Some(s) = r.owner.upgrade() {
                    if !s.keyboard.admit(&r, code) {
                        return glib::Propagation::Stop;
                    }
                }
                if r.key(key, mods.contains(gdk::ModifierType::SHIFT_MASK)) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(self);
        keys.connect_key_released(move |_, key, _, _| {
            if let Some(r) = weak.upgrade() {
                r.release_key(key);
            }
        });
        self.scale.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                if let Some(s) = r.owner.upgrade() {
                    s.keyboard.cancel(&r);
                }
                r.pointer_end(true);
                r.end_keys();
            }
        });
        self.scale.add_controller(focus);
        let weak = Rc::downgrade(self);
        self.root.connect_unmap(move |_| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                if let Some(s) = r.owner.upgrade() {
                    s.keyboard.cancel(&r);
                }
                r.pointer_end(true);
                r.end_keys();
            }
        });
        let weak = Rc::downgrade(self);
        self.scale.connect_value_changed(move |scale| {
            if let Some(r) = weak.upgrade().filter(|r| r.state().is_some()) {
                if r.active.borrow().is_none() {
                    r.discrete(position_to_db(scale.value()));
                }
            }
        });
    }
}
#[cfg(test)]
mod tests;

mod keyboard;
#[cfg(all(test, feature = "x11-input-tests"))]
mod xtest;

mod mute;
