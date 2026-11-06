//! app/tests.rs
//! Tests of the application state and of the keys.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use super::*;
use crate::config::{MAX_TEXT_BYTES, SCROLL_STEP};
use crate::map::FOCUS_ZOOM;
use crate::history::MT_PUBLIC;
use crate::radio::{Firmware, FromRadio, ToRadio};
use super::Filter;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const ALICE: &str = "a4cb8fcbd941";

fn node(prefix: &str, name: &str, kind: NodeKind, pos: Option<Pos>) -> Node {
    Node {
        prefix: prefix.into(),
        name: name.into(),
        kind,
        pos,
        key: format!("{prefix}-key"),
        ..Default::default()
    }
}

fn incoming(conv: &str, text: &str) -> Record {
    Record::Msg(Msg {
        ts: 0,
        conv: conv.into(),
        from: "x".into(),
        text: text.into(),
        outgoing: false,
        ack: None,
    })
}

fn press(app: &mut App, code: KeyCode) -> Option<ToRadio> {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// Connected app with Alice and a room, focus on the input, public channel open, every
/// node listed (All filter).
fn connected() -> App {
    let mut app = App::new("MeshCore-bbok", vec![]);
    app.apply(FromRadio::Link(Link::Connected("MeshCore-bbok".into())));
    app.apply(FromRadio::Nodes(vec![
        node(ALICE, "Alice", NodeKind::Chat, None),
        node("0102030405aa", "Salon", NodeKind::Room, None),
    ]));
    app.typing = true;
    app.filter = Filter::All;
    app
}

#[test]
fn the_contact_list_starts_on_the_people() {
    let mut app = App::new("x", vec![]);
    assert_eq!(app.filter, Filter::Contacts);
    app.apply(FromRadio::MyRadio(my_radio()));
    app.apply(FromRadio::Nodes(vec![
        node("01", "Relay", NodeKind::Repeater, None),
        node("02", "Alice", NodeKind::Chat, None),
    ]));
    assert_eq!(labels(&app), ["Alice"]);
}

fn labels(app: &App) -> Vec<String> {
    app.rows().into_iter().map(|r| r.label).collect()
}

fn chat_labels(app: &App) -> Vec<String> {
    app.chats().into_iter().map(|r| r.label).collect()
}

fn incoming_at(conv: &str, text: &str, ts: i64) -> Record {
    let Record::Msg(msg) = incoming(conv, text) else {
        unreachable!()
    };
    Record::Msg(Msg { ts, ..msg })
}

#[test]
fn contacts_are_sorted_by_name() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::Nodes(vec![
        node("01", "bob", NodeKind::Chat, None),
        node("02", "Alice", NodeKind::Chat, None),
    ]));
    assert_eq!(labels(&app), ["Alice", "bob"]);
}

#[test]
fn chats_list_public_then_latest_first_with_the_last_message() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::Nodes(vec![node(ALICE, "Alice", NodeKind::Chat, None)]));
    app.apply(FromRadio::Record(incoming_at("0102030405aa", "old", 10)));
    app.apply(FromRadio::Record(incoming_at(ALICE, "new", 20)));
    assert_eq!(chat_labels(&app), ["# Public", "Alice", "0102030405aa"]);
    assert_eq!(app.chats()[1].detail, "x: new");
}

#[test]
fn unknown_sender_gets_a_chat() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::Record(incoming(ALICE, "hi")));
    assert_eq!(chat_labels(&app), ["# Public", ALICE]);
    assert_eq!(labels(&app), Vec::<String>::new());
}

#[test]
fn history_is_loaded() {
    let app = App::new(
        "x",
        vec![
            incoming(PUBLIC, "old"),
            Record::Ack {
                tag: "01020304".into(),
            },
        ],
    );
    assert_eq!(app.thread().len(), 1);
    assert!(app.acked.contains("01020304"));
}

