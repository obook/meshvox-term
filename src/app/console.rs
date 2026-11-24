//! app/console.rs
//! The console of the Device tab: a typed command becomes a radio command (radio::ToRadio),
//! a setting checked by settings.rs first. The console shows each command and its answer.
//!
//! Author: O. Booklage
//! Date: November 2026
//! Licence: GPL-3.0

use ratatui::crossterm::event::KeyCode;

use super::settings::PRESETS;
use super::{App, Setting};
use crate::config::SCROLL_STEP;
use crate::radio::ToRadio;

/// Longest command line, in bytes.
const MAX_COMMAND_BYTES: usize = 64;
/// Console lines kept.
const MAX_LOG_LINES: usize = 200;

impl App {
    /// Add a line to the console, forgetting the oldest past MAX_LOG_LINES.
    pub(super) fn say(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        let extra = self.log.len().saturating_sub(MAX_LOG_LINES);
        self.log.drain(..extra);
    }

    /// A key while the console has the keys.
    pub(super) fn console_key(&mut self, code: KeyCode) -> Option<ToRadio> {
        match code {
            KeyCode::Char(c) if self.command.len() + c.len_utf8() <= MAX_COMMAND_BYTES => {
                self.command.push(c)
            }
            KeyCode::Backspace => {
                self.command.pop();
            }
            KeyCode::Up => self.recall(1),
            KeyCode::Down => self.recall(-1),
            KeyCode::PageUp => self.log_scroll = (self.log_scroll + SCROLL_STEP).min(self.log.len()),
            KeyCode::PageDown => self.log_scroll = self.log_scroll.saturating_sub(SCROLL_STEP),
            KeyCode::Esc => self.console = false,
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.command);
                let line = line.trim();
                self.recall = 0;
                self.log_scroll = 0;
                if !line.is_empty() {
                    self.say(format!("> {line}"));
                    return self.run(line);
                }
            }
            _ => {}
        }
        None
    }

    /// Fill the line with the command typed `step` more commands back (0: an empty line).
    fn recall(&mut self, step: isize) {
        let typed: Vec<&str> = self.log.iter().rev().filter_map(|l| l.strip_prefix("> ")).collect();
        self.recall = self.recall.saturating_add_signed(step).min(typed.len());
        self.command = match self.recall {
            0 => String::new(),
            n => typed[n - 1].into(),
        };
    }

    /// Run one command; what it gives goes to the console.
    fn run(&mut self, line: &str) -> Option<ToRadio> {
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match (verb, rest) {
            ("help", _) => {
                for s in Setting::ALL {
                    self.say(format!("set {:<9} {}", s.word(), s.hint()));
                }
                self.say("get [setting]   presets   advert [flood]   map upload");
                self.say("reboot   pair   clear   Esc: close   Up/Down: last commands");
            }
            ("clear", _) => self.log.clear(),
            ("presets", _) => {
                for (n, (title, values)) in PRESETS.iter().enumerate() {
                    self.say(format!("{:>2} {title}: {values}", n + 1));
                }
                self.say("set radio preset <n> takes one");
            }
            ("get", word) => self.get(word),
            ("set", args) => return self.set(args),
            ("advert", "") => return self.advert(false),
            ("advert", "flood") => return self.advert(true),
            ("map", "upload") => {
                self.settable()?;
                return Some(ToRadio::MapUpload);
            }
            ("reboot", "") => return self.reboot(),
            ("pair", "") => return self.pair(),
            _ => self.say("ERR  Unknown command; help lists them"),
        }
        None
    }

    /// Show the value of a setting, or of all of them.
    fn get(&mut self, word: &str) {
        let settings = match word {
            "" => Setting::ALL.to_vec(),
            word => match Setting::named(word) {
                Some(setting) => vec![setting],
                None => return self.say("ERR  Unknown setting; help lists them"),
            },
        };
        let Some(me) = &self.me else {
            return self.say("ERR  No radio connected yet");
        };
        let lines: Vec<String> = settings
            .iter()
            .map(|s| format!("{:<9} {}", s.word(), s.current(me, self.self_pos)))
            .collect();
        for line in lines {
            self.say(line);
        }
    }

    /// "set <setting> <value>": the radio command, or why not.
    fn set(&mut self, args: &str) -> Option<ToRadio> {
        let (word, value) = args.split_once(' ').unwrap_or((args, ""));
        let Some(setting) = Setting::named(word) else {
            self.say("ERR  Unknown setting; help lists them");
            return None;
        };
        let parsed = setting.parse(value, self.settable()?);
        parsed.map_err(|why| self.say(format!("ERR  {why}"))).ok()
    }
}
