//! Synthetic documents and deterministic delivery. No worker/profile/audio I/O.
use super::*;
use crate::settings_storage::{SettingsFile, StorageError, WritePhase};
use std::{
    fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "pecan-prefs-bridge-synthetic-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self) -> SettingsFile {
        SettingsFile::new(self.0.join("settings.json")).unwrap()
    }
    fn snapshot(&self, doc: &SettingsV1) -> LoadedSettings {
        fs::write(self.file().path(), serde_json::to_vec(doc).unwrap()).unwrap();
        self.file().load().unwrap()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn doc(width: f64) -> SettingsV1 {
    let mut p = SettingsV1::default().into_preferences();
    p.destination_width_px = Some(width);
    SettingsV1::new(p).unwrap()
}
fn loaded(s: &mut PreferencesState, dir: &Dir, request: u64, doc: &SettingsV1) {
    s.event(&SettingsEvent::Loaded {
        request,
        snapshot: dir.snapshot(doc),
    });
}
fn admitted(s: &mut PreferencesState, id: u64, doc: SettingsV1) {
    assert_eq!(
        s.submit_with(SettingsOperation::Save(doc), |_| Ok(id))
            .unwrap(),
        id
    );
}
#[test]
fn initial_load_precedes_edit_and_old_saved_results_only_advance_baseline() {
    let d = Dir::new();
    let mut p = PreferencesState::new(true);
    assert!(p.edit(doc(200.)).is_err());
    loaded(&mut p, &d, 0, &doc(170.));
    assert!(p.editing_allowed());
    p.edit(doc(200.)).unwrap();
    admitted(&mut p, 1, doc(200.));
    p.edit(doc(250.)).unwrap();
    p.event(&SettingsEvent::Saved {
        request: 1,
        snapshot: d.snapshot(&doc(200.)),
    });
    assert_eq!(p.loaded().unwrap().settings(), &doc(200.));
    assert_eq!(p.document(), &doc(250.));
    assert!(p.dirty());
    admitted(&mut p, 2, doc(250.));
    p.event(&SettingsEvent::Saved {
        request: 2,
        snapshot: d.snapshot(&doc(250.)),
    });
    assert!(!p.dirty());
    p.event(&SettingsEvent::Saved {
        request: 1,
        snapshot: d.snapshot(&doc(200.)),
    });
    assert_eq!(p.document(), &doc(250.));
    assert_eq!(p.loaded().unwrap().settings(), &doc(250.));
}
#[test]
fn latest_unsent_is_constant_size_and_queue_full_does_not_claim_admission() {
    let d = Dir::new();
    let mut p = PreferencesState::new(true);
    loaded(&mut p, &d, 0, &doc(170.));
    for i in 0..100_000 {
        p.edit(doc(200. + (i % 100) as f64)).unwrap();
    }
    assert!(p.pending.is_empty());
    assert_eq!(p.document(), &doc(299.));
    let error = p
        .submit_with(SettingsOperation::Save(doc(333.)), |op| {
            Err(SubmissionError {
                reason: SubmissionFailure::QueueFull,
                operation: op,
            })
        })
        .unwrap_err();
    assert_eq!(error.reason, SubmissionFailure::QueueFull);
    assert_eq!(
        p.document(),
        &doc(299.),
        "rejected explicit save remains caller-owned"
    );
    assert!(p.dirty());
    assert!(p.message().contains("queue full"));
    admitted(&mut p, 1, doc(299.));
    assert_eq!(p.pending.len(), 1);
}
#[test]
fn post_publish_error_needs_explicit_reload_and_save_confirmation_retaining_draft() {
    let d = Dir::new();
    let mut p = PreferencesState::new(true);
    loaded(&mut p, &d, 0, &doc(170.));
    p.edit(doc(250.)).unwrap();
    admitted(&mut p, 1, doc(250.));
    p.event(&SettingsEvent::Failed {
        request: 1,
        error: Arc::new(StorageError::Io {
            phase: WritePhase::SyncDirectory,
            path: d.file().path().into(),
            published: true,
            recovery_backup: None,
            source: std::io::Error::other("Synthetic published failure"),
        }),
    });
    assert!(!p.can_save());
    assert!(p.resume().is_err());
    p.submit_with(SettingsOperation::Reload, |_| Ok(2)).unwrap();
    loaded(&mut p, &d, 2, &doc(250.));
    assert_eq!(p.document(), &doc(250.));
    assert!(p.dirty());
    assert!(!p.can_save());
    p.resume().unwrap();
    assert!(p.can_save());
    admitted(&mut p, 3, doc(250.));
    p.event(&SettingsEvent::Saved {
        request: 3,
        snapshot: d.snapshot(&doc(250.)),
    });
    assert!(!p.dirty());
}
#[test]
fn corrupt_and_future_input_are_preserved_and_require_exact_snapshot_recovery() {
    for raw in [
        b"{broken".as_slice(),
        br#"{"schema_version":99,"future_field":"synthetic"}"#.as_slice(),
    ] {
        let d = Dir::new();
        fs::write(d.file().path(), raw).unwrap();
        let snap = d.file().load().unwrap();
        let mut p = PreferencesState::new(true);
        p.event(&SettingsEvent::Loaded {
            request: 0,
            snapshot: snap,
        });
        assert!(p.editing_allowed());
        assert!(p.recovery_required());
        p.edit(doc(240.)).unwrap();
        assert!(!p.can_save());
        let SettingsOperation::Recover { from, settings } = p.recover_operation().unwrap() else {
            panic!()
        };
        assert_eq!(from.original_bytes(), Some(raw));
        assert_eq!(settings, doc(240.));
        assert_eq!(fs::read(d.file().path()).unwrap(), raw);
    }
}
#[test]
fn clean_reload_adopts_external_document_but_dirty_reload_preserves_local_changes() {
    let d = Dir::new();
    let mut p = PreferencesState::new(true);
    loaded(&mut p, &d, 0, &doc(170.));
    p.edit(doc(200.)).unwrap();
    admitted(&mut p, 1, doc(200.));
    p.event(&SettingsEvent::Saved {
        request: 1,
        snapshot: d.snapshot(&doc(200.)),
    });
    loaded(&mut p, &d, 2, &doc(300.));
    assert_eq!(p.document(), &doc(300.));
    p.edit(doc(400.)).unwrap();
    loaded(&mut p, &d, 3, &doc(500.));
    assert_eq!(p.document(), &doc(400.));
    assert_eq!(p.loaded().unwrap().settings(), &doc(500.));
    assert!(!p.can_save());
}
#[test]
fn missing_path_mode_is_memory_only_and_drain_disables_edit_without_discarding_document() {
    let mut p = PreferencesState::new(false);
    p.edit(doc(220.)).unwrap();
    assert!(!p.dirty());
    assert!(p.message().contains("in memory"));
    p.begin_drain();
    assert!(!p.editing_allowed());
    assert!(p.edit(doc(230.)).is_err());
    assert_eq!(p.document(), &doc(220.));
}

#[test]
fn rec_writeback_old_save_queue_full_and_display_interleaving_preserve_latest_document() {
    let d = Dir::new();
    let mut p = PreferencesState::new(true);
    loaded(&mut p, &d, 0, &doc(170.));
    p.edit_rec(RecEdit::Db {
        bus: "rec-1-2".into(),
        db: -6.,
    })
    .unwrap();
    let first = p.document().clone();
    admitted(&mut p, 1, first.clone());
    for n in 0..1000 {
        p.edit_rec(RecEdit::Db {
            bus: "rec-1-2".into(),
            db: -10. - (n % 30) as f64,
        })
        .unwrap();
    }
    let edited = crate::display_settings::DisplayEdit::DestinationWidth(Some(333.))
        .apply(p.document())
        .unwrap();
    p.edit(edited).unwrap();
    p.edit_rec(RecEdit::Mute {
        bus: "rec-1-2".into(),
        mute: true,
    })
    .unwrap();
    let latest = p.document().clone();
    p.event(&SettingsEvent::Saved {
        request: 1,
        snapshot: d.snapshot(&first),
    });
    assert_eq!(p.document(), &latest);
    assert!(p.dirty());
    let next = p.document().clone();
    let err = p
        .submit_with(SettingsOperation::Save(next), |operation| {
            Err(SubmissionError {
                reason: SubmissionFailure::QueueFull,
                operation,
            })
        })
        .unwrap_err();
    assert_eq!(err.reason, SubmissionFailure::QueueFull);
    assert_eq!(p.document(), &latest);
    assert_eq!(p.document().preferences().destination_width_px, Some(333.));
    assert!(p.document().preferences().buses.midi);
    admitted(&mut p, 2, latest.clone());
    p.event(&SettingsEvent::Saved {
        request: 2,
        snapshot: d.snapshot(&latest),
    });
    assert!(!p.dirty());
}
