//! tiles/compose.rs
//! The map image: tiles, then marks for the nodes, my radio and the selected node (a pin).
//! The Night look (Hacker theme) recolors the tiles, adds a grid, range rings, a crosshair
//! and scanlines, and shows the nodes as air traffic control targets with labels.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use image::{imageops, Rgba, RgbaImage};

use super::font::{draw_text_at, glyph, CHAR_PX, TEXT_PX};
use super::icons::{draw_icon, Icon, PIN_HEAD, PIN_RED};
use crate::radio::NodeKind;
use super::Tiles;
use crate::config::RINGS_KM;
use crate::map::{MapView, Pos};

/// How the map is drawn: OSM colors, or a dark map with radar overlays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    Plain,
    Night,
}

// Plain palette.
/// Where a tile is missing (Normal look).
pub(super) const PLAIN_BACKGROUND: Rgba<u8> = Rgba([40, 40, 40, 255]);

// Hacker palette.
/// Where a tile is missing (Hacker look).
pub const BACKGROUND: Rgba<u8> = Rgba([0, 8, 2, 255]);
/// A node heard in the last hour (Hacker look).
pub const TARGET_HOUR: Rgba<u8> = Rgba([0, 255, 70, 255]);
/// A node heard in the last day (Hacker look).
pub const TARGET_DAY: Rgba<u8> = Rgba([0, 205, 60, 255]);
/// A node heard longer ago, or never (Hacker look).
pub const TARGET_OLD: Rgba<u8> = Rgba([95, 140, 100, 255]);
/// The selected node, white as an owned data block on an air traffic control screen.
pub const SELECTED: Rgba<u8> = Rgba([255, 255, 255, 255]);
/// My radio's dot, in both looks: blue, unlike the red pin of the selected node.
pub const ME: Rgba<u8> = Rgba([30, 136, 229, 255]);
/// The ring around my radio's dot.
pub const ME_RING: Rgba<u8> = Rgba([255, 255, 255, 255]);
/// Range rings around my radio.
pub const RING: Rgba<u8> = Rgba([150, 255, 180, 255]);
/// The dotted grid.
pub(super) const GRID: Rgba<u8> = Rgba([0, 70, 25, 255]);
/// The crosshair at the center of the map.
pub(super) const CROSSHAIR: Rgba<u8> = Rgba([0, 200, 80, 255]);
/// Outline that keeps marks readable on any tile.
pub(super) const OUTLINE: Rgba<u8> = Rgba([0, 0, 0, 255]);
/// Grid spacing, in pixels; lines fall between the center and the edges.
pub(super) const GRID_PX: i64 = 100;

/// OSM colors to a dark map, the meshmap.net way (CSS `invert(1) hue-rotate(180deg)
/// brightness(1.33)`): light land turns dark, labels turn bright, water stays blue.
pub fn night(c: Rgba<u8>) -> Rgba<u8> {
    // The CSS hue-rotate matrix for 180 degrees.
    const HUE: [[f64; 3]; 3] = [
        [-0.574, 1.430, 0.144],
        [0.426, 0.430, 0.144],
        [0.426, 1.430, -0.856],
    ];
    let inv = [0, 1, 2].map(|i| 255.0 - f64::from(c[i]));
    let px = |m: [f64; 3]| {
        let v = (m[0] * inv[0] + m[1] * inv[1] + m[2] * inv[2]).clamp(0.0, 255.0);
        (v * 1.33).min(255.0) as u8
    };
    Rgba([px(HUE[0]), px(HUE[1]), px(HUE[2]), 255])
}

/// Radius in screen pixels of `km` at latitude `lat` and `zoom` (Web Mercator).
pub fn ring_px(km: f64, lat: f64, zoom: u8) -> f64 {
    let meters_per_px = 156_543.033_92 * lat.to_radians().cos() / f64::from(1u32 << zoom);
    km * 1000.0 / meters_per_px
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Node(Icon),
    Selected(Icon),
    Me,
}

