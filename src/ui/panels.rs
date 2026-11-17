//! ui/panels.rs
//! Header banner, contact list and key footer.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::format::{gauge, spinner, LOW_BATTERY};
use super::{alert, faint, panel, strong, text, Theme};
use crate::app::{App, Tab};
use crate::radio::{Firmware, Link};

/// The banner: link state, battery and node count, or the current notice.
pub(super) fn draw_header(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let low = app.battery.is_some_and(|b| b.pct < LOW_BATTERY);
    let status = match (theme, &app.notice, &app.link) {
        (Theme::Normal, Some(notice), _) => Span::styled(notice.clone(), alert(theme)),
        (Theme::Hacker, Some(notice), _) => {
            Span::styled(format!(">> ERR :: {notice}"), alert(theme))
        }
        (Theme::Normal, None, Link::Connected(name)) => {
            let battery = app
                .battery
                .map(|b| format!(" - battery {} % {}", b.pct, gauge(b.pct, app.ascii)))
                .unwrap_or_default();
            let line = format!(
                "Connected to {name} ({}){battery} - {} contacts",
                app.firmware.name(),
                app.nodes.len()
            );
            Span::styled(
                line,
                if low {
                    alert(theme)
                } else {
                    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
                },
            )
        }
        (Theme::Normal, None, Link::Searching(name)) => Span::styled(
            format!("Searching for {name}... (is a phone connected to it?)"),
            Style::new().fg(Color::Yellow),
        ),
        (Theme::Normal, None, Link::Lost(reason)) => Span::styled(
            format!("Link lost: {reason} - retrying"),
            Style::new().fg(Color::Yellow),
        ),
        (Theme::Hacker, None, Link::Connected(name)) => {
            let power = app
                .battery
                .map(|b| format!(" :: PWR {} {}%", gauge(b.pct, app.ascii), b.pct))
                .unwrap_or_default();
            Span::styled(
                format!(
                    ">> LINK ESTABLISHED :: {name} :: {}{power} :: {} NODES",
                    app.firmware.name().to_uppercase(),
                    app.nodes.len()
                ),
                if low { alert(theme) } else { strong(theme) },
            )
        }
        (Theme::Hacker, None, Link::Searching(name)) => Span::styled(
            format!(
                ">> SCANNING FOR {name} {} :: IS A PHONE CONNECTED TO IT?",
                spinner()
            ),
            strong(theme),
        ),
        (Theme::Hacker, None, Link::Lost(reason)) => Span::styled(
            format!(">> LINK LOST :: {reason} :: RETRYING"),
            strong(theme),
        ),
    };
    let line = match theme {
        Theme::Normal => Line::from(status),
        Theme::Hacker => Line::from(vec![
            status,
            Span::styled(
                if app.ascii { " _" } else { " █" },
                text(theme).add_modifier(Modifier::SLOW_BLINK),
            ),
        ]),
    };
    let block = match theme {
        Theme::Normal => panel(
            theme,
            app.ascii,
            false,
            "meshvox-term  MeshCore and Meshtastic over Bluetooth",
        ),
        Theme::Hacker => {
            let clock = chrono::Local::now().format(" %H:%M:%S ").to_string();
            panel(theme, app.ascii, false, "MESHVOX-TERM")
                .title(Line::styled(clock, text(theme)).right_aligned())
        }
    };
    frame.render_widget(Paragraph::new(line).block(block), area);
}

/// The keys of the open tab, Normal names then Hacker names.
fn keys(app: &App) -> Vec<(&'static str, &'static str, &'static str)> {
    let arrows = if app.ascii { "up/down" } else { "↑↓" };
    let sideways = if app.ascii { "left/right" } else { "←→" };
    let mut keys = match (app.tab, app.typing) {
        (Tab::Chats, true) => vec![
            ("Enter", "send", "TRANSMIT"),
            ("Esc", "leave input", "ABORT"),
            ("PgUp/PgDn", "scroll", "SCROLLBACK"),
        ],
        (Tab::Chats, false) => vec![
            (arrows, "chat", "CHANNEL"),
            ("Enter", "write", "COMPOSE"),
            ("PgUp/PgDn", "scroll", "SCROLLBACK"),
        ],
        (Tab::Contacts, _) => vec![
            (arrows, "move", "SELECT"),
            (sideways, "filter", "FILTER"),
            ("Enter", "write", "COMPOSE"),
            ("p", "ping", "PING"),
        ],
        (Tab::Map, _) => vec![
            ("arrows", "pan", "PAN"),
            ("+/-", "zoom", "ZOOM"),
            ("[ ]", "node", "NODE"),
            ("p", "ping", "PING"),
        ],
        (Tab::Device, _) if app.firmware == Firmware::MeshCore => vec![
            ("a", "advert to neighbours", "ADVERT ZERO-HOP"),
            ("f", "advert to all", "ADVERT FLOOD"),
        ],
        (Tab::Device, _) => vec![],
    };
    if !app.typing {
        keys.extend([("1-4", "tabs", "TABS"), ("q", "quit", "EXIT")]);
    }
    keys
}

/// The keys line at the bottom.
pub(super) fn draw_footer(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let spans: Vec<Span> = keys(app)
        .into_iter()
        .flat_map(|(k, normal, hacker)| match theme {
            Theme::Normal => [
                Span::styled(
                    format!(" {k} "),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{normal}  ")),
            ],
            Theme::Hacker => [
                // "[ ]" is in brackets already.
                Span::styled(
                    match k.starts_with('[') {
                        true => format!(" {k}"),
                        false => format!(" [{}]", k.to_uppercase()),
                    },
                    strong(theme),
                ),
                Span::styled(format!(" {hacker} "), faint(theme)),
            ],
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)).style(text(theme)), area);
}
