//! tiles/tests.rs
//! Tests of the tile cache and of the map image.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use super::compose::*;
use super::icons::*;
use super::*;
use crate::map::MapView;
use crate::map::Pos;
use crate::radio::NodeKind;
use image::Rgba;

const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);

fn red_tile(dir: &Path, z: u8, x: u32, y: u32) -> PathBuf {
    let path = dir.join(format!("{z}/{x}/{y}.png"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    RgbaImage::from_pixel(256, 256, RED).save(&path).unwrap();
    path
}

fn world_z1() -> MapView {
    MapView {
        lat: 0.0,
        lon: 0.0,
        zoom: 1,
    }
}

#[test]
fn cached_tiles_are_placed_and_missing_ones_stay_grey() {
    let dir = tempfile::tempdir().unwrap();
    red_tile(dir.path(), 1, 0, 0);
    let mut tiles = Tiles::offline(dir.path().into());
    let img = compose(&world_z1(), 512, 512, &mut tiles, &[], Look::Night);
    assert_eq!(*img.get_pixel(10, 10), night(RED));
    assert_eq!(*img.get_pixel(300, 300), BACKGROUND);
}

#[test]
fn the_plain_look_keeps_osm_colors_without_overlays() {
    let dir = tempfile::tempdir().unwrap();
    red_tile(dir.path(), 1, 0, 0);
    let mut tiles = Tiles::offline(dir.path().into());
    let me = Pos { lat: 0.0, lon: 0.0 };
    let view = MapView {
        lat: 0.0,
        lon: 0.0,
        zoom: 13,
    };
    let img = compose(&world_z1(), 512, 512, &mut tiles, &[], Look::Plain);
    assert_eq!(*img.get_pixel(10, 10), RED);
    assert_eq!(img.get_pixel(10, 2), img.get_pixel(10, 1), "no scanlines");
    let img = compose(&view, 512, 512, &mut tiles, &[(me, Mark::Me)], Look::Plain);
    assert_ne!(*img.get_pixel(256 + 52, 256), RING, "no range rings");
}

fn icon() -> Icon {
    Icon {
        kind: NodeKind::Repeater,
        name: "33EYSI_F4LW".into(),
        age_mins: Some(2 * 24 * 60),
        hops: 1,
    }
}

#[test]
fn name_colors_match_the_meshcore_map() {
    assert_eq!(fnv1a("Alice"), 752_715_143);
    assert_eq!(name_color("Alice"), Rgba([51, 204, 110, 255]));
    assert_eq!(name_color("bbok"), Rgba([135, 204, 51, 255]));
}

#[test]
fn initials_like_the_meshcore_map() {
    assert_eq!(initials("Alice Bob"), "AB");
    assert_eq!(initials("33CSTR-REP-NASH"), "3");
    assert_eq!(initials("bbok"), "B");
    assert_eq!(
        initials("🐱 SHA-40-GMD"),
        "S",
        "emoji cannot be drawn: the first letter instead"
    );
    assert_eq!(initials(""), "");
}

#[test]
fn freshness_colors() {
    const DAY: u32 = 24 * 60;
    assert_eq!(
        [
            freshness(None),
            freshness(Some(2 * DAY)),
            freshness(Some(7 * DAY)),
            freshness(Some(15 * DAY)),
            freshness(Some(30 * DAY))
        ],
        [SLATE, RECENT, STALE, OLD, EXTINCT]
    );
}

#[test]
fn tint_keeps_the_white_glyph() {
    assert_eq!(
        tint(Rgba([255, 255, 255, 255]), RECENT),
        Rgba([255, 255, 255, 255])
    );
    assert_eq!(tint(SLATE, RECENT), RECENT);
}

#[test]
fn plain_repeater_icon_takes_its_freshness_color() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let center = Pos { lat: 0.0, lon: 0.0 };
    let img = compose(
        &world_z1(),
        512,
        512,
        &mut tiles,
        &[(center, Mark::Node(icon()))],
        Look::Plain,
    );
    assert_eq!(*img.get_pixel(256, 256 - 11), RECENT);
}

#[test]
fn plain_person_icon_is_a_name_colored_disc_with_initials() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let center = Pos { lat: 0.0, lon: 0.0 };
    let alice = Icon {
        kind: NodeKind::Chat,
        name: "Alice".into(),
        age_mins: None,
        hops: -1,
    };
    let img = compose(
        &world_z1(),
        512,
        512,
        &mut tiles,
        &[(center, Mark::Node(alice))],
        Look::Plain,
    );
    assert_eq!(*img.get_pixel(256, 256 - 12), name_color("Alice"));
    let white = (240..272)
        .flat_map(|x| (240..272).map(move |y| (x, y)))
        .filter(|&(x, y)| *img.get_pixel(x, y) == GLYPH)
        .count();
    assert!(white > 10, "initials drawn: {white} white pixels");
}

