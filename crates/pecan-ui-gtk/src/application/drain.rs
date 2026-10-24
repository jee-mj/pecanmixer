//! One off-UI settings drain per quit. No thread joins run on the GTK context.
use super::preferences::PreferencesState;
use crate::settings_worker::{SettingsEvent, SettingsWorker};
use std::{
    sync::{
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct SettingsSummary {
    pub results: u64,
    pub last_request: u64,
    pub errors: Vec<String>,
    pub failures: u64,
    pub recovery_required: bool,
    pub recovered: u64,
    pub worker_panicked: bool,
    pub unsaved_preferences: bool,
}
impl SettingsSummary {
    pub(super) fn event(&mut self, event: SettingsEvent) {
        self.results += 1;
        self.last_request = self.last_request.max(event.request());
        match event {
            SettingsEvent::Loaded { snapshot, .. } | SettingsEvent::Saved { snapshot, .. } => {
                self.recovery_required = snapshot.recovery_required()
            }
            SettingsEvent::Recovered { snapshot, .. } => {
                self.recovered += 1;
                self.recovery_required = snapshot.recovery_required();
            }
            SettingsEvent::Failed { error, .. } | SettingsEvent::Blocked { error, .. } => {
                self.failures += 1;
                if self.errors.len() < 32 {
                    self.errors.push(error.to_string());
                }
            }
        }
    }
    pub(super) fn result(&self) -> Result<(), String> {
        if self.worker_panicked {
            Err("Settings worker panicked; its thread was joined.".into())
        } else if self.failures != 0 || self.unsaved_preferences {
            Err(format!(
                "{} settings operations failed/blocked; {}",
                self.failures,
                self.errors.join("; ")
            ))
        } else {
            Ok(())
        }
    }
}
pub(super) struct DrainResult {
    pub summary: SettingsSummary,
    pub preferences: PreferencesState,
}
pub(super) struct SettingsDrain {
    payload: Arc<Mutex<Option<(SettingsWorker, PreferencesState)>>>,
    receiver: Option<Receiver<DrainResult>>,
    initial: SettingsSummary,
    retry_at: Duration,
    pub warning: Option<String>,
    #[cfg(test)]
    pub gate: Option<(mpsc::SyncSender<thread::ThreadId>, Receiver<()>)>,
    #[cfg(test)]
    pub fail_next_spawn: bool,
}
impl SettingsDrain {
    pub fn new(
        worker: SettingsWorker,
        summary: SettingsSummary,
        mut preferences: PreferencesState,
    ) -> Self {
        preferences.begin_drain();
        Self {
            payload: Arc::new(Mutex::new(Some((worker, preferences)))),
            receiver: None,
            initial: summary,
            retry_at: Duration::ZERO,
            warning: None,
            #[cfg(test)]
            gate: None,
            #[cfg(test)]
            fail_next_spawn: false,
        }
    }
    pub fn poll(&mut self, now: Duration) -> Option<DrainResult> {
        if self.receiver.is_none() && now >= self.retry_at {
            #[cfg(test)]
            if self.fail_next_spawn {
                self.fail_next_spawn = false;
                self.retry_at = now.saturating_add(Duration::from_secs(1));
                self.warning = Some(
                    "Synthetic drain helper spawn failure; settings still owned/pending.".into(),
                );
                return None;
            }
            let payload = self.payload.clone();
            let mut summary = self.initial.clone();
            let (sender, receiver) = mpsc::sync_channel(1);
            #[cfg(test)]
            let gate = self.gate.take();
            let spawned = thread::Builder::new()
                .name("pecanmixer-settings-drain".into())
                .spawn(move || {
                    #[cfg(test)]
                    if let Some((entered, release)) = gate {
                        let _ = entered.send(thread::current().id());
                        let _ = release.recv();
                    }
                    let (mut worker, mut preferences) = payload
                        .lock()
                        .unwrap()
                        .take()
                        .expect("one settings drain helper");
                    // Keep admission open until the last coalesced document is
                    // acknowledged, or an explicit error makes saving impossible.
                    // Results are drained while retrying a full request queue.
                    loop {
                        let mut stopped = false;
                        for _ in 0..32 {
                            match worker.try_event() {
                                Ok(event) => {
                                    preferences.event(&event);
                                    summary.event(event);
                                }
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    stopped = true;
                                    break;
                                }
                            }
                        }
                        if stopped {
                            preferences.stopped();
                            break;
                        }
                        preferences.pump(&mut worker);
                        if !preferences.has_pending() && !preferences.needs_final_admission() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    let (events, thread) = worker.close();
                    for event in events {
                        preferences.event(&event);
                        summary.event(event);
                    }
                    // Includes TLS teardown. A result is sent only after the real
                    // settings worker joined, not merely after its result queue emptied.
                    summary.worker_panicked = thread.join().is_err();
                    if let Some(error) = preferences.unsaved_error() {
                        summary.unsaved_preferences = true;
                        if summary.errors.len() < 32 {
                            summary.errors.push(error);
                        }
                    }
                    preferences.finish_drain();
                    let _ = sender.send(DrainResult {
                        summary,
                        preferences,
                    });
                });
            match spawned {
                Ok(helper) => {
                    // This helper owns no settings worker/files after sending
                    // completion. It is not joined by a GTK callback.
                    drop(helper);
                    self.receiver = Some(receiver);
                    self.warning = None;
                }
                Err(error) => {
                    // The Arc payload retains the worker and its join handle;
                    // failed spawn never drops/detaches that accepted work.
                    self.retry_at = now.saturating_add(Duration::from_secs(1));
                    self.warning = Some(format!(
                        "Settings drain could not start; still pending: {error}"
                    ));
                }
            }
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(summary) => return Some(summary),
                Err(TryRecvError::Disconnected) => self.warning = Some("Settings drain completion channel closed; joined completion is unknown. Still pending.".into()),
                Err(TryRecvError::Empty) => {}
            }
        }
        None
    }
}
