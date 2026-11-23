//! radio/convert.rs
//! Conversions from meshcore-rs types: contacts to nodes, received messages to history
//! records, self info to MyRadio, battery voltage to a charge percentage.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use std::collections::HashMap;

use meshcore_rs::events::{ChannelMessage, Contact, ContactMessage};
use meshcore_rs::parsing::hex_encode;

use super::{Change, MyRadio, Node, NodeKind};
use crate::geo::pos_from_micro;
use crate::history::{Msg, PUBLIC};

/// `dev` is missing on firmwares that do not answer the device query.
pub fn my_radio(
    me: &meshcore_rs::events::SelfInfo,
    dev: Option<&meshcore_rs::events::DeviceInfoData>,
) -> MyRadio {
    MyRadio {
        prefix: hex_encode(&me.public_key[..6]),
        name: me.name.clone(),
        model: dev.and_then(|device| device.model.clone()),
        version: dev.and_then(|device| device.version.clone()),
        build: dev.and_then(|device| device.fw_build.clone()),
        freq_khz: me.radio_freq,
        bw_hz: me.radio_bw,
        sf: me.sf,
        cr: me.cr,
        tx_power: me.tx_power,
        max_tx_power: me.max_tx_power,
        // Firmware ADVERT_LOC_NONE (0): the advert carries no position.
        shares_position: me.adv_loc_policy != 0,
        // Base mode 0: no one; 2: everyone, the two the MeshVox app writes.
        telemetry: me.telemetry_mode_base != 0,
        path_hash: dev.and_then(|device| device.path_hash_mode).map(|mode| mode + 1),
        gps: None,
        gps_switch: None,
        preset: None,
        region: None,
    }
}

/// The companion frame that writes `change`, as the MeshVox app sends it. `me` gives the
/// other values of the frame that carries the position sharing.
pub fn change_frame(change: &Change, me: &meshcore_rs::events::SelfInfo) -> Vec<u8> {
    let micro = |degrees: f64| ((degrees * 1e6).round() as i32).to_le_bytes();
    match change {
        // CMD_SET_ADVERT_NAME
        Change::Name(name) => [&[8], name.as_bytes()].concat(),
        // CMD_SET_RADIO_TX_POWER
        Change::TxPower(dbm) => vec![12, *dbm as u8],
        // CMD_SET_RADIO_PARAMS
        Change::Lora {
            freq_khz,
            bw_hz,
            sf,
            cr,
        } => [
            &[11][..],
            &freq_khz.to_le_bytes(),
            &bw_hz.to_le_bytes(),
            &[*sf, *cr],
        ]
        .concat(),
        // CMD_SET_ADVERT_LATLON
        Change::Position(pos) => [&[14][..], &micro(pos.lat), &micro(pos.lon)].concat(),
        // CMD_SET_CUSTOM_VAR: the firmware splits on ':' (meshcore-rs sends '=').
        Change::Gps(on) => format!("\x29gps:{}", u8::from(*on)).into_bytes(),
        // CMD_SET_OTHER_PARAMS: the frame rewrites all four values.
        Change::SharePosition(on) => other_params(me, me.telemetry_mode_base, u8::from(*on)),
        Change::Telemetry(on) => other_params(me, if *on { 2 } else { 0 }, me.adv_loc_policy),
        // CMD_SET_DEFAULT_FLOOD_SCOPE: the name on 31 bytes then its key; alone, it clears.
        Change::Region(None) => vec![63],
        Change::Region(Some(name)) => {
            let mut padded = [0u8; 31];
            padded[..name.len().min(31)].copy_from_slice(&name.as_bytes()[..name.len().min(31)]);
            [&[63][..], &padded, &region_key(name)].concat()
        }
        // CMD_SET_PATH_HASH_MODE
        Change::PathHash(bytes) => vec![61, 0, bytes - 1],
    }
}

/// Where the community map takes a radio's advert.
pub const MAP_UPLOAD_URL: &str = "https://map.meshcore.io/api/v1/uploader/node";

/// The "data" text of a map.meshcore.io upload, in the format of its uploader
/// (github.com/recrof/map.meshcore.io-uploader): the radio settings in MHz and kHz, and my
/// advert as a meshcore:// link. The radio signs the SHA-256 of this exact text.
pub fn map_data(me: &meshcore_rs::events::SelfInfo, advert_link: &str) -> String {
    // f64 prints 869.618 and 250 as JavaScript does.
    format!(
        r#"{{"params":{{"freq":{},"cr":{},"sf":{},"bw":{}}},"links":["{advert_link}"]}}"#,
        f64::from(me.radio_freq) / 1000.0,
        me.cr,
        me.sf,
        f64::from(me.radio_bw) / 1000.0,
    )
}

/// The body of the upload: the data text, its signature by the radio and the radio's key.
pub fn map_body(data: &str, signature: &[u8], public_key: &[u8]) -> String {
    serde_json::json!({
        "data": data,
        "signature": hex_encode(signature),
        "publicKey": hex_encode(public_key),
    })
    .to_string()
}

