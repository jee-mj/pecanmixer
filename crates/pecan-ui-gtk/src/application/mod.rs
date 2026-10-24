//! Explicit native application ownership and one shared mutation/history path.
//!
//! This library is not called by main startup. A future shell must retain the
//! owner through reviewed asynchronous shutdown. `fixture()` has a separate
//! synthetic endpoint: it never constructs a Graaf or connects to PipeWire.
mod coordinator;
mod drain;
mod fixture;
mod history_input;
mod mixer;
mod shell;
mod view;

pub use coordinator::volume::{GestureToken, StripIdentity};
pub use coordinator::{Request, Session, SessionAdapter, Status};
pub use drain::SettingsSummary;
pub use fixture::{FixtureDriver, FixtureRequest};
pub use mixer::CoordinatedMixer;
pub use shell::{ApplicationShell, InstallFailure, ShellOptions};
pub use view::NativeWorkspace;

use pecan_core::{
    backend::Stamped,
    graph::Momentopname,
    history_identity::HistoryGraph,
    inbox::LatestInbox,
    native_command::{AdmissionError, NativeIntent, NativeReceipt},
};
use pecan_pipewire::graaf::Graaf;
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Build native idle defaults without starting/applying anything. MIDI is
/// unsupported by that API; preserve the separate legacy BusConfig default.
pub fn default_idle_configuration() -> pecan_pipewire::bus_config::IdleBusConfig {
    pecan_pipewire::bus_config::IdleBusConfig {
        buses: pecan_core::domain::buses::BusConfig {
            midi: false,
            ..Default::default()
        },
        clock: None,
        start_stop: false,
    }
}

type Graph = Stamped<Arc<Momentopname>>;
trait Endpoint {
    fn witness(&self, graph: &Graph) -> Result<HistoryGraph, AdmissionError>;
    fn dispatch(&self, intent: NativeIntent) -> Result<NativeReceipt, AdmissionError>;
}
impl Endpoint for Graaf {
    fn witness(&self, graph: &Graph) -> Result<HistoryGraph, AdmissionError> {
        self.history_graph(graph)
    }
    fn dispatch(&self, intent: NativeIntent) -> Result<NativeReceipt, AdmissionError> {
        self.dispatch_native(intent)
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Native,
    Fixture,
}
struct Active {
    session: Session,
    native: Option<Rc<Graaf>>,
    fixture: Option<FixtureDriver>,
}
/// Retain one instance for the entire GApplication lifetime. Repeated activation
/// returns the same session/worker/inbox/history, even after windows are hidden.
/// This is an explicit library boundary, not production startup adoption.
pub struct ApplicationOwner {
    mode: Mode,
    active: RefCell<Option<Active>>,
    shell: RefCell<std::rc::Weak<ApplicationShell>>,
}
impl ApplicationOwner {
    pub fn native() -> Self {
        Self {
            mode: Mode::Native,
            active: RefCell::new(None),
            shell: RefCell::new(std::rc::Weak::new()),
        }
    }
    pub fn fixture() -> Self {
        Self {
            mode: Mode::Fixture,
            active: RefCell::new(None),
            shell: RefCell::new(std::rc::Weak::new()),
        }
    }
    pub fn activate(&self) -> Session {
        let mut active = self.active.borrow_mut();
        let entry = active.get_or_insert_with(|| {
            // Only Session drains this inbox. Receipt polling is also driven by
            // the shell's main-context tick, including when no wake is emitted.
            let (inbox, _wake) = LatestInbox::new(16);
            match self.mode {
                Mode::Native => {
                    let native = Rc::new(Graaf::start_native(inbox.clone()));
                    Active {
                        session: Session::new(inbox, native.clone()),
                        native: Some(native),
                        fixture: None,
                    }
                }
                Mode::Fixture => {
                    let fixture = FixtureDriver::new(inbox.clone());
                    Active {
                        session: Session::new(inbox, fixture.endpoint()),
                        native: None,
                        fixture: Some(fixture),
                    }
                }
            }
        });
        entry.session.clone()
    }
    /// Native lifetime adapter uses this same owner for request_shutdown; it
    /// must keep polling/joining completion before exiting. Never starts it.
    pub fn native_handle(&self) -> Option<Rc<Graaf>> {
        self.active.borrow().as_ref().and_then(|a| a.native.clone())
    }
    /// Available only in fixture mode; activation still creates no audio worker.
    pub fn fixture_driver(&self) -> Option<FixtureDriver> {
        if !matches!(self.mode, Mode::Fixture) {
            return None;
        }
        self.activate();
        self.active
            .borrow()
            .as_ref()
            .and_then(|a| a.fixture.clone())
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "x11-input-tests"))]
mod history_xtest;