#[test]
fn distance_or_dash() {
    let mut app = App::new("x", vec![]);
    app.filter = Filter::All;
    app.apply(FromRadio::Nodes(vec![
        node(
            "01",
            "Alice",
            NodeKind::Chat,
            Some(Pos {
                lat: 44.8207,
                lon: -0.6,
            }),
        ),
        node(
            "02",
            "Relay",
            NodeKind::Repeater,
            Some(Pos {
                lat: 45.0,
                lon: -0.6,
            }),
        ),
    ]));
    assert_eq!(app.rows()[0].detail, "-");
    app.apply(FromRadio::SelfPos(Some(Pos {
        lat: 44.8,
        lon: -0.6,
    })));
    assert_eq!(app.rows()[0].detail, "2.3 km");
    assert_eq!(app.rows()[1].detail, "repeater");
}

#[test]
fn unread_counts_and_clears() {
    let mut app = connected();
    app.apply(FromRadio::Record(incoming(ALICE, "hi")));
    assert_eq!(app.chats()[1].unread, 1);
    app.apply(FromRadio::Record(incoming(PUBLIC, "seen")));
    assert_eq!(app.chats()[0].unread, 0);
    app.typing = false;
    press(&mut app, KeyCode::Down);
    assert_eq!(app.current_conv(), ALICE);
    assert_eq!(app.chats()[1].unread, 0);
}

#[test]
fn a_message_for_the_open_chat_is_unread_on_another_tab() {
    let mut app = connected();
    app.typing = false;
    press(&mut app, KeyCode::Char('3'));
    app.apply(FromRadio::Record(incoming(PUBLIC, "while on the map")));
    assert_eq!(app.chats()[0].unread, 1);
    press(&mut app, KeyCode::Char('1'));
    assert_eq!(app.chats()[0].unread, 0);
}

#[test]
fn ack_marks_received() {
    let mut app = connected();
    let sent = Msg {
        ts: 0,
        conv: ALICE.into(),
        from: "me".into(),
        text: "hi".into(),
        outgoing: true,
        ack: Some("01020304".into()),
    };
    assert_eq!(app.status(&sent), Some("sent"));
    app.apply(FromRadio::Record(Record::Ack {
        tag: "01020304".into(),
    }));
    assert_eq!(app.status(&sent), Some("received"));
    let public = Msg {
        conv: PUBLIC.into(),
        ack: None,
        ..sent
    };
    assert_eq!(app.status(&public), None);
}

#[test]
fn enter_sends_to_the_open_conversation() {
    let mut app = connected();
    type_text(&mut app, "hi");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(ToRadio::SendPublic { text: "hi".into() })
    );
    assert_eq!(app.input, "");
    app.chat = ALICE.into();
    type_text(&mut app, "yo");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(ToRadio::SendDirect {
            prefix: ALICE.into(),
            text: "yo".into()
        })
    );
}

#[test]
fn empty_input_sends_nothing() {
    let mut app = connected();
    assert_eq!(press(&mut app, KeyCode::Enter), None);
}

#[test]
fn enter_offline_keeps_text() {
    let mut app = connected();
    app.apply(FromRadio::Link(Link::Lost("link lost".into())));
    type_text(&mut app, "hi");
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert_eq!(app.input, "hi");
    assert_eq!(
        app.notice.as_deref(),
        Some("Not connected, message not sent")
    );
}

fn failed(conv: &str, text: &str) -> FromRadio {
    FromRadio::SendFailed {
        conv: conv.into(),
        text: text.into(),
        error: "timeout".into(),
    }
}

#[test]
fn send_failure_restores_text() {
    let mut app = connected();
    type_text(&mut app, "hi");
    press(&mut app, KeyCode::Enter);
    app.apply(failed(PUBLIC, "hi"));
    assert_eq!(app.input, "hi");
    assert_eq!(app.notice.as_deref(), Some("Send failed: timeout"));
}

#[test]
fn first_of_two_sends_fails() {
    let mut app = connected();
    type_text(&mut app, "A");
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "B");
    press(&mut app, KeyCode::Enter);
    app.apply(failed(PUBLIC, "A"));
    let b = Msg {
        ts: 0,
        conv: PUBLIC.into(),
        from: "me".into(),
        text: "B".into(),
        outgoing: true,
        ack: None,
    };
    app.apply(FromRadio::Record(Record::Msg(b)));
    assert_eq!(app.input, "A");
}

