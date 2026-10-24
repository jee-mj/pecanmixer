//! Owned wire-compatible graph, transport and command data. No toolkit or server types.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Soort {
    Audio,
    Midi,
    /// Video en wat er verder nog over een poort kan.
    Anders,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Knoop {
    pub id: u32,
    pub serial: Option<u64>,
    /// `node.name`: machineleesbaar en het stabielst, `alsa_output.…`.
    pub naam: String,
    /// `node.description`: "Fireface 400 Pro".
    pub beschrijving: Option<String>,
    /// `node.nick`: "Fireface400 PCM".
    pub bijnaam: Option<String>,
    /// `media.class`: `Audio/Sink`, `Stream/Output/Audio`, `Midi/Bridge`, …
    pub klasse: Option<String>,
    /// `device.id`: de node hoort bij een apparaat (een geluidskaart, HDMI).
    pub apparaat: Option<u32>,
    /// `application.name` van een stream: "Firefox".
    pub app: Option<String>,
    /// `media.name` van een stream: de titel van wat er speelt.
    pub media: Option<String>,
    /// `running`, `idle`, `suspended`, `creating` of `error`.
    pub stand: Option<String>,
    /// `target.object` uit de `default`-metadata: waar de stream naartoe is
    /// verplaatst. Een serial of een `node.name`; `None` = WirePlumber kiest.
    pub doel: Option<String>,
    /// Volume per kanaal, lineair (1.0 = 0 dB), uit de Props van de node.
    pub volume: Option<Vec<f32>>,
    pub mute: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Poort {
    pub id: u32,
    pub knoop: u32,
    /// `port.id`: de volgorde binnen de node, die van de driver.
    pub volgnummer: Option<u32>,
    /// `port.name`: `playback_FL`, `capture_AUX3`, `Fireface400: MIDI 1 (capture)`.
    pub naam: String,
    /// `port.alias`: `Fireface400:Fireface400 MIDI 1`.
    pub alias: Option<String>,
    /// `true` voor een ingang, `false` voor een uitgang.
    pub ingang: bool,
    pub soort: Soort,
    /// `audio.channel`: `FL`, `FR`, `MONO`, `AUX0`.
    pub kanaal: Option<String>,
    pub monitor: bool,
    pub fysiek: bool,
    /// `port.group`: bij de Midi-Bridge het ALSA-client-nummer, dus het apparaat.
    pub groep: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Kabel {
    pub id: u32,
    pub uit_knoop: u32,
    pub uit_poort: u32,
    pub in_knoop: u32,
    pub in_poort: u32,
    /// De client die de link legde. Is dat WirePlumber, dan is het beleid;
    /// anders handwerk.
    pub client: Option<u32>,
}

/// Alles wat de schil nodig heeft om de graph te tekenen.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Momentopname {
    pub verbonden: bool,
    /// Samplerate van de graph: de geforceerde als die er is, anders de standaard.
    pub rate: Option<u32>,
    pub quantum: Option<u32>,
    /// `node.name` van de standaard-uitgang en -ingang.
    pub standaard_sink: Option<String>,
    pub standaard_bron: Option<String>,
    /// De client-ids van WirePlumber (meestal één).
    pub wireplumber: Vec<u32>,
    pub knopen: Vec<Knoop>,
    pub poorten: Vec<Poort>,
    pub kabels: Vec<Kabel>,
}

/// Piek per kanaal sinds de vorige melding, lineair, per node-id. De
/// kanalen staan in de volgorde van de poorten (`port.id`).
pub type Pieken = BTreeMap<u32, Vec<f32>>;

/// Eén bus: een WAV met deze kanalen van de Rec bus. De naam is ook de
/// bestandsnaam, en zo stop je hem weer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BusOpname {
    pub naam: String,
    pub kanalen: Vec<usize>,
}

/// Wat Record doet, en de klok.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Opties {
    /// De gewapende bussen.
    pub bussen: Vec<BusOpname>,
    /// De MIDI-ingang van de bus is gewapend.
    pub midi: bool,
    /// De MIDI-bronpoort van de externe klok; `None` = intern.
    pub klok: Option<u32>,
    /// Starten en stoppen op MIDI Start/Stop van de klok.
    pub start_stop: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TransportStand {
    pub bezig: bool,
    /// De map van de laatst gestarte take.
    pub map: Option<String>,
    /// Hoe lang de oudste lopende take al loopt.
    pub seconden: f64,
    /// Per bus die opneemt: hoe lang al.
    pub bussen: BTreeMap<String, f64>,
    pub klok: KlokStand,
    /// Wat er misging bij de laatste opname.
    pub fout: Option<String>,
}

/// Wat de schil de graph kan laten doen. Alles op object-id: de schil werkt
/// met de momentopname van zojuist, en een id die intussen weg is doet niets.
#[derive(Clone, Debug, PartialEq)]
pub enum Opdracht {
    /// Een link van een uitgangspoort naar een ingangspoort. Blijft bestaan
    /// als Pecan Mixer sluit (`object.linger`): wat je patcht, blijft gepatcht.
    Verbind {
        uit: u32,
        naar: u32,
    },
    /// Een link weghalen, van wie hij ook is.
    Ontkoppel {
        kabel: u32,
    },
    /// `target.object` van een node zetten (een serial) of wissen. Dan
    /// verplaatst WirePlumber de stream, zoals bij pavucontrol.
    ZetDoel {
        knoop: u32,
        doel: Option<String>,
    },
    /// Volume per kanaal, lineair.
    ZetVolume {
        knoop: u32,
        volume: Vec<f32>,
    },
    ZetMute {
        knoop: u32,
        mute: bool,
    },
    /// Opties voor de volgende opname, en welke klok.
    ZetTransport(Opties),
    /// Hoeveel kanalen de Rec bus heeft, en hun gain (lineair, 0 = mute).
    ZetBus {
        kanalen: usize,
        gain: Vec<f32>,
    },
    /// Record: de gewapende bussen en MIDI uit de opties, samen in één take.
    Start,
    /// Eén bus in een eigen take.
    StartBus(BusOpname),
    /// Alles stoppen.
    Stop,
    StopBus(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KlokStand {
    pub bpm: Option<f64>,
    /// De pulsen komen regelmatig binnen.
    pub lock: bool,
}

/// Wat er sinds de vorige oogst uit één bronpoort kwam.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Activiteit {
    pub events: u32,
    /// Bit `n` = kanaal `n + 1`.
    pub kanalen: u16,
    pub soorten: u8,
    /// Het laatste muzikale event ("Note On E4 · vel 76"); klok telt niet mee.
    pub laatste: Option<String>,
}
