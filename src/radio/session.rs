//! radio/session.rs
//! One connection to the radio: start-up, then a loop on radio events, interface commands
//! and a periodic check. Every received message is written to disk before it is shown.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::Sender;

use futures::StreamExt;
use meshcore_rs::commands::Destination;
use meshcore_rs::events::{EventPayload, EventType, LogData};
use meshcore_rs::PayloadType;
use meshcore_rs::parsing::hex_encode;
use meshcore_rs::MeshCore;
use tokio::sync::mpsc::UnboundedReceiver;

use super::convert::{
    change_frame, channel_conv, channel_msg, channel_tag, contact_msg, group_text, gps_from_vars, gps_switch_from_vars, lipo_percent, map_body, map_data,
    my_radio, region_from_frame, CMD_GET_DEFAULT_REGION, MAP_UPLOAD_URL,
};
use super::link::now;
use super::ping::ping;
use super::{
    Battery, Change, Firmware, FromRadio, HistoryWriteFailed, Link, MyRadio, Node, PairAsked,
    ToRadio,
};
use crate::config::{CHECK_EVERY, CONTACTS_TIMEOUT};
use crate::geo::{pos_from_micro, Pos};
use crate::history::{self, Msg, Record, PUBLIC};

/// One connection. Ok(true): the link dropped; Ok(false): the UI is gone.
pub(super) async fn session(
    radio: &MeshCore,
    name: &str,
    history: &Path,
    tx: &Sender<FromRadio>,
    cmds: &mut UnboundedReceiver<ToRadio>,
) -> anyhow::Result<bool> {
    // Subscribe first, so no push is missed while starting.
    let mut events = radio.event_stream();
    let (card, pos) = read_card(radio).await?;
    announce(tx, Firmware::MeshCore, card, pos, name);
    // The firmware refuses to move its clock back (it answers an error, which meshcore-rs only
    // sees as a timeout); a GPS radio is often a second ahead. A clock not set is no link failure.
    let device_time = radio.commands().lock().await.get_time().await?;
    if device_time < now() {
        if let Err(e) = radio.commands().lock().await.set_time(now()).await {
            tracing::info!("radio clock not set: {e}");
        }
    }
    battery(radio, tx).await?;
    let mut names = contacts(radio, tx).await?;
    drain(radio, history, tx, &names).await?;
    // Names and repeat counts are a comfort: a radio that does not give its channels still works.
    let secrets = channels(radio, tx).await.unwrap_or_else(|e| {
        tracing::info!("channels: {e}");
        HashMap::new()
    });
    // My channel messages of this session, by tag: the repeats heard are matched against them.
    let mut ours: HashMap<String, String> = HashMap::new();

    let mut tick = tokio::time::interval(CHECK_EVERY);
    tick.tick().await;
    // Reloading all contacts takes 60 ms per contact: once per check, not once per advert.
    let mut contacts_stale = false;
    loop {
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { return Ok(true) };
                match event.event_type {
                    EventType::MessagesWaiting => drain(radio, history, tx, &names).await?,
                    EventType::Advertisement | EventType::NewContact | EventType::PathUpdate => {
                        contacts_stale = true;
                    }
                    EventType::Ack => {
                        if let EventPayload::Ack { tag } = event.payload {
                            save(history, tx, Record::Ack { tag: hex_encode(&tag) })?;
                        }
                    }
                    EventType::LogData => {
                        if let EventPayload::LogData(log) = &event.payload {
                            heard(log, &secrets, &ours, history, tx)?;
                        }
                    }
                    EventType::Disconnected => return Ok(true),
                    _ => {}
                }
            }
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { return Ok(false) };
                if cmd == ToRadio::Pair {
                    return Err(PairAsked.into());
                }
                if let ToRadio::Ping { key, prefix } = &cmd {
                    ping(radio, key, prefix, tx);
                    continue;
                }
                if cmd == ToRadio::Refresh {
                    // Here, not in send(): the names of the senders follow the new list.
                    let _ = tx.send(match contacts(radio, tx).await {
                        Ok(list) => {
                            names = list;
                            contacts_stale = false;
                            FromRadio::Notice("Contacts refreshed".into())
                        }
                        Err(e) => cmd.failed(format!("{e:#}")),
                    });
                    continue;
                }
                if let Err(e) = send(radio, history, tx, &cmd, &mut ours).await {
                    // Sent but not saved: the message is shown, do not invite a resend.
                    if e.is::<HistoryWriteFailed>() {
                        return Err(e);
                    }
                    let _ = tx.send(cmd.failed(format!("{e:#}")));
                }
            }
            _ = tick.tick() => {
                battery(radio, tx).await?;
                if std::mem::take(&mut contacts_stale) {
                    names = contacts(radio, tx).await?;
                }
                // A MessagesWaiting push lost while the loop was busy must not strand messages.
                drain(radio, history, tx, &names).await?;
            }
        }
    }
}

