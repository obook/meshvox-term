//! radio/mod.rs
//! The radio task: holds the BLE link to the MeshCore companion radio, reconnects on its own,
//! reports to the interface (FromRadio) and takes its commands (ToRadio).
//! link.rs connects and disconnects, session.rs runs one connection, convert.rs turns
//! meshcore-rs types into ours.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

mod convert;
mod meshtastic;
mod link;
mod pair;
mod ping;
mod session;
mod task;
#[cfg(test)]
mod tests;

use crate::geo::Pos;
use crate::history::{Record, PUBLIC};
pub use convert::lipo_percent;
pub use link::{ble_disconnect, candidates, connect_to, serial_connect, serial_radio, Candidate};
pub(crate) use meshtastic::probe as probe_meshtastic;
pub use task::spawn;

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    /// Device name, or a description when none was given.
    Searching(String),
    /// BLE name of the radio.
    Connected(String),
    /// Why the link is down.
    Lost(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub pct: u8,
    pub mv: u16,
    pub used_kb: Option<u32>,
    pub total_kb: Option<u32>,
}

/// What my radio says about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MyRadio {
    pub prefix: String,
    pub name: String,
    pub model: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub freq_khz: u32,
    pub bw_hz: u32,
    pub sf: u8,
    pub cr: u8,
    pub tx_power: u8,
    pub max_tx_power: u8,
    pub shares_position: bool,
    /// Whether any node may read my telemetry (MeshCore).
    pub telemetry: bool,
    /// Bytes per relay in the paths my radio writes; None when the firmware did not say.
    pub path_hash: Option<u8>,
    /// Whether the GPS receiver is on; None when the radio has none or did not say.
    pub gps: Option<bool>,
    /// Whether the GPS switch lets the receiver run; None when the radio has no switch or did
    /// not say. When it is off, the GPS stays off whatever `gps` says.
    pub gps_switch: Option<bool>,
    /// Meshtastic region and modem preset ("EU_868 LONG_FAST"), when it uses one.
    pub preset: Option<String>,
    /// MeshCore default region (flood scope). None: not read (Meshtastic, or a firmware older
    /// than the region commands); Some(None): no region set.
    pub region: Option<Option<String>>,
}

pub enum FromRadio {
    /// What the connected radio runs; sent before `Link::Connected`.
    Firmware(Firmware),
    Link(Link),
    SelfPos(Option<Pos>),
    Battery(Battery),
    MyRadio(MyRadio),
    Nodes(Vec<Node>),
    Record(Record),
    /// The radio did not take the message: `conv` and `text` let the UI give it back.
    SendFailed {
        conv: String,
        text: String,
        error: String,
    },
    Pong {
        prefix: String,
        ping: Ping,
    },
    /// The radio took the advert, or did not (`error`).
    Advert {
        flood: bool,
        error: Option<String>,
    },
    Pairing(Pairing),
    /// The reboot command could not be sent.
    RebootFailed(String),
    /// A setting was saved, or not (the error).
    Setting(Option<String>),
    /// A line for the user: how a command of theirs ended.
    Notice(String),
}

/// A setting of my MeshCore radio to change.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Name(String),
    TxPower(i8),
    Lora {
        freq_khz: u32,
        bw_hz: u32,
        sf: u8,
        cr: u8,
    },
    Position(Pos),
    Gps(bool),
    /// Whether my adverts carry my position.
    SharePosition(bool),
    /// Whether any node may read my telemetry.
    Telemetry(bool),
    /// Default region (flood scope), a public name like "fr-33"; None clears it.
    Region(Option<String>),
    /// Bytes per relay in the paths my radio writes: 1 to 3.
    PathHash(u8),
}

/// Where a pairing stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Pairing {
    /// Looking for the radio, or waiting for its answer.
    Running,
    /// BlueZ asks for the PIN the radio named here shows on its screen.
    Pin(String),
    /// Paired with this radio, at this Unix time.
    Paired(String, i64),
    Failed(String),
}

