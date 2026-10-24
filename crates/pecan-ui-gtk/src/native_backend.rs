//! One shared dispatch client for functional native controls. It retains an
//! existing native owner and forwards exact origin Arcs to its reviewed API.
//! It neither starts a worker nor derives origins from the read-only observer.
use pecan_core::{
    backend::Stamped,
    graph::{Momentopname, Opdracht},
    native_command::{AdmissionError, NativeIntent, NativeReceipt},
};
use pecan_pipewire::graaf::Graaf;
use std::{cell::Cell, rc::Rc, sync::Arc};

type Dispatch = dyn Fn(NativeIntent) -> Result<NativeReceipt, AdmissionError>;
#[derive(Clone)]
pub struct NativeClient {
    dispatch: Rc<Dispatch>,
    ticket: Rc<Cell<u64>>,
}
impl NativeClient {
    /// Supply the same owner that emitted the presentation's native inbox frames.
    /// Clones share ticket allocation and keep that owner alive. The owning shell
    /// remains responsible for explicit shutdown/finalization before exiting.
    pub fn new(owner: Rc<Graaf>) -> Self {
        Self::with_dispatch(move |intent| owner.dispatch_native(intent))
    }
    fn with_dispatch(
        dispatch: impl Fn(NativeIntent) -> Result<NativeReceipt, AdmissionError> + 'static,
    ) -> Self {
        Self {
            dispatch: Rc::new(dispatch),
            ticket: Rc::new(Cell::new(0)),
        }
    }
    /// Reuse this client for mixer and patchbay intents. Limits/identity checks
    /// belong to the backend. Batches remain ordered and may have partial effects.
    pub fn submit(
        &self,
        origin: &Stamped<Arc<Momentopname>>,
        commands: Vec<Opdracht>,
    ) -> Result<NativeReceipt, AdmissionError> {
        let ticket = self
            .ticket
            .get()
            .checked_add(1)
            .ok_or(AdmissionError::InvalidInput)?;
        self.ticket.set(ticket);
        (self.dispatch)(NativeIntent {
            ticket,
            origin: origin.clone(),
            commands,
        })
    }
    #[cfg(test)]
    pub(crate) fn for_test(
        dispatch: impl Fn(NativeIntent) -> Result<NativeReceipt, AdmissionError> + 'static,
    ) -> Self {
        Self::with_dispatch(dispatch)
    }
}
