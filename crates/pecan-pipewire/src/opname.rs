//! Opnemen: de Rec bus, de schrijfdraad en de klok.
//!
//! De **Rec bus** is een eigen, zichtbare node met een instelbaar aantal
//! audio-ingangen (`rec_1`, `rec_2`, …) en een MIDI-ingang. De schil deelt de
//! kanalen in bussen in: een stereopaar of twee mono's, zoals de uitgangen in
//! CachyMix. Je routeert er bronnen heen zoals naar elke andere bestemming;
//! PipeWire telt alles op één ingang op. Elke bus neemt op als een eigen
//! take: één WAV in een eigen map. **Record** start alle gewapende bussen
//! samen in één take, en omdat ze uit één node en één process-ronde
//! komen, liggen ze sample-precies gelijk. `node.autoconnect = false`:
//! WirePlumber hangt er anders de standaard-microfoon aan.
//!
//! **MIDI** komt per bronpoort uit [`crate::midimeter`]: wat bij Record
//! naar de MIDI-ingang van de bus gerouteerd is, krijgt een eigen spoor.
//!
//! Alle process-callbacks draaien op de loop van de graph-draad (geen
//! `RT_PROCESS`). Ze schrijven nooit naar schijf: blokken gaan over een kanaal
//! naar een schrijfdraad per take.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::{c_void, CString};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::ptr;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

pub use crate::writer_queue::{
    WriterSendError, WriterSender, MAX_WRITER_SAMPLES, WRITER_AUDIO_CAPACITY,
};
use crate::writer_status::Reporter;
pub use crate::writer_status::{
    NotOpened, TakeId, TrackId, TrackPhase, TrackReport, WriterOutcome, WriterPending, WriterPhase,
    WriterReceipt, WriterReport, MAX_TRACKED_TRACKS,
};
pub use pecan_core::graph::KlokStand;
use pipewire as pw;
use pw::spa;
use pw::sys as pw_sys;

pub const BUS_NAAM: &str = "pecanmixer-opname";
/// Ticks per kwartnoot in het MIDI-bestand.
const PPQ: u32 = 480;
/// MIDI Clock: 24 pulsen per kwartnoot.
const PULSEN: u32 = 24;
const INTERN_BPM: f64 = 120.0;

/// Gedeeld tussen de graph-draad en de process-callbacks: de lopende takes.
/// Leeg = niet aan het opnemen.
pub type Opnemer = Rc<RefCell<Vec<Actief>>>;

/// Eén lopende take: een eigen map en schrijfdraad.
pub struct Actief {
    pub tx: mpsc::Sender<Bericht>,
    pub t0: Instant,
    /// Per bus die nog opneemt: (spoor, welke kanalen van de Rec bus, naam).
    pub bussen: Vec<(usize, Vec<usize>, String)>,
    /// Per MIDI-bronpoort: spoornummer in het MIDI-bestand.
    pub midi: BTreeMap<u32, usize>,
    /// Namen van de MIDI-sporen, voor het bestand.
    pub midi_namen: Vec<String>,
    /// (spoor, seconden sinds t0, bytes).
    pub midi_events: Vec<(usize, f64, Vec<u8>)>,
    /// Klokpulsen van de externe klok, seconden sinds t0.
    pub pulsen: Vec<f64>,
}

impl Actief {
    pub fn sinds(&self, t: Instant) -> f64 {
        t.saturating_duration_since(self.t0).as_secs_f64()
    }
}

pub enum Bericht {
    /// Interleaved samples voor een spoor.
    Audio(usize, Vec<f32>),
    /// Dit spoor is klaar (zijn bus is gestopt); de rest loopt door.
    Sluit(usize),
    /// Klaar: dit MIDI-bestand nog schrijven (als er een is), dan afsluiten.
    Stop(Option<MidiBestand>),
}

/// Wat de schrijfdraad nodig heeft om het MIDI-bestand te maken.
pub struct MidiBestand {
    pub namen: Vec<String>,
    pub events: Vec<(usize, f64, Vec<u8>)>,
    /// `None` = interne klok.
    pub pulsen: Option<Vec<f64>>,
}

pub struct Spoor {
    pub naam: String,
    pub kanalen: u16,
}

