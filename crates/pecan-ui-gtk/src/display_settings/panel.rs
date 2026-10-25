use super::{DisplayEdit, RowGrip};
use crate::widgets::accessibility::set_name;
use gtk::prelude::*;
use pecan_core::{
    domain::mixer::Mixer,
    settings::{HiddenKey, SettingsV1},
};
use std::{cell::RefCell, rc::Rc};
pub(crate) enum PanelAction {
    Edit(DisplayEdit),
    Reload,
    Save,
    ReviewRecovery,
    ConfirmRecovery,
}
struct State {
    root: gtk::Expander,
    choices: gtk::Box,
    status: gtk::Label,
    callback: RefCell<Option<Rc<dyn Fn(PanelAction)>>>,
    document: RefCell<SettingsV1>,
    fingerprint: RefCell<Option<(u64, Vec<(String, String)>, bool)>>,
    confirm: gtk::Button,
    review: gtk::Label,
}
#[derive(Clone)]
pub(crate) struct DisplayPanel(Rc<State>);
impl DisplayPanel {
    pub fn new() -> Self {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let status = gtk::Label::builder()
            .xalign(0.)
            .wrap(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        body.append(&status);
        let choices = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&choices)
            .max_content_height(280)
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        body.append(&scroll);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let reload = gtk::Button::with_label("Reload settings");
        let save = gtk::Button::with_label("Save local edits");
        let review = gtk::Button::with_label("Review recovery");
        let confirm = gtk::Button::with_label("Confirm reviewed recovery");
        for b in [&reload, &save, &review, &confirm] {
            tools.append(b);
        }
        body.append(&tools);
        let review_text = gtk::Label::builder()
            .xalign(0.)
            .wrap(true)
            .selectable(true)
            .build();
        let review_scroll = gtk::ScrolledWindow::builder()
            .child(&review_text)
            .max_content_height(180)
            .propagate_natural_height(true)
            .build();
        body.append(&review_scroll);
        let root = gtk::Expander::builder()
            .label("Display preferences")
            .child(&body)
            .build();
        let view = Self(Rc::new(State {
            root,
            choices,
            status,
            callback: RefCell::default(),
            document: RefCell::default(),
            fingerprint: RefCell::default(),
            confirm,
            review: review_text,
        }));
        for (button, action) in [(reload, 0), (save, 1), (review, 2)] {
            let weak = Rc::downgrade(&view.0);
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.emit(match action {
                        0 => PanelAction::Reload,
                        1 => PanelAction::Save,
                        _ => PanelAction::ReviewRecovery,
                    });
                }
            });
        }
        let weak = Rc::downgrade(&view.0);
        view.0.confirm.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.emit(PanelAction::ConfirmRecovery);
            }
        });
        view
    }
    pub fn widget(&self) -> &gtk::Expander {
        &self.0.root
    }
    pub fn bind(&self, callback: Rc<dyn Fn(PanelAction)>) {
        *self.0.callback.borrow_mut() = Some(callback);
    }
    pub fn update(
        &self,
        doc: &SettingsV1,
        revision: u64,
        model: &Mixer,
        editable: bool,
        message: &str,
        review: Option<&str>,
    ) {
        self.0.status.set_text(message);
        self.0.confirm.set_visible(review.is_some());
        self.0.review.set_text(review.unwrap_or(""));
        *self.0.document.borrow_mut() = doc.clone();
        let strips: Vec<_> = model
            .apps
            .iter()
            .chain(&model.hardware)
            .chain(&model.destinations)
            .map(|s| (s.key.clone(), format!("{} · {}", s.name, s.subtitle)))
            .collect();
        let fingerprint = (revision, strips.clone(), editable);
        if self.0.fingerprint.borrow().as_ref() == Some(&fingerprint) {
            return;
        }
        *self.0.fingerprint.borrow_mut() = Some(fingerprint);
        while let Some(c) = self.0.choices.first_child() {
            self.0.choices.remove(&c);
        }
        let rows = [
            ("rij|apps", "Apps"),
            ("rij|hardware", "Hardware in"),
            ("rij|bestemmingen", "Destinations"),
        ];
        for (key, label) in rows
            .into_iter()
            .map(|(k, l)| (k.to_string(), l.to_string()))
            .chain(strips)
        {
            let Ok(key) = HiddenKey::new(key) else {
                continue;
            };
            let check = gtk::CheckButton::with_label(&format!("Show {label}"));
            check.set_active(!doc.preferences().hidden.contains(&key));
            check.set_sensitive(editable);
            set_name(&check, &format!("Show {label}"));
            let weak = Rc::downgrade(&self.0);
            check.connect_toggled(move |c| {
                if c.is_sensitive() {
                    if let Some(s) = weak.upgrade() {
                        s.emit(PanelAction::Edit(DisplayEdit::Hidden {
                            key: key.clone(),
                            hidden: !c.is_active(),
                        }));
                    }
                }
            });
            self.0.choices.append(&check);
        }
        for (label, grip) in [
            ("Equal Apps / Hardware", Some(RowGrip::AppsHardware)),
            (
                "Equal Hardware / Destinations",
                Some(RowGrip::HardwareDestinations),
            ),
            ("Fit output width", None),
        ] {
            let button = gtk::Button::with_label(label);
            button.set_sensitive(editable);
            let weak = Rc::downgrade(&self.0);
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    let edit = grip.map_or(DisplayEdit::DestinationWidth(None), |g| {
                        DisplayEdit::Rows(g.equal(s.document.borrow().preferences().rows))
                    });
                    s.emit(PanelAction::Edit(edit));
                }
            });
            self.0.choices.append(&button);
        }
    }
}
impl State {
    fn emit(&self, action: PanelAction) {
        let f = self.callback.borrow().clone();
        if let Some(f) = f {
            f(action);
        }
    }
}
