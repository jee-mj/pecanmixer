//! One routing surface over BusOwner selection and Session receipts/history.
use super::{buses::RoutingKey, BusOrigin, BusOwner, Request, Session};
use crate::routing::{Event, RequestToken, RoutingControls};
use pecan_core::{
    domain::history::Input,
    native_command::{NativeProgress, NativeReport},
};
use std::{cell::RefCell, rc::Rc};

struct Tracked {
    token: RequestToken,
    request: Request,
    last: Option<NativeReport>,
}
pub(super) struct Routes {
    session: Session,
    buses: RefCell<Rc<BusOwner>>,
    controls: RoutingControls<BusOrigin>,
    publication: RefCell<Option<Option<RoutingKey>>>,
    pending: RefCell<Option<Tracked>>,
    // One last terminal diagnostic, not an unbounded per-tick report queue.
    completed: RefCell<Option<Tracked>>,
    navigate: RefCell<Option<Box<dyn Fn()>>>,
}
impl Routes {
    pub fn new(session: Session, buses: Rc<BusOwner>) -> Rc<Self> {
        let routes = Rc::new(Self {
            session,
            buses: RefCell::new(buses),
            controls: RoutingControls::new(),
            publication: RefCell::new(None),
            pending: RefCell::new(None),
            completed: RefCell::new(None),
            navigate: RefCell::new(None),
        });
        let weak = Rc::downgrade(&routes);
        routes.controls.bind_intents(move |event| {
            weak.upgrade()
                .ok_or("Routing owner is no longer available.")?
                .handle(event)
        });
        routes.refresh_capture(true);
        routes
    }
    pub fn widget(&self) -> &gtk::Box {
        self.controls.widget()
    }
    pub fn bind_navigation(&self, callback: impl Fn() + 'static) {
        *self.navigate.borrow_mut() = Some(Box::new(callback));
    }
    fn handle(&self, event: Event<BusOrigin>) -> Result<(), String> {
        let buses = self.buses.borrow().clone();
        match event {
            Event::Selection {
                origin,
                graph,
                binding,
                key,
                ..
            } => {
                buses.routing_selection(&origin, &graph, &binding, &key)?;
                self.refresh_capture(true);
                Ok(())
            }
            Event::OpenPatchbay { origin, graph, .. } => {
                buses.routing_navigation(&origin, &graph)?;
                if let Some(callback) = self.navigate.borrow().as_ref() {
                    callback();
                }
                Ok(())
            }
            Event::History(intent) => {
                if self.pending.borrow().is_some() {
                    return Err(
                        "Retain the preceding routing receipt before another handoff.".into(),
                    );
                }
                buses.authorize_routing(
                    &intent.origin,
                    &intent.graph,
                    &intent.bus_configuration,
                )?;
                let Input::Execute { label, action } = intent.history else {
                    return Err("Routing may submit actions, not history/result metadata.".into());
                };
                let request = self
                    .session
                    .execute(&intent.graph, label, action)?
                    .ok_or("No native commands planned; refresh the routing presentation.")?;
                *self.pending.borrow_mut() = Some(Tracked {
                    token: intent.request,
                    request,
                    last: None,
                });
                Ok(())
            }
        }
    }
    fn refresh_capture(&self, force: bool) {
        let buses = self.buses.borrow().clone();
        let key = buses.routing_key();
        if !force && self.publication.borrow().as_ref() == Some(&key) {
            return;
        }
        *self.publication.borrow_mut() = Some(key);
        if self.session.busy() && self.pending.borrow().is_none() {
            self.controls
                .unavailable("Another shared request is pending; await truthful closure.");
            return;
        }
        match buses.routing_capture() {
            Ok(capture) => {
                if let Err(error) = self.controls.observe(capture) {
                    self.controls.unavailable(&error.to_string());
                }
            }
            Err(error) => self.controls.unavailable(&error),
        }
    }
    /// Called from the existing workspace clock even hidden/disconnected. Read
    /// the retained actual receipt without an inbox drain or synthetic status.
    pub fn poll(&self) {
        let pending = self
            .pending
            .borrow()
            .as_ref()
            .map(|p| (p.token.clone(), p.request.clone(), p.last.clone()));
        if let Some((token, request, last)) = pending {
            let report = request.native_report();
            if last.as_ref() != Some(&report) {
                let accepted = self.controls.report(&token, report.clone());
                if !accepted {
                    self.controls.unavailable("Native routing report did not satisfy its captured identity/barrier; receipt retained.");
                }
                if let Some(p) = self
                    .pending
                    .borrow_mut()
                    .as_mut()
                    .filter(|p| p.token == token)
                {
                    p.last = Some(report.clone());
                }
            }
            let in_progress = match &report.progress {
                NativeProgress::ExecutedLocally(c)
                | NativeProgress::Observed { counts: c, .. }
                | NativeProgress::ObservedNoOp { counts: c, .. }
                | NativeProgress::Rejected { counts: c, .. }
                | NativeProgress::Unconfirmed { counts: c, .. } => c.in_progress,
                NativeProgress::Queued => 0,
            };
            if report.progress.is_terminal()
                && in_progress == 0
                && !self.session.request_pending(&request)
                && self.controls.pending().is_none()
            {
                let mut pending = self.pending.borrow_mut();
                if pending.as_ref().is_some_and(|p| p.token == token) {
                    *self.completed.borrow_mut() = pending.take();
                }
            }
        }
        self.refresh_capture(false);
        // Never call reconciled() from a graph update, an apparent idle state
        // or a retry. The current backend has no per-request quiescence getter;
        // unresolved partial/in-progress recovery stays explicitly gated.
    }
    #[cfg(test)]
    pub fn attach_test_buses(&self, buses: Rc<BusOwner>) {
        assert!(self.pending.borrow().is_none());
        assert!(self.session.same_session(&buses.session));
        *self.buses.borrow_mut() = buses;
        self.refresh_capture(true);
    }
    #[cfg(test)]
    pub fn retained_report(&self) -> Option<NativeReport> {
        self.pending
            .borrow()
            .as_ref()
            .or(self.completed.borrow().as_ref())
            .map(|p| p.request.native_report())
    }
}