/// Het kanaal naar de schrijfdraad en de draad zelf, die zegt of het lukte.
pub type Schrijver = (
    mpsc::Sender<Bericht>,
    std::thread::JoinHandle<Result<(), String>>,
);

/// Native preparation only: this is not connected to command admission or the
/// audio callback. The owning backend must validate origin/configuration before
/// calling it and attach producers only while the receipt is Ready.
pub struct TrackedWriter {
    pub sender: WriterSender,
    pub receipt: WriterReceipt,
}

/// Reserve a unique child take directory and open WAVs on the writer thread.
/// No filesystem operation or join is performed by this function. The supplied
/// backend completion coordinator owns the join and retains all errors.
pub fn tracked_writer(
    root: &Path,
    tracks: Vec<Spoor>,
    rate: u32,
    completion: &crate::shutdown::Shutdown,
) -> std::io::Result<TrackedWriter> {
    tracked_writer_with(root, tracks, rate, completion, |path, spec| {
        hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
    })
}

fn tracked_writer_with<W, F>(
    root: &Path,
    tracks: Vec<Spoor>,
    rate: u32,
    completion: &crate::shutdown::Shutdown,
    mut open: F,
) -> std::io::Result<TrackedWriter>
where
    W: Write + Seek + Send + 'static,
    F: FnMut(&Path, hound::WavSpec) -> std::io::Result<hound::WavWriter<W>> + Send + 'static,
{
    use std::collections::BTreeSet;
    let mut names = BTreeSet::new();
    if tracks.is_empty()
        || tracks.len() > MAX_TRACKED_TRACKS
        || rate == 0
        || root.as_os_str().len() > 4096
        || tracks.iter().any(|t| {
            t.kanalen == 0
                || usize::from(t.kanalen) > MAX_TRACKED_TRACKS
                || t.naam.len() > 256
                || veilig(&t.naam).is_empty()
                || !names.insert(veilig(&t.naam))
                || rate
                    .checked_mul(u32::from(t.kanalen))
                    .and_then(|v| v.checked_mul(4))
                    .is_none()
        })
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid or duplicate tracked writer format/name",
        ));
    }
    let lease = completion.reserve_writer(tracks.len())?;
    let (receipt, reporter) =
        crate::writer_status::status(&tracks, completion.tracked_stop_signal())?;
    let (sender, receiver) = crate::writer_queue::channel(&receipt);
    let root = root.to_owned();
    let producer = reporter.clone();
    let take = receipt.take_id();
    let thread = std::thread::Builder::new()
        .name("pecanmixer-tracked-writer".into())
        .spawn(move || {
            let result = (|| {
                if producer.stopping() {
                    return Ok(());
                }
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                let mut directory = None;
                for attempt in 0..128 {
                    let name = if attempt == 0 {
                        format!("take-{}", take.get())
                    } else {
                        format!("take-{}-{attempt}", take.get())
                    };
                    let candidate = root.join(name);
                    match std::fs::create_dir(&candidate) {
                        Ok(()) => {
                            directory = Some(candidate);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.to_string()),
                    }
                }
                let directory = directory.ok_or("take directory reservation exhausted")?;
                producer.directory(directory.clone());
                let midi = write_tracks(
                    &directory,
                    &tracks,
                    rate,
                    receiver,
                    Some(&producer),
                    &mut open,
                )?;
                if let Some(midi) = midi {
                    schrijf_midi(&directory.join("midi.mid"), &midi)?;
                }
                Ok(())
            })()
            .map_err(|e: String| crate::writer_status::error_text(&e));
            producer.finished(&result);
            result
        })?;
    completion.tracked_writer(thread, reporter, lease);
    Ok(TrackedWriter { sender, receipt })
}

