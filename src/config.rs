//! Persisted settings and the input-tool switching rules copied from the extension
//! (chext_backgroundpage.js, commands "toggle" / "next" / "revert").

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SHIFT: u8 = 1; // same bits as HOTKEYF_SHIFT / CONTROL / ALT
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Hotkey {
    pub vk: u16, // 0 = not set
    pub mods: u8,
}

impl Hotkey {
    pub fn matches(self, vk: u32, mods: u8) -> bool {
        self.vk != 0 && self.vk as u32 == vk && self.mods & 7 == mods
    }
    /// HKM_GETHOTKEY / HKM_SETHOTKEY word: low byte vk, high byte modifiers.
    pub fn to_word(self) -> usize {
        (self.vk as usize & 0xff) | ((self.mods as usize & 7) << 8)
    }
    pub fn from_word(w: usize) -> Self {
        Hotkey { vk: (w & 0xff) as u16, mods: ((w >> 8) & 7) as u8 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub tools: Vec<String>, // "my input tools", in cycle order
    pub cur: String,
    pub on: bool,
    pub prev: String,
    pub prev_on: bool,
    pub activate: Hotkey,
    pub next: Hotkey,
    pub revert: Hotkey,
    pub toggle: Hotkey,
    pub shift_tap: bool,
    pub state: u8, // Chinese / full-width / punctuation bits, shared by the Chinese tools
    pub status_bar: bool,
    pub status_pos: Option<(i32, i32)>,
}

impl Default for Config {
    fn default() -> Self {
        let canto = "yue-hant-t-i0-und".to_string();
        Config {
            tools: vec![canto.clone()],
            cur: canto,
            on: true,
            prev: String::new(),
            prev_on: false,
            activate: Hotkey::default(), // the extension ships this one unset too
            next: Hotkey { vk: b'N' as u16, mods: ALT | SHIFT },
            revert: Hotkey { vk: b'R' as u16, mods: ALT | SHIFT },
            toggle: Hotkey { vk: b'T' as u16, mods: ALT | SHIFT },
            shift_tap: true,
            state: crate::engine::DEFAULT_STATE,
            status_bar: true,
            status_pos: None,
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("GoogleInputTools").join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, serde_json::to_string_pretty(self).unwrap());
    }

    fn remember(&mut self) {
        self.prev = self.cur.clone();
        self.prev_on = self.on;
    }

    pub fn toggle(&mut self) {
        self.remember();
        if !self.cur.is_empty() {
            self.on = !self.on;
        }
    }

    /// tool1 -> tool2 -> ... -> last -> off -> tool1
    pub fn next(&mut self) {
        self.remember();
        let i = match self.tools.iter().position(|t| *t == self.cur) {
            Some(i) if i + 1 != self.tools.len() || self.on => i + 1,
            Some(_) => 0,
            None => 0,
        };
        match self.tools.get(i) {
            Some(t) => {
                self.cur = t.clone();
                self.on = true;
            }
            None => self.on = false,
        }
    }

    /// Swap current and previous (tool, on/off).
    pub fn revert(&mut self) {
        if self.prev.is_empty() {
            return;
        }
        std::mem::swap(&mut self.cur, &mut self.prev);
        std::mem::swap(&mut self.on, &mut self.prev_on);
        if !self.tools.contains(&self.cur) {
            self.tools.push(self.cur.clone());
        }
    }

    pub fn select(&mut self, itc: &str) {
        self.remember();
        self.cur = itc.to_string();
        self.on = true;
        if !self.tools.iter().any(|t| t == itc) {
            self.tools.push(itc.to_string());
        }
    }

    pub fn off(&mut self) {
        self.remember();
        self.on = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(tools: &[&str], cur: &str, on: bool) -> Config {
        Config { tools: tools.iter().map(|s| s.to_string()).collect(), cur: cur.into(), on, ..Config::default() }
    }

    #[test]
    fn next_cycles_through_off() {
        let mut c = cfg(&["a", "b"], "a", true);
        let mut seen = vec![];
        for _ in 0..4 {
            c.next();
            seen.push(if c.on { c.cur.clone() } else { "off".into() });
        }
        assert_eq!(seen, ["b", "off", "a", "b"]);
    }

    #[test]
    fn next_from_unknown_tool_starts_at_first() {
        let mut c = cfg(&["a", "b"], "zz", false);
        c.next();
        assert_eq!((c.cur.as_str(), c.on), ("a", true));
    }

    #[test]
    fn revert_swaps_back_and_forth() {
        let mut c = cfg(&["a", "b"], "a", true);
        c.next(); // b on
        c.toggle(); // b off, prev = b on
        c.revert();
        assert_eq!((c.cur.as_str(), c.on), ("b", true));
        c.revert();
        assert_eq!((c.cur.as_str(), c.on), ("b", false));
    }

    #[test]
    fn revert_readds_removed_tool() {
        let mut c = cfg(&["a", "b"], "a", true);
        c.select("b");
        c.tools.retain(|t| t != "a");
        c.revert();
        assert_eq!(c.cur, "a");
        assert!(c.tools.contains(&"a".to_string()));
    }

    #[test]
    fn hotkey_roundtrip_and_match() {
        let h = Hotkey { vk: b'T' as u16, mods: ALT | SHIFT };
        assert_eq!(Hotkey::from_word(h.to_word()), h);
        assert!(h.matches(b'T' as u32, ALT | SHIFT));
        assert!(!h.matches(b'T' as u32, ALT | SHIFT | CTRL));
        assert!(!Hotkey::default().matches(0, 0));
    }

    #[test]
    fn old_settings_files_still_load() {
        let c: Config = serde_json::from_str(r#"{"cur":"hi-t-i0-und"}"#).unwrap();
        assert_eq!(c.cur, "hi-t-i0-und");
        assert_eq!(c.toggle, Config::default().toggle);
    }
}
