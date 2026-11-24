//! app/mod.rs
//! Application state, without any I/O: link, my radio, contacts, messages, pings, the open
//! tab, the open conversation, the selected contact and the map view. apply() takes the radio's updates (radio::FromRadio) and the ui/ module draws
//! the state. keys.rs turns keys into state changes and radio commands.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

mod console;
mod keys;
mod settings;
#[cfg(test)]
mod tests;
mod update;

pub use settings::Setting;

use std::collections::{HashMap, HashSet};

use crate::config::{FIT_PX, ONLINE_SECS};
use crate::geo::{distance_km, format_distance, Pos};
use crate::history::{is_meshtastic, Msg, Record, MT_PUBLIC, PUBLIC};
use crate::map::MapView;
pub use crate::radio::Ping;
use crate::radio::{Battery, Firmware, Link, MyRadio, Node, NodeKind, Pairing};

/// How far a message I sent went, as MeshVox shows it with ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delivery {
    /// The radio sent it.
    Sent,
    /// The destination acknowledged it (a direct message).
    Received,
    /// My radio heard repeaters relay it this many times (a channel message).
    Repeated(u32),
}

/// Conversation key of the row for my own radio.
pub const MY_RADIO: &str = "radio";

/// The four tabs, as in the MeshVox app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tab {
    Chats,
    Contacts,
    Map,
    Device,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Chats, Tab::Contacts, Tab::Map, Tab::Device];
}

/// The filters of the contact list, in the MeshVox app's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Contacts,
    Favorites,
    Repeaters,
    All,
}

impl Filter {
    pub const ALL: [Filter; 4] = [
        Filter::Contacts,
        Filter::Favorites,
        Filter::Repeaters,
        Filter::All,
    ];

    /// As in MeshVox: Contacts are people, Repeaters are repeaters only (not rooms).
    pub fn keeps(self, node: &Node) -> bool {
        match self {
            Filter::Contacts => node.kind == NodeKind::Chat,
            Filter::Favorites => node.favorite,
            Filter::Repeaters => node.kind == NodeKind::Repeater,
            Filter::All => true,
        }
    }
}

pub struct Row {
    pub conv: String,
    pub label: String,
    pub detail: String,
    pub unread: usize,
}

pub struct App {
    pub link: Link,
    /// What the radio runs; before a radio is found, what the history's last record says.
    pub firmware: Firmware,
    pub self_pos: Option<Pos>,
    pub battery: Option<Battery>,
    pub me: Option<MyRadio>,
    /// When the current link came up.
    pub connected_since: Option<std::time::Instant>,
    pub nodes: Vec<Node>,
    /// The names of the radio's channels, by conversation key.
    pub channels: HashMap<String, String>,
    pub msgs: Vec<Msg>,
    pub acked: HashSet<String>,
    /// Times my radio heard a repeater relay each of my channel messages, by tag.
    pub repeats: HashMap<String, u32>,
    /// Last ping of each node, by prefix.
    pub pings: HashMap<String, Ping>,
    /// The last advert my radio took this session: to the whole network or not, and when.
    pub advert: Option<(bool, i64)>,
    /// The radar sweep turns on the Hacker map (key s).
    pub sweep: bool,
    /// The pairing of a radio (key p in the Device tab), until it fails.
    pub pairing: Option<Pairing>,
    /// The PIN typed while the pairing asks for it.
    pub pin: String,
    /// The console of the Device tab is open: keys go to its command line.
    pub console: bool,
    /// The command being typed.
    pub command: String,
    /// What the console shows: each command ("> " first) and its answer ("OK", "ERR ...").
    pub log: Vec<String>,
    /// How many commands back Up went; 0: the line being typed.
    pub recall: usize,
    /// Console lines scrolled back by PageUp.
    pub log_scroll: usize,
    pub unread: HashMap<String, usize>,
    pub tab: Tab,
    /// Key of the conversation open in the Chats tab.
    pub chat: String,
    /// Index of the selected row in the contact list (rows()).
    pub selected: usize,
    /// Which nodes the contact list shows.
    pub filter: Filter,
    /// The input line of the Chats tab has the keys.
    pub typing: bool,
    pub input: String,
    /// Conversation rows hidden below the view; 0 shows the latest messages.
    pub scroll: usize,
    pub view: MapView,
    /// Size of the map panel in pixels, as last drawn (config::FIT_PX before the first frame).
    pub map_px: (u32, u32),
    pub notice: Option<String>,
    pub quit: bool,
    /// The terminal only shows ASCII, accented letters and line drawing (the Linux
    /// console): no images, no emoji, no symbols.
    pub ascii: bool,
    /// Once the user zooms, pans or picks a node, the map stops fitting the nodes.
    map_moved: bool,
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Chat => "chat",
        NodeKind::Repeater => "repeater",
        NodeKind::Room => "room",
        NodeKind::Sensor => "sensor",
    }
}