/// Start de schrijfdraad met één WAV per spoor in `map`. Geeft het kanaal
/// en de draad terug; de draad eindigt na [`Bericht::Stop`] of kanaalsluiting.
/// `Ok(())` uit de draad betekent dat alle WAV-koppen en buffers expliciet
/// afgerond zijn, en een eventueel MIDI-bestand geschreven is. Geen fsync.
/// WAV/MIDI-bestandsfouten komen via het draadresultaat. Ongeldige formaten,
/// het maken van de map en een spawn-fout komen direct terug, zoals voorheen.
pub fn schrijver(map: &Path, sporen: Vec<Spoor>, rate: u32) -> std::io::Result<Schrijver> {
    if (!sporen.is_empty() && rate == 0) || sporen.iter().any(|s| s.kanalen == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "zero WAV channels or sample rate",
        ));
    }
    // Reserve the take directory before the graph selects another take name.
    // Moving this existing non-RT setup to the writer would allow two rapid
    // starts to pick the same timestamp path before either writer creates it.
    std::fs::create_dir_all(map)?;
    let (tx, rx) = mpsc::channel::<Bericht>();
    let map = map.to_owned();
    let draad = std::thread::Builder::new()
        .name("pecanmixer-schrijver".into())
        .spawn(move || {
            schrijf_take(&map, &sporen, rate, rx).map_err(|e| format!("{}: {e}", map.display()))
        })?;
    Ok((tx, draad))
}

fn schrijf_take(
    map: &Path,
    sporen: &[Spoor],
    rate: u32,
    rx: mpsc::Receiver<Bericht>,
) -> Result<(), String> {
    if let Some(midi) = write_tracks(map, sporen, rate, rx, None, &mut |path, spec| {
        hound::WavWriter::create(path, spec).map_err(std::io::Error::other)
    })? {
        schrijf_midi(&map.join("midi.mid"), &midi)?;
    }
    Ok(())
}

fn write_tracks<W: Write + Seek>(
    map: &Path,
    sporen: &[Spoor],
    rate: u32,
    messages: impl IntoIterator<Item = Bericht>,
    reporter: Option<&Reporter>,
    open: &mut impl FnMut(&Path, hound::WavSpec) -> std::io::Result<hound::WavWriter<W>>,
) -> Result<Option<MidiBestand>, String> {
    let mut wav = Vec::new();
    for (i, s) in sporen.iter().enumerate() {
        if reporter.is_some_and(Reporter::stopping) {
            let errors = finalize_tracks(&mut wav, &[], reporter);
            for index in i..sporen.len() {
                if let Some(r) = reporter {
                    r.track(index, TrackPhase::NotOpened(NotOpened::Stopped));
                }
            }
            return if errors.is_empty() {
                Ok(None)
            } else {
                Err(errors.join("; "))
            };
        }
        let spec = hound::WavSpec {
            channels: s.kanalen,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let pad = map.join(format!("{}.wav", veilig(&s.naam)));
        match open(&pad, spec) {
            Ok(writer) => {
                wav.push(Some(writer));
                if let Some(r) = reporter {
                    r.track(i, TrackPhase::Open);
                }
            }
            Err(error) => {
                let mut errors = vec![format!("{}: {error}", pad.display())];
                errors.extend(finalize_tracks(&mut wav, &[], reporter));
                return Err(errors.join("; "));
            }
        }
    }
    if reporter.is_some_and(Reporter::stopping) {
        let errors = finalize_tracks(&mut wav, &[], reporter);
        return if errors.is_empty() {
            Ok(None)
        } else {
            Err(errors.join("; "))
        };
    }
    if let Some(r) = reporter {
        r.ready();
    }
    process_tracks(wav, messages, reporter)
}

/// Always attempt to finalize every open track, including after another
/// track's write/close fails. Relying on WavWriter's Drop would hide errors.
fn finalize_tracks<W: Write + Seek>(
    wav: &mut [Option<hound::WavWriter<W>>],
    failures: &[Option<String>],
    reporter: Option<&Reporter>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, writer) in wav.iter_mut().enumerate() {
        if let Some(writer) = writer.take() {
            let result = writer
                .finalize()
                .map_err(|error| format!("track {i} finalization: {error}"));
            if let Err(error) = &result {
                errors.push(error.clone());
            }
            let result = match (failures.get(i).and_then(Option::as_ref), result) {
                (Some(error), Err(close)) => Err(format!("{error}; {close}")),
                (Some(error), Ok(())) => Err(error.clone()),
                (None, result) => result,
            };
            if let Some(r) = reporter {
                if let Some(error) = r.finalized(i, result) {
                    errors.push(error);
                }
            }
        }
    }
    errors
}