/// What my radio says about itself, and its position: read at the start of a session and
/// after each setting change, to show what the radio kept.
async fn read_card(radio: &MeshCore) -> anyhow::Result<(MyRadio, Option<Pos>)> {
    let me = radio.commands().lock().await.send_appstart().await?;
    let dev = radio.commands().lock().await.send_device_query().await;
    if let Err(e) = &dev {
        tracing::info!("device query: {e}");
    }
    let mut card = my_radio(&me, dev.as_ref().ok());
    // An older firmware may not answer: the GPS state is then unknown, not a link failure.
    match radio.commands().lock().await.get_custom_vars().await {
        Ok(vars) => {
            card.gps = gps_from_vars(&vars);
            card.gps_switch = gps_switch_from_vars(&vars);
        }
        Err(e) => tracing::info!("custom vars: {e}"),
    }
    // meshcore-rs passes the unknown answer on as raw bytes; an older firmware answers an error.
    let region = radio
        .commands()
        .lock()
        .await
        .send_multi(
            &[CMD_GET_DEFAULT_REGION],
            &[EventType::Unknown, EventType::Error],
            meshcore_rs::commands::DEFAULT_TIMEOUT,
        )
        .await;
    match region {
        Ok(event) => {
            if let EventPayload::Bytes(frame) = event.payload {
                card.region = region_from_frame(&frame);
            }
        }
        Err(e) => tracing::info!("default region: {e}"),
    }
    Ok((card, pos_from_micro(me.adv_lat, me.adv_lon)))
}

/// Write one setting to the radio. An Error reply is a refusal, not a timeout.
async fn set(radio: &MeshCore, change: &Change) -> anyhow::Result<()> {
    let cmd = radio.commands().lock().await;
    let me = cmd.send_appstart().await?;
    let reply = cmd
        .send_multi(
            &change_frame(change, &me),
            &[EventType::Ok, EventType::Error],
            meshcore_rs::commands::DEFAULT_TIMEOUT,
        )
        .await?;
    anyhow::ensure!(
        reply.event_type == EventType::Ok,
        "the radio refused the value"
    );
    Ok(())
}

/// Publish my radio on map.meshcore.io, as the MeshVox app does: the radio exports its own
/// advert and signs the upload, so its private key never leaves it. Returns the map's answer.
async fn map_upload(radio: &MeshCore) -> anyhow::Result<()> {
    use sha2::Digest;
    let body = {
        let cmd = radio.commands().lock().await;
        let me = cmd.send_appstart().await?;
        // The map needs a position, and the official app wants it shared in the adverts.
        anyhow::ensure!(
            me.adv_loc_policy != 0 && (me.adv_lat, me.adv_lon) != (0, 0),
            "set a position and share it in the adverts first"
        );
        let advert = cmd.export_contact(None::<Destination>).await?;
        let data = map_data(&me, &advert);
        let signature = cmd.sign(&sha2::Sha256::digest(&data), 120).await?;
        map_body(&data, &signature, &me.public_key)
    };
    // ureq blocks: off the async threads, like the tile downloads.
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .http_status_as_error(false)
            .build()
            .into();
        // Same content type as the reference uploader.
        let mut reply = agent
            .post(MAP_UPLOAD_URL)
            .header("Content-Type", "text/plain;charset=UTF-8")
            .send(body)?;
        let status = reply.status();
        if status.is_success() {
            return Ok(());
        }
        let text = reply.body_mut().read_to_string().unwrap_or_default();
        // The server answers JSON {"message": ...}; only its message is worth showing.
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|json| json["message"].as_str().map(str::to_string))
            .unwrap_or_else(|| text.chars().take(160).collect());
        anyhow::bail!("the map answered {status}: {message}")
    })
    .await?
}

