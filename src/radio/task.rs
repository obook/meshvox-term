//! radio/task.rs
//! The radio task: connects to a radio, runs a session, reconnects after a loss and stops
//! for good when the history cannot be written or the interface is gone. A pairing (key p,
//! or a stale bond found) runs between two connections.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use meshcore_rs::MeshCore;

use super::link::{
    ble_disconnect, candidates, connect_to, serial_connect, serial_radio, Candidate,
};
use super::meshtastic::{self, RepairNeeded, Transport};
use super::pair::pair;
use super::session::session;
use super::{
    FromRadio, Firmware, HistoryWriteFailed, Link, PairAsked, Pairing, ToRadio, ANY_RADIO,
};
use crate::config::RETRY;

/// Run the radio in `rt`: the MeshCore radio on the serial port `serial`, else a radio found
/// over Bluetooth. It reconnects on its own and stops once the UI is gone; the handle lets
/// the caller wait for its disconnect.
pub fn spawn(
    rt: &tokio::runtime::Runtime,
    device: Option<String>,
    serial: Option<String>,
    history: PathBuf,
    tx: Sender<FromRadio>,
) -> (UnboundedSender<ToRadio>, tokio::task::JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let task = match serial {
        Some(port) => rt.spawn(run_serial(port, None, history, tx, cmd_rx)),
        None => rt.spawn(run(device, history, tx, cmd_rx)),
    };
    (cmd_tx, task)
}

/// Answer to the pair key on a serial link.
const NO_PAIRING: &str = "no pairing over USB";

/// The radio task on a serial port: no scan and no pairing, the port names the radio.
/// `first` is the link the detection left open.
async fn run_serial(
    port: String,
    mut first: Option<MeshCore>,
    history: PathBuf,
    tx: Sender<FromRadio>,
    mut cmds: UnboundedReceiver<ToRadio>,
) {
    loop {
        if tx
            .send(FromRadio::Link(Link::Searching(port.clone())))
            .is_err()
        {
            return;
        }
        let opened = match first.take() {
            Some(radio) => Ok(radio),
            None => serial_connect(&port).await,
        };
        let reason = match opened {
            Ok(radio) => {
                let end = session(&radio, &port, &history, &tx, &mut cmds).await;
                // Closes the port: a rebooting radio gets the same device name back.
                let _ = radio.disconnect().await;
                match end {
                    Ok(true) => "link lost".to_string(),
                    Ok(false) => return, // UI gone
                    Err(e) if e.is::<PairAsked>() => {
                        let _ = tx.send(ToRadio::Pair.failed(NO_PAIRING.into()));
                        continue;
                    }
                    Err(e) if e.is::<HistoryWriteFailed>() => {
                        // Stop for good: every new fetch would lose a message.
                        tracing::error!("radio: {e:#}");
                        let _ = tx.send(FromRadio::Link(Link::Lost(format!(
                            "{e:#} - fix it, then restart"
                        ))));
                        return;
                    }
                    Err(e) => format!("{e:#}"),
                }
            }
            Err(e) => format!("{e:#}"),
        };
        tracing::warn!("radio: {reason}");
        if tx.send(FromRadio::Link(Link::Lost(reason))).is_err() {
            return;
        }
        if !pause_serial(&tx, &mut cmds).await {
            return;
        }
    }
}

/// The pause before the next attempt on the serial port. false: the interface is gone.
async fn pause_serial(tx: &Sender<FromRadio>, cmds: &mut UnboundedReceiver<ToRadio>) -> bool {
    let end = tokio::time::sleep(RETRY);
    tokio::pin!(end);
    loop {
        tokio::select! {
            _ = &mut end => return true,
            cmd = cmds.recv() => match cmd {
                Some(ToRadio::Pin(_)) => {}
                Some(ToRadio::Pair) => {
                    let _ = tx.send(ToRadio::Pair.failed(NO_PAIRING.into()));
                }
                Some(cmd) => {
                    let _ = tx.send(cmd.failed("not connected".into()));
                }
                None => return false,
            }
        }
    }
}

/// Why no radio connected: a stale bond, when one was found, since the user can fix it;
/// else the last error.
pub(super) fn not_found(last: anyhow::Error, repair: Option<anyhow::Error>) -> anyhow::Error {
    repair.unwrap_or(last)
}

/// A connected radio, of either firmware.
enum Radio {
    MeshCore(MeshCore),
    Meshtastic(Transport),
}

