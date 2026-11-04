//! ui/chats.rs
//! The conversation list of the Chats tab: the public channel, then the conversations,
//! latest first, each with its last message and its unread count.
//!
//! Author: O. Booklage
//! Date: November 2026
//! Licence: GPL-3.0

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use super::format::{ascii_safe, cells};
use super::{faint, panel, strong, text, Theme, BLACK, GREEN};
use crate::app::App;

pub(super) fn draw_chats(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let chats = app.chats();
    // Inside the borders and the selection mark.
    let width = usize::from(area.width).saturating_sub(4);
    let items: Vec<ListItem> = chats
        .iter()
        .map(|r| {
            let unread = if r.unread > 0 {
                format!(" {}", r.unread)
            } else {
                String::new()
            };
            let label = match theme {
                Theme::Hacker if r.conv == app.public_conv() => "#PUBLIC".to_string(),
                _ => ascii_safe(&r.label, app.ascii).into_owned(),
            };
            let label = cells(&label, width.saturating_sub(unread.len()));
            let name = Line::from(vec![
                // Unread or online: bold.
                Span::styled(
                    label,
                    if r.unread > 0 || app.is_online(&r.conv) {
                        strong(theme)
                    } else {
                        text(theme)
                    },
                ),
                Span::styled(unread, strong(theme)),
            ]);
            let last = Line::styled(
                cells(&ascii_safe(&r.detail, app.ascii), width),
                faint(theme),
            );
            ListItem::new(vec![name, last])
        })
        .collect();
    let (symbol, highlight) = match theme {
        Theme::Normal => (
            if app.ascii { "> " } else { "▶ " },
            Style::new()
                .bg(Color::Rgb(0, 60, 80))
                .add_modifier(Modifier::BOLD),
        ),
        Theme::Hacker => (
            "> ",
            Style::new()
                .fg(BLACK)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let title = match theme {
        Theme::Normal => format!("Chats ({})", chats.len()),
        Theme::Hacker => format!("COMMS ({})", chats.len()),
    };
    let selected = chats.iter().position(|r| r.conv == app.chat);
    let list = List::new(items)
        .block(panel(theme, app.ascii, !app.typing, &title))
        .highlight_symbol(symbol)
        .highlight_style(highlight);
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
}
