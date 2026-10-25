//! Accepted user Rec intent only. Never construct this from passive readback or
//! native defaults; never replace the full saved document with effective config.
use crate::recording_buses::Action;
use pecan_core::{domain::buses::BusConfig, graph::Momentopname, settings::SettingsV1};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) enum RecEdit {
    Db { bus: String, db: f64 },
    Mute { bus: String, mute: bool },
    Armed(BTreeMap<String, bool>),
    Layout { basis: BusConfig, action: Action },
}
impl RecEdit {
    pub(crate) fn configuration(
        action: &Action,
        before: &BusConfig,
        after: &BusConfig,
    ) -> Result<Self, String> {
        Ok(match action {
            Action::Arm { bus, additive } => Self::Armed(
                after
                    .definitions()
                    .map_err(|e| format!("{e:?}"))?
                    .into_iter()
                    .filter(|d| !additive || &d.key == bus)
                    .map(|d| {
                        let armed = after.state(&d.key).armed;
                        (d.key, armed)
                    })
                    .collect(),
            ),
            _ => Self::Layout {
                basis: before.clone(),
                action: action.clone(),
            },
        })
    }
    pub(crate) fn apply(&self, document: &SettingsV1) -> Result<SettingsV1, String> {
        let mut p = document.clone().into_preferences();
        match self {
            Self::Db { bus, db } => p.buses.states.entry(bus.clone()).or_default().db = *db,
            Self::Mute { bus, mute } => p.buses.states.entry(bus.clone()).or_default().mute = *mute,
            Self::Armed(changes) => {
                for (key, armed) in changes {
                    p.buses.states.entry(key.clone()).or_default().armed = *armed;
                }
            }
            Self::Layout { basis, action } => {
                // Transform the user-selected live layout, carrying latest saved
                // state by stable Rec key. Backend MIDI masking and stale gain
                // readback must not overwrite saved intent from earlier edits.
                let mut logical = basis.clone();
                logical.midi = p.buses.midi;
                logical.states = p.buses.states.clone();
                for def in basis.definitions().map_err(|e| format!("{e:?}"))? {
                    logical
                        .states
                        .entry(def.key.clone())
                        .or_insert_with(|| basis.state(&def.key));
                }
                let next = match action {
                    Action::AddStereo => logical.add(),
                    Action::Split { bus } | Action::Join { bus } | Action::Remove { bus } => {
                        let index = logical
                            .definitions()
                            .map_err(|e| format!("{e:?}"))?
                            .iter()
                            .find(|d| &d.key == bus)
                            .ok_or("Saved layout edit has no captured bus.")?
                            .index;
                        if matches!(action, Action::Remove { .. }) {
                            logical
                                .remove(index, &Momentopname::default())
                                .map(|e| e.config)
                        } else {
                            logical.change_coupling(index)
                        }
                    }
                    Action::Arm { .. } => {
                        return Err("Arming is a field edit, not a layout replacement.".into())
                    }
                }
                .map_err(|e| format!("Invalid Rec preference: {e:?}"))?;
                p.buses = next;
            }
        }
        SettingsV1::new(p).map_err(|e| format!("Invalid Rec preference: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::preferences::PreferencesState;
    use pecan_core::settings::{HiddenKey, RowWeights};
    fn saved() -> SettingsV1 {
        let mut p = SettingsV1::default().into_preferences();
        p.buses = p
            .buses
            .with_db("rec-1-2", -18.)
            .unwrap()
            .with_mute("rec-1-2", true)
            .unwrap();
        p.buses.widths = vec![2, 2];
        p.buses.states.insert(
            "rec-3-4".into(),
            pecan_core::domain::buses::BusState {
                db: -11.,
                mute: false,
                armed: false,
            },
        );
        p.clock.start_stop = true;
        p.rows = RowWeights {
            apps: 2.,
            hardware: 3.,
            destinations: 4.,
        };
        p.hidden.insert(HiddenKey::new("rij|hardware").unwrap());
        p.destination_width_px = Some(222.);
        SettingsV1::new(p).unwrap()
    }
    #[test]
    fn rec_fields_merge_latest_intent_without_masked_defaults_or_layout_replacement() {
        let before = saved();
        let mut state = PreferencesState::new(false);
        state.edit(before.clone()).unwrap();
        state
            .edit_rec(RecEdit::Db {
                bus: "rec-1-2".into(),
                db: -23.,
            })
            .unwrap();
        state
            .edit_rec(RecEdit::Mute {
                bus: "rec-1-2".into(),
                mute: false,
            })
            .unwrap();
        state
            .edit_rec(RecEdit::Armed(BTreeMap::from([("rec-1-2".into(), false)])))
            .unwrap();
        let now = state.document().preferences();
        assert_eq!(
            now.buses.state("rec-1-2"),
            pecan_core::domain::buses::BusState {
                db: -23.,
                mute: false,
                armed: false
            }
        );
        assert_eq!(
            now.buses.state("rec-3-4"),
            before.preferences().buses.state("rec-3-4")
        );
        assert_eq!(now.buses.widths, vec![2, 2]);
        assert!(now.buses.midi && now.clock.start_stop);
        assert_eq!(now.hidden, before.preferences().hidden);
        assert_eq!(now.rows, before.preferences().rows);
        assert_eq!(now.destination_width_px, Some(222.));
        state
            .edit_rec(RecEdit::Db {
                bus: "rec-1-2".into(),
                db: 0.,
            })
            .unwrap();
        assert_eq!(
            state.document().preferences().buses.state("rec-1-2").db,
            0.,
            "explicit native no-op still replaces a different saved intent"
        );
    }
    #[test]
    fn rec_layout_uses_latest_saved_states_and_preserves_inactive_midi_clock() {
        let saved = saved();
        let effective = crate::application::default_idle_configuration().buses;
        let split = RecEdit::configuration(
            &Action::Split {
                bus: "rec-1-2".into(),
            },
            &effective,
            &effective.change_coupling(0).unwrap(),
        )
        .unwrap();
        let next = split.apply(&saved).unwrap();
        assert_eq!(next.preferences().buses.widths, vec![1, 1]);
        for key in ["rec-1", "rec-2"] {
            assert_eq!(
                next.preferences().buses.state(key),
                saved.preferences().buses.state("rec-1-2")
            );
        }
        assert!(next.preferences().buses.midi && next.preferences().clock.start_stop);
        let join = RecEdit::Layout {
            basis: next.preferences().buses.clone(),
            action: Action::Join {
                bus: "rec-1".into(),
            },
        }
        .apply(&next)
        .unwrap();
        assert_eq!(join.preferences().buses.widths, vec![2]);
        assert_eq!(join.preferences().buses.state("rec-1-2").db, -18.);
        let added = RecEdit::Layout {
            basis: join.preferences().buses.clone(),
            action: Action::AddStereo,
        }
        .apply(&join)
        .unwrap();
        assert_eq!(added.preferences().buses.widths, vec![2, 2]);
        let removed = RecEdit::Layout {
            basis: added.preferences().buses.clone(),
            action: Action::Remove {
                bus: "rec-1-2".into(),
            },
        }
        .apply(&added)
        .unwrap();
        assert_eq!(removed.preferences().buses.widths, vec![2]);
        assert_eq!(removed.preferences().clock, saved.preferences().clock);
    }
    #[test]
    fn rec_additive_and_exclusive_arming_touch_only_the_explicit_flags() {
        let before = saved();
        let actual = before.preferences().buses.clone();
        for additive in [true, false] {
            let after = actual.arm("rec-3-4", additive).unwrap();
            let edit = RecEdit::configuration(
                &Action::Arm {
                    bus: "rec-3-4".into(),
                    additive,
                },
                &actual,
                &after,
            )
            .unwrap();
            let next = edit.apply(&before).unwrap();
            for key in ["rec-1-2", "rec-3-4"] {
                assert_eq!(
                    next.preferences().buses.state(key).db,
                    before.preferences().buses.state(key).db
                );
                assert_eq!(
                    next.preferences().buses.state(key).mute,
                    before.preferences().buses.state(key).mute
                );
                assert_eq!(
                    next.preferences().buses.state(key).armed,
                    after.state(key).armed
                );
            }
        }
    }
    #[test]
    fn rec_invalid_gain_or_runtime_key_cannot_modify_validated_document() {
        let doc = saved();
        for edit in [
            RecEdit::Db {
                bus: "rec-1-2".into(),
                db: f64::NAN,
            },
            RecEdit::Db {
                bus: "rec-1-2".into(),
                db: 7.,
            },
            RecEdit::Mute {
                bus: "runtime-node-30".into(),
                mute: true,
            },
        ] {
            assert!(edit.apply(&doc).is_err());
        }
        let mut state = PreferencesState::new(true);
        assert!(state
            .edit_rec(RecEdit::Db {
                bus: "rec-1-2".into(),
                db: -6.
            })
            .is_err());
        assert_eq!(state.revision(), 0);
    }
}
