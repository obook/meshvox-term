//! ui/conversation.rs
//! The conversation panel of the Chats tab: the open thread (wrapped and scrollable) and the input line.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::format::{ascii_safe, hhmm};
use super::{faint, panel, strong, text, Theme, CYAN};
use crate::app::{App, Delivery};
use crate::history::Msg;
use crate::tiles::name_color;

/// My handle in the thread: my radio's node name. Not the link's name, which is the serial
/// port over USB.
pub(super) fn my_handle(app: &App) -> String {
    app.me.as_ref().map_or_else(|| "me".into(), |me| me.name.clone())
}

/// The color of a sender's marker on the map, so its name stands out from the text.
fn sender_color(name: &str) -> Color {
    let [r, g, b, _] = name_color(name).0;
    Color::Rgb(r, g, b)
}

/// One message of the thread: time, sender and text, plus how far a message I sent went:
/// one tick when sent, two when received or repeated (with the count), as in MeshVox.
pub(super) fn thread_line<'a>(
    app: &App,
    msg: &'a Msg,
    me: &str,
    width: usize,
    theme: Theme,
) -> Line<'a> {
    let time = hhmm(msg.ts);
    let from = if msg.outgoing { me } else { &msg.from };
    let from = ascii_safe(from, app.ascii);
    let body = ascii_safe(&msg.text, app.ascii).into_owned();
    let mut line = match theme {
        Theme::Normal => Line::from(vec![
            Span::styled(format!("{time} "), faint(theme)),
            Span::styled(
                format!("{from}  "),
                Style::new()
                    .fg(if msg.outgoing { CYAN } else { sender_color(&msg.from) })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(body),
        ]),
        Theme::Hacker => Line::from(vec![
            Span::styled(format!("[{time}] "), faint(theme)),
            Span::styled(format!("<{from}> "), strong(theme)),
            Span::styled(body, text(theme)),
        ]),
    };
    if let Some(status) = app.status(msg) {
        let tick = if app.ascii { "v" } else { "\u{2713}" };
        let tag = match (theme, status) {
            (Theme::Normal, Delivery::Sent) => tick.to_string(),
            (Theme::Normal, Delivery::Received) => tick.repeat(2),
            (Theme::Normal, Delivery::Repeated(n)) => format!("{} {n}", tick.repeat(2)),
            (Theme::Hacker, Delivery::Sent) => "[TX]".into(),
            (Theme::Hacker, Delivery::Received) => "[ACK]".into(),
            (Theme::Hacker, Delivery::Repeated(n)) => format!("[RPT {n}]"),
        };
        let style = match (theme, status) {
            (_, Delivery::Sent) => faint(theme),
            (Theme::Normal, _) => Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
            (Theme::Hacker, _) => strong(theme),
        };
        // After the text, the whole line being right-aligned.
        line.push_span(Span::raw("  "));
        line.push_span(Span::styled(tag, style));
    }
    // Mine on the right, as in a phone's messaging app, its time (the first span) kept on
    // the left; the whole line on the right when it wraps.
    if msg.outgoing {
        match width.checked_sub(line.width()) {
            Some(pad) => line.spans.insert(1, Span::raw(" ".repeat(pad))),
            None => line = line.right_aligned(),
        }
    }
    line
}

/// The open conversation (anchored at its latest message) and the input line.
pub(super) fn draw_conversation(frame: &mut Frame, app: &App, area: Rect, theme: Theme) {
    let label = if app.chat == app.public_conv() && theme == Theme::Hacker {
        "#PUBLIC".to_string()
    } else {
        ascii_safe(&app.conv_label(&app.chat), app.ascii).into_owned()
    };
    let focused = app.typing;
    let title = if theme == Theme::Normal {
        label
    } else {
        format!("COMMS // {label}")
    };
    let block = panel(theme, app.ascii, focused, &title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [thread_area, input_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let width = usize::from(thread_area.width);

    let me = my_handle(app);
    let lines: Vec<Line> = app
        .thread()
        .into_iter()
        .map(|m| thread_line(app, m, &me, width, theme))
        .collect();
    let thread = Paragraph::new(lines)
        .style(text(theme))
        .wrap(Wrap { trim: false });
    let hidden_above = thread
        .line_count(thread_area.width)
        .saturating_sub(usize::from(thread_area.height));
    let top = hidden_above - app.scroll.min(hidden_above);
    frame.render_widget(
        thread.scroll((u16::try_from(top).unwrap_or(u16::MAX), 0)),
        thread_area,
    );

    // The end of a long input stays in view.
    let (prompt, cursor) = match theme {
        Theme::Normal => ("> ", "_"),
        Theme::Hacker => ("root@mesh:~$ ", if app.ascii { "_" } else { "█" }),
    };
    let room = usize::from(input_area.width).saturating_sub(prompt.len() + 1);
    let typed = app.input.chars().count();
    let shown: String = app.input.chars().skip(typed.saturating_sub(room)).collect();
    let mut input = vec![
        Span::styled(prompt, strong(theme)),
        Span::styled(shown, text(theme)),
    ];
    if focused {
        input.push(Span::styled(
            cursor,
            text(theme).add_modifier(Modifier::SLOW_BLINK),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(input)).style(text(theme)),
        input_area,
    );
}
