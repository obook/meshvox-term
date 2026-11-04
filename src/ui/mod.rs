//! ui/mod.rs
//! Drawing, in two looks: Normal (rounded boxes, cyan accent, like sccast) and Hacker
//! (phosphor green on black, radar map). This file holds the colors, the styles and the
//! layout of the four tabs (Chats, Contacts, Map, Device, as in the MeshVox app); the other
//! files draw one panel each.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

mod card;
mod chats;
mod contacts;
mod conversation;
mod format;
mod map_panel;
mod menu;
mod panels;
mod radar;
mod radio_card;
#[cfg(test)]
mod tests;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};
use card::draw_card;
use chats::draw_chats;
use contacts::draw_contacts;
use conversation::draw_conversation;
use map_panel::draw_map;
pub use map_panel::MapState;
pub use menu::{draw_boot, draw_menu, menu_key, Menu, BOOT};
use panels::{draw_footer, draw_header};
use radio_card::radio_lines;

const GREEN: Color = Color::Rgb(0, 255, 65);
const DIM: Color = Color::Rgb(0, 140, 40);
const RED: Color = Color::Rgb(255, 60, 60);
const BLACK: Color = Color::Rgb(0, 0, 0);
const CYAN: Color = Color::Cyan;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Theme {
    Normal,
    Hacker,
}

fn text(theme: Theme) -> Style {
    match theme {
        Theme::Normal => Style::new(),
        Theme::Hacker => Style::new().fg(GREEN).bg(BLACK),
    }
}

fn faint(theme: Theme) -> Style {
    match theme {
        Theme::Normal => Style::new().fg(Color::DarkGray),
        Theme::Hacker => Style::new().fg(DIM).bg(BLACK),
    }
}

fn strong(theme: Theme) -> Style {
    match theme {
        Theme::Normal => Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        Theme::Hacker => text(theme).add_modifier(Modifier::BOLD),
    }
}

fn alert(theme: Theme) -> Style {
    match theme {
        Theme::Normal => Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        Theme::Hacker => Style::new().fg(RED).bg(BLACK).add_modifier(Modifier::BOLD),
    }
}

/// Normal: rounded box, " title ", cyan when focused. Hacker: square box, "[ TITLE ]".
/// The Linux console (`ascii`) has no rounded corners: square boxes there.
fn panel(theme: Theme, ascii: bool, focused: bool, title: &str) -> Block<'static> {
    let corners = if ascii {
        BorderType::Plain
    } else {
        BorderType::Rounded
    };
    match theme {
        Theme::Normal => Block::bordered()
            .border_type(corners)
            .border_style(Style::new().fg(if focused { CYAN } else { Color::DarkGray }))
            .title(format!(" {title} ")),
        Theme::Hacker => Block::bordered()
            .border_type(BorderType::Plain)
            .border_style(if focused { text(theme) } else { faint(theme) })
            .style(text(theme))
            .title(Span::styled(format!("[ {title} ]"), strong(theme))),
    }
}

pub fn draw(frame: &mut Frame, app: &App, map: &mut MapState, theme: Theme) {
    if theme == Theme::Hacker {
        frame.render_widget(Block::new().style(text(theme)), frame.area());
    }
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    draw_header(frame, app, header, theme);
    draw_tabs(frame, app, tabs, theme);
    match app.tab {
        Tab::Chats => {
            let [list, conversation] =
                Layout::horizontal([Constraint::Length(36), Constraint::Min(0)]).areas(body);
            draw_chats(frame, app, list, theme);
            draw_conversation(frame, app, conversation, theme);
        }
        Tab::Contacts => {
            let [list, card] =
                Layout::horizontal([Constraint::Length(36), Constraint::Min(0)]).areas(body);
            draw_contacts(frame, app, list, theme);
            draw_card(frame, app, card, theme);
        }
        Tab::Map => {
            let [map_area, card] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(32)]).areas(body);
            draw_map(frame, app, map_area, map, theme);
            draw_card(frame, app, card, theme);
        }
        Tab::Device => draw_device(frame, app, body, theme),
    }
    draw_footer(frame, app, footer, theme);
}

/// The tab bar under the banner; each tab with the key that opens it.
fn draw_tabs(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let unread: usize = app.unread.values().sum();
    let titles = Tab::ALL.iter().enumerate().map(|(i, &tab)| {
        let name = match (tab, theme) {
            (Tab::Chats, _) if unread > 0 => format!("Chats ({unread})"),
            (Tab::Chats, _) => "Chats".into(),
            (Tab::Contacts, Theme::Normal) => "Contacts".into(),
            (Tab::Contacts, Theme::Hacker) => "Nodes".into(),
            (Tab::Map, Theme::Normal) => "Map".into(),
            (Tab::Map, Theme::Hacker) => "Radar".into(),
            (Tab::Device, _) => "Device".into(),
        };
        let name = match theme {
            Theme::Normal => name,
            Theme::Hacker => name.to_uppercase(),
        };
        Line::from(format!("{} {name}", i + 1))
    });
    let selected = Tab::ALL.iter().position(|&t| t == app.tab);
    let highlight = match theme {
        Theme::Normal => Style::new()
            .fg(BLACK)
            .bg(CYAN)
            .add_modifier(Modifier::BOLD),
        Theme::Hacker => Style::new()
            .fg(BLACK)
            .bg(GREEN)
            .add_modifier(Modifier::BOLD),
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .style(faint(theme))
        .highlight_style(highlight)
        .divider(if app.ascii { "|" } else { "│" });
    frame.render_widget(tabs, area);
}

/// The Device tab: my radio's card, or why there is none yet.
fn draw_device(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let (title, lines) = match &app.me {
        Some(radio) => (
            match theme {
                Theme::Normal => format!("My radio: {}", format::ascii_safe(&radio.name, app.ascii)),
                Theme::Hacker => format!("RADIO :: {}", format::ascii_safe(&radio.name, app.ascii)),
            },
            radio_lines(app, radio, theme),
        ),
        None => (
            match theme {
                Theme::Normal => "My radio".to_string(),
                Theme::Hacker => "RADIO".to_string(),
            },
            vec![Line::styled("No radio connected yet", faint(theme))],
        ),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(text(theme))
            .block(panel(theme, app.ascii, true, &title)),
        area,
    );
}
