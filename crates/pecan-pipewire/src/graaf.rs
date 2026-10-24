//! De PipeWire-graph zoals hij nu is: nodes, poorten, links, en wat de
//! session manager als standaard aanwijst.
//!
//! Een eigen draad met een eigen verbinding met de server: een PipeWire-loop
//! is niet `Send`, en de UI mag nooit op graph-verkeer wachten. Wat de draad
//! ziet gaat als één [`Momentopname`] naar buiten, hooguit 25 keer per
//! seconde: bij het opstarten komen er honderden globals tegelijk binnen, en
//! de schil hoeft die niet een voor een te tekenen.
//!
//! Hier zit geen beleid in. Dit is een spiegel; wat er verborgen wordt, welke
//! kolom iets krijgt en wat "een app" is, beslist de schil.
//!
//! Object-ids zijn per sessie: een apparaat dat terugkomt krijgt nieuwe.
//! Herkennen gaat daarom via `node.name`, `port.name` en de serials, die hier
//! allemaal meekomen.
//!
//! Veranderen gaat via [`Opdracht`]: een link leggen of weghalen, een stream
//! verplaatsen met `target.object`, volume en mute. Ook dat is uitvoeren, geen
//! beslissen.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use pecan_core::backend::{Backend, BackendCommand, DispatchError, Stamped};
pub use pecan_core::graph::{
    BusOpname, Kabel, Knoop, Momentopname, Opdracht, Opties, Pieken, Poort, Soort, TransportStand,
};
use pipewire as pw;
use pw::node::NodeState;
use pw::spa::param::ParamType;
use pw::spa::pod::{Object, Pod, Property, Value, ValueArray};
use pw::spa::utils::dict::DictRef;
use pw::types::ObjectType;

use crate::commands::Commands;
use crate::meter::{self, Meter};
use crate::midimeter::{Activiteit, MidiMeter};
use crate::opname::{self, Actief, Bericht, Bus, Klok, MidiBestand, Opnemer, Spoor};

/// Wat PipeWire in `format.dsp` zet voor MIDI: oude bytes of UMP.
const DSP_MIDI: &str = "8 bit raw midi";
const DSP_UMP: &str = "32 bit raw UMP";
/// En voor één kanaal audio, de enige vorm die een DSP-poort heeft.
const DSP_AUDIO: &str = "32 bit float mono audio";

/// Hoe vaak de draad kijkt of er iets te melden is.
const MELD_INTERVAL: Duration = Duration::from_millis(40);
/// Hoe lang hij wacht voor hij opnieuw verbindt als de server weg is.
const OPNIEUW_NA: Duration = Duration::from_secs(2);

fn soort_van_dsp(dsp: Option<&str>) -> Soort {
    match dsp {
        Some(DSP_AUDIO) => Soort::Audio,
        Some(DSP_MIDI | DSP_UMP) => Soort::Midi,
        _ => Soort::Anders,
    }
}