#[cfg(test)]
fn verwerk<W: Write + Seek>(
    wav: Vec<Option<hound::WavWriter<W>>>,
    berichten: impl IntoIterator<Item = Bericht>,
) -> Result<Option<MidiBestand>, String> {
    process_tracks(wav, berichten, None)
}
fn process_tracks<W: Write + Seek>(
    mut wav: Vec<Option<hound::WavWriter<W>>>,
    berichten: impl IntoIterator<Item = Bericht>,
    reporter: Option<&Reporter>,
) -> Result<Option<MidiBestand>, String> {
    let mut midi = None;
    let mut errors = Vec::new();
    let mut failures = vec![None; wav.len()];
    'messages: for bericht in berichten {
        match bericht {
            Bericht::Audio(i, samples) => {
                if let Some(Some(writer)) = wav.get_mut(i) {
                    let mut written = 0;
                    for sample in samples {
                        if let Err(error) = writer.write_sample(sample) {
                            let error = format!("track {i} write: {error}");
                            failures[i] = Some(error.clone());
                            errors.push(error);
                            break;
                        }
                        written += 1;
                    }
                    if let Some(r) = reporter {
                        r.written(i, written);
                    }
                    if failures[i].is_some() {
                        break 'messages;
                    }
                }
            }
            Bericht::Sluit(i) => {
                if let Some(writer) = wav.get_mut(i).and_then(Option::take) {
                    let result = writer
                        .finalize()
                        .map_err(|error| format!("track {i} close: {error}"));
                    if let Some(r) = reporter {
                        if let Some(error) = r.finalized(i, result.clone()) {
                            errors.push(error);
                            break;
                        }
                    }
                    if let Err(error) = result {
                        errors.push(error);
                        break;
                    }
                }
            }
            Bericht::Stop(value) => {
                midi = value;
                break;
            }
        }
    }
    if let Some(r) = reporter {
        r.finalizing();
    }
    errors.extend(finalize_tracks(&mut wav, &failures, reporter));
    if errors.is_empty() {
        Ok(midi)
    } else {
        Err(errors.join("; "))
    }
}

/// Een bestandsnaam zonder slash en dergelijke.
fn veilig(naam: &str) -> String {
    naam.chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.()".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `~/Recordings/2026-10-18_1043`, met een volgnummer als die al bestaat.
pub fn nieuwe_map() -> PathBuf {
    let basis = opnamemap().join(tijdstempel());
    let mut map = basis.clone();
    let mut n = 2;
    while map.exists() {
        map = PathBuf::from(format!("{}-{n}", basis.display()));
        n += 1;
    }
    map
}

/// `~/Recordings`: elke take krijgt hierin een eigen map.
pub fn opnamemap() -> PathBuf {
    let thuis = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| ".".into());
    thuis.join("Recordings")
}

/// Opent `map` in de bestandsbeheerder (`xdg-open`), maar alleen als hij in
/// `~/Recordings` ligt; anders `~/Recordings` zelf, die zo nodig eerst
/// gemaakt wordt.
pub fn open_map(map: Option<&str>) -> std::io::Result<()> {
    let basis = opnamemap();
    std::fs::create_dir_all(&basis)?;
    let binnen = |m: &PathBuf| match (m.canonicalize(), basis.canonicalize()) {
        (Ok(m), Ok(b)) => m.starts_with(b),
        _ => false,
    };
    let doel = map.map(PathBuf::from).filter(binnen).unwrap_or(basis);
    let mut kind = std::process::Command::new("xdg-open").arg(&doel).spawn()?;
    // Oogsten als hij klaar is, anders blijft er een zombie hangen.
    std::thread::spawn(move || kind.wait());
    Ok(())
}

fn tijdstempel() -> String {
    // SAFETY: localtime_r schrijft alleen in de meegegeven struct.
    unsafe {
        let t = libc::time(ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02}_{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        )
    }
}

// --- Klok ---

/// Seconden → ticks. Intern op vaste tijd; extern op de getelde pulsen, met
/// interpolatie tussen twee pulsen.
pub fn naar_ticks(t: f64, pulsen: Option<&[f64]>) -> u64 {
    let per_puls = f64::from(PPQ / PULSEN);
    let Some(p) = pulsen.filter(|p| p.len() >= 2) else {
        return (t.max(0.0) * INTERN_BPM / 60.0 * f64::from(PPQ)).round() as u64;
    };
    // De puls ervoor: binair zoeken.
    let i = p.partition_point(|&x| x <= t);
    let (i, a, b) = if i == 0 {
        (0, p[0], p[1])
    } else if i >= p.len() {
        (p.len() - 2, p[p.len() - 2], p[p.len() - 1])
    } else {
        (i - 1, p[i - 1], p[i])
    };
    let d = (b - a).max(1e-6);
    let pos = i as f64 + (t - a) / d;
    (pos.max(0.0) * per_puls).round() as u64
}