/// CMD_SET_OTHER_PARAMS with `base` as the telemetry base mode and `share` as the position
/// policy; what the frame also carries is written back as read.
fn other_params(me: &meshcore_rs::events::SelfInfo, base: u8, share: u8) -> Vec<u8> {
    vec![
        38,
        u8::from(me.manual_add_contacts),
        base | me.telemetry_mode_loc << 2 | me.telemetry_mode_env << 4,
        share,
        me.multi_acks,
    ]
}

/// Key of a public region: the first 16 bytes of SHA-256("#" + name), as the firmware
/// derives it (TransportKeyStore::getAutoKeyFor).
fn region_key(name: &str) -> [u8; 16] {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format!("#{name}"));
    let mut key = [0u8; 16];
    key.copy_from_slice(&digest[..16]);
    key
}

/// Firmware CMD_GET_DEFAULT_FLOOD_SCOPE; meshcore-rs does not know it.
pub const CMD_GET_DEFAULT_REGION: u8 = 64;

/// The region in a RESP_CODE_DEFAULT_FLOOD_SCOPE (28) frame: the code alone when none is set,
/// else the name on 31 bytes (NUL padded), then a 16-byte key. None: another frame.
pub fn region_from_frame(frame: &[u8]) -> Option<Option<String>> {
    let (&28, rest) = frame.split_first()? else {
        return None;
    };
    let name = rest.get(..31).unwrap_or_default();
    let name = name.split(|&b| b == 0).next().unwrap_or_default();
    Some((!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()))
}

/// The GPS state in the radio's custom variables: "gps" is 1 or 0, and only there when the
/// firmware has a GPS.
pub fn gps_from_vars(vars: &std::collections::HashMap<String, String>) -> Option<bool> {
    flag_from_vars(vars, "gps")
}

/// The GPS switch in the radio's custom variables: "gps_switch" is 1 or 0, read-only, and only
/// there when the firmware reports it (our M1 build).
pub fn gps_switch_from_vars(vars: &std::collections::HashMap<String, String>) -> Option<bool> {
    flag_from_vars(vars, "gps_switch")
}

fn flag_from_vars(vars: &std::collections::HashMap<String, String>, name: &str) -> Option<bool> {
    match vars.get(name)?.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Charge of a single-cell LiPo from its voltage, interpolated on a typical resting
/// discharge curve. meshcore-rs maps 3.00-3.93 V linearly, so a battery above 3.93 V
/// (still a third of its charge left to lose) always read 100 %.
pub fn lipo_percent(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 21] = [
        (3270, 0),
        (3610, 5),
        (3690, 10),
        (3710, 15),
        (3730, 20),
        (3750, 25),
        (3770, 30),
        (3790, 35),
        (3800, 40),
        (3820, 45),
        (3840, 50),
        (3850, 55),
        (3870, 60),
        (3910, 65),
        (3950, 70),
        (3980, 75),
        (4020, 80),
        (4080, 85),
        (4110, 90),
        (4150, 95),
        (4200, 100),
    ];
    let i = CURVE
        .iter()
        .position(|&(v, _)| mv < v)
        .unwrap_or(CURVE.len());
    match i {
        0 => 0,
        i if i == CURVE.len() => 100,
        i => {
            let ((v0, p0), (v1, p1)) = (CURVE[i - 1], CURVE[i]);
            let frac = f32::from(mv - v0) / f32::from(v1 - v0);
            (f32::from(p0) + frac * f32::from(p1 - p0)).round() as u8
        }
    }
}

impl From<&Contact> for Node {
    fn from(contact: &Contact) -> Node {
        let kind = match contact.contact_type {
            2 => NodeKind::Repeater,
            3 => NodeKind::Room,
            4 => NodeKind::Sensor,
            _ => NodeKind::Chat,
        };
        Node {
            prefix: contact.prefix_hex(),
            name: contact.adv_name.clone(),
            kind,
            pos: pos_from_micro(contact.adv_lat, contact.adv_lon),
            key: contact.public_key_hex(),
            // The sender's advert clock can be off; my radio's update time is not.
            heard: if contact.last_modification_timestamp != 0 {
                contact.last_modification_timestamp
            } else {
                contact.last_advert
            },
            path_len: contact.path_len,
            path: contact.out_path.clone(),
            favorite: contact.flags & 1 != 0,
        }
    }
}

/// Our clock, not the sender's: a node's clock can be far off.
pub(super) fn received_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Channel text arrives as "<sender>: <text>". Channel 0 is the public one.
pub fn channel_msg(message: &ChannelMessage) -> Msg {
    let (from, text) = message
        .text
        .split_once(": ")
        .unwrap_or(("?", &message.text));
    let conv = if message.channel_idx == 0 {
        PUBLIC.to_string()
    } else {
        format!("channel:{}", message.channel_idx)
    };
    Msg {
        ts: received_now(),
        conv,
        from: from.into(),
        text: text.into(),
        outgoing: false,
        ack: None,
    }
}

/// `names` maps a contact prefix (hex) to its name.
pub fn contact_msg(message: &ContactMessage, names: &HashMap<String, String>) -> Msg {
    let prefix = hex_encode(&message.sender_prefix);
    let from = names
        .get(&prefix)
        .cloned()
        .unwrap_or_else(|| prefix.clone());
    Msg {
        ts: received_now(),
        conv: prefix,
        from,
        text: message.text.clone(),
        outgoing: false,
        ack: None,
    }
}
