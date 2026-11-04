//! ui/contacts.rs
//! The contact list of the Contacts tab: the filters (as in the MeshVox app), the online
//! counts, then my radio and the nodes the filter keeps.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::format::{ascii_safe, cells, node_id, short};
use super::{faint, panel, text, Theme, BLACK, GREEN};
use crate::app::{App, Filter, MY_RADIO};

fn filter_name(filter: Filter) -> &'static str {
    match filter {
        Filter::Contacts => "Contacts",
        Filter::Favorites => "Favorites",
        Filter::Repeaters => "Repeaters",
        Filter::All => "All",
    }
}

/// The filters on one line, the current one highlighted.
fn filter_bar(app: &App, theme: Theme, highlight: Style) -> Line<'static> {
    let spans = Filter::ALL.iter().flat_map(|&f| {
        let name = match theme {
            Theme::Normal => filter_name(f).to_string(),
            Theme::Hacker => filter_name(f).to_uppercase(),
        };
        let style = if f == app.filter {
            highlight
        } else {
            faint(theme)
        };
        [Span::styled(name, style), Span::raw(" ")]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// "3 online - 2 offline - 5 total" for the nodes the filter keeps, as in MeshVox.
fn counts(app: &App) -> String {
    let kept: Vec<_> = app.nodes.iter().filter(|n| app.filter.keeps(n)).collect();
    let online = kept.iter().filter(|n| app.is_online(&n.prefix)).count();
    format!(
        "{online} online - {} offline - {} total",
        kept.len() - online,
        kept.len()
    )
}

/// The contact list: my radio, then the nodes.
pub(super) fn draw_contacts(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let items: Vec<ListItem> = app
        .rows()
        .into_iter()
        .map(|r| {
            let text = match theme {
                Theme::Normal => {
                    let unread = if r.unread > 0 {
                        r.unread.to_string()
                    } else {
                        String::new()
                    };
                    let label = match r.conv.as_str() {
                        MY_RADIO => format!("{} {}", if app.ascii { "*" } else { "◆" }, r.label),
                        conv if app.is_online(conv) => {
                            format!("{} {}", if app.ascii { "*" } else { "●" }, r.label)
                        }
                        _ => format!("  {}", r.label),
                    };
                    format!(
                        "{} {:>9} {unread:>3}",
                        cells(&ascii_safe(&label, app.ascii), 14),
                        r.detail
                    )
                }
                Theme::Hacker => {
                    let unread = if r.unread > 0 {
                        format!("*{}", r.unread)
                    } else {
                        String::new()
                    };
                    if r.conv == MY_RADIO {
                        let id = app
                            .me
                            .as_ref()
                            .map_or("", |me| &me.prefix[..6.min(me.prefix.len())]);
                        format!(
                            "{id:<6} {} {:>6}",
                            cells(&ascii_safe(&r.label, app.ascii), 10),
                            "LOCAL"
                        )
                    } else {
                        let mark = if app.is_online(&r.conv) { '+' } else { ' ' };
                        format!(
                            "{:<6}{mark}{} {:>6}{unread:>3}",
                            node_id(&r.conv),
                            cells(&ascii_safe(&r.label, app.ascii), 10),
                            short(&r.detail)
                        )
                    }
                }
            };
            // Online nodes in bold, so they stand out from the ones that may not answer.
            let style = if app.is_online(&r.conv) {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            ListItem::new(text).style(style)
        })
        .collect();
    let (title, symbol, highlight) = match theme {
        Theme::Normal => (
            format!("Contacts ({})", app.nodes.len()),
            if app.ascii { "> " } else { "▶ " },
            Style::new()
                .bg(Color::Rgb(0, 60, 80))
                .add_modifier(Modifier::BOLD),
        ),
        Theme::Hacker => (
            format!("NODES ({})", app.nodes.len()),
            ">",
            Style::new()
                .fg(BLACK)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let block = panel(theme, app.ascii, true, &title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [bar, count, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(filter_bar(app, theme, highlight)), bar);
    frame.render_widget(Paragraph::new(counts(app)).style(faint(theme)), count);
    if items.is_empty() {
        let empty = match app.filter {
            Filter::Favorites => "No favorites yet: set them in the MeshVox app.",
            _ => "No node heard yet.",
        };
        frame.render_widget(
            Paragraph::new(empty)
                .style(text(theme))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            list_area,
        );
        return;
    }
    let list = List::new(items)
        .highlight_symbol(symbol)
        .highlight_style(highlight);
    frame.render_stateful_widget(
        list,
        list_area,
        &mut ListState::default().with_selected(Some(app.selected)),
    );
}