#[test]
fn failure_goes_back_to_its_conversation() {
    let mut app = connected();
    app.chat = ALICE.into();
    type_text(&mut app, "for Alice");
    press(&mut app, KeyCode::Enter);
    app.chat = PUBLIC.into();
    app.apply(failed(ALICE, "for Alice"));
    assert_eq!(
        (app.current_conv().as_str(), app.input.as_str()),
        (ALICE, "for Alice")
    );
}

#[test]
fn failure_while_typing_keeps_both_texts() {
    let mut app = connected();
    type_text(&mut app, "draft");
    app.apply(failed(ALICE, "for Alice"));
    assert_eq!(app.input, "draft");
    assert_eq!(
        app.notice.as_deref(),
        Some("Send to Alice failed: timeout - text: for Alice")
    );
}

#[test]
fn no_send_to_a_room() {
    let mut app = connected();
    app.chat = "0102030405aa".into();
    type_text(&mut app, "hi");
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert_eq!(app.input, "hi");
    assert_eq!(
        app.notice.as_deref(),
        Some("Messages to a room are not supported")
    );
}

#[test]
fn no_send_to_a_private_channel() {
    let mut app = connected();
    app.apply(FromRadio::Record(incoming("channel:1", "Bob: yo")));
    app.chat = "channel:1".into();
    type_text(&mut app, "hi");
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert_eq!(
        app.notice.as_deref(),
        Some("Messages to private channels are not supported")
    );
}

#[test]
fn input_capped_in_bytes() {
    let mut app = connected();
    type_text(&mut app, &"é".repeat(61));
    assert_eq!(app.input.len(), MAX_TEXT_BYTES);
    assert_eq!(app.input.chars().count(), 60);
}

#[test]
fn q_in_a_full_input_does_not_quit() {
    let mut app = connected();
    type_text(&mut app, &"a".repeat(MAX_TEXT_BYTES));
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit);
}

#[test]
fn backspace_removes_a_char() {
    let mut app = connected();
    type_text(&mut app, "hé");
    press(&mut app, KeyCode::Backspace);
    assert_eq!(app.input, "h");
}

#[test]
fn selecting_a_located_node_centers_map() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::Nodes(vec![node(
        "01",
        "Alice",
        NodeKind::Chat,
        Some(Pos {
            lat: 44.8,
            lon: -0.6,
        }),
    )]));
    app.tab = Tab::Contacts;
    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.view,
        MapView {
            lat: 44.8,
            lon: -0.6,
            zoom: FOCUS_ZOOM
        }
    );
}

#[test]
fn digits_open_the_tabs_and_tab_toggles_the_input() {
    let mut app = App::new("x", vec![]);
    assert_eq!((app.tab, app.typing), (Tab::Chats, false));
    for (key, tab) in [('2', Tab::Contacts), ('3', Tab::Map), ('4', Tab::Device), ('1', Tab::Chats)] {
        press(&mut app, KeyCode::Char(key));
        assert_eq!(app.tab, tab);
    }
    press(&mut app, KeyCode::Tab);
    assert!(app.typing);
    type_text(&mut app, "2");
    assert_eq!((app.tab, app.input.as_str()), (Tab::Chats, "2"));
    press(&mut app, KeyCode::Esc);
    assert!(!app.typing);
}

#[test]
fn map_keys_zoom_and_pan() {
    let mut app = App::new("x", vec![]);
    app.typing = false;
    app.tab = Tab::Map;
    let start = app.view;
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.view.zoom, start.zoom + 1);
    press(&mut app, KeyCode::Right);
    assert!(app.view.lon > start.lon);
    press(&mut app, KeyCode::Char('-'));
    assert_eq!(app.view.zoom, start.zoom);
}

