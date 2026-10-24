//! Synthetic channels, keys and owner tokens only. No backend or display.
use super::*;
use pecan_core::{domain::mixer::Row, graph::Knoop};
use serde_json::{json, Value};

fn strip(key: &str, channels: &[usize], volume: &[f32]) -> Strip {
    let node: Knoop = serde_json::from_value(json!({
        "id":1,"serial":11,"naam":"synthetic-node","volume":volume,"mute":false
    }))
    .unwrap();
    Strip {
        key: key.into(),
        node,
        ports: channels
            .iter()
            .map(|channel| {
                serde_json::from_value(json!({
                    "id":100+channel,"knoop":1,"naam":format!("synthetic-channel-{channel}"),
                    "ingang":true,"soort":"audio","monitor":false,"fysiek":false
                }))
                .unwrap()
            })
            .collect(),
        name: "Synthetic partial strip".into(),
        subtitle: "Synthetic fixture".into(),
        row: Row::Destinations,
        channels: channels.to_vec(),
        targetable: false,
        bus: None,
        silent: false,
    }
}
fn vector(value: &Value) -> Vec<f32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect()
}
fn same_vector(actual: &[f32], expected: &Value, name: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{name}");
    for (a, b) in actual.iter().zip(expected) {
        let b = b.as_f64().unwrap();
        assert!((f64::from(*a) - b).abs() < 1e-6, "{name}: {a} != {b}");
    }
}
fn finish(memory: &mut Memory<u64>, identity: u64, s: &mut Strip, proposal: Proposal<u64>) {
    s.node.volume = Some(proposal.after().to_vec()); // synthetic observed completion
    memory.apply_memory(proposal, &identity, s).unwrap();
}
#[test]
fn source_callbacks_match_mean_restore_channel_isolation_and_full_undo_vectors() {
    let fixture: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let traces = fixture["scenarios"].as_array().unwrap();
    assert_eq!(traces.len(), 13);
    for trace in traces {
        let name = trace["name"].as_str().unwrap();
        let mut memory = Memory::new(16);
        let mut volume = vector(&trace["initial"]);
        let mut expected = trace["expected"].as_array().unwrap().iter();
        for step in trace["steps"].as_array().unwrap() {
            if step["op"] == "observe" {
                volume = vector(&step["volume"]);
                continue;
            }
            let channels: Vec<_> = step["channels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i.as_u64().unwrap() as usize)
                .collect();
            let mut s = strip(step["key"].as_str().unwrap(), &channels, &volume);
            let proposal = if step["op"] == "toggle" {
                memory.propose_toggle(1, &s)
            } else {
                memory.propose_fader(1, &s, step["db"].as_f64().unwrap())
            }
            .unwrap();
            let expected = expected.next().unwrap();
            assert_eq!(
                match proposal.kind() {
                    Kind::Mute => "mute",
                    Kind::Restore => "restore",
                    Kind::Fader => "fader",
                },
                expected["kind"].as_str().unwrap(),
                "{name}"
            );
            same_vector(proposal.before(), &expected["before"], name);
            same_vector(proposal.after(), &expected["after"], name);
            for (channel, before) in proposal.before().iter().enumerate() {
                if !s.channels.contains(&channel) {
                    assert_eq!(
                        before.to_bits(),
                        proposal.after()[channel].to_bits(),
                        "{name}: unselected channel {channel} changed"
                    );
                }
            }
            if !expected["undo"].is_null() {
                same_vector(proposal.before(), &expected["undo"]["before"], name);
                same_vector(proposal.after(), &expected["undo"]["after"], name);
            }
            // Preparing a write does not mutate local memory. Only this test's
            // explicit successful-observation step applies its transition.
            finish(&mut memory, 1, &mut s, proposal);
            volume = s.node.volume.clone().unwrap();
            assert_eq!(
                memory.is_muted(&1, &s).unwrap(),
                expected["muted"].as_bool().unwrap(),
                "{name}"
            );
            let entries = expected["memory"].as_array().unwrap();
            assert_eq!(memory.len(), entries.len(), "{name}");
            for ((key, entry), expected) in memory.entries.iter().zip(entries) {
                assert_eq!(key, expected[0].as_str().unwrap(), "{name}");
                assert!(
                    (entry.db - expected[1].as_f64().unwrap()).abs() < 1e-9,
                    "{name}"
                );
            }
        }
        assert!(expected.next().is_none());
    }
}
#[test]
fn proposal_drop_or_rejection_does_not_optimistically_mute_or_forget_gain() {
    let mut memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let proposal = memory.propose_toggle(1, &s).unwrap();
    assert_eq!(proposal.before(), &[1., 0.8, 0.25, 0.75]);
    assert_eq!(proposal.after(), &[1., 0.8, 0., 0.]);
    assert!(memory.is_empty());
    assert!(!memory.is_muted(&1, &s).unwrap());
    drop(proposal);
    let proposal = memory.propose_toggle(1, &s).unwrap();
    finish(&mut memory, 1, &mut s, proposal);
    let restore = memory.propose_toggle(1, &s).unwrap();
    assert_eq!(restore.kind(), Kind::Restore);
    drop(restore);
    assert!(memory.is_muted(&1, &s).unwrap());
    assert_eq!(memory.len(), 1);
    let fader = memory.propose_fader(1, &s, 0.).unwrap();
    drop(fader);
    assert!(memory.is_muted(&1, &s).unwrap());
}
#[test]
fn identical_visible_replacement_cannot_restore_old_owner_gain() {
    let mut memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let proposal = memory.propose_toggle(1, &s).unwrap();
    finish(&mut memory, 1, &mut s, proposal);
    assert!(memory.is_muted(&1, &s).unwrap());
    assert!(!memory.is_muted(&2, &s).unwrap());
    assert_eq!(memory.propose_toggle(2, &s).unwrap().kind(), Kind::Mute);
    let old = memory.propose_toggle(1, &s).unwrap();
    memory.invalidate_identity(&1).unwrap();
    assert_eq!(memory.apply_memory(old, &1, &s), Err(Error::StaleProposal));
    assert!(memory.is_empty());
}
#[test]
fn changed_binding_shape_or_owner_rejects_a_prepared_transition() {
    let mut memory = Memory::new(8);
    let s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    for mutation in 0..6 {
        let proposal = memory.propose_toggle(1, &s).unwrap();
        let mut changed = s.clone();
        match mutation {
            0 => changed.channels = vec![0, 1],
            1 => changed.ports[0].id += 1,
            2 => changed.node.id += 1,
            3 => changed.node.serial = Some(12),
            4 => changed.key.push_str(" extra"),
            _ => changed.node.volume.as_mut().unwrap().push(1.),
        }
        assert_eq!(
            memory.apply_memory(proposal, &1, &changed),
            Err(Error::DifferentBinding)
        );
        assert!(memory.is_empty());
    }
    let proposal = memory.propose_toggle(1, &s).unwrap();
    assert_eq!(
        memory.apply_memory(proposal, &2, &s),
        Err(Error::DifferentBinding)
    );
}
#[test]
fn invalidation_retires_plans_even_without_existing_memory_and_cross_cache_apply_fails() {
    let mut memory = Memory::new(8);
    let mut other = Memory::new(8);
    let s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let proposal = memory.propose_toggle(1, &s).unwrap();
    assert_eq!(
        other.apply_memory(proposal, &1, &s),
        Err(Error::StaleProposal)
    );
    for operation in 0..3 {
        let proposal = memory.propose_toggle(1, &s).unwrap();
        match operation {
            0 => memory.invalidate_identity(&1),
            1 => memory.invalidate_key(&s.key),
            _ => memory.clear(),
        }
        .unwrap();
        assert_eq!(
            memory.apply_memory(proposal, &1, &s),
            Err(Error::StaleProposal)
        );
    }
}
#[test]
fn bounded_memory_reports_capacity_without_eviction_and_still_allows_restore() {
    let mut memory = Memory::new(1);
    let mut first = strip("synthetic A", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let second = strip("synthetic B", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let proposal = memory.propose_toggle(1, &first).unwrap();
    finish(&mut memory, 1, &mut first, proposal);
    assert!(matches!(
        memory.propose_toggle(1, &second),
        Err(Error::Capacity)
    ));
    assert!(memory.is_muted(&1, &first).unwrap());
    let restore = memory.propose_toggle(1, &first).unwrap();
    finish(&mut memory, 1, &mut first, restore);
    assert!(memory.is_empty());
    assert!(memory.propose_toggle(1, &second).is_ok());
    assert!(matches!(
        Memory::new(0).propose_toggle(1, &first),
        Err(Error::Capacity)
    ));
}
#[test]
fn zero_gain_can_toggle_memory_without_claiming_a_nonzero_restore() {
    let mut memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0., 0.]);
    for muted in [true, false, true, false] {
        let proposal = memory.propose_toggle(1, &s).unwrap();
        assert_eq!(proposal.before(), proposal.after());
        finish(&mut memory, 1, &mut s, proposal);
        assert_eq!(memory.is_muted(&1, &s).unwrap(), muted);
    }
}
#[test]
fn unrelated_memory_changes_retire_prepared_plans_in_the_serial_owner_contract() {
    let mut memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let first = memory.propose_toggle(1, &s).unwrap();
    let delayed = memory.propose_toggle(1, &s).unwrap();
    finish(&mut memory, 1, &mut s, first);
    assert_eq!(
        memory.apply_memory(delayed, &1, &s),
        Err(Error::StaleProposal)
    );
}
#[test]
fn malformed_vectors_whole_node_and_bus_are_not_partial_mute() {
    let memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let mut whole = s.clone();
    whole.channels = vec![0, 1, 2, 3];
    assert!(matches!(
        memory.propose_toggle(1, &whole),
        Err(Error::WholeNodeOrBus)
    ));
    whole = s.clone();
    whole.bus = Some(pecan_core::domain::buses::BusDef {
        key: "synthetic".into(),
        name: "Synthetic bus".into(),
        file: "synthetic".into(),
        channels: vec![2, 3],
        index: 0,
        coupling: None,
    });
    assert!(matches!(
        memory.propose_toggle(1, &whole),
        Err(Error::WholeNodeOrBus)
    ));
    for channels in [vec![], vec![2, 2], vec![8]] {
        let mut bad = s.clone();
        bad.channels = channels;
        assert!(matches!(
            memory.propose_toggle(1, &bad),
            Err(Error::InvalidChannels)
        ));
    }
    for value in [f32::NAN, f32::INFINITY, -1.] {
        let mut bad = s.clone();
        bad.node.volume.as_mut().unwrap()[0] = value;
        assert!(matches!(
            memory.propose_toggle(1, &bad),
            Err(Error::InvalidGain)
        ));
    }
    assert!(matches!(
        memory.propose_fader(1, &s, f64::NAN),
        Err(Error::InvalidGain)
    ));
    s.node.volume = None;
    assert!(matches!(
        memory.propose_toggle(1, &s),
        Err(Error::MissingVolume)
    ));
}
#[test]
fn revision_exhaustion_rejects_work_without_erasing_memory() {
    let mut memory = Memory::new(8);
    let mut s = strip("synthetic", &[2, 3], &[1., 0.8, 0.25, 0.75]);
    let proposal = memory.propose_toggle(1, &s).unwrap();
    finish(&mut memory, 1, &mut s, proposal);
    memory.revision = u64::MAX;
    assert!(matches!(
        memory.propose_toggle(1, &s),
        Err(Error::RevisionExhausted)
    ));
    assert_eq!(memory.clear(), Err(Error::RevisionExhausted));
    assert_eq!(memory.len(), 1);
}
