//! Bounded tracked-writer FIFO. Stop and one close per track have reserved
//! capacity; they retain ordering behind previously accepted sample messages.
use crate::{
    opname::Bericht,
    writer_status::{Signals, TrackId, WriterReceipt},
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    time::Duration,
};

pub const WRITER_AUDIO_CAPACITY: usize = 32;
pub const MAX_WRITER_SAMPLES: usize = 65_536;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterSendError {
    Busy,
    Full,
    NotReady,
    Closed,
    WrongTrack,
    InvalidSamples,
}
struct State {
    messages: VecDeque<Bericht>,
    audio: usize,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    signals: Arc<Signals>,
    producers: AtomicBool,
}
struct Producers(Weak<Shared>);
impl Drop for Producers {
    fn drop(&mut self) {
        if let Some(s) = self.0.upgrade() {
            s.producers.store(false, Ordering::Release);
            s.wake.notify_all();
        }
    }
}
#[derive(Clone)]
pub struct WriterSender {
    shared: Arc<Shared>,
    _producers: Arc<Producers>,
    receipt: WriterReceipt,
}
pub(crate) struct Receiver(Arc<Shared>);
pub(crate) fn channel(receipt: &WriterReceipt) -> (WriterSender, Receiver) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            messages: VecDeque::with_capacity(
                WRITER_AUDIO_CAPACITY + crate::writer_status::MAX_TRACKED_TRACKS + 1,
            ),
            audio: 0,
            closed: false,
        }),
        wake: Condvar::new(),
        signals: receipt.signals.clone(),
        producers: AtomicBool::new(true),
    });
    let sender = WriterSender {
        _producers: Arc::new(Producers(Arc::downgrade(&shared))),
        shared: shared.clone(),
        receipt: receipt.clone(),
    };
    (sender, Receiver(shared))
}
impl WriterSender {
    /// Reserved, nonblocking stop request. Reject future ingress and drain all
    /// already-admitted FIFO messages before finalizing; this is not completion.
    pub fn request_stop(&self) {
        self.shared.signals.stop.store(true, Ordering::Release);
        self.shared.wake.notify_all();
    }
    fn index(&self, track: TrackId) -> Result<usize, WriterSendError> {
        if self.receipt.track_id(track.index()) == Some(track) {
            Ok(track.index())
        } else {
            Err(WriterSendError::WrongTrack)
        }
    }
    /// Nonblocking. A failure means this sample message was not accepted.
    /// The caller must handle overload explicitly; samples are never retried
    /// or silently dropped by this queue.
    pub fn try_audio(&self, track: TrackId, samples: Vec<f32>) -> Result<(), WriterSendError> {
        let index = self.index(track)?;
        // Retained allocation, not just length, must fit the packet budget.
        // A short slice of a previously huge Vec must not bypass the bound.
        if samples.capacity() > MAX_WRITER_SAMPLES || samples.iter().any(|s| !s.is_finite()) {
            return Err(WriterSendError::InvalidSamples);
        }
        let mut state = self
            .shared
            .state
            .try_lock()
            .map_err(|_| WriterSendError::Busy)?;
        if state.closed
            || self.shared.signals.closed.load(Ordering::Acquire)
            || self.shared.signals.stopping()
            || self.shared.signals.closing.load(Ordering::Acquire) & (1 << index) != 0
        {
            return Err(WriterSendError::Closed);
        }
        if !self.shared.signals.ready.load(Ordering::Acquire) {
            return Err(WriterSendError::NotReady);
        }
        if state.audio >= WRITER_AUDIO_CAPACITY {
            return Err(WriterSendError::Full);
        }
        let accepted = &self.shared.signals.accepted[index];
        accepted.store(
            accepted
                .load(Ordering::Relaxed)
                .saturating_add(samples.len() as u64),
            Ordering::Release,
        );
        state.messages.push_back(Bericht::Audio(index, samples));
        state.audio += 1;
        self.shared.wake.notify_one();
        Ok(())
    }
    /// Accepted close requests are idempotent, but completion is evidenced only
    /// by TrackPhase::Finalized in the receipt.
    pub fn try_close(&self, track: TrackId) -> Result<(), WriterSendError> {
        let index = self.index(track)?;
        let bit = 1 << index;
        let mut state = self
            .shared
            .state
            .try_lock()
            .map_err(|_| WriterSendError::Busy)?;
        if self.shared.signals.stopping()
            || self.shared.signals.closing.load(Ordering::Acquire) & bit != 0
        {
            return Ok(());
        }
        if state.closed || self.shared.signals.closed.load(Ordering::Acquire) {
            return Err(WriterSendError::Closed);
        }
        if !self.shared.signals.ready.load(Ordering::Acquire) {
            return Err(WriterSendError::NotReady);
        }
        state.messages.push_back(Bericht::Sluit(index));
        self.shared.signals.closing.fetch_or(bit, Ordering::Release);
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn try_stop(&self) -> Result<(), WriterSendError> {
        let mut state = self
            .shared
            .state
            .try_lock()
            .map_err(|_| WriterSendError::Busy)?;
        if self.shared.signals.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if state.closed || self.shared.signals.closed.load(Ordering::Acquire) {
            return Err(WriterSendError::Closed);
        }
        state.messages.push_back(Bericht::Stop(None));
        self.shared.signals.stop.store(true, Ordering::Release);
        self.shared.wake.notify_one();
        Ok(())
    }
}
impl Iterator for Receiver {
    type Item = Bericht;
    fn next(&mut self) -> Option<Bericht> {
        let mut state = self.0.state.lock().unwrap();
        loop {
            if let Some(message) = state.messages.pop_front() {
                if matches!(message, Bericht::Audio(..)) {
                    state.audio -= 1;
                }
                return Some(message);
            }
            if self.0.signals.stopping() {
                return Some(Bericht::Stop(None));
            }
            if state.closed || !self.0.producers.load(Ordering::Acquire) {
                return None;
            }
            state = self
                .0
                .wake
                .wait_timeout(state, Duration::from_millis(5))
                .unwrap()
                .0;
        }
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.0.signals.closed.store(true, Ordering::Release);
        let mut state = self.0.state.lock().unwrap();
        state.closed = true;
        state.messages.clear();
        state.audio = 0;
        self.0.wake.notify_all();
    }
}