#[test]
fn map_fits_located_nodes_until_the_user_moves_it() {
    let mut app = App::new("x", vec![]);
    let a = Pos {
        lat: 44.80,
        lon: -0.60,
    };
    let b = Pos {
        lat: 44.85,
        lon: -0.50,
    };
    app.apply(FromRadio::Nodes(vec![
        node("01", "A", NodeKind::Repeater, Some(a)),
        node("02", "B", NodeKind::Repeater, Some(b)),
    ]));
    assert!(app.view.zoom >= 10, "{:?}", app.view);
    assert!(
        app.view.lat > 44.8 && app.view.lat < 44.85,
        "{:?}",
        app.view
    );
    app.typing = false;
    app.tab = Tab::Map;
    press(&mut app, KeyCode::Char('-'));
    let moved = app.view;
    app.apply(FromRadio::SelfPos(Some(Pos {
        lat: 48.85,
        lon: 2.35,
    })));
    assert_eq!(app.view, moved);
}

#[test]
fn q_types_in_input_and_quits_elsewhere() {
    let mut app = connected();
    press(&mut app, KeyCode::Char('q'));
    assert_eq!((app.input.as_str(), app.quit), ("q", false));
    app.typing = false;
    app.tab = Tab::Contacts;
    press(&mut app, KeyCode::Char('q'));
    assert!(app.quit);
    let mut app = connected();
    app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.quit);
}

#[test]
fn selection_follows_the_contact_when_list_changes() {
    let mut app = connected();
    app.selected = 0;
    app.apply(FromRadio::Nodes(vec![
        node("ffffffffffff", "Aaron", NodeKind::Chat, None),
        node(ALICE, "Alice", NodeKind::Chat, None),
    ]));
    assert_eq!(app.selected_conv(), ALICE);
}

#[test]
fn page_keys_scroll_and_selecting_resets() {
    let mut app = App::new(
        "x",
        (0..10).map(|i| incoming(PUBLIC, &i.to_string())).collect(),
    );
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.scroll, SCROLL_STEP);
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll, 0);
    press(&mut app, KeyCode::PageUp);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.scroll, 0);
}

#[test]
fn scroll_is_bounded_by_the_thread() {
    let mut app = App::new("x", vec![incoming(PUBLIC, "one")]);
    for _ in 0..100 {
        press(&mut app, KeyCode::PageUp);
    }
    assert!(app.scroll <= 4);
}

#[test]
fn map_fits_only_the_nodes_near_me() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::SelfPos(Some(Pos {
        lat: 44.80,
        lon: -0.60,
    })));
    app.apply(FromRadio::Nodes(vec![
        node(
            "01",
            "Near",
            NodeKind::Repeater,
            Some(Pos {
                lat: 44.89,
                lon: -0.60,
            }),
        ),
        node(
            "02",
            "Pyrenees",
            NodeKind::Repeater,
            Some(Pos {
                lat: 43.0,
                lon: -0.60,
            }),
        ),
    ]));
    assert!(app.view.zoom >= 10, "{:?}", app.view);
}

#[test]
fn p_pings_the_selected_room_or_repeater() {
    let mut app = connected();
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 1;
    let prefix = "0102030405aa";
    assert_eq!(
        press(&mut app, KeyCode::Char('p')),
        Some(ToRadio::Ping {
            key: format!("{prefix}-key"),
            prefix: prefix.into()
        })
    );
    assert_eq!(app.pings.get(prefix), Some(&Ping::Waiting));
}

#[test]
fn p_on_a_person_explains() {
    let mut app = connected();
    app.typing = false;
    app.tab = Tab::Map;
    app.selected = 0;
    assert_eq!(press(&mut app, KeyCode::Char('p')), None);
    assert_eq!(
        app.notice.as_deref(),
        Some("PING: repeaters and rooms only")
    );
}

#[test]
fn p_types_in_the_input() {
    let mut app = connected();
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(app.input, "p");
}

#[test]
fn ping_offline_is_refused() {
    let mut app = connected();
    app.apply(FromRadio::Link(Link::Lost("x".into())));
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 1;
    assert_eq!(press(&mut app, KeyCode::Char('p')), None);
    assert_eq!(app.notice.as_deref(), Some("Not connected, ping not sent"));
}