/// Tempo per tel uit de pulsen: (tick, microseconden per kwartnoot). Een nieuw
/// tempo alleen als het meer dan een half procent afwijkt van het vorige.
pub fn tempokaart(pulsen: &[f64]) -> Vec<(u64, u32)> {
    let mut uit: Vec<(u64, u32)> = Vec::new();
    let tel = PULSEN as usize;
    let mut k = 0;
    while (k + 1) * tel < pulsen.len() {
        let duur = pulsen[(k + 1) * tel] - pulsen[k * tel];
        let us = (duur * 1e6).round().clamp(1.0, 16_777_215.0) as u32;
        let anders = uit
            .last()
            .is_none_or(|(_, v)| (f64::from(us) / f64::from(*v) - 1.0).abs() > 0.005);
        if anders {
            uit.push((k as u64 * u64::from(PPQ), us));
        }
        k += 1;
    }
    uit
}

fn schrijf_midi(pad: &Path, m: &MidiBestand) -> Result<(), String> {
    use midly::num::{u24, u28};
    use midly::{Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind};

    let arena = midly::Arena::new();
    let pulsen = m.pulsen.as_deref();
    let mut sporen: Vec<Vec<TrackEvent>> = Vec::new();

    // Spoor 0: het tempo.
    let tempo: Vec<(u64, u32)> = match pulsen {
        Some(p) if p.len() > PULSEN as usize => tempokaart(p),
        _ => vec![(0, (60e6 / INTERN_BPM) as u32)],
    };
    let mut nul = Vec::new();
    let mut vorige = 0u64;
    for (tick, us) in tempo {
        nul.push(TrackEvent {
            delta: u28::new((tick - vorige) as u32),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us))),
        });
        vorige = tick;
    }
    nul.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    sporen.push(nul);

    for (i, naam) in m.namen.iter().enumerate() {
        let mut ev: Vec<(u64, TrackEventKind)> = vec![(
            0,
            TrackEventKind::Meta(MetaMessage::TrackName(arena.add(naam.as_bytes()))),
        )];
        for (spoor, t, bytes) in &m.events {
            if *spoor != i {
                continue;
            }
            // Klok en transport horen niet in de muziek.
            if bytes.first().is_some_and(|&b| b >= 0xf8) {
                continue;
            }
            if let Ok(live) = midly::live::LiveEvent::parse(bytes) {
                ev.push((naar_ticks(*t, pulsen), live.as_track_event(&arena)));
            }
        }
        ev.sort_by_key(|(t, _)| *t);
        let mut spoor = Vec::new();
        let mut vorige = 0u64;
        for (t, kind) in ev {
            spoor.push(TrackEvent {
                delta: u28::new((t - vorige).min(0x0fff_ffff) as u32),
                kind,
            });
            vorige = t;
        }
        spoor.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        sporen.push(spoor);
    }

    let smf = Smf {
        header: Header::new(
            Format::Parallel,
            Timing::Metrical(midly::num::u15::new(PPQ as u16)),
        ),
        tracks: sporen,
    };
    smf.save(pad).map_err(|e| e.to_string())
}

/// De externe klok zoals de MIDI-meter hem ziet.
#[derive(Default)]
pub struct Klok {
    /// De bronpoort waar de klok vandaan komt; `None` = intern.
    pub bron: Option<u32>,
    pub start_stop: bool,
    /// De laatste pulsen, voor het tempo op het scherm.
    pub recent: VecDeque<Instant>,
    /// Start (`true`) of Stop (`false`) gezien op de klokpoort.
    pub verzoek: Option<bool>,
}

impl Klok {
    pub fn puls(&mut self, t: Instant) {
        self.recent.push_back(t);
        while self.recent.len() > PULSEN as usize + 1 {
            self.recent.pop_front();
        }
    }