pub(super) async fn run(
    device: Option<String>,
    history: PathBuf,
    tx: Sender<FromRadio>,
    mut cmds: UnboundedReceiver<ToRadio>,
) {
    let label = device.clone().unwrap_or_else(|| ANY_RADIO.into());
    // With --device the firmware cannot change between two links: skip the 4 s rescan.
    let mut known: Option<Candidate> = None;
    // A stale bond is paired again once on its own; after that, on the user's key only, so a
    // cancelled PIN is not asked again and again.
    let mut auto_paired = false;
    // Serial ports where no MeshCore radio answered.
    let mut silent = std::collections::HashSet::new();
    // Why a serial port did not open: shown when no radio connects.
    let mut refused = None;
    loop {
        if device.is_none() {
            if let Some((port, radio)) = serial_radio(&mut silent, &mut refused).await {
                // ponytail: the task stays on this port until the program is restarted; go
                // back to the Bluetooth scan after a loss if someone needs both.
                return run_serial(port, Some(radio), history, tx, cmds).await;
            }
        }
        if tx
            .send(FromRadio::Link(Link::Searching(label.clone())))
            .is_err()
        {
            return;
        }
        // Each candidate is announced first: the UI shows it, and a quit during the connect
        // knows which radio to drop.
        let mut found = Err(anyhow::anyhow!("no radio found"));
        let list = match known.take() {
            Some(c) => vec![c],
            None => candidates(device.as_deref()).await.unwrap_or_else(|e| {
                tracing::info!("BLE scan failed: {e:#}");
                vec![]
            }),
        };
        // A radio whose bond is stale does not stop the others from being tried.
        let mut repair = None;
        for Candidate { name, firmware } in list {
            if tx
                .send(FromRadio::Link(Link::Searching(name.clone())))
                .is_err()
            {
                return;
            }
            found = match firmware {
                Firmware::MeshCore => connect_to(&name).await.map(Radio::MeshCore),
                Firmware::Meshtastic => Transport::connect(&name).await.map(Radio::Meshtastic),
            }
            .map(|radio| (radio, name.clone()));
            match found {
                Ok(_) => {
                    if device.is_some() {
                        known = Some(Candidate { name, firmware });
                    }
                    break;
                }
                Err(e) if e.is::<RepairNeeded>() => {
                    tracing::warn!("radio: {e:#}");
                    repair = Some(e);
                    found = Err(anyhow::anyhow!("no radio found"));
                }
                Err(_) => {}
            }
        }
        let found = found.map_err(|e| not_found(e, repair));
        let reason = match found {
            Ok((mut radio, name)) => {
                let end = match &mut radio {
                    Radio::MeshCore(r) => session(r, &name, &history, &tx, &mut cmds).await,
                    Radio::Meshtastic(t) => {
                        meshtastic::session(t, &name, &history, &tx, &mut cmds).await
                    }
                };
                match radio {
                    Radio::MeshCore(r) => {
                        let _ = r.disconnect().await;
                    }
                    Radio::Meshtastic(t) => t.disconnect().await,
                }
                if let Err(e) = ble_disconnect(&name).await {
                    tracing::warn!("BLE disconnect failed: {e:#}");
                }
                match end {
                    Ok(true) => "link lost".to_string(),
                    Ok(false) => return, // UI gone
                    Err(e) if e.is::<PairAsked>() => {
                        if !pair_radio(Some(&name), &tx, &mut cmds).await {
                            return;
                        }
                        continue;
                    }
                    Err(e) if e.is::<HistoryWriteFailed>() => {
                        // Stop for good: every new fetch would lose a message.
                        tracing::error!("radio: {e:#}");
                        let _ = tx.send(FromRadio::Link(Link::Lost(format!(
                            "{e:#} - fix it, then restart"
                        ))));
                        return;
                    }
                    Err(e) => format!("{e:#}"),
                }
            }
            Err(e) if e.is::<RepairNeeded>() => {
                // Retrying will not heal a stale bond: pair the radio again, or say what to do
                // and keep trying, so the radio comes back once it is paired again.
                let _ = tx.send(FromRadio::Link(Link::Lost(format!("{e:#}"))));
                let stale = e.downcast_ref::<RepairNeeded>().map(|r| r.0.clone());
                let going = match std::mem::replace(&mut auto_paired, true) {
                    false => pair_radio(stale.as_deref(), &tx, &mut cmds).await,
                    true => pause(device.as_deref(), &tx, &mut cmds).await,
                };
                if !going {
                    return;
                }
                continue;
            }
            Err(e) => {
                // Most often the radio is busy with a phone: keep the "Searching" banner
                // and its hint.
                tracing::info!("radio not reached: {e:#}");
                if let Some(why) = &refused {
                    let _ = tx.send(FromRadio::Link(Link::Lost(why.clone())));
                }
                if !pause(device.as_deref(), &tx, &mut cmds).await {
                    return;
                }
                continue;
            }
        };
        tracing::warn!("radio: {reason}");
        if tx.send(FromRadio::Link(Link::Lost(reason))).is_err() {
            return;
        }
        if !pause(device.as_deref(), &tx, &mut cmds).await {
            return;
        }
    }
}

/// The pause before the next attempt; a pair request (key p) is run during it. false: the
/// interface is gone.
async fn pause(
    device: Option<&str>,
    tx: &Sender<FromRadio>,
    cmds: &mut UnboundedReceiver<ToRadio>,
) -> bool {
    let end = tokio::time::sleep(RETRY);
    tokio::pin!(end);
    loop {
        tokio::select! {
            _ = &mut end => return true,
            cmd = cmds.recv() => match cmd {
                Some(ToRadio::Pair) => return pair_radio(device, tx, cmds).await,
                Some(ToRadio::Pin(_)) => {} // left from a pairing that ended
                Some(cmd) => {
                    let _ = tx.send(cmd.failed("not connected".into()));
                }
                None => return false,
            }
        }
    }
}

/// Pair `device`, or the first radio found, and tell the interface how it went. false: the
/// interface is gone.
async fn pair_radio(
    device: Option<&str>,
    tx: &Sender<FromRadio>,
    cmds: &mut UnboundedReceiver<ToRadio>,
) -> bool {
    let state = match pair(device, tx, cmds).await {
        Ok(name) => {
            tracing::info!("paired with {name}");
            Pairing::Paired(name, chrono::Utc::now().timestamp())
        }
        Err(e) => {
            tracing::warn!("pairing failed: {e:#}");
            Pairing::Failed(format!("{e:#}"))
        }
    };
    tx.send(FromRadio::Pairing(state)).is_ok() && !cmds.is_closed()
}
