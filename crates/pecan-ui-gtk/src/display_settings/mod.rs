//! Presentation-only preferences. Full graph/origin/history state stays with the
//! caller. Bus, clock and transport settings never become commands here.
use pecan_core::{
    domain::mixer::{Mixer, Row, Strip},
    settings::{HiddenKey, RowWeights, SettingsV1},
};
use std::collections::BTreeSet;
mod layout;
pub use layout::PreferenceLayout;

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayPreferences {
    hidden: BTreeSet<HiddenKey>,
    weights: RowWeights,
    destination_width: Option<f64>,
}
impl Default for DisplayPreferences {
    fn default() -> Self {
        Self::from(&SettingsV1::default())
    }
}
impl From<&SettingsV1> for DisplayPreferences {
    fn from(settings: &SettingsV1) -> Self {
        let p = settings.preferences();
        Self {
            hidden: p.hidden.clone(),
            weights: p.rows,
            destination_width: p.destination_width_px,
        }
    }
}
impl DisplayPreferences {
    pub fn row_visible(&self, row: Row) -> bool {
        let key = match row {
            Row::Apps => "rij|apps",
            Row::Hardware => "rij|hardware",
            Row::Destinations => "rij|bestemmingen",
        };
        !self.hidden.iter().any(|hidden| hidden.as_str() == key)
    }
    pub fn strip_visible(&self, strip: &Strip) -> bool {
        self.row_visible(strip.row)
            && !self
                .hidden
                .iter()
                .any(|hidden| hidden.as_str() == strip.key)
    }
    /// Views select keys; the complete model, graph and origin are never pruned.
    /// Empty rows remain visible unless explicitly hidden, matching Mixer.tsx.
    pub fn visible_keys(&self, model: &Mixer) -> Vec<String> {
        model
            .apps
            .iter()
            .chain(&model.hardware)
            .chain(&model.destinations)
            .filter(|strip| self.strip_visible(strip))
            .map(|strip| strip.key.clone())
            .collect()
    }
    pub fn fractions(&self) -> [f64; 3] {
        let mut weights = [
            self.weights.apps,
            self.weights.hardware,
            self.weights.destinations,
        ];
        for (weight, row) in weights
            .iter_mut()
            .zip([Row::Apps, Row::Hardware, Row::Destinations])
        {
            if !self.row_visible(row) {
                *weight = 0.0;
            }
        }
        let total = weights.iter().sum::<f64>();
        if total == 0.0 {
            return [0.0; 3];
        }
        weights.map(|weight| weight / total)
    }
    /// Allocation clamp is display-only: it never rewrites a stored preference.
    pub fn output_width(&self, available: i32, natural: i32) -> i32 {
        self.destination_width
            .unwrap_or(f64::from(natural.max(0)))
            .round()
            .clamp(0.0, f64::from(available.max(0))) as i32
    }
}

#[cfg(test)]
mod tests;