#[derive(Debug, PartialEq)]
pub enum ToRadio {
    SendDirect {
        prefix: String,
        text: String,
    },
    SendPublic {
        text: String,
    },
    /// Status request to a repeater or room; `key` is its full public key in hex.
    Ping {
        key: String,
        prefix: String,
    },
    /// Advert of my radio: to the whole network (flood), or to the neighbours only (zero hop).
    Advert {
        flood: bool,
    },
    /// Reboot my radio (MeshCore); it does not answer, the link drops and comes back.
    Reboot,
    /// Change a setting of my radio (MeshCore).
    Set(Change),
    /// Read the contact list again.
    Refresh,
    /// Publish my radio on map.meshcore.io: name, key, position, radio settings (MeshCore).
    MapUpload,
    /// Remove the radio's bond and pair it again with the PIN it shows.
    Pair,
    /// The PIN typed for the pairing; None cancels it.
    Pin(Option<u32>),
}

/// Outcome of a ping (status request).
#[derive(Clone, Debug, PartialEq)]
pub enum Ping {
    Waiting,
    Pong {
        rtt_ms: u64,
        snr: f32,
        battery_mv: u16,
        uptime: u32,
        noise: i16,
    },
    Failed(String),
    /// A Meshtastic traceroute answer.
    Trace {
        rtt_ms: u64,
        hops: u8,
        snr: Option<f32>,
    },
}

impl ToRadio {
    pub fn failed(&self, error: String) -> FromRadio {
        let (conv, text) = match self {
            ToRadio::SendDirect { prefix, text } => (prefix.clone(), text.clone()),
            ToRadio::SendPublic { text } => (PUBLIC.to_string(), text.clone()),
            ToRadio::Ping { prefix, .. } => {
                return FromRadio::Pong {
                    prefix: prefix.clone(),
                    ping: Ping::Failed(error),
                }
            }
            ToRadio::Advert { flood } => {
                return FromRadio::Advert {
                    flood: *flood,
                    error: Some(error),
                }
            }
            ToRadio::Reboot => return FromRadio::RebootFailed(error),
            ToRadio::Set(_) => return FromRadio::Setting(Some(error)),
            ToRadio::Refresh => return FromRadio::Notice(format!("Refresh failed: {error}")),
            ToRadio::MapUpload => return FromRadio::Notice(format!("Map upload failed: {error}")),
            ToRadio::Pair | ToRadio::Pin(_) => return FromRadio::Pairing(Pairing::Failed(error)),
        };
        FromRadio::SendFailed { conv, text, error }
    }
}

/// The history file cannot be written: fetching more messages would lose them.
#[derive(Debug)]
pub struct HistoryWriteFailed(pub(super) std::io::Error);

impl std::fmt::Display for HistoryWriteFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "history write failed: {}", self.0)
    }
}

impl std::error::Error for HistoryWriteFailed {}

/// The user asked to pair the radio: the session ends so its bond can be removed.
#[derive(Debug)]
pub struct PairAsked;

impl std::fmt::Display for PairAsked {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "pairing asked")
    }
}

impl std::error::Error for PairAsked {}

/// Shown while searching when no device name was given.
pub const ANY_RADIO: &str = "a MeshCore or Meshtastic radio";

/// What the radio runs, found from what it advertises over Bluetooth.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Firmware {
    #[default]
    MeshCore,
    Meshtastic,
}

impl Firmware {
    pub fn name(self) -> &'static str {
        match self {
            Firmware::MeshCore => "MeshCore",
            Firmware::Meshtastic => "Meshtastic",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NodeKind {
    #[default]
    Chat,
    Repeater,
    Room,
    Sensor,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// First 6 bytes of the public key, in hex: the conversation key.
    pub prefix: String,
    pub name: String,
    pub kind: NodeKind,
    pub pos: Option<Pos>,
    /// Full public key in hex, needed for a status request.
    pub key: String,
    /// Last time my radio heard of it (Unix seconds, my radio's clock); 0 if never.
    pub heard: u32,
    /// Hops on the known route; -1 when none is known (messages flood the mesh).
    pub path_len: i8,
    /// The route's relay hashes.
    pub path: Vec<u8>,
    /// Marked as a favorite on the radio (MeshCore: bit 0 of the contact flags, the one
    /// the MeshVox app sets; Meshtastic: the node database's favorite flag).
    pub favorite: bool,
}
