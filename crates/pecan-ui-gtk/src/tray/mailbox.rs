//! Fixed-size application control slots. These are shell actions, not backend
//! audio commands. Quit has its own reserved flag, independent of wake capacity.
use crate::lifetime::{Completion, Effect, Event, Lifetime, RequestId, TrayAvailability};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityAction {
    None,
    Toggle,
    Present,
    Hide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayEvidence {
    pub availability: TrayAvailability,
    pub observed_at: Duration,
}
#[derive(Debug)]
pub struct ActionBatch {
    pub visibility: VisibilityAction,
    pub quit: bool,
    pub acknowledge_failure: Option<RequestId>,
    pub tray: Option<TrayEvidence>,
}
impl ActionBatch {
    /// Availability precedes visibility; quit cannot be starved by visibility
    /// traffic. The pure policy refuses hide while finalization is in progress.
    pub fn apply<C: Completion>(self, policy: &mut Lifetime<C>, now: Duration) -> Vec<Effect> {
        let mut effects = vec![];
        if let Some(tray) = self.tray {
            effects.extend(policy.apply_tray_evidence(tray.availability, tray.observed_at, now));
        }
        if self.quit {
            effects.extend(policy.apply(Event::Quit, now));
        }
        if let Some(acknowledgement) = self.acknowledge_failure {
            effects.extend(policy.apply(Event::ExitAfterError { acknowledgement }, now));
        }
        let event = match self.visibility {
            VisibilityAction::None => None,
            VisibilityAction::Toggle => Some(Event::Toggle),
            VisibilityAction::Present => Some(Event::Activate),
            VisibilityAction::Hide => Some(Event::Hide),
        };
        if let Some(event) = event {
            effects.extend(policy.apply(event, now));
        }
        effects
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumerStopped;
pub struct ActionMailbox {
    visibility: AtomicU8,
    quit: AtomicBool,
    failure_ack: AtomicU64,
    tray: AtomicU64,
    tray_dirty: AtomicBool,
    observed_nanos: AtomicU64,
    origin: Instant,
    next_generation: AtomicU64,
    wake: async_channel::Sender<()>,
}
const GENERATION_MAX: u64 = u64::MAX >> 2;
fn packed(generation: u64, state: TrayAvailability) -> u64 {
    generation << 2
        | match state {
            TrayAvailability::Unknown => 0,
            TrayAvailability::Usable => 1,
            TrayAvailability::Unavailable => 2,
        }
}
fn state(value: u64) -> TrayAvailability {
    match value & 3 {
        1 => TrayAvailability::Usable,
        2 => TrayAvailability::Unavailable,
        _ => TrayAvailability::Unknown,
    }
}
fn update(value: &AtomicU64, f: impl Fn(u64) -> Option<u64>) -> Result<u64, u64> {
    let mut old = value.load(Ordering::SeqCst);
    loop {
        let next = f(old).ok_or(old)?;
        match value.compare_exchange_weak(old, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(previous) => return Ok(previous),
            Err(current) => old = current,
        }
    }
}
impl ActionMailbox {
    pub fn new() -> (Arc<Self>, async_channel::Receiver<()>) {
        let (wake, receiver) = async_channel::bounded(1);
        (
            Arc::new(Self {
                visibility: AtomicU8::new(0),
                quit: AtomicBool::new(false),
                failure_ack: AtomicU64::new(0),
                tray: AtomicU64::new(0),
                tray_dirty: AtomicBool::new(false),
                observed_nanos: AtomicU64::new(0),
                origin: Instant::now(),
                next_generation: AtomicU64::new(0),
                wake,
            }),
            receiver,
        )
    }
    /// Clock shared by tray evidence and the application policy/tick.
    pub fn now(&self) -> Duration {
        self.origin.elapsed()
    }
    fn notify(&self) -> Result<(), ConsumerStopped> {
        match self.wake.try_send(()) {
            Ok(()) | Err(async_channel::TrySendError::Full(_)) => Ok(()),
            Err(async_channel::TrySendError::Closed(_)) => Err(ConsumerStopped),
        }
    }
    pub fn present(&self) -> Result<(), ConsumerStopped> {
        self.visibility.store(2, Ordering::SeqCst);
        self.notify()
    }
    pub fn toggle(&self) -> Result<(), ConsumerStopped> {
        // Compose a Boolean function, preserving Show/Toggle order and toggle
        // parity under a burst rather than retaining an unbounded click queue.
        let mut old = self.visibility.load(Ordering::SeqCst);
        loop {
            let next = match old {
                0 => 1,
                1 => 0,
                2 => 3,
                _ => 2,
            };
            match self.visibility.compare_exchange_weak(
                old,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(current) => old = current,
            }
        }
        self.notify()
    }
    pub fn quit(&self) -> Result<(), ConsumerStopped> {
        self.quit.store(true, Ordering::SeqCst);
        self.notify()
    }
    pub fn acknowledge_failure(&self, id: RequestId) -> Result<(), ConsumerStopped> {
        self.failure_ack.store(id.get(), Ordering::SeqCst);
        self.notify()
    }
    pub fn drain(&self) -> ActionBatch {
        ActionBatch {
            tray: self
                .tray_dirty
                .swap(false, Ordering::SeqCst)
                .then(|| TrayEvidence {
                    availability: state(self.tray.load(Ordering::SeqCst)),
                    observed_at: Duration::from_nanos(self.observed_nanos.load(Ordering::SeqCst)),
                }),
            quit: self.quit.swap(false, Ordering::SeqCst),
            acknowledge_failure: RequestId::from_token(self.failure_ack.swap(0, Ordering::SeqCst)),
            visibility: match self.visibility.swap(0, Ordering::SeqCst) {
                1 => VisibilityAction::Toggle,
                2 => VisibilityAction::Present,
                3 => VisibilityAction::Hide,
                _ => VisibilityAction::None,
            },
        }
    }
    pub(super) fn begin_generation(&self) -> Option<u64> {
        let generation = update(&self.next_generation, |n| {
            (n < GENERATION_MAX).then_some(n + 1)
        })
        .ok()?
            + 1;
        // A late old service cannot overwrite availability from a newer one.
        self.tray.fetch_max(
            packed(generation, TrayAvailability::Unknown),
            Ordering::SeqCst,
        );
        self.tray_dirty.store(true, Ordering::SeqCst);
        let _ = self.notify();
        Some(generation)
    }
    pub(super) fn availability(&self, generation: u64, availability: TrayAvailability) {
        let changed = update(&self.tray, |old| {
            (old >> 2 == generation).then_some(packed(generation, availability))
        })
        .is_ok();
        if changed {
            self.tray_dirty.store(true, Ordering::SeqCst);
            let _ = self.notify();
        }
    }
    pub(super) fn heartbeat(&self, generation: u64, observed_at: Duration) {
        let value = packed(generation, TrayAvailability::Usable);
        if self
            .tray
            .compare_exchange(value, value, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.observed_nanos.fetch_max(
                observed_at.as_nanos().min(u128::from(u64::MAX)) as u64,
                Ordering::SeqCst,
            );
            self.tray_dirty.store(true, Ordering::SeqCst);
            let _ = self.notify();
        }
    }
    pub(super) fn registered(&self, generation: u64, observed_at: Duration) {
        // Offline can arrive before spawn returns. Never overwrite it with a
        // late registration success from that same service.
        if self
            .tray
            .compare_exchange(
                packed(generation, TrayAvailability::Unknown),
                packed(generation, TrayAvailability::Usable),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            self.observed_nanos.fetch_max(
                observed_at.as_nanos().min(u128::from(u64::MAX)) as u64,
                Ordering::SeqCst,
            );
            self.tray_dirty.store(true, Ordering::SeqCst);
            let _ = self.notify();
        }
    }
}

#[cfg(test)]
mod tests;