/// A map pin whose tip is on (x, y): a tapering body and a round head.
pub(super) fn pin(img: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>) {
    let top = y - PIN_HEAD + 8;
    for row in top..=y {
        let half = 12 * (y - row) / (y - top);
        (-half..=half).for_each(|dx| put(img, x + dx, row, color));
    }
    for dy in -17..=17_i64 {
        for dx in -17..=17_i64 {
            if dx * dx + dy * dy <= 17 * 17 {
                put(img, x + dx, y - PIN_HEAD + dy, color);
            }
        }
    }
}

/// The map as a `w` x `h` image, then marks (the selected node on top). The night look
/// recolors the tiles and adds a grid, range rings, a crosshair and scanlines.
pub fn compose(
    view: &MapView,
    w: u32,
    h: u32,
    tiles: &mut Tiles,
    marks: &[(Pos, Mark)],
    look: Look,
) -> RgbaImage {
    let night_look = look == Look::Night;
    let mut img = RgbaImage::from_pixel(
        w,
        h,
        if night_look {
            BACKGROUND
        } else {
            PLAIN_BACKGROUND
        },
    );
    for (x, y, left, top) in view.tiles(w, h) {
        if let Some(tile) = tiles.get((view.zoom, x, y)) {
            if night_look {
                let mut tile = tile.clone();
                tile.pixels_mut().for_each(|p| *p = night(*p));
                imageops::overlay(&mut img, &tile, left, top);
            } else {
                imageops::overlay(&mut img, tile, left, top);
            }
        }
    }
    if night_look {
        radar(&mut img, view, marks);
        targets(&mut img, view, marks);
    }
    // Nodes, then my radio, then the selected node on top.
    for pass in 0..3 {
        for (pos, mark) in marks {
            let layer = match mark {
                Mark::Node(_) => 0,
                Mark::Me => 1,
                Mark::Selected(_) => 2,
            };
            if layer != pass {
                continue;
            }
            let (x, y) = view.screen_px(*pos, w, h);
            let (x, y) = (x.round() as i64, y.round() as i64);
            match (mark, look) {
                (Mark::Me, _) => here(&mut img, x, y),
                (_, Look::Night) => {} // drawn by targets()
                (Mark::Node(icon), Look::Plain) => draw_icon(&mut img, x, y, icon),
                (Mark::Selected(icon), Look::Plain) => {
                    pin(&mut img, x, y, PIN_RED);
                    draw_icon(&mut img, x, y - PIN_HEAD, icon);
                }
            }
        }
    }
    img
}

/// Night-look overlays: grid, range rings around my radio, crosshair, scanlines.
pub(super) fn radar(img: &mut RgbaImage, view: &MapView, marks: &[(Pos, Mark)]) {
    let (w, h) = img.dimensions();
    let (cx, cy) = (i64::from(w / 2), i64::from(h / 2));
    let (wi, hi) = (i64::from(w), i64::from(h));
    // Dotted grid, its lines halfway between the center and the next line.
    for x in (0..wi).filter(|x| (x - cx - GRID_PX / 2).rem_euclid(GRID_PX) == 0) {
        (0..hi).step_by(2).for_each(|y| put(img, x, y, GRID));
    }
    for y in (0..hi).filter(|y| (y - cy - GRID_PX / 2).rem_euclid(GRID_PX) == 0) {
        (0..wi).step_by(2).for_each(|x| put(img, x, y, GRID));
    }
    for &(pos, _) in marks.iter().filter(|(_, m)| matches!(m, Mark::Me)) {
        let (x, y) = view.screen_px(pos, w, h);
        for km in RINGS_KM {
            let r = ring_px(km, pos.lat, view.zoom).round() as i64;
            if (6..=2 * wi.max(hi)).contains(&r) {
                circle(img, x.round() as i64, y.round() as i64, r, RING, true);
            }
        }
    }
    for d in 6..=20 {
        for (dx, dy) in [(d, 0), (-d, 0), (0, d), (0, -d)] {
            put(img, cx + dx, cy + dy, CROSSHAIR);
        }
    }
    // CRT scanlines.
    for (_, y, pixel) in img.enumerate_pixels_mut() {
        if y % 3 == 2 {
            *pixel = Rgba([pixel[0] / 4 * 3, pixel[1] / 4 * 3, pixel[2] / 4 * 3, 255]);
        }
    }
}

