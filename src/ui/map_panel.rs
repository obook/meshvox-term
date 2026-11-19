//! ui/map_panel.rs
//! The map panel: rebuilds the map image only when something on it changes, and shows it
//! in the terminal with ratatui-image.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use image::DynamicImage;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

use super::format::coord;
use super::radar::{draw_radar, radar_px, scale};
use super::{faint, panel, Theme};
use crate::app::App;
use crate::map::{MapView, Pos};
use crate::tiles::{compose, sweep, Icon, Look, Mark, Tiles, SWEEP_STEPS};

/// What the map image needs between frames: it is rebuilt only when something on it changes.
pub struct MapState {
    picker: Picker,
    tiles: Tiles,
    shown: Option<Shown>,
    /// The map as last composed, without the sweep.
    base: Option<image::RgbaImage>,
    /// The sweep position shown; None when it is off.
    sweep_at: Option<u32>,
    image: Option<StatefulProtocol>,
    /// The map's size in pixels at the last drawing.
    size_px: Option<(u32, u32)>,
}

#[derive(PartialEq)]
pub(super) struct Shown {
    view: MapView,
    w: u32,
    h: u32,
    marks: Vec<(Pos, Mark)>,
    look: Look,
}

impl MapState {
    /// `picker` says how the terminal shows images; `tiles` is the tile cache.
    pub fn new(picker: Picker, tiles: Tiles) -> MapState {
        MapState {
            picker,
            tiles,
            shown: None,
            base: None,
            sweep_at: None,
            image: None,
            size_px: None,
        }
    }

    /// Ask the terminal its cell size again, after a font change (Ctrl +/- in Konsole): the
    /// image is sized from it. The image protocol stays as chosen at start.
    pub fn remeasure(&mut self) {
        match Picker::from_query_stdio() {
            Ok(mut picker) => {
                picker.set_protocol_type(self.picker.protocol_type());
                tracing::info!("map: font {:?}", picker.font_size());
                self.picker = picker;
            }
            Err(e) => tracing::warn!("terminal image query failed: {e}"),
        }
    }

    /// The map's size in pixels at the last drawing; None before the first one.
    pub fn size_px(&self) -> Option<(u32, u32)> {
        self.size_px
    }
}

/// The map panel: title, OpenStreetMap credit and the map image.
pub(super) fn draw_map(frame: &mut Frame, app: &App, area: Rect, map: &mut MapState, theme: Theme) {
    let conv = app.selected_conv();
    // The selected node's name and distance are in the node card.
    let (title, look) = match theme {
        Theme::Normal => ("Map".to_string(), Look::Plain),
        Theme::Hacker => (
            format!(
                "RADAR :: {} :: Z{}",
                coord(app.view.lat, app.view.lon),
                app.view.zoom
            ),
            Look::Night,
        ),
    };
    // No images on the Linux console: a radar of letters, with a scale instead of the tiles' credit.
    let bottom = if app.ascii {
        format!(" {} ", scale(app))
    } else {
        map.tiles.source.credit().to_string()
    };
    let block = panel(theme, app.ascii, true, &title)
        .title_bottom(Line::styled(bottom, faint(theme)).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.ascii {
        map.size_px = Some(radar_px(inner));
        draw_radar(frame, app, inner, theme);
        return;
    }

    let font = map.picker.font_size();
    let (w, h) = (
        u32::from(inner.width) * u32::from(font.width),
        u32::from(inner.height) * u32::from(font.height),
    );
    map.size_px = Some((w, h));
    let now = chrono::Utc::now().timestamp();
    let mut marks: Vec<(Pos, Mark)> = app
        .nodes
        .iter()
        .filter_map(|n| {
            // In minutes: the map is rebuilt at most once a minute for it.
            let age_mins = (n.heard != 0).then(|| ((now - i64::from(n.heard)).max(0) / 60) as u32);
            let icon = Icon {
                kind: n.kind,
                name: n.name.clone(),
                age_mins,
                hops: n.path_len,
            };
            n.pos.map(|p| {
                (
                    p,
                    if n.prefix == conv {
                        Mark::Selected(icon)
                    } else {
                        Mark::Node(icon)
                    },
                )
            })
        })
        .collect();
    marks.extend(app.self_pos.map(|p| (p, Mark::Me)));
    // The sweep turns around my radio, or the middle of the map before its position is known.
    let center = app.self_pos.map_or((f64::from(w) / 2.0, f64::from(h) / 2.0), |p| {
        app.view.screen_px(p, w, h)
    });
    let shown = Shown {
        view: app.view,
        w,
        h,
        marks,
        look,
    };
    let rebuilt = map.tiles.poll() || map.shown.as_ref() != Some(&shown);
    if rebuilt {
        map.base = Some(compose(&shown.view, w, h, &mut map.tiles, &shown.marks, look));
        map.shown = Some(shown);
    }
    // One turn in 6 s, a step every 100 ms: the main loop draws at that pace.
    let step = (chrono::Utc::now().timestamp_millis() / 100) as u32 % SWEEP_STEPS;
    let sweep_at = (app.sweep && look == Look::Night).then_some(step);
    if let (true, Some(base)) = (rebuilt || sweep_at != map.sweep_at, &map.base) {
        let mut img = base.clone();
        if let Some(step) = sweep_at {
            sweep(&mut img, center, step);
        }
        map.image = Some(
            map.picker
                .new_resize_protocol(DynamicImage::ImageRgba8(img)),
        );
        map.sweep_at = sweep_at;
    }
    if let Some(image) = &mut map.image {
        frame.render_stateful_widget(StatefulImage::default(), inner, image);
        drop_erase(frame.buffer_mut(), inner);
    }
}

/// ratatui-image erases the whole area before each iTerm2 image, and Konsole shows its gray
/// background until the new PNG is decoded: a gray flash at each map move. The map image is
/// opaque and fills the area, so the erase goes; the image escape stays as it is.
/// shortcut: relies on ratatui-image 11.1 putting the erase before "ESC ]1337" in the first
/// cell (outside tmux); map_drops_the_erase_before_an_iterm2_image catches a change.
pub(super) fn drop_erase(buf: &mut ratatui::buffer::Buffer, area: Rect) {
    let Some(cell) = buf.cell_mut((area.x, area.y)) else {
        return;
    };
    let sym = cell.symbol();
    if let (true, Some(start)) = (sym.starts_with("\x1b["), sym.find("\x1b]1337;")) {
        let image = sym[start..].to_string();
        cell.set_symbol(&image);
    }
}
