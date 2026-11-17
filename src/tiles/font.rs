//! tiles/font.rs
//! A 5 x 7 bitmap font of capitals, digits and a few signs: a node's initials, and the
//! labels of the Hacker map.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use image::RgbaImage;

use image::Rgba;

use super::compose::put;
use super::icons::GLYPH;

/// 5x7 capitals, digits and the signs node names often have.
pub(super) fn glyph(c: char) -> Option<[u8; 7]> {
    #[rustfmt::skip]
    const FONT: [(char, [u8; 7]); 39] = [
        ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
        ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
        ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
        ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
        ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
        ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
        ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
        ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
        ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
        ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
        ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
        ('N', [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001]),
        ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
        ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
        ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
        ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
        ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
        ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
        ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
        ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
        ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
        ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
        ('Y', [0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100]),
        ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
        ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
        ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
        ('3', [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110]),
        ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
        ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
        ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
        ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
        ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
        ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
        ('_', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111]),
        ('-', [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
        ('.', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100]),
    ];
    FONT.iter().find(|(k, _)| *k == c).map(|(_, rows)| *rows)
}

/// A character's advance and a line's height, in pixels (font pixels are 2 x 2).
pub(super) const CHAR_PX: i64 = 12;
pub(super) const TEXT_PX: i64 = 14;

/// Draw `text` in white, centered on (cx, cy).
pub(super) fn draw_text(img: &mut RgbaImage, cx: i64, cy: i64, text: &str) {
    let n = text.chars().count() as i64;
    draw_text_at(img, cx - (n * CHAR_PX - 2) / 2, cy - 7, text, GLYPH);
}

/// Draw `text` from its top left corner; a character the font lacks leaves a blank.
pub(super) fn draw_text_at(img: &mut RgbaImage, left: i64, top: i64, text: &str, color: Rgba<u8>) {
    for (i, c) in text.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        let x0 = left + i as i64 * CHAR_PX;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0b10000 >> col) != 0 {
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        put(img, x0 + col * 2 + dx, top + row as i64 * 2 + dy, color);
                    }
                }
            }
        }
    }
}