#[test]
fn ping_results_are_kept() {
    let mut app = connected();
    let pong = Ping::Pong {
        rtt_ms: 1800,
        snr: 6.2,
        battery_mv: 4100,
        uptime: 3 * 86400,
        noise: -112,
    };
    app.apply(FromRadio::Pong {
        prefix: "0102030405aa".into(),
        ping: pong.clone(),
    });
    assert_eq!(app.pings.get("0102030405aa"), Some(&pong));
}

fn my_radio() -> MyRadio {
    MyRadio {
        prefix: "abcdef012345".into(),
        name: "bbok".into(),
        ..Default::default()
    }
}

#[test]
fn my_radio_row_comes_first_and_keeps_the_selection() {
    let mut app = connected();
    app.apply(FromRadio::MyRadio(my_radio()));
    assert_eq!(labels(&app), ["bbok", "Alice", "Salon"]);
    assert_eq!(app.selected_conv(), ALICE);
}

#[test]
fn selecting_my_radio_centers_the_map_on_me() {
    let mut app = connected();
    let me = Pos {
        lat: 44.83,
        lon: -0.56,
    };
    app.apply(FromRadio::SelfPos(Some(me)));
    app.apply(FromRadio::MyRadio(my_radio()));
    app.typing = false;
    app.tab = Tab::Contacts;
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected_conv(), MY_RADIO);
    assert_eq!((app.view.lat, app.view.lon), (me.lat, me.lon));
}

#[test]
fn enter_on_my_radio_opens_the_device_tab() {
    let mut app = connected();
    app.apply(FromRadio::MyRadio(my_radio()));
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 0;
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert_eq!(app.tab, Tab::Device);
}

#[test]
fn enter_on_a_contact_opens_its_chat_ready_to_type() {
    let mut app = connected();
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 0;
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.tab, app.typing), (Tab::Chats, true));
    assert_eq!(app.current_conv(), ALICE);
    assert_eq!(chat_labels(&app), ["# Public", "Alice"], "listed before its first message");
}

#[test]
fn enter_on_a_room_explains_and_stays() {
    let mut app = connected();
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 1;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.tab, Tab::Contacts);
    assert_eq!(
        app.notice.as_deref(),
        Some("Messages to a room are not supported")
    );
}

#[test]
fn no_message_or_ping_to_my_own_id() {
    let mut app = connected();
    let me = my_radio();
    // An old conversation keyed by my own id, as the history can hold.
    app.apply(FromRadio::Record(incoming(&me.prefix, "old")));
    app.apply(FromRadio::MyRadio(me));
    app.chat = "abcdef012345".into();
    type_text(&mut app, "hi");
    assert_eq!(press(&mut app, KeyCode::Enter), None);
    assert_eq!(app.notice.as_deref(), Some("That is your own radio"));
    app.notice = None;
    app.input.clear();
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 0;
    assert_eq!(press(&mut app, KeyCode::Char('p')), None);
    assert_eq!(app.notice.as_deref(), Some("That is your own radio"));
}

#[test]
fn battery_details_and_link_time_are_kept() {
    let mut app = App::new("x", vec![]);
    app.apply(FromRadio::Battery(Battery {
        pct: 87,
        mv: 3980,
        used_kb: Some(12),
        total_kb: Some(1024),
    }));
    assert_eq!(app.battery.map(|b| (b.pct, b.mv)), Some((87, 3980)));
    assert!(app.connected_since.is_none());
    app.apply(FromRadio::Link(Link::Connected("MeshCore-bbok".into())));
    assert!(app.connected_since.is_some());
    app.apply(FromRadio::Link(Link::Lost("x".into())));
    assert!(app.connected_since.is_none());
}

#[test]
fn link_lost_is_kept() {
    let mut app = connected();
    app.apply(FromRadio::Link(Link::Lost("x".into())));
    assert_eq!(app.link, Link::Lost("x".into()));
}

fn inside(view: MapView, pos: Pos, (w, h): (u32, u32)) -> bool {
    let (x, y) = view.screen_px(pos, w, h);
    (0.0..f64::from(w)).contains(&x) && (0.0..f64::from(h)).contains(&y)
}