    /// Tempo gemiddeld over ongeveer één tel, tegen jitter.
    pub fn stand(&self) -> KlokStand {
        let r = &self.recent;
        let laatste = r.back().map(|t| t.elapsed().as_secs_f64());
        if r.len() < 4 || laatste.is_none_or(|s| s > 0.5) {
            return KlokStand::default();
        }
        let tussen: Vec<f64> = r
            .iter()
            .zip(r.iter().skip(1))
            .map(|(a, b)| b.duration_since(*a).as_secs_f64())
            .collect();
        let gem = tussen.iter().sum::<f64>() / tussen.len() as f64;
        let max_afwijking = tussen
            .iter()
            .map(|d| (d / gem - 1.0).abs())
            .fold(0.0, f64::max);
        KlokStand {
            bpm: Some(60.0 / (gem * f64::from(PULSEN))),
            lock: r.len() > PULSEN as usize && max_afwijking < 0.2,
        }
    }
}

// --- De Rec bus ---

const DSP_AUDIO: &str = "32 bit float mono audio";
const DSP_MIDI: &str = "8 bit raw midi";
/// Meer dan genoeg; elke bus is één of twee kanalen.
pub const MAX_KANALEN: usize = 64;

struct BusBinnen {
    /// De audio-ingangen `rec_1`, `rec_2`, …, in volgorde.
    kanalen: Vec<*mut c_void>,
    midi: *mut c_void,
    /// Lineair, per kanaal: de fader van de bus, 0 als hij gemute is.
    gain: Vec<f32>,
    /// Hoogste piek per kanaal sinds de vorige oogst, na de gain.
    pieken: Vec<f32>,
    opnemer: Opnemer,
}

/// De Rec bus: één node met een instelbaar aantal audio-ingangen en één
/// MIDI-ingang. Welke kanalen samen een (stereo)bus vormen, bepaalt de schil;
/// hier is het alleen een rij kanalen met elk een gain en een meter.
pub struct Bus {
    filter: *mut pw_sys::pw_filter,
    _haak: Box<spa::sys::spa_hook>,
    _events: Box<pw_sys::pw_filter_events>,
    binnen: Box<RefCell<BusBinnen>>,
}

/// Voegt één ingang toe aan het filter.
///
/// # Safety
/// `filter` moet leven; process mag niet tegelijk lopen (zelfde draad).
unsafe fn bus_poort(
    filter: *mut pw_sys::pw_filter,
    dsp: &str,
    naam: &str,
    kanaal: Option<&str>,
) -> *mut c_void {
    let mut props = pw::properties::properties! {
        *pw::keys::FORMAT_DSP => dsp,
        *pw::keys::PORT_NAME => naam,
    };
    if let Some(k) = kanaal {
        props.insert(*pw::keys::AUDIO_CHANNEL, k);
    }
    // SAFETY: zie boven; de props gaan naar PipeWire.
    unsafe {
        pw_sys::pw_filter_add_port(
            filter,
            spa::utils::Direction::Input.as_raw(),
            pw_sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
            std::mem::size_of::<usize>(),
            props.into_raw(),
            ptr::null_mut(),
            0,
        )
    }
}

