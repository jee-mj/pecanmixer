use super::*;
use crate::shutdown::{Shutdown, ShutdownReport};
use crate::test_support::{assert_wav, TestDir};
use std::io::{Cursor, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn track(name: &str, channels: u16) -> Spoor {
    Spoor {
        naam: name.into(),
        kanalen: channels,
    }
}

fn completion(writer: std::thread::JoinHandle<Result<(), String>>) -> ShutdownReport {
    let completion = Shutdown::new();
    completion.writer(writer);
    completion.worker(std::thread::spawn(|| {}));
    completion.wait_timeout(Duration::from_secs(2)).unwrap()
}

#[test]
fn stop_acknowledges_actual_float_wav_headers_samples_and_frame_counts() {
    let dir = TestDir::new("wav-stop");
    let (tx, writer) =
        schrijver(&dir.0, vec![track("Mono", 1), track("Stereo", 2)], 48_000).unwrap();
    tx.send(Bericht::Audio(0, vec![0.25, -0.5])).unwrap();
    tx.send(Bericht::Audio(1, vec![0.25, -0.25, 0.5, -0.5]))
        .unwrap();
    tx.send(Bericht::Audio(0, vec![1.0, 0.0])).unwrap();
    tx.send(Bericht::Audio(1, vec![1.0, 0.0, -1.0, 0.75]))
        .unwrap();
    tx.send(Bericht::Stop(Some(MidiBestand {
        namen: vec!["Synthetic MIDI".into()],
        events: vec![(0, 0.0, vec![0x90, 60, 100]), (0, 0.5, vec![0x80, 60, 0])],
        pulsen: None,
    })))
    .unwrap();
    let report = completion(writer);
    assert!(report.is_success(), "{report:?}");
    assert_eq!(report.writers_finalized, 1);
    assert_wav(&dir.0.join("Mono.wav"), 1, 48_000, &[0.25, -0.5, 1.0, 0.0]);
    assert_wav(
        &dir.0.join("Stereo.wav"),
        2,
        48_000,
        &[0.25, -0.25, 0.5, -0.5, 1.0, 0.0, -1.0, 0.75],
    );
    let midi = std::fs::read(dir.0.join("midi.mid")).unwrap();
    assert_eq!(midly::Smf::parse(&midi).unwrap().tracks.len(), 2);
}

#[test]
fn closing_one_bus_preserves_its_length_while_other_tracks_continue() {
    let dir = TestDir::new("wav-close");
    let (tx, writer) =
        schrijver(&dir.0, vec![track("First", 1), track("Other", 1)], 44_100).unwrap();
    for message in [
        Bericht::Audio(0, vec![0.1, 0.2]),
        Bericht::Audio(1, vec![0.3, 0.4]),
        Bericht::Sluit(0),
        Bericht::Audio(0, vec![9.0]),
        Bericht::Audio(1, vec![0.5, 0.6]),
        Bericht::Stop(None),
    ] {
        tx.send(message).unwrap();
    }
    assert!(completion(writer).is_success());
    assert_wav(&dir.0.join("First.wav"), 1, 44_100, &[0.1, 0.2]);
    assert_wav(&dir.0.join("Other.wav"), 1, 44_100, &[0.3, 0.4, 0.5, 0.6]);
}

#[test]
fn sender_disconnect_finalizes_queued_samples_without_an_explicit_stop() {
    let dir = TestDir::new("wav-eof");
    let (tx, writer) = schrijver(&dir.0, vec![track("Audio", 1)], 48_000).unwrap();
    tx.send(Bericht::Audio(0, vec![0.25, 0.5])).unwrap();
    drop(tx);
    assert!(completion(writer).is_success());
    assert_wav(&dir.0.join("Audio.wav"), 1, 48_000, &[0.25, 0.5]);
}

#[test]
fn initialization_failure_is_reported_and_previously_opened_tracks_are_finalized() {
    let dir = TestDir::new("wav-open-fail");
    std::fs::create_dir(dir.0.join("Blocked.wav")).unwrap();
    let (tx, writer) =
        schrijver(&dir.0, vec![track("First", 1), track("Blocked", 1)], 48_000).unwrap();
    drop(tx);
    let report = completion(writer);
    assert!(!report.is_success());
    assert_eq!(report.writers_failed, 1);
    assert!(report.writer_errors[0].contains("Blocked.wav"));
    assert_wav(&dir.0.join("First.wav"), 1, 48_000, &[]);
    let not_directory = dir.0.join("file");
    std::fs::write(&not_directory, b"existing file").unwrap();
    assert!(schrijver(&not_directory, vec![track("Audio", 1)], 48_000).is_err());
}

#[test]
fn incomplete_interleaved_frame_and_invalid_specs_cannot_report_success() {
    let dir = TestDir::new("wav-partial-frame");
    assert!(schrijver(&dir.0, vec![track("Bad", 0)], 48_000).is_err());
    assert!(schrijver(&dir.0, vec![track("Bad", 1)], 0).is_err());
    let (tx, writer) = schrijver(&dir.0, vec![track("Stereo", 2)], 48_000).unwrap();
    tx.send(Bericht::Audio(0, vec![0.1, 0.2, 0.3])).unwrap();
    tx.send(Bericht::Stop(None)).unwrap();
    let report = completion(writer);
    assert!(!report.is_success());
    assert_eq!(report.writers_failed, 1);
    assert!(report.writer_errors[0].contains("finalization"));
}

#[test]
fn midi_write_error_is_not_hidden_after_audio_finalization() {
    let dir = TestDir::new("wav-midi-fail");
    std::fs::create_dir(dir.0.join("midi.mid")).unwrap();
    let (tx, writer) = schrijver(&dir.0, vec![track("Audio", 1)], 48_000).unwrap();
    tx.send(Bericht::Audio(0, vec![0.25])).unwrap();
    tx.send(Bericht::Stop(Some(MidiBestand {
        namen: vec!["MIDI".into()],
        events: vec![],
        pulsen: None,
    })))
    .unwrap();
    let report = completion(writer);
    assert!(!report.is_success());
    assert_eq!(report.writers_failed, 1);
    assert_wav(&dir.0.join("Audio.wav"), 1, 48_000, &[0.25]);
}

#[derive(Clone, Copy)]
enum Failure {
    Write,
    Seek,
    Flush,
}

#[derive(Default)]
struct SinkState {
    data: Cursor<Vec<u8>>,
    fail: Option<Failure>,
    flushes: usize,
}
struct Sink(Arc<Mutex<SinkState>>);
impl Write for Sink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut state = self.0.lock().unwrap();
        if matches!(state.fail, Some(Failure::Write)) {
            return Err(std::io::Error::other("injected write failure"));
        }
        state.data.write(data)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut state = self.0.lock().unwrap();
        if matches!(state.fail, Some(Failure::Flush)) {
            return Err(std::io::Error::other("injected flush failure"));
        }
        state.flushes += 1;
        Ok(())
    }
}
impl Seek for Sink {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let mut state = self.0.lock().unwrap();
        if matches!(state.fail, Some(Failure::Seek)) {
            return Err(std::io::Error::other("injected seek failure"));
        }
        state.data.seek(pos)
    }
}