#[test]
fn selecting_my_radio_fits_my_neighbours_in_the_real_map_size() {
    let mut app = connected();
    let me = Pos {
        lat: 44.80,
        lon: -0.60,
    };
    let relay = Pos {
        lat: 44.80,
        lon: -0.50,
    };
    app.apply(FromRadio::SelfPos(Some(me)));
    app.apply(FromRadio::Nodes(vec![node(
        "0a",
        "Relay",
        NodeKind::Repeater,
        Some(relay),
    )]));
    app.apply(FromRadio::MyRadio(MyRadio {
        name: "bbok".into(),
        ..Default::default()
    }));
    app.set_map_px((360, 300));
    app.typing = false;
    app.tab = Tab::Map;
    press(&mut app, KeyCode::Char('+'));
    app.typing = false;
    app.tab = Tab::Contacts;
    app.selected = 1;
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected_conv(), MY_RADIO);
    assert!(
        inside(app.view, me, (360, 300)) && inside(app.view, relay, (360, 300)),
        "{:?}",
        app.view
    );
}

#[test]
fn a_new_map_size_refits_until_the_user_moves_the_map() {
    let mut app = App::new("x", vec![]);
    let a = Pos {
        lat: 44.80,
        lon: -0.60,
    };
    let b = Pos {
        lat: 44.90,
        lon: -0.40,
    };
    app.apply(FromRadio::Nodes(vec![
        node("01", "A", NodeKind::Repeater, Some(a)),
        node("02", "B", NodeKind::Repeater, Some(b)),
    ]));
    app.set_map_px((200, 150));
    assert!(
        inside(app.view, a, (200, 150)) && inside(app.view, b, (200, 150)),
        "{:?}",
        app.view
    );
    app.typing = false;
    app.tab = Tab::Map;
    press(&mut app, KeyCode::Char('-'));
    let moved = app.view;
    app.set_map_px((900, 600));
    assert_eq!(app.view, moved);
}

fn convs(app: &App) -> Vec<String> {
    app.chats().into_iter().map(|r| r.conv).collect()
}

#[test]
fn firmware_comes_from_the_last_history_record() {
    let mt = App::new("x", vec![incoming(PUBLIC, "a"), incoming(MT_PUBLIC, "b")]);
    assert_eq!(mt.firmware, Firmware::Meshtastic);
    let mc = App::new("x", vec![incoming(MT_PUBLIC, "b"), incoming("a1b2c3d4e5f6", "a")]);
    assert_eq!(mc.firmware, Firmware::MeshCore);
    assert_eq!(App::new("x", vec![]).firmware, Firmware::MeshCore);
}

#[test]
fn chats_show_only_the_current_firmware() {
    let mut app = App::new(
        "x",
        vec![
            incoming(PUBLIC, "a"),
            incoming(MT_PUBLIC, "c"),
            incoming("mt:!0badcafe", "d"),
            incoming("a1b2c3d4e5f6", "b"),
        ],
    );
    assert_eq!(convs(&app), [PUBLIC, "a1b2c3d4e5f6"]);
    app.apply(FromRadio::Firmware(Firmware::Meshtastic));
    assert_eq!(convs(&app), [MT_PUBLIC, "mt:!0badcafe"]);
    assert_eq!((chat_labels(&app)[0].as_str(), app.current_conv().as_str()), ("# Public", MT_PUBLIC));
}

#[test]
fn switching_firmware_clears_the_radio_state() {
    let mut app = connected();
    app.apply(FromRadio::Battery(Battery {
        pct: 50,
        mv: 3840,
        used_kb: None,
        total_kb: None,
    }));
    app.apply(FromRadio::MyRadio(MyRadio::default()));
    app.apply(FromRadio::Firmware(Firmware::MeshCore));
    assert_eq!(app.nodes.len(), 2, "same firmware: nothing changes");
    app.apply(FromRadio::Firmware(Firmware::Meshtastic));
    assert!(app.nodes.is_empty());
    assert!(app.battery.is_none() && app.me.is_none());
}

#[test]
fn enter_on_the_meshtastic_public_row_sends_public() {
    let mut app = connected();
    app.apply(FromRadio::Firmware(Firmware::Meshtastic));
    type_text(&mut app, "hi");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(ToRadio::SendPublic { text: "hi".into() })
    );
}

