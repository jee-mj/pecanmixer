//! Display-only edits and legacy resize math. No runtime IDs, dispatch or I/O.
use pecan_core::{
    domain::mixer::Row,
    settings::{HiddenKey, RowWeights, SettingsV1},
};
#[derive(Clone, Debug)]
pub enum DisplayEdit {
    Hidden { key: HiddenKey, hidden: bool },
    Rows(RowWeights),
    DestinationWidth(Option<f64>),
}
impl DisplayEdit {
    pub fn apply(&self, settings: &SettingsV1) -> Result<SettingsV1, String> {
        let mut p = settings.clone().into_preferences();
        match self {
            Self::Hidden { key, hidden } => {
                if *hidden {
                    p.hidden.insert(key.clone());
                } else {
                    p.hidden.remove(key);
                }
            }
            Self::Rows(rows) => p.rows = *rows,
            Self::DestinationWidth(width) => p.destination_width_px = *width,
        };
        SettingsV1::new(p).map_err(|e| format!("Invalid display preference: {e:?}"))
    }
}
#[derive(Clone, Copy, Debug)]
pub enum RowGrip {
    AppsHardware,
    HardwareDestinations,
}
impl RowGrip {
    pub fn rows(self) -> [Row; 2] {
        match self {
            Self::AppsHardware => [Row::Apps, Row::Hardware],
            Self::HardwareDestinations => [Row::Hardware, Row::Destinations],
        }
    }
    fn indices(self) -> [usize; 2] {
        match self {
            Self::AppsHardware => [0, 1],
            Self::HardwareDestinations => [1, 2],
        }
    }
    pub fn equal(self, weights: RowWeights) -> RowWeights {
        let mut v = values(weights);
        let [a, b] = self.indices();
        let half = (v[a] + v[b]) / 2.;
        v[a] = half;
        v[b] = half;
        weights_from(v)
    }
}
#[derive(Clone, Copy)]
pub struct RowResize {
    grip: RowGrip,
    weights: RowWeights,
    first: f64,
    total: f64,
}
impl RowResize {
    pub fn new(grip: RowGrip, weights: RowWeights, first: f64, second: f64) -> Option<Self> {
        (first.is_finite()
            && second.is_finite()
            && first >= 0.
            && second >= 0.
            && first + second > 0.)
            .then_some(Self {
                grip,
                weights,
                first,
                total: first + second,
            })
    }
    pub fn drag(self, delta: f64) -> Option<RowWeights> {
        if !delta.is_finite() {
            return None;
        }
        let min = 110f64.min(self.total / 2.);
        let first = (self.first + delta).clamp(min, self.total - min);
        let mut v = values(self.weights);
        let [a, b] = self.grip.indices();
        let sum = v[a] + v[b];
        v[a] = sum * first / self.total;
        v[b] = sum - v[a];
        Some(weights_from(v))
    }
}
#[derive(Clone, Copy)]
pub struct SplitResize {
    first: f64,
    total: f64,
    handle: f64,
}
impl SplitResize {
    pub fn new(first: f64, total: f64, handle: f64) -> Option<Self> {
        [first, total, handle]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
            .then_some(Self {
                first,
                total,
                handle,
            })
    }
    pub fn drag(self, delta: f64) -> Option<f64> {
        delta.is_finite().then(|| {
            (self.first + delta)
                .clamp(120., (self.total - 120. - self.handle).max(120.))
                .round()
        })
    }
}
fn values(v: RowWeights) -> [f64; 3] {
    [v.apps, v.hardware, v.destinations]
}
fn weights_from(v: [f64; 3]) -> RowWeights {
    RowWeights {
        apps: v[0],
        hardware: v[1],
        destinations: v[2],
    }
}
