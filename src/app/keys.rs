//! app/keys.rs
//! Keyboard handling: tabs, selection, map moves, scrolling, sending, pinging and adverts. key()
//! returns the radio command (radio::ToRadio) a key asks for, if any.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: GPL-3.0

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{kind_label, App, Filter, Ping, Tab, MY_RADIO};
use crate::config::{MAX_TEXT_BYTES, SCROLL_STEP};
use crate::map::MapView;
use crate::radio::{Firmware, Link, NodeKind, ToRadio};

impl App {
    /// Handle one key press; returns the radio command it asks for, if any.
    pub fn key(&mut self, key: KeyEvent) -> Option<ToRadio> {
        self.notice = None;
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit |= key.code == KeyCode::Char('c');
            return None;
        }
        if self.typing {
            return self.type_key(key.code);
        }
        match (self.tab, key.code) {
            (_, KeyCode::Char('q')) => self.quit = true,
            (_, KeyCode::Char(c @ '1'..='4')) => {
                self.open_tab(Tab::ALL[usize::from(c as u8 - b'1')])
            }
            (Tab::Chats, KeyCode::Up) => self.move_chat(-1),
            (Tab::Chats, KeyCode::Down) => self.move_chat(1),
            (Tab::Chats, KeyCode::Enter | KeyCode::Tab) => self.typing = true,
            (Tab::Chats, KeyCode::PageUp | KeyCode::PageDown) => self.scroll_thread(key.code),
            (Tab::Contacts, KeyCode::Up) => self.select(self.selected.saturating_sub(1)),
            (Tab::Contacts, KeyCode::Down) => self.select(self.selected + 1),
            (Tab::Contacts, KeyCode::Enter) => self.write_to_selected(),
            (Tab::Contacts, KeyCode::Left) => self.move_filter(-1),
            (Tab::Contacts, KeyCode::Right) => self.move_filter(1),
            (Tab::Contacts | Tab::Map, KeyCode::Char('p')) => return self.ping(),
            (Tab::Map, KeyCode::Char('[')) => self.select(self.selected.saturating_sub(1)),
            (Tab::Map, KeyCode::Char(']')) => self.select(self.selected + 1),
            (Tab::Map, KeyCode::Char('+' | '=')) => self.move_map(MapView::zoom_in),
            (Tab::Map, KeyCode::Char('-')) => self.move_map(MapView::zoom_out),
            (Tab::Map, KeyCode::Left) => self.move_map(|v| v.pan(-1, 0)),
            (Tab::Map, KeyCode::Right) => self.move_map(|v| v.pan(1, 0)),
            (Tab::Map, KeyCode::Up) => self.move_map(|v| v.pan(0, 1)),
            (Tab::Map, KeyCode::Down) => self.move_map(|v| v.pan(0, -1)),
            (Tab::Device, KeyCode::Char('a')) => return self.advert(false),
            (Tab::Device, KeyCode::Char('f')) => return self.advert(true),
            _ => {}
        }
        None
    }

    /// A key while the input line of the Chats tab has the keys.
    fn type_key(&mut self, code: KeyCode) -> Option<ToRadio> {
        match code {
            KeyCode::Char(c) if self.input.len() + c.len_utf8() <= MAX_TEXT_BYTES => {
                self.input.push(c)
            }
            KeyCode::Char(_) => {} // input full
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter => return self.send(),
            KeyCode::Esc | KeyCode::Tab => self.typing = false,
            KeyCode::PageUp | KeyCode::PageDown => self.scroll_thread(code),
            _ => {}
        }
        None
    }

    fn open_tab(&mut self, tab: Tab) {
        self.tab = tab;
        if tab == Tab::Chats {
            self.unread.remove(&self.chat);
        }
    }

    /// Bound assumes a message takes at most 4 rows (120 bytes, 40+ columns); the drawing
    /// clamps the real offset anyway.
    fn scroll_thread(&mut self, code: KeyCode) {
        self.scroll = if code == KeyCode::PageUp {
            (self.scroll + SCROLL_STEP).min(self.thread().len() * 4)
        } else {
            self.scroll.saturating_sub(SCROLL_STEP)
        };
    }

    /// Open the conversation `step` rows above or below the open one.
    fn move_chat(&mut self, step: isize) {
        let chats = self.chats();
        let here = chats.iter().position(|r| r.conv == self.chat).unwrap_or(0);
        let there = here.saturating_add_signed(step).min(chats.len() - 1);
        self.open_chat(chats[there].conv.clone());
    }

    /// The filter `step` places left or right of the current one; the selection stays on
    /// its node when the new filter keeps it.
    fn move_filter(&mut self, step: isize) {
        let here = Filter::ALL.iter().position(|&f| f == self.filter).unwrap_or(0);
        let there = here.saturating_add_signed(step).min(Filter::ALL.len() - 1);
        let keep = self.selected_conv();
        self.filter = Filter::ALL[there];
        self.selected = self.rows().iter().position(|r| r.conv == keep).unwrap_or(0);
    }

    fn open_chat(&mut self, conv: String) {
        self.unread.remove(&conv);
        self.chat = conv;
        self.scroll = 0;
    }

    /// Enter on a contact: its conversation, ready to type; my radio opens the Device tab.
    fn write_to_selected(&mut self) {
        let conv = self.selected_conv();
        if conv.is_empty() {
            return;
        }
        if conv == MY_RADIO {
            self.tab = Tab::Device;
            return;
        }
        if let Some(refused) = self.refuses_messages(&conv) {
            self.notice = Some(refused);
            return;
        }
        self.open_chat(conv);
        self.tab = Tab::Chats;
        self.typing = true;
    }

    fn move_map(&mut self, change: impl FnOnce(&mut MapView)) {
        change(&mut self.view);
        self.map_moved = true;
    }

    fn select(&mut self, index: usize) {
        let count = self.rows().len();
        if count == 0 {
            return;
        }
        self.selected = index.min(count - 1);
        let conv = self.selected_conv();
        if conv == MY_RADIO {
            // My radio with its neighbours: centering on it alone could leave them all off-screen.
            if let Some(view) = self.nearby_view() {
                self.move_map(|v| *v = view);
            }
            return;
        }
        let pos = self
            .nodes
            .iter()
            .find(|n| n.prefix == conv)
            .and_then(|n| n.pos);
        if let Some(pos) = pos {
            self.move_map(|v| v.center_on(pos));
        }
    }

    /// Status request to the selected repeater or room.
    fn ping(&mut self) -> Option<ToRadio> {
        if !matches!(self.link, Link::Connected(_)) {
            self.notice = Some("Not connected, ping not sent".into());
            return None;
        }
        let conv = self.selected_conv();
        if self.is_mine(&conv) {
            self.notice = Some("That is your own radio".into());
            return None;
        }
        // MeshCore asks repeaters and rooms for their status; Meshtastic traces the route to
        // any node.
        let meshtastic = self.firmware == Firmware::Meshtastic;
        let Some(node) = self
            .nodes
            .iter()
            .find(|n| n.prefix == conv && (meshtastic || n.kind != NodeKind::Chat))
        else {
            self.notice = Some("PING: repeaters and rooms only".into());
            return None;
        };
        let cmd = ToRadio::Ping {
            key: node.key.clone(),
            prefix: node.prefix.clone(),
        };
        self.pings.insert(conv, Ping::Waiting);
        Some(cmd)
    }

    /// Advert of my radio, to the neighbours only or to the whole network (flood).
    fn advert(&mut self, flood: bool) -> Option<ToRadio> {
        if !matches!(self.link, Link::Connected(_)) {
            self.notice = Some("Not connected, advert not sent".into());
            return None;
        }
        if self.firmware != Firmware::MeshCore {
            self.notice = Some("Adverts are MeshCore only".into());
            return None;
        }
        Some(ToRadio::Advert { flood })
    }

    /// My own radio, by its row or by its id (an old conversation can carry it).
    fn is_mine(&self, conv: &str) -> bool {
        conv == MY_RADIO || self.me.as_ref().is_some_and(|me| me.prefix == conv)
    }

    /// Why `conv` cannot get a message, if it cannot.
    fn refuses_messages(&self, conv: &str) -> Option<String> {
        if self.is_mine(conv) {
            return Some("That is your own radio".into());
        }
        if conv.contains("channel:") {
            return Some("Messages to private channels are not supported".into());
        }
        // Meshtastic routers take direct messages; MeshCore repeaters and rooms do not.
        self.nodes
            .iter()
            .find(|n| {
                n.prefix == conv && n.kind != NodeKind::Chat && self.firmware == Firmware::MeshCore
            })
            .map(|n| format!("Messages to a {} are not supported", kind_label(n.kind)))
    }

    fn send(&mut self) -> Option<ToRadio> {
        if self.input.is_empty() {
            return None;
        }
        if !matches!(self.link, Link::Connected(_)) {
            self.notice = Some("Not connected, message not sent".into());
            return None;
        }
        let conv = self.current_conv();
        if let Some(refused) = self.refuses_messages(&conv) {
            self.notice = Some(refused);
            return None;
        }
        let text = std::mem::take(&mut self.input);
        self.scroll = 0;
        Some(if conv == self.public_conv() {
            ToRadio::SendPublic { text }
        } else {
            ToRadio::SendDirect { prefix: conv, text }
        })
    }
}
