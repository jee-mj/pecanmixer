//! Pure `Mixer.tsx::Fader` interaction policy. See `fader/README.md` for the
//! source oracle and the still-required owner/identity/completion integration.
//!
//! Outputs are proposals, never audio completion or permission to record history.
//! `T` is an opaque caller-owned interaction token, not a backend ticket. No GTK
//! objects, graph IDs, clocks, timers, I/O or dispatch handles are stored here.
use pecan_core::domain::levels::{db_to_position, position_to_db, MAX_DB, OFF_DB};
use std::time::Duration;

pub const WHEEL_IDLE: Duration = Duration::from_millis(600);
const DEFAULT_HEIGHT: f64 = 160.0;
const THUMB_HEIGHT: f64 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pointer,
    Wheel,
    Reset,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Released,
    /// Legacy pointercancel calls the same finalizer as pointerup. It does not
    /// undo already proposed/submitted values or imply backend cancellation.
    PointerCancelled,
    WheelIdle,
    Reset,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Output<T> {
    Begin {
        token: T,
        kind: Kind,
    },
    Update {
        token: T,
        db: f64,
    },
    End {
        token: T,
        db: f64,
        reason: End,
    },
    /// Explicit owner-side invalidation, distinct from legacy pointercancel.
    /// This is not rollback; the owner still owns any admitted work/results.
    Abandon {
        token: T,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    InvalidHeight,
    AlreadyDragging,
    ClockRegressed,
}
struct Drag<T> {
    token: T,
    y: f64,
}
struct Wheel<T> {
    token: T,
    deadline: Duration,
    db: f64,
}
/// Constant-size state: one pointer and one wheel slot, just as the original
/// Fader's independent refs. Each call returns at most three outputs; motion is
/// never retained in a queue. Overlap outputs require owner arbitration.
pub struct Fader<T> {
    observed_db: f64,
    position: f64,
    drag: Option<Drag<T>>,
    wheel: Option<Wheel<T>>,
    last_clock: Duration,
}
impl<T: Clone> Fader<T> {
    pub fn new(observed_db: f64) -> Result<Self, Error> {
        finite(observed_db)?;
        Ok(Self {
            observed_db,
            position: db_to_position(observed_db),
            drag: None,
            wheel: None,
            last_clock: Duration::ZERO,
        })
    }
    /// A value received from observation, not an optimistic local proposal.
    /// While dragging, only the observed value changes; local relative motion
    /// keeps its own position despite delayed graph updates, matching JS.
    pub fn observe(&mut self, db: f64) -> Result<(), Error> {
        finite(db)?;
        self.observed_db = db;
        if self.drag.is_none() {
            self.position = db_to_position(db);
        }
        Ok(())
    }
    pub fn observed_db(&self) -> f64 {
        self.observed_db
    }
    /// Legacy local fader position. Wheel/reset proposals do not optimistically
    /// change it; their later observed render does. Never label this server state.
    pub fn position(&self) -> f64 {
        self.position
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn wheel_deadline(&self) -> Option<Duration> {
        self.wheel.as_ref().map(|w| w.deadline)
    }
    pub fn pointer_down(
        &mut self,
        token: T,
        button: PointerButton,
        y: f64,
    ) -> Result<Vec<Output<T>>, Error> {
        if button != PointerButton::Primary {
            return Ok(vec![]);
        }
        finite(y)?;
        if self.drag.is_some() {
            return Err(Error::AlreadyDragging);
        }
        self.drag = Some(Drag {
            token: token.clone(),
            y,
        });
        Ok(vec![Output::Begin {
            token,
            kind: Kind::Pointer,
        }])
    }
    pub fn pointer_move(
        &mut self,
        y: f64,
        height: Option<f64>,
        shift: bool,
    ) -> Result<Vec<Output<T>>, Error> {
        let Some(drag) = self.drag.as_mut() else {
            return Ok(vec![]);
        };
        finite(y)?;
        let height = height.unwrap_or(DEFAULT_HEIGHT);
        finite(height)?;
        if height <= THUMB_HEIGHT {
            return Err(Error::InvalidHeight);
        }
        let dy = y - drag.y;
        // Reject overflowing finite-coordinate subtraction before changing state.
        finite(dy)?;
        let next = self.position - (dy / (height - THUMB_HEIGHT)) * if shift { 0.2 } else { 1.0 };
        // Infinity from a finite motion saturates like JS Math.min/Math.max.
        // Valid height and finite dy cannot produce NaN here.
        self.position = next.clamp(0.0, 1.0);
        drag.y = y;
        Ok(vec![Output::Update {
            token: drag.token.clone(),
            db: drag_db(self.position),
        }])
    }
    pub fn pointer_up(&mut self) -> Vec<Output<T>> {
        self.end_pointer(End::Released)
    }
    pub fn pointer_cancel(&mut self) -> Vec<Output<T>> {
        self.end_pointer(End::PointerCancelled)
    }
    fn end_pointer(&mut self, reason: End) -> Vec<Output<T>> {
        self.drag.take().map_or_else(Vec::new, |drag| {
            vec![Output::End {
                token: drag.token,
                db: drag_db(self.position),
                reason,
            }]
        })
    }
    /// Each step uses the latest *observed* db, not the last proposed wheel db.
    /// The first token survives the entire burst; a later callback's supplied
    /// token cannot retarget the ongoing gesture. Zero delta steps downward in JS.
    pub fn wheel(
        &mut self,
        token: T,
        delta_y: f64,
        shift: bool,
        now: Duration,
    ) -> Result<Vec<Output<T>>, Error> {
        finite(delta_y)?;
        self.clock(now)?;
        let step = if shift { 0.1 } else { 1.0 };
        let base = if self.observed_db <= OFF_DB {
            -61.0
        } else {
            self.observed_db
        };
        let next = (base + if delta_y < 0.0 { step } else { -step }).min(MAX_DB);
        let db = if next < -60.0 {
            OFF_DB
        } else {
            round_tenth(next)
        };
        let mut outputs = Vec::with_capacity(2);
        let wheel = self.wheel.get_or_insert_with(|| {
            outputs.push(Output::Begin {
                token: token.clone(),
                kind: Kind::Wheel,
            });
            Wheel {
                token,
                deadline: now,
                db,
            }
        });
        wheel.db = db;
        wheel.deadline = now.saturating_add(WHEEL_IDLE);
        outputs.push(Output::Update {
            token: wheel.token.clone(),
            db,
        });
        Ok(outputs)
    }
    /// Deliver the timer callback in actual event-loop order. A wheel event
    /// delivered first may replace an already-due timer, just like clearTimeout.
    pub fn advance(&mut self, now: Duration) -> Result<Vec<Output<T>>, Error> {
        self.clock(now)?;
        if self.wheel.as_ref().is_some_and(|w| now >= w.deadline) {
            let wheel = self.wheel.take().unwrap();
            Ok(vec![Output::End {
                token: wheel.token,
                db: wheel.db,
                reason: End::WheelIdle,
            }])
        } else {
            Ok(vec![])
        }
    }
    /// JS double-click is begin/onChange(0)/end(0), independently of a wheel
    /// timer. The owner must arbitrate overlapping interactions before dispatch.
    pub fn double_click(&self, token: T) -> Vec<Output<T>> {
        vec![
            Output::Begin {
                token: token.clone(),
                kind: Kind::Reset,
            },
            Output::Update {
                token: token.clone(),
                db: 0.0,
            },
            Output::End {
                token,
                db: 0.0,
                reason: End::Reset,
            },
        ]
    }
    /// Explicit replacement/disconnect/unmount hook. Emits no proposed value,
    /// does not claim completion/history, and removes both retained tokens.
    pub fn invalidate(&mut self) -> Vec<Output<T>> {
        let mut outputs = Vec::with_capacity(2);
        if let Some(drag) = self.drag.take() {
            outputs.push(Output::Abandon { token: drag.token });
        }
        if let Some(wheel) = self.wheel.take() {
            outputs.push(Output::Abandon { token: wheel.token });
        }
        outputs
    }
    fn clock(&mut self, now: Duration) -> Result<(), Error> {
        if now < self.last_clock {
            return Err(Error::ClockRegressed);
        }
        self.last_clock = now;
        Ok(())
    }
}
fn finite(value: f64) -> Result<(), Error> {
    value.is_finite().then_some(()).ok_or(Error::NonFinite)
}
fn drag_db(position: f64) -> f64 {
    let db = round_tenth(position_to_db(position));
    if db <= OFF_DB + 0.05 {
        OFF_DB
    } else {
        db
    }
}
/// Math.round ties toward positive infinity, unlike Rust f64::round. Avoid
/// `(x + 0.5).floor()`, which misrounds the float just below positive 0.5.
fn round_tenth(db: f64) -> f64 {
    let scaled = db * 10.0;
    let lower = scaled.floor();
    let rounded = lower + if scaled - lower >= 0.5 { 1.0 } else { 0.0 };
    if rounded == 0.0 && scaled.is_sign_negative() {
        -0.0
    } else {
        rounded / 10.0
    }
}

#[cfg(test)]
mod tests;
