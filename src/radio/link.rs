//! radio/link.rs
//! Finding the radios over BLE and telling their firmware apart, connecting and
//! disconnecting a MeshCore radio (meshcore-rs on BlueZ).
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use meshcore_rs::MeshCore;
use uuid::Uuid;

use super::Firmware;

/// MeshCore companion: the Nordic UART service, generic, so the name tells it apart too.
pub const MESHCORE_SERVICE: Uuid = Uuid::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);
pub const MESHTASTIC_SERVICE: Uuid = Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd);

/// How long a scan listens for radios.
const SCAN_TIME: Duration = Duration::from_secs(4);

/// A radio to try: its BLE name (or the address given with --device) and its firmware.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub name: String,
    pub firmware: Firmware,
}

/// The firmware a radio runs, from its advertised name and services; None when it is
/// neither. The name decides first: BlueZ updates it from each advert, while its list of
/// services keeps those of the other firmware after a switch.
pub fn classify(name: Option<&str>, services: &[Uuid]) -> Option<Firmware> {
    if name.is_some_and(|n| n.starts_with("MeshCore-")) {
        Some(Firmware::MeshCore)
    } else if services.contains(&MESHTASTIC_SERVICE) {
        Some(Firmware::Meshtastic)
    } else {
        None
    }
}

/// Whether a scanned radio is the one asked with --device (a name or an address, any case).
pub(super) fn is_device(device: Option<&str>, name: Option<&str>, address: &str) -> bool {
    device.is_none_or(|d| {
        name.is_some_and(|n| n.eq_ignore_ascii_case(d)) || address.eq_ignore_ascii_case(d)
    })
}

/// Every MeshCore or Meshtastic radio BlueZ reports after a scan, or only `device` (a name
/// or an address). BlueZ also reports radios it only remembers, so the caller tries them in
/// turn. A `device` the scan does not find is tried as a MeshCore radio, as before.
pub async fn candidates(device: Option<&str>) -> anyhow::Result<Vec<Candidate>> {
    let found = scan().await?;
    let mut list: Vec<Candidate> = found
        .into_iter()
        .filter(|(name, address, _)| is_device(device, name.as_deref(), address))
        .filter_map(|(name, address, services)| {
            let firmware = classify(name.as_deref(), &services)?;
            Some(Candidate {
                name: device.map_or_else(|| name.unwrap_or(address), str::to_string),
                firmware,
            })
        })
        .collect();
    if let (Some(d), true) = (device, list.is_empty()) {
        list.push(Candidate {
            name: d.to_string(),
            firmware: Firmware::MeshCore,
        });
    }
    Ok(list)
}

/// Name, address and advertised services of every radio BlueZ knows after a scan for
/// either service, through the process-wide BLE manager.
async fn scan() -> anyhow::Result<Vec<(Option<String>, String, Vec<Uuid>)>> {
    let adapter = meshcore_rs::ble_manager()
        .await?
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter"))?;
    // All scans share one D-Bus sender, and BlueZ refuses a scan from a sender that left
    // one running.
    let _ = adapter.stop_scan().await;
    adapter
        .start_scan(ScanFilter {
            services: vec![MESHCORE_SERVICE, MESHTASTIC_SERVICE],
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start BLE scan: {e}"))?;
    tokio::time::sleep(SCAN_TIME).await;
    let _ = adapter.stop_scan().await;
    let mut found = vec![];
    for p in adapter.peripherals().await? {
        if let Ok(Some(props)) = p.properties().await {
            found.push((props.local_name, props.address.to_string(), props.services));
        }
    }
    Ok(found)
}

/// Connect to the radio `name`; a link left half open by a failed connect is dropped, or the
/// radio, which takes a single central, would refuse a phone.
pub async fn connect_to(name: &str) -> anyhow::Result<MeshCore> {
    match MeshCore::ble_connect(name).await {
        Ok(radio) => Ok(radio),
        Err(e) => {
            let _ = ble_disconnect(name).await;
            Err(e.into())
        }
    }
}

/// Speed of the MeshCore Companion USB firmware; a radio with native USB ignores it.
const SERIAL_BAUD: u32 = 115_200;

/// Open the MeshCore radio on the serial port `port` (Companion USB firmware).
pub async fn serial_connect(port: &str) -> anyhow::Result<MeshCore> {
    Ok(MeshCore::serial(port, SERIAL_BAUD).await?)
}

/// Where udev lists the serial ports under a stable name.
const SERIAL_DIR: &str = "/dev/serial/by-id";
/// How long a serial port has to answer as a MeshCore radio.
const SERIAL_ANSWER: Duration = Duration::from_secs(2);

/// The first serial port where a MeshCore radio answers, with its open link. A port that
/// opens but stays silent is some other device: it goes to `silent` and is not written to
/// again while it stays plugged in. `refused` gets why a port did not open, for the user.
// ponytail: Linux only (udev's by-id directory), and a radio that resets when its port opens
// (USB-UART bridge) stays silent; --serial covers both.
pub async fn serial_radio(
    silent: &mut HashSet<PathBuf>,
    refused: &mut Option<String>,
) -> Option<(String, MeshCore)> {
    *refused = None;
    let mut ports: Vec<PathBuf> = std::fs::read_dir(SERIAL_DIR)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    ports.sort();
    silent.retain(|port| ports.contains(port));
    for id in ports {
        if silent.contains(&id) {
            continue;
        }
        // The interface shows the short device name (/dev/ttyACM0).
        let Ok(port) = std::fs::canonicalize(&id) else {
            continue;
        };
        let port = port.to_string_lossy().into_owned();
        let radio = match serial_connect(&port).await {
            Ok(radio) => radio,
            Err(e) => {
                tracing::info!("{port}: {e:#}");
                *refused = Some(refusal(&port, &format!("{e:#}")));
                continue;
            }
        };
        let hello = tokio::time::timeout(SERIAL_ANSWER, async {
            radio.commands().lock().await.send_appstart().await
        })
        .await;
        if matches!(hello, Ok(Ok(_))) {
            return Some((port, radio));
        }
        tracing::info!("{port}: no MeshCore radio answered");
        let _ = radio.disconnect().await;
        silent.insert(id);
    }
    None
}

/// Why the serial port `port` did not open, with what to do when the user lacks the right.
pub(super) fn refusal(port: &str, error: &str) -> String {
    if error.contains("Permission denied") {
        // The group that owns serial ports differs: dialout on Debian, uucp on Arch.
        let group = port_group(port).unwrap_or_else(|| "dialout".into());
        format!("{port}: permission denied (join the {group} group, then log in again)")
    } else {
        format!("{port}: {error}")
    }
}

/// The name of the group that owns the file `port`.
fn port_group(port: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let gid = std::fs::metadata(port).ok()?.gid();
    group_name(&std::fs::read_to_string("/etc/group").ok()?, gid)
}

/// The name of the group `gid` in `etc_group`, the text of /etc/group.
pub(super) fn group_name(etc_group: &str, gid: u32) -> Option<String> {
    etc_group.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        (fields.nth(1)?.parse() == Ok(gid)).then(|| name.to_string())
    })
}

/// meshcore-rs 0.2.0 `disconnect()` only stops its own tasks: the BLE link stays up and a
/// phone cannot connect to the radio. Drop the link of the radio named `name` (or address).
pub async fn ble_disconnect(name: &str) -> anyhow::Result<()> {
    Ok(MeshCore::ble_disconnect(name).await?)
}

/// Unix time in seconds, as the radio counts it.
pub fn now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}