/// Wat de draad meldt: een nieuwe stand van de graph, of nieuwe pieken.
/// Apart, want pieken komen 25 keer per seconde en de graph zelden.
pub enum Melding<'a> {
    Graaf(&'a Momentopname),
    Pieken(&'a Pieken),
    /// MIDI-activiteit per bronpoort (poort-id), sinds de vorige melding.
    Midi(&'a BTreeMap<u32, Activiteit>),
    /// Hoe het met de opname en de klok staat; vijf keer per seconde.
    Transport(&'a TransportStand),
}

/// MIDI staat voorlopig uit: nog niet getest met echte apparaten. Dan geen
/// MIDI-meter (die legt links naar elke MIDI-uitgang), geen MIDI-ingang op de
/// Rec bus en geen externe klok. De schil heeft een eigen schakelaar
/// (`MIDI_AAN` in api.ts).
pub const MIDI: bool = false;

/// Wordt aangeroepen vanaf de graph-draad.
pub type Melder = Box<dyn Fn(Melding) + Send + 'static>;

/// De levende spiegel. Droppen vraagt de graph-draad om te stoppen.
///
/// `Backend::dispatch` accepts stamped work into a bounded FIFO (255 ordinary
/// entries plus one reserved stop-all slot, at most 32 dispatched per 40 ms
/// tick). Acceptance is not server completion;
/// worker-side rejection is logged. The legacy `doe` API stamps at admission
/// and retains its unit return and unchanged IPC payloads. Legacy callers
/// cannot supply an intent epoch: already obsolete frontend IDs that happen
/// to be reused before admission require a future deliberate IPC migration.
/// Queued commands retain the published graph's object generations at admission.
/// An intent from a graph already superseded before admission needs richer
/// caller identity than the unchanged epoch-only command payload.
/// The native read-only `Observer` exposes neither command entry point.
/// Cancellation clears queued work; one already dequeued command may still
/// execute or finish. It does not interrupt or roll back that command.
pub struct Graaf {
    opdrachten: Arc<Commands>,
    stop: Arc<StopSignal>,
}

impl Graaf {
    pub fn start(melder: Melder) -> Self {
        let opdrachten = Arc::new(Commands::default());
        let gedeeld = Arc::clone(&opdrachten);
        let stop = Arc::new(StopSignal::default());
        let worker_stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("pecanmixer-graaf".to_owned())
            .spawn(move || draad(gedeeld, melder, false, worker_stop))
            .expect("graph-draad starten");
        Self { opdrachten, stop }
    }

    pub fn nu(&self) -> Momentopname {
        (*self.current_graph().value).clone()
    }

    /// Geeft de opdracht door aan de graph-draad en wacht niet.
    pub fn doe(&self, o: Opdracht) {
        if let Err(error) = self.opdrachten.legacy(o) {
            eprintln!("pecanmixer: opdracht geweigerd: {error:?}");
        }
    }
}

impl Backend for Graaf {
    fn current_graph(&self) -> Stamped<Arc<Momentopname>> {
        self.opdrachten.current()
    }

    fn dispatch(&self, command: BackendCommand) -> Result<(), DispatchError> {
        self.opdrachten.enqueue(command)
    }
}

impl Drop for Graaf {
    fn drop(&mut self) {
        self.opdrachten.stop();
        self.stop.request();
    }
}

/// A strictly observational worker: no Rec bus, meters, links or command API.
/// Dropping it requests cancellation without blocking the GTK main thread.
pub struct Observer {
    stop: Arc<StopSignal>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Observer {
    /// Wait for cancellation off the UI thread, e.g. in a headless harness.
    pub fn stop_and_join(mut self) -> std::thread::Result<()> {
        self.stop.request();
        self.thread.take().map_or(Ok(()), |thread| thread.join())
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.request();
    }
}

#[derive(Default)]
struct StopSignal {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl StopSignal {
    fn request(&self) {
        *self.stopped.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.wake.notify_all();
    }

    fn requested(&self) -> bool {
        *self.stopped.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn retry_wait(&self) {
        let guard = self.stopped.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .wake
            .wait_timeout_while(guard, OPNIEUW_NA, |stop| !*stop);
    }
}

/// Publish owned, stamped events into a bounded inbox. The legacy shell keeps
/// its borrowed serialization callback while the native UI uses this seam.
pub fn observe(inbox: Arc<pecan_core::inbox::LatestInbox>) -> std::io::Result<Observer> {
    use pecan_core::backend::EventSequencer;
    let sequence = RefCell::new(EventSequencer::default());
    let melder: Melder = Box::new(move |event| {
        let mut sequence = sequence.borrow_mut();
        let owned = match event {
            Melding::Graaf(value) => sequence.graph(value.clone()),
            Melding::Pieken(value) => sequence.peaks(value.clone()),
            Melding::Midi(value) => sequence.midi(value.clone()),
            Melding::Transport(value) => sequence.transport(value.clone()),
        };
        inbox.publish(owned);
    });
    let stop = Arc::new(StopSignal::default());
    let worker_stop = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("pecanmixer-observer".into())
        .spawn(move || draad(Arc::default(), melder, true, worker_stop))?;
    Ok(Observer {
        stop,
        thread: Some(thread),
    })
}

/// Blijft verbinden: als de server herstart of even weg is, komt de spiegel
/// vanzelf terug.
fn draad(opdrachten: Arc<Commands>, melder: Melder, read_only: bool, stop: Arc<StopSignal>) {
    pw::init();
    let melder = Rc::new(melder);
    let meld: Rc<dyn Fn(Momentopname)> = Rc::new({
        let melder = Rc::clone(&melder);
        let opdrachten = Arc::clone(&opdrachten);
        move |m: Momentopname| {
            let graph = opdrachten.publish(m);
            melder(Melding::Graaf(&graph.value));
        }
    });
    let overig: Rc<dyn Fn(Melding)> = Rc::new(move |m: Melding| melder(m));
    while !stop.requested() {
        if let Err(fout) = sessie(
            Rc::clone(&meld),
            Rc::clone(&overig),
            Arc::clone(&opdrachten),
            read_only,
            Arc::clone(&stop),
        ) {
            eprintln!("pecanmixer: PipeWire: {fout}");
        }
        meld(Momentopname::default());
        stop.retry_wait();
    }
    opdrachten.stop();
}

/// Wat de draad bijhoudt tussen de callbacks door.
#[derive(Default)]
struct Staat {
    knopen: BTreeMap<u32, Knoop>,
    poorten: BTreeMap<u32, Poort>,
    kabels: BTreeMap<u32, Kabel>,
    /// Client-id → `application.name`.
    clients: BTreeMap<u32, String>,
    /// `target.object` per node-id, uit de `default`-metadata. Apart van de
    /// knopen, want de metadata kan eerder binnenkomen dan de node.
    doelen: BTreeMap<u32, String>,
    /// Uit de `settings`-metadata.
    rate: Option<u32>,
    force_rate: Option<u32>,
    quantum: Option<u32>,
    force_quantum: Option<u32>,
    /// Uit de `default`-metadata.
    standaard_sink: Option<String>,
    standaard_bron: Option<String>,
    /// Onze eigen meter-nodes: die horen niet in de graph van de gebruiker.
    eigen: BTreeSet<u32>,
    /// De poorten van onze eigen nodes: (node, poortnaam) → id, om te linken.
    eigen_poorten: BTreeMap<(u32, String), u32>,
    /// Er is iets veranderd sinds de laatste melding.
    vuil: bool,
}

impl Staat {
    /// Check immediately before execution. The published snapshot can lag a
    /// registry removal by one graph tick; never destroy an unknown global.
    fn geldige_objecten(&self, o: &Opdracht) -> bool {
        match o {
            Opdracht::Verbind { uit, naar } => {
                self.poorten
                    .get(uit)
                    .is_some_and(|p| !p.ingang && self.knopen.contains_key(&p.knoop))
                    && self
                        .poorten
                        .get(naar)
                        .is_some_and(|p| p.ingang && self.knopen.contains_key(&p.knoop))
            }
            Opdracht::Ontkoppel { kabel } => self.kabels.contains_key(kabel),
            Opdracht::ZetDoel { knoop, .. }
            | Opdracht::ZetVolume { knoop, .. }
            | Opdracht::ZetMute { knoop, .. } => self.knopen.contains_key(knoop),
            Opdracht::ZetTransport(opties) => opties.klok.is_none_or(|id| {
                self.poorten.get(&id).is_some_and(|p| {
                    !p.ingang && p.soort == Soort::Midi && self.knopen.contains_key(&p.knoop)
                })
            }),
            Opdracht::ZetBus { .. }
            | Opdracht::Start
            | Opdracht::StartBus(_)
            | Opdracht::Stop
            | Opdracht::StopBus(_) => true,
        }
    }

    fn momentopname(&self) -> Momentopname {
        Momentopname {
            verbonden: true,
            rate: self.force_rate.or(self.rate),
            quantum: self.force_quantum.or(self.quantum),
            standaard_sink: self.standaard_sink.clone(),
            standaard_bron: self.standaard_bron.clone(),
            wireplumber: self
                .clients
                .iter()
                .filter(|(_, naam)| naam.as_str() == "WirePlumber")
                .map(|(id, _)| *id)
                .collect(),
            knopen: self
                .knopen
                .values()
                .map(|k| Knoop {
                    doel: self.doelen.get(&k.id).cloned(),
                    ..k.clone()
                })
                .collect(),
            poorten: self.poorten.values().cloned().collect(),
            kabels: self.kabels.values().cloned().collect(),
        }
    }

    /// Een global uit het register. Nodes worden daarnaast gebonden (zie
    /// [`sessie`]), omdat `application.name` en `media.name` alleen in de
    /// info staan, niet in de props van het register.
    fn global(&mut self, id: u32, soort: &ObjectType, props: &DictRef) {
        match soort {
            ObjectType::Node => {
                if props
                    .get("node.name")
                    .is_some_and(|n| n.starts_with(meter::VOORVOEGSEL))
                {
                    self.eigen.insert(id);
                    return;
                }
                self.knopen.insert(
                    id,
                    Knoop {
                        id,
                        serial: getal(props, "object.serial"),
                        naam: tekst(props, "node.name").unwrap_or_default(),
                        beschrijving: tekst(props, "node.description"),
                        bijnaam: tekst(props, "node.nick"),
                        klasse: tekst(props, "media.class"),
                        apparaat: getal(props, "device.id"),
                        app: None,
                        media: None,
                        stand: None,
                        doel: None,
                        volume: None,
                        mute: None,
                    },
                );
            }
            ObjectType::Port => {
                let Some(knoop) = getal(props, "node.id") else {
                    return;
                };
                if self.eigen.contains(&knoop) {
                    if let Some(n) = tekst(props, "port.name") {
                        self.eigen_poorten.insert((knoop, n), id);
                    }
                    return;
                }
                self.poorten.insert(
                    id,
                    Poort {
                        id,
                        knoop,
                        volgnummer: getal(props, "port.id"),
                        naam: tekst(props, "port.name").unwrap_or_default(),
                        alias: tekst(props, "port.alias"),
                        ingang: props.get("port.direction") == Some("in"),
                        soort: soort_van_dsp(props.get("format.dsp")),
                        kanaal: tekst(props, "audio.channel"),
                        monitor: props.get("port.monitor") == Some("true"),
                        fysiek: props.get("port.physical") == Some("true"),
                        groep: tekst(props, "port.group"),
                    },
                );
            }
            ObjectType::Link => {
                let (Some(uit_knoop), Some(uit_poort), Some(in_knoop), Some(in_poort)) = (
                    getal(props, "link.output.node"),
                    getal(props, "link.output.port"),
                    getal(props, "link.input.node"),
                    getal(props, "link.input.port"),
                ) else {
                    return;
                };
                if self.eigen.contains(&uit_knoop) || self.eigen.contains(&in_knoop) {
                    return;
                }
                self.kabels.insert(
                    id,
                    Kabel {
                        id,
                        uit_knoop,
                        uit_poort,
                        in_knoop,
                        in_poort,
                        client: getal(props, "client.id"),
                    },
                );
            }
            ObjectType::Client => {
                self.clients
                    .insert(id, tekst(props, "application.name").unwrap_or_default());
            }
            _ => return,
        }
        self.vuil = true;
    }

    fn weg(&mut self, id: u32) {
        let was = self.knopen.remove(&id).is_some()
            | self.poorten.remove(&id).is_some()
            | self.kabels.remove(&id).is_some()
            | self.clients.remove(&id).is_some()
            | self.doelen.remove(&id).is_some();
        self.eigen.remove(&id);
        self.eigen_poorten.retain(|(k, _), p| *p != id && *k != id);
        self.vuil |= was;
    }

    fn node_info(&mut self, id: u32, info: &pw::node::NodeInfoRef) {
        let Some(k) = self.knopen.get_mut(&id) else {
            return;
        };
        k.stand = Some(
            match info.state() {
                NodeState::Running => "running",
                NodeState::Idle => "idle",
                NodeState::Suspended => "suspended",
                NodeState::Creating => "creating",
                NodeState::Error(_) => "error",
            }
            .to_owned(),
        );
        if let Some(props) = info.props() {
            k.app = tekst(props, "application.name");
            k.media = tekst(props, "media.name");
            // De info kan rijker zijn dan het register.
            if let Some(b) = tekst(props, "node.description") {
                k.beschrijving = Some(b);
            }
            if let Some(b) = tekst(props, "node.nick") {
                k.bijnaam = Some(b);
            }
        }
        self.vuil = true;
    }

    /// De Props van een node: volume en mute. Streams sturen er soms een
    /// zonder volume tussendoor; alleen wat er staat wordt overgenomen.
    fn props(&mut self, id: u32, pod: &Pod) {
        let Some(k) = self.knopen.get_mut(&id) else {
            return;
        };
        let Ok((_, Value::Object(obj))) =
            pw::spa::pod::deserialize::PodDeserializer::deserialize_any_from(pod.as_bytes())
        else {
            return;
        };
        for p in obj.properties {
            match (p.key, p.value) {
                (
                    pw::spa::sys::SPA_PROP_channelVolumes,
                    Value::ValueArray(ValueArray::Float(v)),
                ) => {
                    k.volume = Some(v);
                    self.vuil = true;
                }
                (pw::spa::sys::SPA_PROP_mute, Value::Bool(m)) => {
                    k.mute = Some(m);
                    self.vuil = true;
                }
                _ => {}
            }
        }
    }

    /// `target.object` van één node, of het wissen ervan.
    fn doel(&mut self, knoop: u32, waarde: Option<&str>) {
        match waarde.map(|w| w.trim_matches('"')) {
            Some(w) if !w.is_empty() => self.doelen.insert(knoop, w.to_owned()),
            _ => self.doelen.remove(&knoop),
        };
        self.vuil = true;
    }

    /// Welke nodes een meter horen te hebben: id → (serial, is sink, kanalen).
    fn meterbaar(&self) -> BTreeMap<u32, (u64, bool, Vec<String>)> {
        let mut uit = BTreeMap::new();
        for k in self.knopen.values() {
            let klasse = k.klasse.as_deref().unwrap_or_default();
            let sink = klasse.starts_with("Audio/Sink");
            let ok = sink
                || klasse == "Stream/Output/Audio"
                || klasse.starts_with("Audio/Source")
                || klasse == "Audio/Duplex";
            let Some(serial) = k.serial.filter(|_| ok) else {
                continue;
            };
            let mut ps: Vec<&Poort> = self
                .poorten
                .values()
                .filter(|p| {
                    p.knoop == k.id && p.soort == Soort::Audio && !p.monitor && p.ingang == sink
                })
                .collect();
            ps.sort_by_key(|p| (p.volgnummer.unwrap_or(p.id), p.id));
            let kanalen: Option<Vec<String>> = ps.iter().map(|p| p.kanaal.clone()).collect();
            if let Some(kanalen) = kanalen.filter(|k| !k.is_empty()) {
                uit.insert(k.id, (serial, sink, kanalen));
            }
        }
        uit
    }

    fn metadata(&mut self, metadata: &str, sleutel: &str, waarde: Option<&str>) {
        let getal = || {
            waarde
                .and_then(|w| w.trim().parse::<u32>().ok())
                .filter(|&n| n > 0)
        };
        match (metadata, sleutel) {
            ("settings", "clock.rate") => self.rate = getal(),
            ("settings", "clock.force-rate") => self.force_rate = getal(),
            ("settings", "clock.quantum") => self.quantum = getal(),
            ("settings", "clock.force-quantum") => self.force_quantum = getal(),
            ("default", "default.audio.sink") => {
                self.standaard_sink = waarde.and_then(naam_uit_json)
            }
            ("default", "default.audio.source") => {
                self.standaard_bron = waarde.and_then(naam_uit_json)
            }
            _ => return,
        }
        self.vuil = true;
    }
}

fn tekst(props: &DictRef, sleutel: &str) -> Option<String> {
    props
        .get(sleutel)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn getal<T: std::str::FromStr>(props: &DictRef, sleutel: &str) -> Option<T> {
    props.get(sleutel).and_then(|s| s.parse().ok())
}

/// `{"name":"alsa_output.…"}` → `alsa_output.…`.
fn naam_uit_json(waarde: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(waarde)
        .ok()?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

/// Een Props-pod met één eigenschap, om naar een node te sturen.
fn props_pod(eigenschap: Property) -> Option<Vec<u8>> {
    let obj = Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties: vec![eigenschap],
    };
    pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(obj),
    )
    .ok()
    .map(|(c, _)| c.into_inner())
}

/// Gebonden proxies met hun listener. Ze worden nooit gelezen: ze bewaren is
/// het luisteren, weggooien is ophouden.
#[allow(dead_code)]
enum Binding {
    Knoop(pw::node::Node, pw::node::NodeListener),
    /// Met de naam: `default` of `settings`.
    Metadata(
        pw::metadata::Metadata,
        pw::metadata::MetadataListener,
        String,
    ),
}

/// Eén verbinding met de server, tot die wegvalt. De begrensde wachtrij en
/// epoch blijven op de worker bestaan tussen sessies.
fn sessie(
    meld: Rc<dyn Fn(Momentopname)>,
    meld_overig: Rc<dyn Fn(Melding)>,
    opdrachten: Arc<Commands>,
    read_only: bool,
    stop: Arc<StopSignal>,
) -> Result<(), pw::Error> {
    let opzet = || -> Result<_, pw::Error> {
        let mainloop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&mainloop, None)?;
        let core = context.connect_rc(None)?;
        let registry = core.get_registry_rc()?;
        Ok((mainloop, context, core, registry))
    };
    let (mainloop, _context, core, registry) = opzet()?;

    let staat = Rc::new(RefCell::new(Staat::default()));
    let bindingen: Rc<RefCell<BTreeMap<u32, Binding>>> = Rc::default();
    let verbonden = Rc::new(std::cell::Cell::new(true));

    // Fout op het core-object (id 0) betekent dat de server weg is: de loop
    // stopt en `draad` verbindt opnieuw.
    let _core_listener = core
        .add_listener_local()
        .error({
            let mainloop = mainloop.downgrade();
            let verbonden = Rc::clone(&verbonden);
            let meld = Rc::clone(&meld);
            move |id, _seq, _res, bericht| {
                if id == 0 {
                    verbonden.set(false);
                    meld(Momentopname::default());
                    eprintln!("pecanmixer: PipeWire-core: {bericht}");
                    if let Some(l) = mainloop.upgrade() {
                        l.quit();
                    }
                }
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let staat = Rc::clone(&staat);
            let bindingen = Rc::clone(&bindingen);
            let registry = registry.downgrade();
            let opdrachten = Arc::clone(&opdrachten);
            move |g| {
                opdrachten.object_added(g.id);
                let Some(props) = g.props else {
                    return;
                };
                staat.borrow_mut().global(g.id, &g.type_, props);
                if staat.borrow().eigen.contains(&g.id) {
                    return;
                }
                let Some(registry) = registry.upgrade() else {
                    return;
                };
                let id = g.id;
                match g.type_ {
                    ObjectType::Node => {
                        let Ok(node) = registry.bind::<pw::node::Node, _>(g) else {
                            return;
                        };
                        let listener = node
                            .add_listener_local()
                            .info({
                                let staat = Rc::clone(&staat);
                                move |info| staat.borrow_mut().node_info(id, info)
                            })
                            .param({
                                let staat = Rc::clone(&staat);
                                move |_seq, soort, _index, _next, pod| {
                                    if let (ParamType::Props, Some(pod)) = (soort, pod) {
                                        staat.borrow_mut().props(id, pod);
                                    }
                                }
                            })
                            .register();
                        node.subscribe_params(&[ParamType::Props]);
                        bindingen
                            .borrow_mut()
                            .insert(id, Binding::Knoop(node, listener));
                    }
                    ObjectType::Metadata => {
                        let Some(naam) = props.get("metadata.name").map(str::to_owned) else {
                            return;
                        };
                        if naam != "default" && naam != "settings" {
                            return;
                        }
                        let Ok(meta) = registry.bind::<pw::metadata::Metadata, _>(g) else {
                            return;
                        };
                        let staat = Rc::clone(&staat);
                        let naam_ook = naam.clone();
                        let listener = meta
                            .add_listener_local()
                            .property(move |subject, sleutel, _type, waarde| {
                                let mut s = staat.borrow_mut();
                                match sleutel {
                                    // Alles wissen.
                                    None => {
                                        if naam == "default" {
                                            s.doelen.clear();
                                            s.vuil = true;
                                        }
                                    }
                                    Some("target.object") if naam == "default" && subject != 0 => {
                                        s.doel(subject, waarde)
                                    }
                                    Some(sleutel) if subject == 0 => {
                                        s.metadata(&naam, sleutel, waarde)
                                    }
                                    _ => {}
                                }
                                0
                            })
                            .register();
                        bindingen
                            .borrow_mut()
                            .insert(id, Binding::Metadata(meta, listener, naam_ook));
                    }
                    _ => {}
                }
            }
        })
        .global_remove({
            let staat = Rc::clone(&staat);
            let bindingen = Rc::clone(&bindingen);
            let opdrachten = Arc::clone(&opdrachten);
            move |id| {
                opdrachten.object_removed(id);
                staat.borrow_mut().weg(id);
                bindingen.borrow_mut().remove(&id);
            }
        })
        .register();

    // Links die wij legden. Ze blijven bestaan zonder proxy (linger), maar de
    // proxy moet leven tot de server hem heeft gemaakt; een handvol bewaren
    // is ruim genoeg.
    let gelegd: Rc<RefCell<std::collections::VecDeque<pw::link::Link>>> = Rc::default();

    // De opname: wat er loopt, de klok, en de Rec bus zelf.
    let opnemer: Opnemer = Rc::default();
    let klok: Rc<RefCell<Klok>> = Rc::default();
    let transport: Rc<RefCell<Transport>> = Rc::default();
    let bus = Rc::new(
        (!read_only)
            .then(|| Bus::maak(&core, Rc::clone(&opnemer), 2))
            .flatten(),
    );

    // Eén meter per node die geluid maakt of ontvangt.
    let meters: Rc<RefCell<BTreeMap<u32, Meter>>> = Rc::default();
    // En één node die alle MIDI-uitgangen afluistert.
    let midi = Rc::new(RefCell::new(
        (MIDI && !read_only)
            .then(|| MidiMeter::maak(&core, Rc::clone(&opnemer), Rc::clone(&klok)))
            .flatten(),
    ));
    let tikken = std::cell::Cell::new(0u32);

    let timer = mainloop.loop_().add_timer({
        let mainloop = mainloop.downgrade();
        let staat = Rc::clone(&staat);
        let meters = Rc::clone(&meters);
        let midi = Rc::clone(&midi);
        let core = core.clone();
        let opnemer = Rc::clone(&opnemer);
        let klok = Rc::clone(&klok);
        let transport = Rc::clone(&transport);
        let bus = Rc::clone(&bus);
        move |_| {
            if stop.requested() || !verbonden.get() {
                if let Some(mainloop) = mainloop.upgrade() {
                    mainloop.quit();
                }
                return;
            }
            // Start of Stop van de externe klok.
            let verzoek = klok.borrow_mut().verzoek.take();
            match verzoek {
                Some(true) if opnemer.borrow().is_empty() => begin(
                    &staat.borrow(),
                    &mut transport.borrow_mut(),
                    &opnemer,
                    bus.as_ref().as_ref().and_then(Bus::knoop_id),
                    None,
                ),
                Some(false) if !opnemer.borrow().is_empty() => eindig(&opnemer, &klok.borrow()),
                _ => {}
            }
            tikken.set(tikken.get().wrapping_add(1));
            if tikken.get().is_multiple_of(5) {
                let mut t = transport.borrow_mut();
                t.ruim_op();
                let takes = opnemer.borrow();
                let stand = TransportStand {
                    bezig: !takes.is_empty(),
                    map: t.map.as_ref().map(|m| m.display().to_string()),
                    seconden: takes
                        .iter()
                        .map(|a| a.t0.elapsed().as_secs_f64())
                        .fold(0.0, f64::max),
                    bussen: takes
                        .iter()
                        .flat_map(|a| {
                            a.bussen
                                .iter()
                                .map(|b| (b.2.clone(), a.t0.elapsed().as_secs_f64()))
                        })
                        .collect(),
                    klok: klok.borrow().stand(),
                    fout: t.fout.clone(),
                };
                drop(takes);
                meld_overig(Melding::Transport(&stand));
            }
            if let Some(mm) = midi.borrow_mut().as_mut() {
                let s = staat.borrow();
                let bronnen: Vec<u32> = s
                    .poorten
                    .values()
                    .filter(|p| p.soort == Soort::Midi && !p.ingang)
                    .map(|p| p.id)
                    .collect();
                mm.stem_af(&core, &bronnen, &s.eigen_poorten);
                let activiteit = mm.oogst();
                if !activiteit.is_empty() {
                    meld_overig(Melding::Midi(&activiteit));
                }
            }
            let mut s = staat.borrow_mut();
            if s.vuil {
                s.vuil = false;
                meld(s.momentopname());
                // Meters bijwerken: weg wat weg is, nieuw wat nieuw is.
                let moet = if read_only {
                    BTreeMap::new()
                } else {
                    s.meterbaar()
                };
                drop(s);
                let mut m = meters.borrow_mut();
                m.retain(|id, meter| moet.get(id).is_some_and(|(_, _, k)| *k == meter.kanalen));
                for (id, (serial, sink, kanalen)) in moet {
                    if let std::collections::btree_map::Entry::Vacant(plek) = m.entry(id) {
                        if let Some(meter) = meter::maak(&core, id, serial, sink, kanalen) {
                            plek.insert(meter);
                        }
                    }
                }
            } else {
                drop(s);
            }
            if !read_only {
                for error in opdrachten.drain(
                    |o| staat.borrow().geldige_objecten(o),
                    |o| match o {
                        Opdracht::ZetBus { kanalen, gain } => {
                            if let Some(b) = bus.as_ref() {
                                b.zet_kanalen(kanalen);
                                b.zet_gain(&gain);
                            }
                        }
                        Opdracht::ZetTransport(opties) => {
                            let mut k = klok.borrow_mut();
                            if k.bron != opties.klok {
                                k.recent.clear();
                            }
                            k.bron = opties.klok.filter(|_| MIDI);
                            k.start_stop = opties.start_stop;
                            transport.borrow_mut().opties = Some(opties);
                        }
                        Opdracht::Start => begin(
                            &staat.borrow(),
                            &mut transport.borrow_mut(),
                            &opnemer,
                            bus.as_ref().as_ref().and_then(Bus::knoop_id),
                            None,
                        ),
                        Opdracht::StartBus(b) => begin(
                            &staat.borrow(),
                            &mut transport.borrow_mut(),
                            &opnemer,
                            bus.as_ref().as_ref().and_then(Bus::knoop_id),
                            Some(b),
                        ),
                        Opdracht::Stop => eindig(&opnemer, &klok.borrow()),
                        Opdracht::StopBus(naam) => eindig_bus(&opnemer, &klok.borrow(), &naam),
                        o => uitvoeren(o, &core, &registry, &bindingen.borrow(), &gelegd),
                    },
                ) {
                    eprintln!("pecanmixer: opdracht geweigerd op worker: {error:?}");
                }
            }
            let m = meters.borrow();
            let mut pieken: Pieken = m.iter().map(|(id, meter)| (*id, meter.oogst())).collect();
            // De Rec bus meet zichzelf, onder zijn eigen node-id.
            if let Some((b, id)) = bus
                .as_ref()
                .as_ref()
                .and_then(|b| b.knoop_id().map(|id| (b, id)))
            {
                pieken.insert(id, b.oogst());
            }
            if !pieken.is_empty() {
                meld_overig(Melding::Pieken(&pieken));
            }
        }
    });
    timer.update_timer(Some(MELD_INTERVAL), Some(MELD_INTERVAL));
    // Ook een lege graph is een verbonden graph.
    staat.borrow_mut().vuil = true;

    mainloop.run();
    drop(timer);
    // Wat nog opnam, netjes afsluiten.
    eindig(&opnemer, &klok.borrow());
    meters.borrow_mut().clear();
    midi.borrow_mut().take();
    Ok(())
}

/// Wat de graph-draad over de opname bijhoudt.
#[derive(Default)]
struct Transport {
    opties: Option<Opties>,
    map: Option<std::path::PathBuf>,
    fout: Option<String>,
    /// De schrijfdraden, tot ze klaar zijn.
    schrijvers: Vec<std::thread::JoinHandle<Result<(), String>>>,
}

impl Transport {
    /// Klare schrijvertjes oogsten, zodat een fout op het scherm komt.
    fn ruim_op(&mut self) {
        let (klaar, bezig): (Vec<_>, Vec<_>) = std::mem::take(&mut self.schrijvers)
            .into_iter()
            .partition(|d| d.is_finished());
        self.schrijvers = bezig;
        for d in klaar {
            if let Err(e) = d.join().unwrap_or(Err("writer panicked".into())) {
                self.fout = Some(e);
            }
        }
    }
}

/// Start een take in een nieuwe map. `None` = Record: de bussen en MIDI
/// uit de opties, samen. Anders alleen die ene bus. Een bus die al opneemt,
/// doet niet nog eens mee.
fn begin(
    s: &Staat,
    t: &mut Transport,
    opnemer: &Opnemer,
    bus: Option<u32>,
    een: Option<BusOpname>,
) {
    t.fout = None;
    let alles = een.is_none();
    let opties = t.opties.clone().unwrap_or(Opties {
        bussen: Vec::new(),
        midi: true,
        klok: None,
        start_stop: false,
    });
    let lopend: BTreeSet<String> = opnemer
        .borrow()
        .iter()
        .flat_map(|a| a.bussen.iter().map(|b| b.2.clone()))
        .collect();
    let bussen: Vec<BusOpname> = een
        .map_or(opties.bussen, |b| vec![b])
        .into_iter()
        .filter(|b| !b.kanalen.is_empty() && !lopend.contains(&b.naam))
        .collect();
    let midi =
        MIDI && alles && opties.midi && !opnemer.borrow().iter().any(|a| !a.midi_namen.is_empty());
    match plan(s, &bussen, midi, bus) {
        Err(e) => {
            // Alles loopt al: dan is er niets mis.
            if lopend.is_empty() {
                t.fout = Some(e);
            }
        }
        Ok((sporen, bussen, midi, midi_namen)) => {
            let map = opname::nieuwe_map();
            let rate = s.force_rate.or(s.rate).unwrap_or(48_000);
            match opname::schrijver(&map, sporen, rate) {
                Err(e) => t.fout = Some(format!("{}: {e}", map.display())),
                Ok((tx, draad)) => {
                    opnemer.borrow_mut().push(Actief {
                        tx,
                        t0: std::time::Instant::now(),
                        bussen,
                        midi,
                        midi_namen,
                        midi_events: Vec::new(),
                        pulsen: Vec::new(),
                    });
                    t.map = Some(map);
                    t.schrijvers.push(draad);
                }
            }
        }
    }
}

type Plan = (
    Vec<Spoor>,
    Vec<(usize, Vec<usize>, String)>,
    BTreeMap<u32, usize>,
    Vec<String>,
);

/// Welke sporen er komen: per bus een WAV, en als `midi` per MIDI-bron die
/// naar de bus gaat een spoor in het MIDI-bestand.
fn plan(s: &Staat, bussen: &[BusOpname], midi: bool, bus: Option<u32>) -> Result<Plan, String> {
    let bus = s
        .knopen
        .get(&bus.ok_or("The Rec bus is not there")?)
        .ok_or("The Rec bus is not there")?;
    let midi_in: BTreeSet<u32> = s
        .poorten
        .values()
        .filter(|p| p.knoop == bus.id && p.soort == Soort::Midi)
        .map(|p| p.id)
        .collect();

    let mut sporen = Vec::new();
    let mut lijst = Vec::new();
    for b in bussen {
        lijst.push((sporen.len(), b.kanalen.clone(), b.naam.clone()));
        sporen.push(Spoor {
            naam: b.naam.clone(),
            kanalen: b.kanalen.len() as u16,
        });
    }

    // MIDI: per bronpoort die naar de bus gaat.
    let mut sporen_midi = BTreeMap::new();
    let mut midi_namen = Vec::new();
    if midi {
        for k in s.kabels.values().filter(|k| midi_in.contains(&k.in_poort)) {
            if sporen_midi.contains_key(&k.uit_poort) {
                continue;
            }
            let p = s.poorten.get(&k.uit_poort);
            let knoop = s.knopen.get(&k.uit_knoop);
            let naam = match (p, knoop) {
                (Some(p), Some(n)) if n.naam == "Midi-Bridge" => p
                    .alias
                    .as_deref()
                    .and_then(|a| a.split(':').next())
                    .unwrap_or(&p.naam)
                    .to_owned(),
                (_, Some(n)) => n
                    .app
                    .clone()
                    .or(n.beschrijving.clone())
                    .unwrap_or(n.naam.clone()),
                _ => format!("MIDI {}", k.uit_poort),
            };
            sporen_midi.insert(k.uit_poort, midi_namen.len());
            midi_namen.push(naam);
        }
    }

    if sporen.is_empty() && sporen_midi.is_empty() {
        return Err("Nothing armed: press ● on a Rec bus".into());
    }
    Ok((sporen, lijst, sporen_midi, midi_namen))
}

/// Stopt alle takes: de schrijfdraden maken de bestanden af.
fn eindig(opnemer: &Opnemer, klok: &Klok) {
    let takes = std::mem::take(&mut *opnemer.borrow_mut());
    for a in takes {
        sluit(a, klok);
    }
}

/// Stopt één bus. Was hij de laatste van zijn take, dan stopt de take, en
/// het MIDI-bestand van die take met hem.
fn eindig_bus(opnemer: &Opnemer, klok: &Klok, naam: &str) {
    let mut takes = opnemer.borrow_mut();
    let Some(i) = takes
        .iter()
        .position(|a| a.bussen.iter().any(|b| b.2 == naam))
    else {
        return;
    };
    let a = &mut takes[i];
    if let Some(j) = a.bussen.iter().position(|b| b.2 == naam) {
        let (spoor, ..) = a.bussen.remove(j);
        let _ = a.tx.send(Bericht::Sluit(spoor));
    }
    if a.bussen.is_empty() {
        let a = takes.remove(i);
        drop(takes);
        sluit(a, klok);
    }
}

fn sluit(a: Actief, klok: &Klok) {
    let midi = (!a.midi_namen.is_empty()).then(|| MidiBestand {
        namen: a.midi_namen,
        events: a.midi_events,
        pulsen: klok.bron.is_some().then_some(a.pulsen),
    });
    let _ = a.tx.send(Bericht::Stop(midi));
}

fn uitvoeren(
    o: Opdracht,
    core: &pw::core::CoreRc,
    registry: &pw::registry::RegistryRc,
    bindingen: &BTreeMap<u32, Binding>,
    gelegd: &RefCell<std::collections::VecDeque<pw::link::Link>>,
) {
    match o {
        Opdracht::Verbind { uit, naar } => {
            let props = pw::properties::properties! {
                "link.output.port" => uit.to_string(),
                "link.input.port" => naar.to_string(),
                "object.linger" => "true",
            };
            match core.create_object::<pw::link::Link>("link-factory", &props) {
                Ok(link) => {
                    let mut g = gelegd.borrow_mut();
                    g.push_back(link);
                    while g.len() > 64 {
                        g.pop_front();
                    }
                }
                Err(fout) => eprintln!("pecanmixer: link {uit} → {naar}: {fout}"),
            }
        }
        Opdracht::Ontkoppel { kabel } => {
            registry.destroy_global(kabel);
        }
        Opdracht::ZetDoel { knoop, doel } => {
            let Some(meta) = bindingen.values().find_map(|b| match b {
                Binding::Metadata(m, _, naam) if naam == "default" => Some(m),
                _ => None,
            }) else {
                return;
            };
            meta.set_property(knoop, "target.object", None, doel.as_deref());
        }
        Opdracht::ZetVolume { knoop, volume } => {
            let eigenschap = Property::new(
                pw::spa::sys::SPA_PROP_channelVolumes,
                Value::ValueArray(ValueArray::Float(volume)),
            );
            stuur_props(bindingen, knoop, eigenschap);
        }
        Opdracht::ZetMute { knoop, mute } => {
            let eigenschap = Property::new(pw::spa::sys::SPA_PROP_mute, Value::Bool(mute));
            stuur_props(bindingen, knoop, eigenschap);
        }
        // Die handelt de begrensde dispatch in `sessie` zelf af.
        Opdracht::ZetTransport(_)
        | Opdracht::ZetBus { .. }
        | Opdracht::Start
        | Opdracht::StartBus(_)
        | Opdracht::Stop
        | Opdracht::StopBus(_) => {}
    }
}

fn stuur_props(bindingen: &BTreeMap<u32, Binding>, knoop: u32, eigenschap: Property) {
    let Some(Binding::Knoop(node, _)) = bindingen.get(&knoop) else {
        return;
    };
    let Some(bytes) = props_pod(eigenschap) else {
        return;
    };
    if let Some(pod) = Pod::from_bytes(&bytes) {
        node.set_param(ParamType::Props, 0, pod);
    }
}

#[cfg(test)]
mod observer_tests {
    use super::*;

    #[test]
    fn cancellation_interrupts_reconnect_delay_without_a_server_or_display() {
        let stop = Arc::new(StopSignal::default());
        let worker_stop = Arc::clone(&stop);
        let (ready, waiting) = std::sync::mpsc::channel();
        let (done, completed) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready.send(()).unwrap();
            worker_stop.retry_wait();
            done.send(worker_stop.requested()).unwrap();
        });
        waiting.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.request();
        assert!(completed.recv_timeout(Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use crate::commands::{CAPACITY, NORMAL_CAPACITY, PER_TICK};

    fn graph() -> Momentopname {
        serde_json::from_str(include_str!("../../../fixtures/graph/audio-basic.json")).unwrap()
    }

    fn live(graph: &Momentopname) -> Staat {
        Staat {
            knopen: graph.knopen.iter().map(|v| (v.id, v.clone())).collect(),
            poorten: graph.poorten.iter().map(|v| (v.id, v.clone())).collect(),
            kabels: graph.kabels.iter().map(|v| (v.id, v.clone())).collect(),
            ..Default::default()
        }
    }

    fn publish_fixture(backend: &Graaf) {
        let graph = graph();
        for id in graph
            .knopen
            .iter()
            .map(|v| v.id)
            .chain(graph.poorten.iter().map(|v| v.id))
            .chain(graph.kabels.iter().map(|v| v.id))
        {
            backend.opdrachten.object_added(id);
        }
        backend.opdrachten.publish(graph);
    }

    fn backend() -> Graaf {
        let backend = Graaf {
            opdrachten: Arc::default(),
            stop: Arc::default(),
        };
        publish_fixture(&backend);
        backend
    }

    fn command(backend: &Graaf, command: Opdracht) -> BackendCommand {
        BackendCommand {
            epoch: backend.current_graph().stamp.epoch,
            command,
        }
    }

    fn all_commands() -> Vec<Opdracht> {
        vec![
            Opdracht::Verbind {
                uit: 201,
                naar: 101,
            },
            Opdracht::Ontkoppel { kabel: 500 },
            Opdracht::ZetDoel {
                knoop: 20,
                doel: Some("-1".into()),
            },
            Opdracht::ZetVolume {
                knoop: 20,
                volume: vec![0.5, 0.5],
            },
            Opdracht::ZetMute {
                knoop: 20,
                mute: true,
            },
            Opdracht::ZetTransport(Opties {
                bussen: vec![BusOpname {
                    naam: "Stereo".into(),
                    kanalen: vec![0, 1],
                }],
                midi: false,
                klok: None,
                start_stop: false,
            }),
            Opdracht::ZetBus {
                kanalen: 2,
                gain: vec![1.0, 1.0],
            },
            Opdracht::Start,
            Opdracht::StartBus(BusOpname {
                naam: "Stereo".into(),
                kanalen: vec![0, 1],
            }),
            Opdracht::Stop,
            Opdracht::StopBus("Stereo".into()),
        ]
    }

    #[test]
    fn production_boundary_dispatches_all_existing_commands_in_fifo_order() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Graaf>();
        let backend = backend();
        let state = live(&graph());
        let expected = all_commands();
        for o in &expected {
            backend.dispatch(command(&backend, o.clone())).unwrap();
        }
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|o| state.geldige_objecten(o), |o| executed.push(o),)
            .is_empty());
        assert_eq!(executed, expected);
    }

    #[test]
    fn reconnect_rejects_every_pending_command_even_when_ids_are_reused() {
        let backend = backend();
        let state = live(&graph());
        let old = command(
            &backend,
            Opdracht::Verbind {
                uit: 201,
                naar: 101,
            },
        );
        for o in all_commands() {
            backend.dispatch(command(&backend, o)).unwrap();
        }
        let old_stamp = backend.current_graph().stamp;
        backend.opdrachten.publish(Momentopname::default());
        publish_fixture(&backend); // Deliberately reuse every ID.
        assert!(backend.current_graph().stamp.epoch > old_stamp.epoch);
        assert_eq!(backend.dispatch(old), Err(DispatchError::StaleEpoch));
        let fresh = Opdracht::ZetMute {
            knoop: 20,
            mute: false,
        };
        backend.dispatch(command(&backend, fresh.clone())).unwrap();
        let mut executed = Vec::new();
        let rejected = backend
            .opdrachten
            .drain(|o| state.geldige_objecten(o), |o| executed.push(o));
        assert!(
            rejected.is_empty(),
            "old pending entries were discarded on invalidation"
        );
        assert_eq!(executed, [fresh]);
    }

    #[test]
    fn disconnect_rejects_queued_and_new_work_without_replaying_it() {
        let backend = backend();
        let old = command(&backend, Opdracht::Start);
        backend.dispatch(old.clone()).unwrap();
        backend.opdrachten.publish(Momentopname::default());
        assert_eq!(backend.dispatch(old), Err(DispatchError::StaleEpoch));
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Start)),
            Err(DispatchError::Disconnected)
        );
        backend.doe(Opdracht::Start); // Same protection for the legacy entry.
        assert!(backend
            .opdrachten
            .drain(
                |_| panic!("no object checks for disconnected commands"),
                |_| panic!("no execution while disconnected"),
            )
            .is_empty());
        publish_fixture(&backend);
        assert!(backend
            .opdrachten
            .drain(|_| true, |_| panic!("no replay"))
            .is_empty());
    }

    #[test]
    fn removed_objects_are_checked_against_the_live_registry_not_the_ui_snapshot() {
        let backend = backend();
        let mut state = live(&graph());
        let pending = [
            Opdracht::Verbind {
                uit: 201,
                naar: 101,
            },
            Opdracht::Verbind {
                uit: 202,
                naar: 102,
            },
            Opdracht::Ontkoppel { kabel: 500 },
            Opdracht::ZetDoel {
                knoop: 20,
                doel: None,
            },
            Opdracht::ZetVolume {
                knoop: 20,
                volume: vec![1.0],
            },
            Opdracht::ZetMute {
                knoop: 20,
                mute: true,
            },
        ];
        for o in pending.iter().cloned() {
            backend.dispatch(command(&backend, o)).unwrap();
        }
        // Remove on the worker before the next coalesced graph publication.
        state.weg(201);
        state.weg(500);
        state.weg(20);
        assert!(backend
            .current_graph()
            .value
            .knopen
            .iter()
            .any(|n| n.id == 20));
        assert_eq!(
            backend.opdrachten.drain(
                |o| state.geldige_objecten(o),
                |_| panic!("removed IDs must never reach PipeWire"),
            ),
            vec![DispatchError::InvalidObject; pending.len()]
        );
    }

    #[test]
    fn invalid_link_direction_and_clock_ids_are_rejected() {
        let backend = backend();
        let mut state = live(&graph());
        let midi = Poort {
            id: 999,
            soort: Soort::Midi,
            ..state.poorten[&201].clone()
        };
        state.poorten.insert(midi.id, midi);
        let options = |klok| {
            Opdracht::ZetTransport(Opties {
                bussen: vec![],
                midi: false,
                klok,
                start_stop: false,
            })
        };
        for o in [
            Opdracht::Verbind {
                uit: 101,
                naar: 201,
            },
            Opdracht::Verbind {
                uit: 9999,
                naar: 101,
            },
            options(Some(9999)),
            options(Some(201)), // Audio cannot supply MIDI clock.
            options(Some(101)), // Input cannot supply clock.
        ] {
            backend.dispatch(command(&backend, o)).unwrap();
        }
        assert_eq!(
            backend
                .opdrachten
                .drain(|o| state.geldige_objecten(o), |_| panic!("invalid object"),),
            vec![DispatchError::InvalidObject; 5]
        );
        assert!(state.geldige_objecten(&options(Some(999))));
        assert!(state.geldige_objecten(&options(None)));
    }

    #[test]
    fn queue_capacity_and_per_tick_budget_are_bounded_without_reordering() {
        let backend = backend();
        for i in 0..NORMAL_CAPACITY {
            backend
                .dispatch(command(&backend, Opdracht::StopBus(i.to_string())))
                .unwrap();
        }
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Start)),
            Err(DispatchError::QueueFull)
        );
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|_| true, |o| executed.push(o))
            .is_empty());
        assert_eq!(executed.len(), PER_TICK);
        // A producer can refill, but cannot extend this tick's budget.
        for i in 0..PER_TICK {
            backend
                .dispatch(command(
                    &backend,
                    Opdracht::StopBus((NORMAL_CAPACITY + i).to_string()),
                ))
                .unwrap();
        }
        while executed.len() < NORMAL_CAPACITY + PER_TICK {
            let before = executed.len();
            assert!(backend
                .opdrachten
                .drain(|_| true, |o| executed.push(o))
                .is_empty());
            assert!(executed.len() - before <= PER_TICK);
        }
        assert_eq!(
            executed,
            (0..NORMAL_CAPACITY + PER_TICK)
                .map(|i| Opdracht::StopBus(i.to_string()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn legacy_entry_stamps_at_admission_and_preserves_existing_command_values() {
        let backend = backend();
        let expected = all_commands();
        for o in expected.iter().cloned() {
            backend.doe(o);
        }
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|_| true, |o| executed.push(o))
            .is_empty());
        assert_eq!(executed, expected);
        backend.doe(Opdracht::Start);
        backend.opdrachten.publish(Momentopname::default());
        publish_fixture(&backend);
        assert!(backend
            .opdrachten
            .drain(|_| true, |_| panic!("no restamping"))
            .is_empty());
    }

    #[test]
    fn saturated_queue_reserves_stop_all_without_reordering_or_unbounded_duplicates() {
        let backend = backend();
        let mut expected = Vec::new();
        for i in 0..NORMAL_CAPACITY {
            let o = Opdracht::StopBus(i.to_string());
            backend.dispatch(command(&backend, o.clone())).unwrap();
            expected.push(o);
        }
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Start)),
            Err(DispatchError::QueueFull)
        );
        // Both public admissions use the reservation, and adjacent duplicates
        // consume no extra entries. This cannot overtake already accepted work.
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        for _ in 0..10_000 {
            backend.doe(Opdracht::Stop);
        }
        expected.push(Opdracht::Stop);
        assert_eq!(expected.len(), CAPACITY);
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Start)),
            Err(DispatchError::QueueFull)
        );
        let mut executed = Vec::new();
        for _ in 0..CAPACITY.div_ceil(PER_TICK) {
            assert!(backend
                .opdrachten
                .drain(|_| true, |o| executed.push(o))
                .is_empty());
        }
        assert_eq!(executed, expected);
        assert!(backend
            .opdrachten
            .drain(|_| true, |_| panic!("duplicate stop grew queue"))
            .is_empty());
        // Do not merge stops with an intervening Start.
        for o in [Opdracht::Stop, Opdracht::Start, Opdracht::Stop] {
            backend.dispatch(command(&backend, o)).unwrap();
        }
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|_| true, |o| executed.push(o))
            .is_empty());
        assert_eq!(executed, [Opdracht::Stop, Opdracht::Start, Opdracht::Stop]);
    }

    #[test]
    fn invalidation_releases_all_stale_capacity_before_any_worker_drain() {
        let backend = backend();
        let old = command(&backend, Opdracht::Start);
        for _ in 0..NORMAL_CAPACITY {
            backend.dispatch(old.clone()).unwrap();
        }
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        backend.opdrachten.publish(Momentopname::default());
        assert_eq!(backend.dispatch(old), Err(DispatchError::StaleEpoch));
        publish_fixture(&backend);
        // No intervening drain: all ordinary capacity and the reserved slot
        // must be available atomically with the new epoch publication.
        for i in 0..NORMAL_CAPACITY {
            backend
                .dispatch(command(&backend, Opdracht::StopBus(i.to_string())))
                .unwrap();
        }
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        let mut executed = Vec::new();
        for _ in 0..CAPACITY.div_ceil(PER_TICK) {
            assert!(backend
                .opdrachten
                .drain(|_| true, |o| executed.push(o))
                .is_empty());
        }
        let mut expected: Vec<_> = (0..NORMAL_CAPACITY)
            .map(|i| Opdracht::StopBus(i.to_string()))
            .collect();
        expected.push(Opdracht::Stop);
        assert_eq!(executed, expected);
    }

    #[test]
    fn cancellation_discards_pending_work_and_rejects_future_commands() {
        let backend = backend();
        let commands = Arc::clone(&backend.opdrachten);
        backend
            .dispatch(command(&backend, Opdracht::Start))
            .unwrap();
        let stop = Arc::clone(&backend.stop);
        drop(backend);
        assert!(stop.requested());
        assert_eq!(
            commands.legacy(Opdracht::Start),
            Err(DispatchError::Stopped)
        );
        assert_eq!(
            commands.enqueue(BackendCommand {
                epoch: 0,
                command: Opdracht::Stop
            }),
            Err(DispatchError::Stopped)
        );
        assert!(commands
            .drain(|_| panic!("cancelled"), |_| panic!("cancelled"))
            .is_empty());
    }

    #[test]
    fn cancellation_between_commands_stops_the_current_drain() {
        let backend = backend();
        backend
            .dispatch(command(&backend, Opdracht::Start))
            .unwrap();
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(
                |_| true,
                |o| {
                    executed.push(o);
                    backend.opdrachten.stop();
                },
            )
            .is_empty());
        assert_eq!(executed, [Opdracht::Start]);
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Start)),
            Err(DispatchError::Stopped)
        );
    }

    #[test]
    fn graph_revisions_within_a_session_do_not_invalidate_queued_commands() {
        let backend = backend();
        let old = backend.current_graph().stamp;
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        let mut next = graph();
        next.quantum = Some(512);
        backend.opdrachten.publish(next);
        assert_eq!(backend.current_graph().stamp.epoch, old.epoch);
        assert!(backend.current_graph().stamp.revision > old.revision);
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|_| true, |o| executed.push(o))
            .is_empty());
        assert_eq!(executed, [Opdracht::Stop]);
    }

    #[test]
    fn admitted_node_and_parent_identities_cannot_bind_to_same_session_replacements() {
        let backend = backend();
        let mut state = live(&graph());
        let old_stamp = backend.current_graph().stamp;
        let pending = [
            Opdracht::ZetMute {
                knoop: 20,
                mute: true,
            },
            Opdracht::ZetVolume {
                knoop: 20,
                volume: vec![0.5],
            },
            Opdracht::ZetDoel {
                knoop: 20,
                doel: Some("-1".into()),
            },
            Opdracht::Verbind {
                uit: 201,
                naar: 101,
            },
            Opdracht::Ontkoppel { kabel: 500 },
        ];
        for o in pending.iter().cloned() {
            backend.dispatch(command(&backend, o)).unwrap();
        }
        let mut replacement = state.knopen[&20].clone();
        replacement.serial = Some(999_999);
        // Same hooks used by the actual registry callbacks, before any
        // coalesced graph publication. Ports may still refer to node ID 20.
        backend.opdrachten.object_removed(20);
        state.weg(20);
        backend.opdrachten.object_added(20);
        state.knopen.insert(20, replacement);
        assert_eq!(backend.current_graph().stamp, old_stamp);
        assert!(
            pending.iter().all(|o| state.geldige_objecten(o)),
            "numeric presence alone would pass"
        );
        assert_eq!(
            backend.opdrachten.drain(
                |o| state.geldige_objecten(o),
                |_| panic!("must never mutate the replacement"),
            ),
            vec![DispatchError::InvalidObject; pending.len()]
        );
        // Until publication, fresh admission against the old snapshot must
        // not adopt the replacement's live generation either.
        let fresh = Opdracht::ZetMute {
            knoop: 20,
            mute: false,
        };
        backend.dispatch(command(&backend, fresh.clone())).unwrap();
        assert_eq!(
            backend
                .opdrachten
                .drain(|o| state.geldige_objecten(o), |_| panic!("old snapshot")),
            [DispatchError::InvalidObject]
        );
        backend.opdrachten.publish(state.momentopname());
        backend.dispatch(command(&backend, fresh.clone())).unwrap();
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(|o| state.geldige_objecten(o), |o| executed.push(o))
            .is_empty());
        assert_eq!(executed, [fresh]);
    }

    #[test]
    fn port_link_and_clock_generations_reject_reuse_even_with_identical_dtos() {
        for (id, o) in [
            (
                201,
                Opdracht::Verbind {
                    uit: 201,
                    naar: 101,
                },
            ),
            (500, Opdracht::Ontkoppel { kabel: 500 }),
            (
                201,
                Opdracht::ZetTransport(Opties {
                    bussen: vec![],
                    midi: false,
                    klok: Some(201),
                    start_stop: false,
                }),
            ),
        ] {
            let backend = backend();
            let mut state = live(&graph());
            if matches!(&o, Opdracht::ZetTransport(_)) {
                state.poorten.get_mut(&201).unwrap().soort = Soort::Midi;
            }
            assert!(state.geldige_objecten(&o));
            backend.dispatch(command(&backend, o.clone())).unwrap();
            backend.opdrachten.object_removed(id);
            backend.opdrachten.object_added(id);
            // No serialized serial field exists for ports or links, and the
            // replacement may expose exactly the same DTO. Generation still
            // changes and the admitted command must not cross that boundary.
            assert_eq!(
                backend.opdrachten.drain(
                    |o| state.geldige_objecten(o),
                    |_| panic!("must not bind old work to a replacement"),
                ),
                [DispatchError::InvalidObject]
            );
            backend.opdrachten.publish(state.momentopname());
            backend.dispatch(command(&backend, o.clone())).unwrap();
            let mut executed = Vec::new();
            assert!(backend
                .opdrachten
                .drain(|o| state.geldige_objecten(o), |o| executed.push(o))
                .is_empty());
            assert_eq!(executed, [o]);
        }
    }

    #[test]
    fn cancellation_after_dequeue_allows_only_the_already_claimed_command() {
        let backend = backend();
        backend
            .dispatch(command(&backend, Opdracht::Start))
            .unwrap();
        backend.dispatch(command(&backend, Opdracht::Stop)).unwrap();
        let mut executed = Vec::new();
        assert!(backend
            .opdrachten
            .drain(
                |_| {
                    backend.opdrachten.stop();
                    true
                },
                |o| executed.push(o),
            )
            .is_empty());
        assert_eq!(
            executed,
            [Opdracht::Start],
            "cancellation is not rollback of claimed work"
        );
        assert_eq!(
            backend.dispatch(command(&backend, Opdracht::Stop)),
            Err(DispatchError::Stopped)
        );
        assert!(backend
            .opdrachten
            .drain(|_| panic!("cancelled"), |_| panic!("cancelled"))
            .is_empty());
    }
}