/// Set a pixel, ignoring what falls outside the image.
pub(super) fn put(img: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < i64::from(img.width()) && y < i64::from(img.height()) {
        img.put_pixel(x as u32, y as u32, color);
    }
}

/// Midpoint circle outline, dashed or not.
pub(super) fn circle(img: &mut RgbaImage, cx: i64, cy: i64, r: i64, color: Rgba<u8>, dashed: bool) {
    let (mut x, mut y, mut err) = (r, 0, 1 - r);
    while x >= y {
        if !dashed || (y / 4) % 2 == 0 {
            for (dx, dy) in [
                (x, y),
                (y, x),
                (-y, x),
                (-x, y),
                (-x, -y),
                (-y, -x),
                (y, -x),
                (x, -y),
            ] {
                put(img, cx + dx, cy + dy, color);
            }
        }
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

/// Distance from a target to its label, along both axes.
const LEADER_PX: i64 = 22;
/// Longer names are cut on the map; the card has them whole.
const LABEL_CHARS: usize = 16;

/// The radar sweep: one turn in SWEEP_STEPS positions.
pub const SWEEP_STEPS: u32 = 60;
/// How far behind the beam the afterglow reaches, in radians (60 degrees).
const AFTERGLOW: f64 = std::f64::consts::PI / 3.0;
/// The phosphor green of the beam and its afterglow.
const GLOW: [f64; 3] = [60.0, 255.0, 120.0];

/// The radar sweep at `step`, turning clockwise from north around `center`: a bright beam
/// and the afterglow of the phosphor fading behind it. Fake: it shows nothing real.
pub fn sweep(img: &mut RgbaImage, center: (f64, f64), step: u32) {
    use std::f64::consts::TAU;
    let beam = f64::from(step % SWEEP_STEPS) * TAU / f64::from(SWEEP_STEPS);
    // Directions clockwise from north, y pointing down: the beam and the end of the afterglow.
    let dir = |a: f64| (a.sin(), -a.cos());
    // A hair ahead of the beam (here and below), so rounding does not drop the pixels on it.
    let (b, t) = (dir(beam + 0.01), dir(beam - AFTERGLOW));
    for (x, y, p) in img.enumerate_pixels_mut() {
        let (dx, dy) = (f64::from(x) - center.0, f64::from(y) - center.1);
        // Cheap test first: only pixels between the two directions need an angle.
        if t.0 * dy - t.1 * dx < 0.0 || dx * b.1 - dy * b.0 < 0.0 {
            continue;
        }
        // Clockwise from north; then how far the beam has gone past this pixel.
        let angle = dx.atan2(-dy).rem_euclid(TAU);
        let behind = (beam + 0.01 - angle).rem_euclid(TAU) - 0.01;
        let glow = match behind {
            b if b < 0.03 => 0.8,
            b if b < AFTERGLOW => 0.45 * (1.0 - b / AFTERGLOW),
            _ => continue,
        };
        for i in 0..3 {
            p[i] = (f64::from(p[i]) + (GLOW[i] - f64::from(p[i])) * glow) as u8;
        }
    }
}

/// A box on the image: left, top, right, bottom.
type Area = (i64, i64, i64, i64);

/// Night-look nodes, the air traffic control way: a symbol per kind, colored by how long ago
/// the node was heard, and its label on a leader line where it fits without covering another.
/// The selected node is white with a three-line label and is placed first, then the most
/// recently heard.
fn targets(img: &mut RgbaImage, view: &MapView, marks: &[(Pos, Mark)]) {
    let (w, h) = img.dimensions();
    let px = |pos: Pos| {
        let (x, y) = view.screen_px(pos, w, h);
        (x.round() as i64, y.round() as i64)
    };
    let mut nodes: Vec<(i64, i64, &Icon, bool)> = marks
        .iter()
        .filter_map(|(pos, mark)| {
            let (icon, selected) = match mark {
                Mark::Node(icon) => (icon, false),
                Mark::Selected(icon) => (icon, true),
                Mark::Me => return None,
            };
            let (x, y) = px(*pos);
            Some((x, y, icon, selected))
        })
        .collect();
    nodes.sort_by_key(|(_, _, icon, selected)| (!selected, icon.age_mins.unwrap_or(u32::MAX)));
    // Labels cover no symbol and not my radio's dot.
    let mut taken: Vec<Area> = nodes.iter().map(|&(x, y, ..)| (x - 8, y - 8, x + 8, y + 8)).collect();
    for (pos, _) in marks.iter().filter(|(_, m)| *m == Mark::Me) {
        let (x, y) = px(*pos);
        taken.push((x - 12, y - 12, x + 12, y + 12));
    }
    let (w, h) = (i64::from(w), i64::from(h));
    let color = |icon: &Icon, selected| if selected { SELECTED } else { target_color(icon.age_mins) };
    // The selected node's symbol last, on top.
    for &(x, y, icon, selected) in nodes.iter().rev() {
        target(img, x, y, icon.kind, color(icon, selected));
    }
    for &(x, y, icon, selected) in &nodes {
        let color = color(icon, selected);
        let lines = data_block(icon, selected);
        let width = lines.iter().map(|l| l.chars().count() as i64).max().unwrap_or(0) * CHAR_PX - 2;
        let height = lines.len() as i64 * (TEXT_PX + 4) - 4;
        // Up right first, as on a controller's screen, then the other corners.
        let spot = [(1, -1), (1, 1), (-1, -1), (-1, 1)].into_iter().find_map(|(sx, sy)| {
            let left = if sx > 0 { x + LEADER_PX } else { x - LEADER_PX - width };
            let top = if sy < 0 { y - LEADER_PX - height } else { y + LEADER_PX };
            let area = (left - 3, top - 3, left + width + 3, top + height + 3);
            let inside = area.0 >= 0 && area.1 >= 0 && area.2 < w && area.3 < h;
            (inside && !taken.iter().any(|t| overlap(*t, area))).then_some((sx, sy, area))
        });
        let Some((sx, sy, area)) = spot else { continue };
        taken.push(area);
        let end = (if sx > 0 { area.0 } else { area.2 }, if sy < 0 { area.3 } else { area.1 });
        leader(img, (x + sx * 6, y + sy * 6), end, color);
        shade(img, area);
        for (i, line) in lines.iter().enumerate() {
            draw_text_at(img, area.0 + 3, area.1 + 3 + i as i64 * (TEXT_PX + 4), line, color);
        }
    }
}

fn target_color(age_mins: Option<u32>) -> Rgba<u8> {
    match age_mins {
        Some(m) if m < 60 => TARGET_HOUR,
        Some(m) if m < 24 * 60 => TARGET_DAY,
        _ => TARGET_OLD,
    }
}

/// The name, in the capitals and signs the font has; the selected node adds its kind and
/// route, and when it was last heard.
pub(super) fn data_block(icon: &Icon, selected: bool) -> Vec<String> {
    let name: String = icon
        .name
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| *c == ' ' || glyph(*c).is_some())
        .take(LABEL_CHARS)
        .collect();
    let mut lines = vec![name.trim().to_string()];
    if selected {
        let kind = match icon.kind {
            NodeKind::Chat => "CLI",
            NodeKind::Repeater => "RPT",
            NodeKind::Room => "ROOM",
            NodeKind::Sensor => "SNS",
        };
        let route = match icon.hops {
            ..0 => "FLOOD".to_string(),
            0 => "DIRECT".to_string(),
            1 => "1 HOP".to_string(),
            n => format!("{n} HOPS"),
        };
        let heard = match icon.age_mins {
            None => "NEVER".to_string(),
            Some(m) if m < 60 => format!("{m}M"),
            Some(m) if m < 48 * 60 => format!("{}H", m / 60),
            Some(m) => format!("{}D", m / (24 * 60)),
        };
        lines.push(format!("{kind} {route}"));
        lines.push(format!("HEARD {heard}"));
    }
    lines
}

fn overlap(a: Area, b: Area) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// A target's symbol, 3 px wide and outlined: a square for a repeater, a circle for a person,
/// a diamond for a room, a triangle for a sensor.
fn target(img: &mut RgbaImage, x: i64, y: i64, kind: NodeKind, color: Rgba<u8>) {
    // Whether (dx, dy) is inside the shape at size s.
    let inside = |dx: i64, dy: i64, s: i64| match kind {
        NodeKind::Repeater => dx.abs().max(dy.abs()) <= s,
        NodeKind::Chat => dx * dx + dy * dy <= s * s + s,
        NodeKind::Room => (dx.abs() + dy.abs()) * 5 <= s * 6,
        NodeKind::Sensor => dy * 5 <= s * 4 && (dy + s) * 5 >= dx.abs() * 9,
    };
    for (paint, outer, inner) in [(OUTLINE, 8, 2), (color, 7, 4)] {
        for dy in -outer..=outer {
            for dx in -outer..=outer {
                if inside(dx, dy, outer) && !inside(dx, dy, inner) {
                    put(img, x + dx, y + dy, paint);
                }
            }
        }
    }
}

/// The line from a target to its label, outlined.
fn leader(img: &mut RgbaImage, from: (i64, i64), to: (i64, i64), color: Rgba<u8>) {
    let steps = (to.0 - from.0).abs().max((to.1 - from.1).abs()).max(1);
    let point = |i: i64| (from.0 + (to.0 - from.0) * i / steps, from.1 + (to.1 - from.1) * i / steps);
    for (paint, around) in [(OUTLINE, true), (color, false)] {
        for i in 0..=steps {
            let (x, y) = point(i);
            if around {
                [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().for_each(|(dx, dy)| put(img, x + dx, y + dy, paint));
            } else {
                put(img, x, y, paint);
            }
        }
    }
}

/// Darken an area, so a label reads on any tile.
fn shade(img: &mut RgbaImage, (left, top, right, bottom): Area) {
    for y in top.max(0)..=bottom.min(i64::from(img.height()) - 1) {
        for x in left.max(0)..=right.min(i64::from(img.width()) - 1) {
            let p = img.get_pixel_mut(x as u32, y as u32);
            let dim = |c: u8| (u16::from(c) * 2 / 5) as u8;
            *p = Rgba([dim(p[0]), dim(p[1]), dim(p[2]), 255]);
        }
    }
}

/// My radio, the "you are here" way: a blue dot in a white ring, outlined, in a pale halo.
pub(super) fn here(img: &mut RgbaImage, x: i64, y: i64) {
    // The halo lets the tile show through: 70/255 of the blue over it.
    let mix = |a: u8, b: u8| ((u32::from(a) * 185 + u32::from(b) * 70) / 255) as u8;
    disk(img, x, y, 12, |p| Rgba([mix(p[0], ME[0]), mix(p[1], ME[1]), mix(p[2], ME[2]), 255]));
    disk(img, x, y, 8, |_| OUTLINE);
    disk(img, x, y, 7, |_| ME_RING);
    disk(img, x, y, 5, |_| ME);
}

/// Repaint each pixel of a disk with `paint`, ignoring what falls outside the image.
fn disk(img: &mut RgbaImage, x: i64, y: i64, r: i64, paint: impl Fn(Rgba<u8>) -> Rgba<u8>) {
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    for dy in -r..=r {
        for dx in -r..=r {
            let (px, py) = (x + dx, y + dy);
            if dx * dx + dy * dy <= r * r + r && (0..w).contains(&px) && (0..h).contains(&py) {
                let p = img.get_pixel_mut(px as u32, py as u32);
                *p = paint(*p);
            }
        }
    }
}
