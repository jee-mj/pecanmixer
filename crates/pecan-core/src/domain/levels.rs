//! Fader law from Mixer.tsx and clock-driven meter math from meters.ts.
pub const OFF_DB: f64 = -90.0;
pub const MAX_DB: f64 = 6.0;
const LAW: [(f64, f64); 8] = [
    (0.0, OFF_DB),
    (0.04, -60.0),
    (0.2, -40.0),
    (0.42, -20.0),
    (0.62, -10.0),
    (0.76, -4.0),
    (0.86, 0.0),
    (1.0, MAX_DB),
];

pub fn position_to_db(position: f64) -> f64 {
    let position = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if position <= 0.005 {
        return OFF_DB;
    }
    for pair in LAW.windows(2) {
        let [(s0, d0), (s1, d1)] = [pair[0], pair[1]];
        if position <= s1 {
            return d0 + (position - s0) / (s1 - s0) * (d1 - d0);
        }
    }
    MAX_DB
}

pub fn db_to_position(db: f64) -> f64 {
    if db.is_nan() || db <= OFF_DB {
        return 0.0;
    }
    for pair in LAW.windows(2) {
        let [(s0, d0), (s1, d1)] = [pair[0], pair[1]];
        if db <= d1 {
            return s0 + (db - d0) / (d1 - d0) * (s1 - s0);
        }
    }
    1.0
}

pub fn db_to_linear(db: f64) -> f32 {
    if !db.is_finite() || db <= OFF_DB {
        0.0
    } else {
        10_f64.powf(db / 20.0) as f32
    }
}

pub fn linear_to_meter(linear: f32) -> f64 {
    if linear <= 0.0 || linear.is_nan() {
        return 0.0;
    }
    ((20.0 * f64::from(linear).log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meter {
    pub level: f64,
    pub held: f64,
    pub clipped: bool,
    hold_until_ms: f64,
    clip_until_ms: f64,
}

impl Meter {
    /// `now_ms` is a monotonic caller clock. Long stalls are capped at 100ms,
    /// matching the current frontend; no real-time audio work occurs here.
    pub fn tick(&mut self, channels: &[f32], selected: &[usize], now_ms: f64, elapsed_ms: f64) {
        if !now_ms.is_finite() || !elapsed_ms.is_finite() {
            return;
        }
        let linear = selected
            .iter()
            .filter_map(|i| channels.get(*i))
            .copied()
            .filter(|v| v.is_finite())
            .fold(0_f32, f32::max);
        let target = linear_to_meter(linear);
        self.level = if target >= self.level {
            target
        } else {
            target.max(self.level - (24.0 / 60.0) * elapsed_ms.clamp(0.0, 100.0) / 1000.0)
        };
        if target >= self.held || now_ms > self.hold_until_ms {
            self.held = target;
            self.hold_until_ms = now_ms + 1500.0;
        }
        if linear >= 0.999 {
            self.clip_until_ms = now_ms + 2000.0;
        }
        self.clipped = self.clip_until_ms > now_ms;
    }
}
