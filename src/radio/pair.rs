//! radio/pair.rs
//! Pairing a radio with BlueZ: removes its old bond, then pairs it again with the PIN its
//! screen shows, typed in the Device tab. A bond made without that PIN (KDE settings, or no
//! agent) is dropped by the radio a few seconds after each connection.
//!
//! Author: O. Booklage
//! Date: November 2026
//! Licence: GPL-3.0

use std::sync::mpsc::Sender;
use std::time::Duration;

use bluer::agent::{Agent, ReqError};
use bluer::{Adapter, Address};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::oneshot;

use super::link::is_device;
use super::{FromRadio, Pairing, ToRadio};

/// How long the radio has to show up in the scan.
const LOOK_TIME: Duration = Duration::from_secs(60);

/// A MeshCore or Meshtastic radio, from its BLE name.
pub(super) fn is_radio(name: &str) -> bool {
    name.starts_with("MeshCore-") || name.starts_with("Meshtastic")
}

/// Pair `device` (a name or an address), or the first radio found; returns its name. The
/// PIN comes from the UI as a ToRadio::Pin on `cmds`.
pub(super) async fn pair(
    device: Option<&str>,
    tx: &Sender<FromRadio>,
    cmds: &mut UnboundedReceiver<ToRadio>,
) -> anyhow::Result<String> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    // The agent BlueZ asks for the PIN; it passes each request on to the loop below.
    let (ask_tx, mut asks) = mpsc::unbounded_channel::<oneshot::Sender<Option<u32>>>();
    let agent = Agent {
        request_default: true,
        request_passkey: Some(Box::new(move |_| {
            let ask_tx = ask_tx.clone();
            Box::pin(async move {
                let (reply, pin) = oneshot::channel();
                ask_tx.send(reply).map_err(|_| ReqError::Canceled)?;
                pin.await.ok().flatten().ok_or(ReqError::Rejected)
            })
        })),
        ..Default::default()
    };
    let _agent = session.register_agent(agent).await?;
    // Scans as long as it lives.
    let _scan = adapter.discover_devices().await?;
    let _ = tx.send(FromRadio::Pairing(Pairing::Running));
    let (addr, name) = find(&adapter, device).await?.ok_or_else(|| {
        anyhow::anyhow!("no MeshCore or Meshtastic radio found: is it on, and near enough?")
    })?;
    // An old bond goes first: the radio would not take a new one on top of it.
    if adapter.device(addr)?.is_paired().await? {
        tracing::info!("pairing: removing the old bond of {name}");
        adapter.remove_device(addr).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        // The scan must see it again before it can be paired.
        find(&adapter, Some(&addr.to_string()))
            .await?
            .ok_or_else(|| anyhow::anyhow!("{name} did not show again after its bond was removed"))?;
    }
    let radio = adapter.device(addr)?;
    let pairing = radio.pair();
    tokio::pin!(pairing);
    let mut waiting = None;
    loop {
        tokio::select! {
            done = &mut pairing => {
                done?;
                break;
            }
            Some(reply) = asks.recv() => {
                waiting = Some(reply);
                let _ = tx.send(FromRadio::Pairing(Pairing::Pin(name.clone())));
            }
            cmd = cmds.recv() => match cmd {
                Some(ToRadio::Pin(pin)) => {
                    if let Some(reply) = waiting.take() {
                        let _ = reply.send(pin);
                    }
                }
                Some(ToRadio::Pair) => {} // already pairing
                Some(cmd) => {
                    let _ = tx.send(cmd.failed("pairing in progress".into()));
                }
                None => anyhow::bail!("interface gone"),
            }
        }
    }
    radio.set_trusted(true).await?;
    Ok(name)
}

/// The address and name of `device` (a name or an address), or of the first radio, once
/// BlueZ knows it; None after LOOK_TIME.
async fn find(adapter: &Adapter, device: Option<&str>) -> anyhow::Result<Option<(Address, String)>> {
    let end = tokio::time::Instant::now() + LOOK_TIME;
    while tokio::time::Instant::now() < end {
        for addr in adapter.device_addresses().await? {
            let name = adapter.device(addr)?.name().await?;
            let found = match device {
                Some(_) => is_device(device, name.as_deref(), &addr.to_string()),
                None => name.as_deref().is_some_and(is_radio),
            };
            if found {
                return Ok(Some((addr, name.unwrap_or_else(|| addr.to_string()))));
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(None)
}
