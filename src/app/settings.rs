//! app/settings.rs
//! The settings of my MeshCore radio, changed from the console of the Device tab: a typed
//! value (name, TX power, LoRa parameters, position, region, path hash size, GPS, position
//! sharing, telemetry) is checked here before it goes to the radio, as the MeshVox app does.
//!
//! Author: O. Booklage
//! Date: November 2026
//! Licence: GPL-3.0

use super::App;
use crate::geo::Pos;
use crate::radio::{Change, Firmware, Link, MyRadio, ToRadio};

/// Upper TX power limit when the radio did not give its own.
const DEFAULT_MAX_TX_DBM: i8 = 22;

/// Suggested LoRa settings of the official MeshCore list (api.meshcore.nz/api/v1/config, as
/// the MeshVox app copied it on 2026-11-02): title, then what would be typed.
// ponytail: copied list, no download; update it by hand when the official list changes.
pub const PRESETS: [(&str, &str); 27] = [
    ("Australia", "915.8 250 10 5"),
    ("Australia (Narrow)", "916.575 62.5 7 7"),
    ("Australia (Mid)", "915.075 125 9 5"),
    ("Australia: SA, WA", "923.125 62.5 8 8"),
    ("Australia: QLD", "923.125 62.5 8 5"),
    ("Brazil", "923.125 62.5 8 8"),
    ("Canada", "910.525 62.5 7 5"),
    ("Costa Rica", "910.525 125 11 5"),
    ("EU/UK (Narrow)", "869.618 62.5 8 8"),
    ("EU/UK (Deprecated)", "869.525 250 11 5"),
    ("Czech Republic (Narrow)", "869.432 62.5 7 5"),
    ("EU 433MHz (Long Range)", "433.65 250 11 5"),
    ("EU 433MHz (Narrow)", "433.65 62.5 8 8"),
    ("Hungary", "869.618 62.5 7 5"),
    ("Netherlands", "869.618 62.5 7 5"),
    ("Netherlands (Limburg)", "869.618 62.5 8 8"),
    ("New Zealand (Narrow)", "917.375 62.5 7 5"),
    ("New Zealand (Gisborne)", "917.375 250 11 5"),
    ("Portugal 433", "433.375 62.5 9 6"),
    ("Portugal 868", "869.618 62.5 7 6"),
    ("Slovakia", "869.618 62.5 7 5"),
    ("Switzerland", "869.618 62.5 8 8"),
    ("Thailand", "921 62.5 7 5"),
    ("USA", "910.525 62.5 7 5"),
    ("USA - Southern California", "927.875 62.5 7 5"),
    ("Vietnam (Narrow)", "920.25 62.5 8 5"),
    ("Vietnam (Deprecated)", "920.25 250 11 5"),
];

/// A setting of my radio, as the console names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setting {
    Name,
    TxPower,
    Lora,
    Position,
    Region,
    PathHash,
    Gps,
    Share,
    Telemetry,
}

impl Setting {
    pub const ALL: [Setting; 9] = [
        Setting::Name,
        Setting::TxPower,
        Setting::Lora,
        Setting::Position,
        Setting::Region,
        Setting::PathHash,
        Setting::Gps,
        Setting::Share,
        Setting::Telemetry,
    ];