/// Meshtastic, connected, with Bob (a person) and a router.
fn meshtastic() -> App {
    let mut app = connected();
    app.apply(FromRadio::Firmware(Firmware::Meshtastic));
    app.apply(FromRadio::Nodes(vec![
        node("mt:!0badcafe", "Bob", NodeKind::Chat, None),
        node("mt:!0000beef", "Roof", NodeKind::Repeater, None),
    ]));
    app
}

#[test]
fn meshtastic_pings_any_node() {
    let mut app = meshtastic();
    app.typing = false;
    app.tab = Tab::Map;
    app.selected = 0;
    assert_eq!(
        press(&mut app, KeyCode::Char('p')),
        Some(ToRadio::Ping {
            key: "mt:!0badcafe-key".into(),
            prefix: "mt:!0badcafe".into()
        })
    );
}

#[test]
fn meshtastic_sends_to_a_router() {
    let mut app = meshtastic();
    app.chat = "mt:!0000beef".into();
    type_text(&mut app, "hi");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        Some(ToRadio::SendDirect {
            prefix: "mt:!0000beef".into(),
            text: "hi".into()
        })
    );
}

/// Alice (person, favorite), Bob (person), a repeater and a room, with my radio.
fn filtered() -> App {
    let mut app = connected();
    app.apply(FromRadio::MyRadio(my_radio()));
    app.apply(FromRadio::Nodes(vec![
        Node {
            favorite: true,
            ..node(ALICE, "Alice", NodeKind::Chat, None)
        },
        node("0b0b0b0b0b0b", "Bob", NodeKind::Chat, None),
        node("0102030405bb", "Relay", NodeKind::Repeater, None),
        node("0102030405aa", "Salon", NodeKind::Room, None),
    ]));
    app.typing = false;
    app.tab = Tab::Contacts;
    app.filter = Filter::All;
    app
}

#[test]
fn filters_keep_what_meshvox_keeps() {
    let mut app = filtered();
    assert_eq!(labels(&app), ["bbok", "Alice", "Bob", "Relay", "Salon"]);
    for (filter, want) in [
        (Filter::Contacts, vec!["Alice", "Bob"]),
        (Filter::Favorites, vec!["Alice"]),
        (Filter::Repeaters, vec!["Relay"]),
    ] {
        app.filter = filter;
        assert_eq!(labels(&app), want, "{filter:?}");
    }
}

#[test]
fn left_and_right_change_the_filter_and_keep_the_node() {
    let mut app = filtered();
    assert_eq!(app.filter, Filter::All);
    app.selected = 1;
    assert_eq!(app.selected_conv(), ALICE);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.filter, Filter::Repeaters);
    assert_eq!(app.selected, 0, "Alice is gone: first row");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.filter, Filter::All, "no wrap past the last filter");
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    app.selected = 0;
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.filter, Filter::Contacts, "no wrap past the first one");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected_conv(), "0b0b0b0b0b0b");
    press(&mut app, KeyCode::Right);
    assert_eq!(app.filter, Filter::Favorites);
    assert_eq!(app.selected_conv(), ALICE, "Bob is no favorite: first row");
}

#[test]
fn an_empty_filter_does_not_break_the_keys() {
    let mut app = filtered();
    app.apply(FromRadio::Nodes(vec![]));
    app.filter = Filter::Favorites;
    for code in [KeyCode::Down, KeyCode::Up, KeyCode::Enter, KeyCode::Char('p')] {
        press(&mut app, code);
    }
    assert_eq!(app.selected_conv(), "");
}

#[test]
fn chats_with_equal_stamps_keep_a_stable_order() {
    let mut app = App::new("x", vec![]);
    for conv in ["0102030405aa", "0102030405bb", "0102030405cc", "0102030405dd"] {
        app.apply(FromRadio::Record(incoming_at(conv, "hi", 10)));
    }
    let first = chat_labels(&app);
    for _ in 0..50 {
        assert_eq!(chat_labels(&app), first);
    }
}