#[test]
fn the_selected_node_gets_a_pin_in_both_looks() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let center = Pos { lat: 0.0, lon: 0.0 };
    let selected = [(center, Mark::Selected(icon()))];
    let img = compose(&world_z1(), 512, 512, &mut tiles, &selected, Look::Plain);
    assert_eq!(
        *img.get_pixel(256, 256 - 10),
        PIN_RED,
        "pin body above the node"
    );
    assert_eq!(
        *img.get_pixel(256, 256 - PIN_HEAD as u32 - 11),
        RECENT,
        "the node's icon in the head"
    );
    // The Hacker look: a white square for a repeater, and its label up right.
    let img = compose(&world_z1(), 512, 512, &mut tiles, &selected, Look::Night);
    assert_eq!(*img.get_pixel(256 + 5, 256), SELECTED);
    let label = (256 + 22..512)
        .flat_map(|x| (0..256 - 22).map(move |y| (x, y)))
        .filter(|&(x, y)| *img.get_pixel(x, y) == SELECTED)
        .count();
    assert!(label > 100, "label drawn: {label} white pixels");
}

#[test]
fn the_hacker_label_says_who_and_when() {
    let mut node = icon();
    assert_eq!(data_block(&node, false), ["33EYSI_F4LW"]);
    assert_eq!(data_block(&node, true), ["33EYSI_F4LW", "RPT 1 HOP", "HEARD 2D"]);
    node.name = "Café 🚀 relais de la côte d'argent".into();
    node.hops = -1;
    node.age_mins = Some(90);
    assert_eq!(
        data_block(&node, true),
        ["CAF  RELAIS DE L", "RPT FLOOD", "HEARD 1H"],
        "capitals the font has, 16 at most"
    );
}

#[test]
fn night_darkens_land_lights_up_text_and_keeps_water_blue() {
    let land = night(Rgba([242, 239, 233, 255]));
    assert!(land.0[..3].iter().all(|&v| v < 40), "OSM land: {land:?}");
    assert_eq!(night(Rgba([30, 30, 30, 255])), Rgba([255, 255, 255, 255]), "OSM labels");
    let water = night(Rgba([170, 211, 223, 255]));
    assert!(water[2] > water[0] + 40, "OSM water: {water:?}");
}

#[test]
fn every_third_row_is_a_darker_scanline() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let img = compose(&world_z1(), 512, 512, &mut tiles, &[], Look::Night);
    assert!(img.get_pixel(20, 2)[1] < img.get_pixel(20, 1)[1]);
}

#[test]
fn range_rings_scale_with_zoom() {
    assert!(
        (ring_px(1.0, 0.0, 13) - 52.33).abs() < 0.1,
        "{}",
        ring_px(1.0, 0.0, 13)
    );
    assert!(
        (ring_px(1.0, 60.0, 13) - 104.66).abs() < 0.2,
        "a degree is shorter up north"
    );
}

#[test]
fn my_radio_gets_range_rings() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let view = MapView {
        lat: 0.0,
        lon: 0.0,
        zoom: 13,
    };
    let img = compose(
        &view,
        512,
        512,
        &mut tiles,
        &[(Pos { lat: 0.0, lon: 0.0 }, Mark::Me)],
        Look::Night,
    );
    assert_eq!(*img.get_pixel(256 + 52, 256), RING, "1 km ring");
}

#[test]
fn a_missing_tile_is_asked_for_once() {
    let dir = tempfile::tempdir().unwrap();
    red_tile(dir.path(), 1, 0, 0);
    let mut tiles = Tiles::offline(dir.path().into());
    compose(&world_z1(), 512, 512, &mut tiles, &[], Look::Night);
    compose(&world_z1(), 512, 512, &mut tiles, &[], Look::Night);
    assert_eq!(tiles.waiting(), 3);
}