/// The start of a session, in this order: the firmware first, so the interface drops what it
/// knew of the other firmware before it gets this radio's card.
pub(super) fn announce(
    tx: &Sender<FromRadio>,
    firmware: Firmware,
    me: MyRadio,
    pos: Option<Pos>,
    name: &str,
) {
    let _ = tx.send(FromRadio::Firmware(firmware));
    let _ = tx.send(FromRadio::MyRadio(me));
    let _ = tx.send(FromRadio::SelfPos(pos));
    let _ = tx.send(FromRadio::Link(Link::Connected(name.to_string())));
}

/// Read the battery; doubles as the link check, since a dead link makes it fail.
pub(super) async fn battery(radio: &MeshCore, tx: &Sender<FromRadio>) -> anyhow::Result<()> {
    let info = radio.commands().lock().await.get_bat().await?;
    let _ = tx.send(FromRadio::Battery(Battery {
        pct: lipo_percent(info.battery_mv),
        mv: info.battery_mv,
        used_kb: info.used_kb,
        total_kb: info.total_kb,
    }));
    Ok(())
}

/// Send the contact list to the UI; return prefix -> name.
pub(super) async fn contacts(
    radio: &MeshCore,
    tx: &Sender<FromRadio>,
) -> anyhow::Result<HashMap<String, String>> {
    let list = radio
        .commands()
        .lock()
        .await
        .get_contacts_with_timeout(0, CONTACTS_TIMEOUT)
        .await?;
    let nodes: Vec<Node> = list.iter().map(Node::from).collect();
    let names = nodes
        .iter()
        .map(|n| (n.prefix.clone(), n.name.clone()))
        .collect();
    let _ = tx.send(FromRadio::Nodes(nodes));
    Ok(names)
}

/// Send the names of the radio's channels to the UI (the public one keeps its own); return
/// the secret of each channel, by index. An empty slot has no name.
async fn channels(radio: &MeshCore, tx: &Sender<FromRadio>) -> anyhow::Result<HashMap<u8, [u8; 16]>> {
    // Past max_channels the firmware answers an error, which meshcore-rs only sees as a timeout.
    let dev = radio.commands().lock().await.send_device_query().await?;
    let mut names = HashMap::new();
    let mut secrets = HashMap::new();
    for idx in 0..dev.max_channels.unwrap_or(0) {
        let info = radio.commands().lock().await.get_channel(idx).await?;
        if !info.name.is_empty() {
            secrets.insert(idx, info.secret);
            if idx > 0 {
                names.insert(channel_conv(idx), info.name);
            }
        }
    }
    let _ = tx.send(FromRadio::Channels(names));
    Ok(secrets)
}

/// Count a repeat when a packet the radio heard is one of my channel messages, relayed.
fn heard(
    log: &LogData,
    secrets: &HashMap<u8, [u8; 16]>,
    ours: &HashMap<String, String>,
    history: &Path,
    tx: &Sender<FromRadio>,
) -> anyhow::Result<()> {
    if !matches!(&log.header, Some(h) if matches!(h.payload_type, PayloadType::GroupText)) {
        return Ok(());
    }
    for (&channel, secret) in secrets {
        let Some((ts, text)) = group_text(&log.payload, secret) else {
            continue;
        };
        let tag = channel_tag(channel, ts);
        // The radio sends "name: text".
        if ours.get(&tag).is_some_and(|mine| text.ends_with(&format!(": {mine}"))) {
            save(history, tx, Record::Repeat { tag })?;
        }
    }
    Ok(())
}