#[test]
fn write_seek_and_flush_failures_still_explicitly_finalize_other_tracks() {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    for failure in [Failure::Write, Failure::Seek, Failure::Flush] {
        for early_close in [false, true] {
            let bad = Arc::new(Mutex::new(SinkState::default()));
            let good = Arc::new(Mutex::new(SinkState::default()));
            let wav = vec![
                Some(hound::WavWriter::new(Sink(Arc::clone(&bad)), spec).unwrap()),
                Some(hound::WavWriter::new(Sink(Arc::clone(&good)), spec).unwrap()),
            ];
            bad.lock().unwrap().fail = Some(failure);
            let mut messages = vec![
                Bericht::Audio(1, vec![0.25, 0.5]),
                Bericht::Audio(0, vec![0.75]),
            ];
            if early_close {
                messages.push(Bericht::Sluit(0));
            }
            messages.push(Bericht::Stop(None));
            let error = verwerk(wav, messages).err().unwrap();
            assert!(error.contains("injected"), "{error}");
            let good = good.lock().unwrap();
            assert!(good.flushes > 0, "other track must be explicitly flushed");
            let mut reader = hound::WavReader::new(Cursor::new(good.data.get_ref())).unwrap();
            assert_eq!(reader.duration(), 2);
            assert_eq!(
                reader
                    .samples::<f32>()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                [0.25, 0.5]
            );
        }
    }
}