impl App {
    pub fn new(device: &str, history: Vec<Record>) -> App {
        let mut app = App {
            link: Link::Searching(device.into()),
            firmware: Firmware::MeshCore,
            self_pos: None,
            battery: None,
            me: None,
            connected_since: None,
            nodes: vec![],
            channels: HashMap::new(),
            msgs: vec![],
            acked: HashSet::new(),
            repeats: HashMap::new(),
            pings: HashMap::new(),
            advert: None,
            sweep: false,
            pairing: None,
            pin: String::new(),
            console: false,
            command: String::new(),
            log: vec![],
            recall: 0,
            log_scroll: 0,
            unread: HashMap::new(),
            tab: Tab::Chats,
            chat: PUBLIC.into(),
            selected: 0,
            filter: Filter::Contacts,
            typing: false,
            input: String::new(),
            scroll: 0,
            view: MapView::default(),
            map_px: FIT_PX,
            notice: None,
            quit: false,
            ascii: false,
            map_moved: false,
        };
        let last_key = history.last().map(|rec| match rec {
            Record::Msg(m) => m.conv.as_str(),
            Record::Ack { tag } | Record::Repeat { tag } => tag.as_str(),
        });
        if last_key.is_some_and(is_meshtastic) {
            app.firmware = Firmware::Meshtastic;
            app.chat = MT_PUBLIC.into();
        }
        for rec in history {
            match rec {
                Record::Msg(m) => app.msgs.push(m),
                Record::Ack { tag } => {
                    app.acked.insert(tag);
                }
                Record::Repeat { tag } => *app.repeats.entry(tag).or_default() += 1,
            }
        }
        app
    }

    /// Conversation key of the public channel of the current firmware.
    pub fn public_conv(&self) -> &'static str {
        match self.firmware {
            Firmware::MeshCore => PUBLIC,
            Firmware::Meshtastic => MT_PUBLIC,
        }
    }

    /// The contact list: my radio (with the All filter only), then the nodes the filter
    /// keeps, by name.
    pub fn rows(&self) -> Vec<Row> {
        let unread = |conv: &str| self.unread.get(conv).copied().unwrap_or(0);
        let mut rows: Vec<Row> = self
            .me
            .iter()
            .filter(|_| self.filter == Filter::All)
            .map(|me| Row {
                conv: MY_RADIO.into(),
                label: me.name.clone(),
                detail: String::new(),
                unread: 0,
            })
            .collect();
        let mut nodes: Vec<&Node> = self.nodes.iter().filter(|n| self.filter.keeps(n)).collect();
        nodes.sort_by_key(|n| n.name.to_lowercase());
        for n in nodes {
            let detail = match (n.kind, self.self_pos, n.pos) {
                (NodeKind::Chat, Some(a), Some(b)) => format_distance(distance_km(a, b)),
                // Unknown distance: left blank.
                (NodeKind::Chat, _, _) => String::new(),
                (kind, _, _) => kind_label(kind).into(),
            };
            rows.push(Row {
                conv: n.prefix.clone(),
                label: n.name.clone(),
                detail,
                unread: unread(&n.prefix),
            });
        }
        rows
    }

    /// The conversation list: the public channel, then the current firmware's conversations,
    /// latest message first. The open conversation is listed even before its first message.
    /// `detail` is the last message.
    pub fn chats(&self) -> Vec<Row> {
        let meshtastic = self.firmware == Firmware::Meshtastic;
        // Last message of each conversation; a map, not a scan per row: the history grows for
        // months.
        let mut last: HashMap<&str, &Msg> = HashMap::new();
        for m in &self.msgs {
            if is_meshtastic(&m.conv) == meshtastic {
                last.insert(&m.conv, m);
            }
        }
        let public = self.public_conv();
        let mut convs: Vec<&str> = last.keys().copied().filter(|&c| c != public).collect();
        if self.chat != public && !last.contains_key(self.chat.as_str()) {
            convs.push(&self.chat);
        }
        // The key breaks ties: HashMap order changes at each call, so equal stamps would swap
        // rows at each redraw.
        convs.sort_by_key(|c| (std::cmp::Reverse(last.get(c).map_or(i64::MAX, |m| m.ts)), *c));
        std::iter::once(public)
            .chain(convs)
            .map(|conv| Row {
                conv: conv.into(),
                label: self.conv_label(conv),
                detail: last.get(conv).map_or_else(String::new, |m| {
                    let from = if m.outgoing { "me" } else { &m.from };
                    format!("{from}: {}", m.text)
                }),
                unread: self.unread.get(conv).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Whether the node `conv` was heard in the last ONLINE_SECS (a 0 stamp means never).
    pub fn is_online(&self, conv: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.nodes
            .iter()
            .any(|n| n.prefix == conv && n.heard != 0 && now - i64::from(n.heard) < ONLINE_SECS)
    }

    /// "# Public", "# " and the channel's name, the node's name, or the conversation key when
    /// the node or the channel's name is unknown.
    pub fn conv_label(&self, conv: &str) -> String {
        if conv == self.public_conv() {
            return "# Public".into();
        }
        if let Some(name) = self.channels.get(conv) {
            return format!("# {}", name.trim_start_matches('#'));
        }
        self.nodes
            .iter()
            .find(|n| n.prefix == conv)
            .map_or_else(|| conv.to_string(), |n| n.name.clone())
    }

    /// The conversation open in the Chats tab.
    pub fn current_conv(&self) -> String {
        self.chat.clone()
    }

    /// The contact selected in the Contacts and Map tabs; empty when the list is.
    pub fn selected_conv(&self) -> String {
        self.rows()
            .into_iter()
            .nth(self.selected)
            .map_or_else(String::new, |r| r.conv)
    }

    pub fn thread(&self) -> Vec<&Msg> {
        self.msgs.iter().filter(|m| m.conv == self.chat).collect()
    }

    /// How far a message I sent went; None for someone else's, or one I cannot follow (a
    /// Meshtastic channel message).
    pub fn status(&self, m: &Msg) -> Option<Delivery> {
        let tag = m.ack.as_ref().filter(|_| m.outgoing)?;
        Some(match self.repeats.get(tag) {
            Some(&n) => Delivery::Repeated(n),
            None if self.acked.contains(tag) => Delivery::Received,
            None => Delivery::Sent,
        })
    }
}