/// Fetch every waiting message. The radio forgets a message once fetched, so each one is
/// written to disk before the next fetch.
pub(super) async fn drain(
    radio: &MeshCore,
    history: &Path,
    tx: &Sender<FromRadio>,
    names: &HashMap<String, String>,
) -> anyhow::Result<()> {
    loop {
        let Some(event) = radio.commands().lock().await.get_msg().await? else {
            return Ok(());
        };
        let msg = match &event.payload {
            EventPayload::ContactMessage(m) => contact_msg(m, names),
            EventPayload::ChannelMessage(m) => channel_msg(m),
            _ => continue,
        };
        save(history, tx, Record::Msg(msg))?;
    }
}

/// Write then report. On a write failure the record is still shown, and the error stops
/// the session so no more messages leave the radio.
pub(super) fn save(history: &Path, tx: &Sender<FromRadio>, rec: Record) -> anyhow::Result<()> {
    let written = history::append(history, &rec);
    let _ = tx.send(FromRadio::Record(rec));
    Ok(written.map_err(HistoryWriteFailed)?)
}

/// Send a direct or public message, then save it as sent; or send an advert.
pub(super) async fn send(
    radio: &MeshCore,
    history: &Path,
    tx: &Sender<FromRadio>,
    cmd: &ToRadio,
    ours: &mut HashMap<String, String>,
) -> anyhow::Result<()> {
    let ts = chrono::Utc::now().timestamp();
    let msg = match cmd {
        ToRadio::SendDirect { prefix, text } => {
            let info = radio
                .commands()
                .lock()
                .await
                .send_msg(Destination::Hex(prefix.clone()), text, None)
                .await?;
            Msg {
                ts,
                conv: prefix.clone(),
                from: "me".into(),
                text: text.clone(),
                outgoing: true,
                ack: Some(hex_encode(&info.expected_ack)),
            }
        }
        ToRadio::SendChannel { channel, text } => {
            // My timestamp, not the radio's: the repeats heard carry it.
            let sent_at = ts as u32;
            radio
                .commands()
                .lock()
                .await
                .send_channel_msg(*channel, text, Some(sent_at))
                .await?;
            let tag = channel_tag(*channel, sent_at);
            // Repeats come within seconds: an old message no longer needs its entry.
            if ours.len() >= 64 {
                ours.clear();
            }
            ours.insert(tag.clone(), text.clone());
            Msg {
                ts,
                conv: match channel {
                    0 => PUBLIC.into(),
                    n => channel_conv(*n),
                },
                from: "me".into(),
                text: text.clone(),
                outgoing: true,
                ack: Some(tag),
            }
        }
        // Ping and Refresh: handled by the session loop; Pair: ends the session; Pin: only
        // while pairing.
        ToRadio::Ping { .. } | ToRadio::Refresh | ToRadio::Pair | ToRadio::Pin(_) => return Ok(()),
        ToRadio::Set(change) => {
            set(radio, change).await?;
            let _ = tx.send(FromRadio::Done(None));
            let (card, pos) = read_card(radio).await?;
            let _ = tx.send(FromRadio::MyRadio(card));
            let _ = tx.send(FromRadio::SelfPos(pos));
            return Ok(());
        }
        ToRadio::MapUpload => {
            map_upload(radio).await?;
            let _ = tx.send(FromRadio::Done(None));
            return Ok(());
        }
        ToRadio::Reboot => {
            // No reply: the link drops and the task reconnects.
            radio.commands().lock().await.reboot().await?;
            return Ok(());
        }
        ToRadio::Advert { flood } => {
            radio.commands().lock().await.send_advert(*flood).await?;
            let _ = tx.send(FromRadio::Advert {
                flood: *flood,
                error: None,
            });
            return Ok(());
        }
    };
    save(history, tx, Record::Msg(msg))
}