#[test]
fn a_stale_tile_is_shown_and_refreshed() {
    let dir = tempfile::tempdir().unwrap();
    let path = red_tile(dir.path(), 1, 0, 0);
    let old = std::time::SystemTime::now() - TILE_MAX_AGE - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let img = compose(&world_z1(), 256, 256, &mut tiles, &[], Look::Night);
    assert_eq!(*img.get_pixel(10, 10), night(RED));
    assert!(tiles.asked.contains_key(&(1, 0, 0)));
}

#[test]
fn a_downloaded_tile_is_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    compose(&world_z1(), 256, 256, &mut tiles, &[], Look::Night);
    assert!(!tiles.poll());
    red_tile(dir.path(), 1, 0, 0);
    tiles.done_tx.send(((1, 0, 0), true)).unwrap();
    assert!(tiles.poll());
    assert!(!tiles.asked.contains_key(&(1, 0, 0)));
    let img = compose(&world_z1(), 256, 256, &mut tiles, &[], Look::Night);
    assert_eq!(*img.get_pixel(10, 10), night(RED));
}

#[test]
fn an_unreadable_tile_is_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("1/0/0.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not a png").unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    compose(&world_z1(), 256, 256, &mut tiles, &[], Look::Night);
    assert!(tiles.asked.contains_key(&(1, 0, 0)));
}

#[test]
fn a_waiting_tile_is_never_asked_twice_but_a_failed_one_is_retried() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let (tx, rx) = std::sync::mpsc::channel();
    tiles.requests = Some(tx);
    tiles.ask((1, 0, 0));
    tiles.asked.insert((1, 0, 0), Asked::Waiting);
    tiles.ask((1, 0, 0));
    assert_eq!(rx.try_iter().count(), 1, "still in the download queue");
    tiles.asked.insert(
        (1, 0, 0),
        Asked::Failed(Instant::now() - TILE_RETRY - Duration::from_secs(1)),
    );
    tiles.ask((1, 0, 0));
    assert_eq!(rx.try_iter().count(), 1, "retried after a failure");
}

#[test]
fn a_failed_download_is_not_retried_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    compose(&world_z1(), 256, 256, &mut tiles, &[], Look::Night);
    tiles.done_tx.send(((1, 0, 0), false)).unwrap();
    assert!(!tiles.poll());
    assert!(tiles.asked.contains_key(&(1, 0, 0)));
}

#[test]
fn marks_are_drawn_at_their_position() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let center = Pos { lat: 0.0, lon: 0.0 };
    let img = compose(
        &world_z1(),
        512,
        512,
        &mut tiles,
        &[(center, Mark::Me)],
        Look::Night,
    );
    assert_eq!(*img.get_pixel(256, 256), ME);
    assert_eq!(*img.get_pixel(256 + 6, 256), ME_RING);
    let halo = *img.get_pixel(256 + 11, 256);
    assert!(halo != BACKGROUND && halo != ME, "a pale halo over the tile: {halo:?}");
    let img = compose(
        &world_z1(),
        512,
        512,
        &mut tiles,
        &[
            (center, Mark::Node(icon())),
            (center, Mark::Selected(icon())),
        ],
        Look::Night,
    );
    assert_eq!(
        *img.get_pixel(256 + 5, 256),
        SELECTED,
        "the selected node's symbol is drawn on top"
    );
    assert_eq!(*img.get_pixel(256, 300), BACKGROUND);
}

#[test]
fn a_mark_near_the_edge_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut tiles = Tiles::offline(dir.path().into());
    let corner = crate::map::from_px(1.0, 1.0, 1);
    compose(
        &world_z1(),
        512,
        512,
        &mut tiles,
        &[(corner, Mark::Selected(icon()))],
        Look::Night,
    );
}

#[test]
fn the_sweep_lights_behind_its_beam_only() {
    let dark = Rgba([0, 8, 2, 255]);
    let mut img = RgbaImage::from_pixel(200, 200, dark);
    // Step 15 of 60: the beam points east, so north east is in its afterglow.
    sweep(&mut img, (100.0, 100.0), 15);
    assert!(img.get_pixel(190, 100)[1] > 150, "on the beam");
    assert!(img.get_pixel(160, 50)[1] > 8, "afterglow behind it");
    assert_eq!(*img.get_pixel(100, 190), dark, "not yet swept");
    assert_eq!(*img.get_pixel(40, 100), dark);
}