impl Bus {
    pub fn maak(core: &pw::core::CoreRc, opnemer: Opnemer, kanalen: usize) -> Option<Self> {
        let naam = CString::new(BUS_NAAM).ok()?;
        let props = pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Production",
            *pw::keys::MEDIA_CLASS => "Stream/Input/Audio",
            *pw::keys::NODE_NAME => BUS_NAAM,
            *pw::keys::NODE_DESCRIPTION => "Rec bus",
            *pw::keys::MEDIA_NAME => "Rec bus",
            *pw::keys::APP_NAME => "Pecan Mixer",
            "node.autoconnect" => "false",
            "node.dont-reconnect" => "true",
        };
        // SAFETY: zoals in midimeter: de core leeft langer dan de bus.
        let filter =
            unsafe { pw_sys::pw_filter_new(core.as_raw_ptr(), naam.as_ptr(), props.into_raw()) };
        if filter.is_null() {
            return None;
        }
        let binnen = Box::new(RefCell::new(BusBinnen {
            kanalen: Vec::new(),
            // SAFETY: filter leeft, nog niet verbonden.
            midi: if crate::graaf::MIDI {
                // SAFETY: filter leeft, nog niet verbonden.
                unsafe { bus_poort(filter, DSP_MIDI, "midi_in", None) }
            } else {
                ptr::null_mut()
            },
            gain: Vec::new(),
            pieken: Vec::new(),
            opnemer,
        }));
        let events = Box::new(pw_sys::pw_filter_events {
            version: pw_sys::PW_VERSION_FILTER_EVENTS,
            process: Some(bus_process),
            // SAFETY: nul = geen callback.
            ..unsafe { std::mem::zeroed() }
        });
        // SAFETY: een lege haak.
        let mut haak: Box<spa::sys::spa_hook> = Box::new(unsafe { std::mem::zeroed() });
        // SAFETY: alles leeft tot Drop.
        unsafe {
            pw_sys::pw_filter_add_listener(
                filter,
                haak.as_mut(),
                events.as_ref(),
                ptr::from_ref(binnen.as_ref()).cast_mut().cast(),
            );
            if pw_sys::pw_filter_connect(filter, 0, ptr::null_mut(), 0) < 0 {
                pw_sys::pw_filter_destroy(filter);
                return None;
            }
        }
        let bus = Self {
            filter,
            _haak: haak,
            _events: events,
            binnen,
        };
        bus.zet_kanalen(kanalen);
        Some(bus)
    }

    /// Zoveel audio-ingangen, door aan het eind toe te voegen of weg te halen.
    /// Wat aan een weggehaalde ingang hing, verdwijnt met hem.
    pub fn zet_kanalen(&self, n: usize) {
        let n = n.clamp(1, MAX_KANALEN);
        let mut b = self.binnen.borrow_mut();
        while b.kanalen.len() > n {
            if let Some(p) = b.kanalen.pop() {
                // SAFETY: een poort van dit filter; process draait niet nu.
                unsafe { pw_sys::pw_filter_remove_port(p) };
            }
        }
        while b.kanalen.len() < n {
            let i = b.kanalen.len();
            // SAFETY: filter leeft.
            let p = unsafe {
                bus_poort(
                    self.filter,
                    DSP_AUDIO,
                    &format!("rec_{}", i + 1),
                    Some(&format!("AUX{i}")),
                )
            };
            if p.is_null() {
                break;
            }
            b.kanalen.push(p);
        }
        let k = b.kanalen.len();
        b.gain.resize(k, 1.0);
        b.pieken.resize(k, 0.0);
    }

    pub fn zet_gain(&self, gain: &[f32]) {
        let mut b = self.binnen.borrow_mut();
        for (i, g) in b.gain.iter_mut().enumerate() {
            *g = gain.get(i).copied().unwrap_or(1.0).clamp(0.0, 4.0);
        }
    }

    /// Het node-id van de bus, zodra de server hem kent. Niet de naam: een
    /// tweede Pecan Mixer (een voorbeeld naast de app) heeft er ook een.
    pub fn knoop_id(&self) -> Option<u32> {
        // SAFETY: het filter leeft.
        let id = unsafe { pw_sys::pw_filter_get_node_id(self.filter) };
        (id != u32::MAX).then_some(id)
    }

    pub fn oogst(&self) -> Vec<f32> {
        let mut b = self.binnen.borrow_mut();
        let uit = b.pieken.clone();
        b.pieken.iter_mut().for_each(|p| *p = 0.0);
        uit
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        // SAFETY: zelfde draad, process draait niet.
        unsafe {
            pw_sys::pw_filter_disconnect(self.filter);
            pw_sys::pw_filter_destroy(self.filter);
        }
    }
}