    /// The word that names it in the console.
    pub fn word(self) -> &'static str {
        match self {
            Setting::Name => "name",
            Setting::TxPower => "tx",
            Setting::Lora => "radio",
            Setting::Position => "pos",
            Setting::Region => "region",
            Setting::PathHash => "hash",
            Setting::Gps => "gps",
            Setting::Share => "share",
            Setting::Telemetry => "telemetry",
        }
    }

    pub fn named(word: &str) -> Option<Setting> {
        Setting::ALL.into_iter().find(|s| s.word() == word)
    }

    /// What to type after the word.
    pub fn hint(self) -> &'static str {
        match self {
            Setting::Name => "node name, 31 bytes at most",
            Setting::TxPower => "dBm",
            Setting::Lora => "MHz kHz SF CR, e.g. 869.618 62.5 8 8; or preset <n>",
            Setting::Position => "latitude longitude, e.g. 44.8378 -0.5792",
            Setting::Region => "public region, e.g. fr-33; nothing: none",
            Setting::PathHash => "bytes per relay in paths: 1, 2 or 3",
            Setting::Gps => "on or off: the GPS receiver",
            Setting::Share => "on or off: my position in my adverts",
            Setting::Telemetry => "on or off: my telemetry for every node or no one",
        }
    }

    /// The value the radio has now, as it would be typed.
    pub(super) fn current(self, me: &MyRadio, pos: Option<Pos>) -> String {
        let on_off = |on: bool| if on { "on" } else { "off" }.to_string();
        match self {
            Setting::Name => me.name.clone(),
            Setting::TxPower => (me.tx_power as i8).to_string(),
            Setting::Lora => format!(
                "{} {} {} {}",
                f64::from(me.freq_khz) / 1000.0,
                f64::from(me.bw_hz) / 1000.0,
                me.sf,
                me.cr
            ),
            Setting::Position => pos.map_or_else(String::new, |p| format!("{} {}", p.lat, p.lon)),
            Setting::Region => me.region.clone().flatten().unwrap_or_default(),
            Setting::PathHash => me.path_hash.map_or_else(String::new, |n| n.to_string()),
            Setting::Gps => me.gps.map_or_else(|| "-".into(), on_off),
            Setting::Share => on_off(me.shares_position),
            Setting::Telemetry => on_off(me.telemetry),
        }
    }

    /// The command `text` asks for, or why it cannot be sent. The ranges are the firmware's,
    /// as the MeshVox app checks them.
    pub(super) fn parse(self, text: &str, me: &MyRadio) -> Result<ToRadio, String> {
        let text = text.trim();
        // A decimal comma is accepted; the values of a line are separated by spaces.
        let number = |word: &str| word.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
        let on_off = |what: &str| match text {
            "on" => Ok(true),
            "off" => Ok(false),
            _ => Err(format!("{what}: on or off")),
        };
        let change = match self {
            Setting::Name => match text.len() {
                1..=31 => Change::Name(text.into()),
                _ => return Err("Name: 1 to 31 bytes".into()),
            },
            Setting::TxPower => {
                let max = match me.max_tx_power as i8 {
                    max if max > 0 => max,
                    _ => DEFAULT_MAX_TX_DBM,
                };
                match text.parse::<i8>() {
                    Ok(dbm) if (-9..=max).contains(&dbm) => Change::TxPower(dbm),
                    _ => return Err(format!("TX power: -9 to {max} dBm")),
                }
            }
            Setting::Lora => {
                let text = match text.strip_prefix("preset") {
                    Some(n) => match n.trim().parse::<usize>().ok().and_then(|n| PRESETS.get(n.checked_sub(1)?)) {
                        Some((_, values)) => values,
                        None => return Err(format!("Preset: 1 to {}; presets lists them", PRESETS.len())),
                    },
                    None => text,
                };
                let words: Vec<&str> = text.split_whitespace().collect();
                let [mhz, khz, sf, cr] = words[..] else {
                    return Err("LoRa: four values, MHz kHz SF CR".into());
                };
                let scaled = |word| number(word).map(|v| (v * 1000.0).round());
                let freq = scaled(mhz).filter(|f| (150_000.0..=2_500_000.0).contains(f));
                let bw = scaled(khz).filter(|b| (7_000.0..=500_000.0).contains(b));
                let sf = sf.parse::<u8>().ok().filter(|sf| (5..=12).contains(sf));
                let cr = cr.parse::<u8>().ok().filter(|cr| (5..=8).contains(cr));
                match (freq, bw, sf, cr) {
                    (Some(freq), Some(bw), Some(sf), Some(cr)) => Change::Lora {
                        freq_khz: freq as u32,
                        bw_hz: bw as u32,
                        sf,
                        cr,
                    },
                    (None, ..) => return Err("Frequency: 150 to 2500 MHz".into()),
                    (_, None, ..) => return Err("Bandwidth: 7 to 500 kHz".into()),
                    (_, _, None, _) => return Err("Spreading factor: 5 to 12".into()),
                    (.., None) => return Err("Coding rate: 5 to 8".into()),
                }
            }
            Setting::Position => {
                let words: Vec<&str> = text.split_whitespace().collect();
                let [lat, lon] = words[..] else {
                    return Err("Position: latitude then longitude".into());
                };
                let lat = number(lat).filter(|lat| (-90.0..=90.0).contains(lat));
                let lon = number(lon).filter(|lon| (-180.0..=180.0).contains(lon));
                match (lat, lon) {
                    (Some(lat), Some(lon)) => Change::Position(Pos { lat, lon }),
                    (None, _) => return Err("Latitude: -90 to 90".into()),
                    (_, None) => return Err("Longitude: -180 to 180".into()),
                }
            }
            Setting::Region => {
                // Region names are public and lowercase: a letter or digit, then up to 29
                // letters, digits or hyphens.
                let name = text.to_lowercase();
                let valid = name.len() <= 30
                    && !name.starts_with('-')
                    && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                if !valid {
                    return Err("Region: letters, digits and hyphens, 30 at most".into());
                }
                Change::Region((!name.is_empty()).then_some(name))
            }
            Setting::PathHash => match text.parse::<u8>() {
                Ok(bytes @ 1..=3) => Change::PathHash(bytes),
                _ => return Err("Path hash: 1, 2 or 3 bytes".into()),
            },
            Setting::Gps => match me.gps {
                Some(_) => Change::Gps(on_off("GPS")?),
                None => return Err("This radio has no GPS setting".into()),
            },
            Setting::Share => Change::SharePosition(on_off("Position sharing")?),
            Setting::Telemetry => Change::Telemetry(on_off("Telemetry")?),
        };
        Ok(ToRadio::Set(change))
    }
}

impl App {
    /// My radio's card when its settings can be changed; else the console says why not.
    pub(super) fn settable(&mut self) -> Option<&MyRadio> {
        if !matches!(self.link, Link::Connected(_)) {
            self.say("ERR  Not connected, settings not changed");
            return None;
        }
        if self.firmware != Firmware::MeshCore {
            self.say("ERR  Settings are MeshCore only");
            return None;
        }
        self.me.as_ref()
    }
}
