//! ui/radio_card.rs
//! The card of my own radio: model, firmware, battery, radio settings, position and link;
//! and the pairing, with or without a radio.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use ratatui::text::Line;

use super::card::field;
use super::format::{coord, hhmm, khz, uptime, LOW_BATTERY};
use super::{alert, faint, text, Theme};
use crate::app::App;
use crate::radio::{Firmware, Link, MyRadio, Pairing};

/// What my radio says about itself, its battery and the link.
pub(super) fn radio_lines<'a>(app: &App, radio: &MyRadio, theme: Theme) -> Vec<Line<'a>> {
    let or_dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
    let fw = match (&radio.version, &radio.build) {
        (Some(v), Some(b)) => format!("{v} ({b})"),
        (Some(x), None) | (None, Some(x)) => x.clone(),
        (None, None) => "-".into(),
    };
    let mut lines = vec![
        field(theme, "MODEL", or_dash(&radio.model), text(theme)),
        field(theme, "FW", fw, text(theme)),
    ];
    lines.push(match app.battery {
        Some(b) => field(
            theme,
            "BAT",
            // The percentage is in the banner already, with its gauge.
            format!("{:.2} V", f64::from(b.mv) / 1000.0),
            if b.pct < LOW_BATTERY {
                alert(theme)
            } else {
                text(theme)
            },
        ),
        None => field(theme, "BAT", "-".into(), faint(theme)),
    });
    if let Some((used, total)) = app.battery.and_then(|b| b.used_kb.zip(b.total_kb)) {
        lines.push(field(
            theme,
            "STORE",
            format!("{used} / {total} KB"),
            text(theme),
        ));
    }
    // Meshtastic on a preset gives no frequency, only the region and preset names.
    if radio.freq_khz != 0 {
        lines.push(field(
            theme,
            "FREQ",
            format!("{:.3} MHz", f64::from(radio.freq_khz) / 1000.0),
            text(theme),
        ));
    }
    let lora = match &radio.preset {
        Some(preset) => preset.clone(),
        None => format!(
            "BW {} kHz SF {} CR {}",
            khz(radio.bw_hz),
            radio.sf,
            radio.cr
        ),
    };
    lines.push(field(theme, "LORA", lora, text(theme)));
    let max = if radio.max_tx_power > 0 {
        format!(" (max {})", radio.max_tx_power)
    } else {
        String::new()
    };
    lines.push(field(
        theme,
        "TX",
        format!("{} dBm{max}", radio.tx_power),
        text(theme),
    ));
    // Shown as MeshVox does: no line when the firmware does not know regions.
    match &radio.region {
        Some(Some(name)) => lines.push(field(theme, "REGION", name.clone(), text(theme))),
        Some(None) => lines.push(field(theme, "REGION", "NONE".into(), faint(theme))),
        None => {}
    }
    lines.push(match app.self_pos {
        Some(p) => field(theme, "POS", coord(p.lat, p.lon), text(theme)),
        None => field(theme, "POS", "UNKNOWN".into(), faint(theme)),
    });
    // Whether my adverts carry the position, as MeshVox's "Position in adverts".
    lines.push(field(
        theme,
        "ADVERT",
        if radio.shares_position {
            "POSITION SHARED".into()
        } else {
            "POSITION HIDDEN".into()
        },
        text(theme),
    ));
    if let Some((flood, ts)) = app.advert {
        let to = if flood { "ALL" } else { "NEIGHBOURS" };
        lines.push(field(theme, "SENT", format!("TO {to} {}", hhmm(ts)), text(theme)));
    }
    if app.firmware == Firmware::MeshCore {
        // Who may read my telemetry, as MeshVox's switch: every node or no one.
        let telemetry = if radio.telemetry { "FOR ALL" } else { "FOR NO ONE" };
        lines.push(field(theme, "TELEM", telemetry.into(), text(theme)));
    }
    if let Some(bytes) = radio.path_hash {
        let unit = if bytes == 1 { "BYTE" } else { "BYTES" };
        lines.push(field(theme, "HASH", format!("{bytes} {unit} PER RELAY"), text(theme)));
    }
    // Whether the GPS receiver looks for satellites.
    lines.push(match radio.gps {
        Some(true) => field(theme, "GPS", "ON".into(), text(theme)),
        Some(false) => field(theme, "GPS", "OFF".into(), text(theme)),
        None => field(theme, "GPS", "-".into(), faint(theme)),
    });
    // shortcut: the maximum is not shown; meshcore-rs 0.2.0 caps the firmware's value at 255.
    lines.push(field(
        theme,
        "NODES",
        app.nodes.len().to_string(),
        text(theme),
    ));
    lines.push(match (&app.link, app.connected_since) {
        (Link::Connected(name), Some(since)) => field(
            theme,
            "LINK",
            format!("{name}  {}", uptime(since.elapsed().as_secs() as u32)),
            text(theme),
        ),
        _ => field(theme, "LINK", "DOWN".into(), alert(theme)),
    });
    lines
}

/// The setting being typed: its value so far, what to type and the keys, in the alert color
/// like the PIN.
pub(super) fn editing_lines<'a>(app: &App, theme: Theme) -> Vec<Line<'a>> {
    let Some(setting) = app.editing else {
        // The setting keys: the footer has no room for them on 80 columns.
        let settable = app.me.is_some() && app.firmware == Firmware::MeshCore;
        return match settable {
            true => vec![
                Line::default(),
                field(theme, "SET", "n name   t TX   l LoRa   e region   h path hash".into(), faint(theme)),
                field(theme, "", "o position   g GPS   s share position   m telemetry".into(), faint(theme)),
                field(theme, "", "u publish on map.meshcore.io".into(), faint(theme)),
            ],
            false => vec![],
        };
    };
    vec![
        Line::default(),
        field(theme, setting.label(), format!("> {}_", app.edit), alert(theme)),
        field(theme, "", setting.hint().into(), faint(theme)),
        field(theme, "", "Enter: save   Esc: cancel".into(), faint(theme)),
    ]
}

/// Where the pairing stands. When it asks for the PIN: what to type, where to read it and
/// what is typed so far, in the alert color, to catch the eye.
pub(super) fn pairing_lines<'a>(app: &App, theme: Theme) -> Vec<Line<'a>> {
    match &app.pairing {
        Some(Pairing::Running) => vec![field(theme, "PAIR", "IN PROGRESS...".into(), text(theme))],
        Some(Pairing::Pin(name)) => {
            let ask = match theme {
                Theme::Normal => format!("Type the PIN shown on the screen of {name}"),
                Theme::Hacker => format!("ENTER PIN FROM {name} SCREEN"),
            };
            vec![
                Line::default(),
                field(theme, "PIN", ask, alert(theme)),
                field(theme, "", format!("> {}_", app.pin), alert(theme)),
                field(theme, "", "Enter: pair   Esc: cancel".into(), faint(theme)),
            ]
        }
        Some(Pairing::Paired(name, ts)) => vec![field(
            theme,
            "PAIR",
            format!("PAIRED {name} {}", hhmm(*ts)),
            text(theme),
        )],
        Some(Pairing::Failed(_)) | None => vec![], // a failure is in the notice
    }
}