unsafe extern "C" fn bus_process(data: *mut c_void, positie: *mut spa::sys::spa_io_position) {
    // SAFETY: `data` is de RefCell<BusBinnen> uit `maak`.
    let binnen = unsafe { &*data.cast::<RefCell<BusBinnen>>() };
    let Ok(mut b) = binnen.try_borrow_mut() else {
        return;
    };
    let n = if positie.is_null() {
        0
    } else {
        // SAFETY: PipeWire geeft een geldige positie mee.
        unsafe { (*positie).clock.duration as usize }
    };
    // De MIDI-poort leegmaken; wat erop binnenkomt neemt de MIDI-meter al per
    // bron op.
    // SAFETY: levende poort; dequeue en queue in één blok.
    unsafe {
        if !b.midi.is_null() {
            let buf = pw_sys::pw_filter_dequeue_buffer(b.midi);
            if !buf.is_null() {
                pw_sys::pw_filter_queue_buffer(b.midi, buf);
            }
        }
    }
    if n == 0 {
        return;
    }
    let BusBinnen {
        kanalen,
        gain,
        pieken,
        opnemer,
        ..
    } = &mut *b;
    let op = opnemer.try_borrow().ok();
    let takes = op.as_deref().filter(|t| !t.is_empty());
    // Elk kanaal: gain erop, piek bijhouden, en bewaren als er opgenomen wordt.
    let mut blok: Vec<Vec<f32>> = Vec::new();
    for (i, &poort) in kanalen.iter().enumerate() {
        // SAFETY: get_dsp_buffer geeft n samples of null (= stilte).
        let p = unsafe { pw_sys::pw_filter_get_dsp_buffer(poort, n as u32).cast::<f32>() };
        let g = gain[i];
        let mut piek = pieken[i];
        let mut kanaal = Vec::new();
        if takes.is_some() {
            kanaal.reserve(n);
        }
        for j in 0..n {
            // SAFETY: binnen de n samples die PipeWire gaf.
            let x = if p.is_null() {
                0.0
            } else {
                unsafe { *p.add(j) }
            } * g;
            piek = piek.max(x.abs());
            if takes.is_some() {
                kanaal.push(x);
            }
        }
        pieken[i] = piek;
        blok.push(kanaal);
    }
    let Some(takes) = takes else { return };
    for a in takes {
        for (spoor, kan, _) in &a.bussen {
            let mut uit = Vec::with_capacity(n * kan.len());
            for j in 0..n {
                for &c in kan {
                    uit.push(blok.get(c).and_then(|k| k.get(j)).copied().unwrap_or(0.0));
                }
            }
            let _ = a.tx.send(Bericht::Audio(*spoor, uit));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interne_klok() {
        // 120 BPM: een seconde is twee tellen.
        assert_eq!(naar_ticks(1.0, None), 2 * u64::from(PPQ));
    }

    #[test]
    fn externe_klok_interpoleert() {
        // Pulsen elke 1/48 s = 120 BPM.
        let p: Vec<f64> = (0..200).map(|i| f64::from(i) / 48.0).collect();
        assert_eq!(naar_ticks(0.5, Some(&p)), u64::from(PPQ));
        // Halverwege twee pulsen: tien ticks verder dan de puls ervoor.
        assert_eq!(naar_ticks(1.5 / 48.0, Some(&p)), 30);
    }

    #[test]
    fn tempo_alleen_bij_verandering() {
        // Eerst 120 BPM, na vier tellen 100 BPM.
        let mut p = Vec::new();
        let mut t = 0.0;
        for i in 0..(24 * 8 + 1) {
            p.push(t);
            t += if i < 24 * 4 { 0.5 / 24.0 } else { 0.6 / 24.0 };
        }
        let k = tempokaart(&p);
        assert_eq!(k.len(), 2);
        assert_eq!(k[0], (0, 500_000));
        assert_eq!(k[1], (4 * u64::from(PPQ), 600_000));
    }

    #[test]
    fn midi_bestand_schrijven() {
        let map = std::env::temp_dir().join(format!("pecanmixer-test-{}", std::process::id()));
        std::fs::create_dir_all(&map).unwrap();
        let pad = map.join("t.mid");
        let m = MidiBestand {
            namen: vec!["FP-30".into()],
            events: vec![
                (0, 0.0, vec![0x90, 64, 76]),
                (0, 0.5, vec![0x80, 64, 0]),
                (0, 0.6, vec![0xf8]),
            ],
            pulsen: None,
        };
        schrijf_midi(&pad, &m).unwrap();
        let bytes = std::fs::read(&pad).unwrap();
        let smf = midly::Smf::parse(&bytes).unwrap();
        assert_eq!(smf.tracks.len(), 2);
        // Naam, noot aan, noot uit (klok eruit), einde.
        assert_eq!(smf.tracks[1].len(), 4);
        assert_eq!(u32::from(smf.tracks[1][2].delta), PPQ);
        std::fs::remove_dir_all(map).unwrap();
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod writer_tests;
