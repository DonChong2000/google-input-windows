//! Composition state machine copied from the extension's `tC`/`kC`/`sC` classes.
//! Pure logic, no Win32: the hook turns key events into `Key`s, this decides what is
//! swallowed, what gets typed, and what to look up.

use crate::google::{Cands, Query};
use crate::tools::{self, Tool};

pub const CHINESE: u8 = 1; // state bit 1: Chinese mode (tlang)
pub const FULL: u8 = 2; // bit 2: full-width letters/digits
pub const PUNCT: u8 = 4; // bit 4: Chinese punctuation
pub const DEFAULT_STATE: u8 = CHINESE | PUNCT; // config key 15 = 5
const MAX_SOURCE: usize = 50; // key 13
const CTX_CHARS: usize = 20; // Hx.D(20)

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Char(char), // printable, as the keyboard layout types it (Shift applied)
    Space,
    Enter,
    Esc,
    Back,
    Delete,
    Tab,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    PgUp,
    PgDn,
}

#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub swallow: bool,
    pub commit: String,
    /// The key needs candidates that haven't arrived: hold it and feed it again later.
    pub wait: bool,
}

impl Out {
    fn eat() -> Self {
        Out { swallow: true, ..Out::default() }
    }
    fn text(s: String) -> Self {
        Out { swallow: true, commit: s, wait: false }
    }
}

pub struct Engine {
    pub tool: &'static Tool,
    pub state: u8,
    src: Vec<char>,              // composition letters as typed
    caret: usize,                // kC's A; 0 = at the front, which also means "look up everything"
    segs: Vec<(String, String)>, // partial commits (letters, text) held until the rest is chosen
    raw: bool,                   // uppercase-first "Fa" mode: no lookups, commit as typed
    hl: usize,
    cands: Option<Cands>, // answer for the current query
    shown: Option<Cands>, // what the box shows; stays (greyed) while the next answer loads
    more: bool,           // the server may have more than we asked for
    mult: usize,          // request num = mult * page + 1
    quotes: [bool; 2],
    ctx: String, // what was typed just before the caret, sent as pre-context
}

/// What the candidate box shows.
pub struct View {
    pub segs: String,
    pub comp: String,
    pub caret: usize, // characters of segs+comp before the caret
    pub words: Vec<String>,
    pub ann: Vec<String>,
    pub numbers: Vec<String>,
    pub hl: Option<usize>, // within `words`
    pub stale: bool,
    pub prev_page: bool,
    pub next_page: bool,
}

impl Engine {
    pub fn new(tool: &'static Tool, state: u8) -> Self {
        Engine {
            tool,
            state,
            src: Vec::new(),
            caret: 0,
            segs: vec![],
            raw: false,
            hl: 0,
            cands: None,
            shown: None,
            more: false,
            mult: 2,
            quotes: [false; 2],
            ctx: String::new(),
        }
    }

    pub fn composing(&self) -> bool {
        !self.src.is_empty() || !self.segs.is_empty()
    }

    fn zh_mode(&self) -> bool {
        !self.tool.chinese || self.state & CHINESE != 0
    }

    /// Discard the composition (Esc, a click elsewhere, Ctrl/Alt shortcuts, another window).
    pub fn reset(&mut self) {
        let (tool, state, quotes, ctx) = (self.tool, self.state, self.quotes, std::mem::take(&mut self.ctx));
        *self = Engine { quotes, ctx, ..Engine::new(tool, state) };
    }

    /// The caret moved somewhere we can't follow.
    pub fn forget_context(&mut self) {
        self.ctx.clear();
    }

    fn held(&self) -> String {
        self.segs.iter().map(|s| s.1.as_str()).collect()
    }

    /// Commit what was typed as-is (Enter, Shift tap, no candidates). Cangjie commits radicals.
    pub fn commit_raw(&mut self) -> String {
        let s = self.held() + &self.display();
        self.finish(s)
    }

    /// Every commit of a key-3 tool gets a trailing space (sC.S on each "cmt").
    fn finish(&mut self, text: String) -> String {
        let text = self.spaced(text);
        self.reset();
        self.remember(&text);
        text
    }

    fn spaced(&self, mut text: String) -> String {
        if self.tool.trailing_space && !text.is_empty() {
            text.push(' ');
        }
        text
    }

    fn remember(&mut self, text: &str) {
        self.ctx.push_str(text);
        let n = self.ctx.chars().count();
        if n > CTX_CHARS {
            self.ctx = self.ctx.chars().skip(n - CTX_CHARS).collect();
        }
    }

    fn display(&self) -> String {
        if self.tool.radicals {
            self.src.iter().map(|&c| tools::radical(c).unwrap_or(c)).collect()
        } else {
            self.src.iter().collect()
        }
    }

    /// kC.S: letters up to the caret, or all of them when the caret is at the front.
    fn req_src(&self) -> String {
        let s: String = if self.caret == 0 { self.src.iter().collect() } else { self.src[..self.caret].iter().collect() };
        if self.tool.case_sensitive {
            s
        } else {
            s.to_lowercase()
        }
    }

    fn make_query(&self) -> Query {
        let held = self.held();
        let ctx = if self.tool.context { format!("{}{held}", self.ctx) } else { held };
        Query::new(self.tool.itc, &ctx, &self.req_src(), self.mult * self.tool.page + 1)
    }

    /// The lookup the current composition needs, if its answer isn't here yet.
    pub fn query(&self) -> Option<Query> {
        (!self.src.is_empty() && !self.raw && self.cands.is_none()).then(|| self.make_query())
    }

    /// An answer arrived. Ignored (false) unless it is for what is typed now.
    pub fn answer(&mut self, q: &Query, mut c: Cands) -> bool {
        if self.query().as_ref() != Some(q) {
            return false;
        }
        self.more = c.words.len() >= q.num;
        if self.more {
            c.truncate(q.num - 1);
        }
        if !self.tool.enter_raw {
            self.offer_typed(&mut c);
        }
        if self.hl >= c.words.len() {
            self.hl = 0;
        }
        self.shown = Some(c.clone());
        self.cands = Some(c);
        true
    }

    /// Bw: tools whose Enter picks a candidate also offer the typed text itself,
    /// at the end of the first page (or the end of a shorter list).
    fn offer_typed(&self, c: &mut Cands) {
        let typed = self.req_src();
        if c.words.is_empty() || c.words.contains(&typed) {
            return;
        }
        let at = if c.words.len() >= self.tool.page { self.tool.page - 1 } else { c.words.len() };
        c.words.insert(at, typed.clone());
        c.lens.insert(at, typed.chars().count());
        c.ann.insert(at.min(c.ann.len()), String::new());
    }

    /// A lookup failed: held keys fall back to the raw letters.
    pub fn failed(&mut self, q: &Query) {
        if self.query().as_ref() == Some(q) {
            self.cands = Some(Cands::default());
            self.more = false;
        }
    }

    fn edited(&mut self) {
        self.cands = None;
        self.hl = 0;
        self.mult = 2;
        if !self.composing() {
            self.reset();
        }
    }

    pub fn view(&self) -> View {
        let page = self.tool.page;
        let all = match (&self.shown, self.raw) {
            (Some(c), false) => c.clone(),
            _ => Cands::default(),
        };
        let first = (self.hl / page * page).min(all.words.len());
        let end = (first + page).min(all.words.len());
        let words = all.words[first..end].to_vec();
        // numbers are hidden for a lone suggestion
        let numbers = (1..=words.len()).map(|n| if all.words.len() == 1 { String::new() } else { tools::number(self.tool, n) }).collect();
        let segs = self.held();
        View {
            caret: segs.chars().count() + self.caret,
            segs,
            comp: self.display(),
            ann: if self.tool.annotate { all.ann[first.min(all.ann.len())..end.min(all.ann.len())].to_vec() } else { vec![] },
            numbers,
            hl: (self.cands.is_some() && !words.is_empty()).then(|| self.hl - first),
            words,
            stale: self.cands.is_none(),
            prev_page: first > 0,
            next_page: end < all.words.len() || self.more,
        }
    }

    fn convert(&mut self, c: char) -> Option<String> {
        tools::convert(self.tool, self.state, c, &mut self.quotes)
    }

    fn pick(&mut self, i: usize) -> Out {
        let Some(c) = &self.cands else { return Out { wait: true, ..Out::eat() } };
        if c.words.is_empty() {
            return Out::text(self.commit_raw());
        }
        if i >= c.words.len() {
            return Out::eat();
        }
        let (target, used) = (c.words[i].clone(), c.lens[i].min(self.src.len()));
        if used == 0 || used >= self.src.len() {
            let all = self.held() + &target;
            return Out::text(self.finish(all));
        }
        // partial match: hold this segment and look up the rest (kC.I)
        self.segs.push((self.src[..used].iter().collect(), target));
        self.src.drain(..used);
        self.caret = self.caret.saturating_sub(used);
        self.edited();
        Out::eat()
    }

    fn move_hl(&mut self, to: isize) -> Out {
        let Some(c) = &self.cands else { return Out::eat() };
        let len = c.words.len() as isize;
        if len == 0 {
            return Out::eat();
        }
        if to >= len {
            if self.more {
                // past what was loaded: ask again for twice as many (qa)
                self.mult *= 2;
                self.cands = None;
                self.hl = to as usize;
            } else {
                self.hl = 0;
            }
        } else {
            self.hl = to.max(0) as usize;
        }
        Out::eat()
    }

    fn page_start(&self) -> isize {
        (self.hl / self.tool.page * self.tool.page) as isize
    }

    fn is_letter(&self, c: char) -> bool {
        c.is_ascii_alphabetic() || self.tool.extra.contains(c)
    }

    /// Digits that are also composition letters (hi, ar...) select only once a non-digit
    /// was typed, and never in tools without candidate numbers (tC.Da).
    fn digit_selects(&self) -> bool {
        self.tool.digit0.is_some() && self.src.iter().any(|c| !c.is_ascii_digit())
    }

    pub fn key(&mut self, k: Key) -> Out {
        if !self.composing() {
            return self.idle(k);
        }
        let page = self.tool.page as isize;
        let loaded = self.cands.is_some() && !self.raw;
        match k {
            Key::Char(c) if self.is_letter(c) && !(c.is_ascii_digit() && self.digit_selects()) => {
                if self.src.len() < MAX_SOURCE {
                    let at = self.caret;
                    self.src.insert(at, c);
                    self.caret = at + 1;
                    self.edited();
                }
                Out::eat()
            }
            Key::Char(c @ '0'..='9') => {
                let n = c as usize - '0' as usize;
                let i = self.page_start() as usize + n.wrapping_sub(1);
                let have = self.cands.as_ref().is_some_and(|c| i < c.words.len());
                if loaded && (1..=self.tool.page).contains(&n) && have {
                    self.pick(i)
                } else {
                    Out::eat() // also in raw mode and while loading (sC.J)
                }
            }
            Key::Char(c) if loaded && self.tool.next_keys.contains(c) => self.move_hl(self.page_start() + page),
            Key::Char(c) if loaded && self.tool.prev_keys.contains(c) => self.move_hl(self.page_start() - page),
            Key::Char(c) => {
                // commit trigger: the highlighted candidate, then the (converted) character
                let mut out = if self.raw { Out::text(self.commit_raw()) } else { self.pick(self.hl) };
                if out.wait {
                    return out;
                }
                if self.composing() {
                    out.commit += &self.commit_raw(); // a partial match left letters behind
                }
                out.commit += &self.punct(c);
                out
            }
            Key::Space if self.raw => {
                // ponytail: by code reading Google eats this space in raw mode; we keep it
                let s = self.commit_raw() + " ";
                Out::text(s)
            }
            Key::Space if self.tool.extra.contains(' ') => self.key(Key::Char(' ')), // ar: a letter
            Key::Space => self.pick(self.hl),
            Key::Enter if self.raw || self.tool.enter_raw => Out::text(self.commit_raw()),
            Key::Enter => self.pick(self.hl),
            Key::Esc => {
                self.reset();
                Out::eat()
            }
            Key::Back => {
                if let Some((s, _)) = self.segs.pop() {
                    let n = s.chars().count();
                    self.src.splice(0..0, s.chars());
                    self.caret += n;
                } else if self.caret > 0 {
                    self.src.remove(self.caret - 1);
                    self.caret -= 1;
                }
                self.edited();
                Out::eat()
            }
            Key::Left | Key::Right | Key::Home | Key::End if self.tool.caret_move => {
                let len = self.src.len();
                let to = match k {
                    Key::Left => self.caret.saturating_sub(1),
                    Key::Right => (self.caret + 1).min(len),
                    Key::Home => 0,
                    _ => len,
                };
                if to != self.caret {
                    // ha(): only an actual move re-queries
                    self.caret = to;
                    self.cands = None;
                    self.hl = 0;
                    self.mult = 2;
                }
                Out::eat()
            }
            Key::Left | Key::Right if !self.raw => {
                // tools without caret movement page with the arrows (H(±1)); RTL mirrored
                let d = if (k == Key::Right) != self.tool.rtl { page } else { -page };
                self.move_hl(self.page_start() + d)
            }
            Key::Home if loaded => self.move_hl(0),
            Key::End if loaded => {
                let last = self.cands.as_ref().map_or(0, |c| c.words.len()) as isize - 1;
                self.move_hl(last)
            }
            Key::Up => self.move_hl(self.hl as isize - 1),
            Key::Down => self.move_hl(self.hl as isize + 1),
            Key::PgUp => self.move_hl(self.page_start() - page),
            Key::PgDn => self.move_hl(self.page_start() + page),
            _ => Out::eat(), // Tab, Delete
        }
    }

    /// A punctuation key typed as a commit: converted, spaced for key-3 tools, remembered.
    fn punct(&mut self, c: char) -> String {
        let p = self.convert(c).unwrap_or_else(|| c.to_string());
        let p = self.spaced(p);
        self.remember(&p);
        p
    }

    /// A key held because its candidates hadn't arrived, run now that they have (or
    /// failed). Like the extension's delayed commit: the first candidate is committed in
    /// full, whatever it matched, and a held punctuation key follows it.
    pub fn resume(&mut self, k: Key) -> Out {
        if !self.composing() {
            return self.key(k);
        }
        let first = self.cands.as_ref().and_then(|c| c.words.first().cloned());
        let mut commit = match first {
            Some(w) => {
                let all = self.held() + &w;
                self.finish(all)
            }
            None => self.commit_raw(),
        };
        if let Key::Char(c) = k {
            commit += &self.punct(c);
        }
        Out::text(commit)
    }

    fn idle(&mut self, k: Key) -> Out {
        let Key::Char(c) = k else {
            match k {
                Key::Space => self.remember(" "),
                Key::Back => {
                    self.ctx.pop();
                }
                _ => self.forget_context(),
            }
            return Out::default();
        };
        // sC.U runs before the bindings: punctuation / full-width first
        if let Some(s) = self.convert(c) {
            let s = self.spaced(s);
            self.remember(&s);
            return Out::text(s);
        }
        if self.zh_mode() && self.is_letter(c) {
            self.raw = self.tool.chinese && !self.tool.case_sensitive && c.is_ascii_uppercase();
            self.src.push(c);
            self.caret = 1;
            self.edited();
            return Out::eat();
        }
        self.remember(&c.to_string());
        Out::default()
    }

    /// Shift tapped alone: commit the raw letters, switch Chinese/English.
    pub fn toggle_lang(&mut self) -> String {
        let s = if self.composing() { self.commit_raw() } else { String::new() };
        self.state ^= CHINESE;
        s
    }

    /// Shift+Space: while composing it only commits the raw letters.
    pub fn toggle_width(&mut self) -> String {
        if self.composing() {
            return self.commit_raw();
        }
        self.state ^= FULL;
        String::new()
    }

    /// Ctrl+.: discards the composition, then switches punctuation.
    pub fn toggle_punct(&mut self) {
        self.reset();
        self.state ^= PUNCT;
    }

    /// Mouse click on candidate `i` of the shown page.
    pub fn click(&mut self, i: usize) -> Out {
        if self.cands.is_none() {
            return Out::eat();
        }
        self.pick(self.page_start() as usize + i)
    }

    pub fn click_page(&mut self, next: bool) {
        let p = self.tool.page as isize;
        self.move_hl(self.page_start() + if next { p } else { -p });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::find;

    fn eng(itc: &str) -> Engine {
        Engine::new(find(itc).unwrap(), DEFAULT_STATE)
    }

    fn typ(e: &mut Engine, s: &str) -> String {
        s.chars().map(|c| e.key(Key::Char(c)).commit).collect()
    }

    fn cands(words: &[&str], lens: &[usize]) -> Cands {
        Cands { words: words.iter().map(|s| s.to_string()).collect(), lens: lens.to_vec(), ann: vec![String::new(); words.len()] }
    }

    fn answer(e: &mut Engine, words: &[&str], lens: &[usize]) {
        let q = e.query().expect("a lookup is due");
        assert!(e.answer(&q, cands(words, lens)));
    }

    #[test]
    fn space_commits_highlighted_without_trailing_space() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "neihou");
        assert_eq!(e.query().unwrap().text, "neihou");
        assert_eq!(e.query().unwrap().num, 13);
        answer(&mut e, &["你好", "您好"], &[6, 6]);
        e.key(Key::Down);
        assert_eq!(e.key(Key::Space).commit, "您好");
        assert!(!e.composing());
    }

    #[test]
    fn transliteration_tools_add_a_space_and_enter_picks() {
        let mut e = eng("hi-t-i0-und");
        typ(&mut e, "namaste");
        answer(&mut e, &["नमस्ते"], &[7]);
        assert_eq!(e.key(Key::Enter).commit, "नमस्ते ");
    }

    #[test]
    fn chinese_enter_commits_letters() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "abc");
        assert_eq!(e.key(Key::Enter).commit, "abc");
    }

    #[test]
    fn partial_match_is_held_until_the_rest_is_chosen() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "ngodeigau");
        answer(&mut e, &["我哋搞", "我哋"], &[9, 6]);
        let out = e.key(Key::Char('2'));
        assert_eq!(out.commit, "", "nothing typed yet");
        assert_eq!(e.view().segs, "我哋");
        assert_eq!(e.view().comp, "gau");
        assert_eq!(e.query().unwrap().text, "|我哋,gau", "held text goes in as context");
        answer(&mut e, &["搞"], &[3]);
        assert_eq!(e.key(Key::Space).commit, "我哋搞");
    }

    #[test]
    fn backspace_restores_a_held_segment() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "ngodeigau");
        answer(&mut e, &["我哋搞", "我哋"], &[9, 6]);
        e.key(Key::Char('2'));
        e.key(Key::Back);
        assert_eq!(e.view().comp, "ngodeigau");
        assert_eq!(e.view().segs, "");
    }

    #[test]
    fn punctuation_commits_candidate_then_converts() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        answer(&mut e, &["你"], &[3]);
        assert_eq!(e.key(Key::Char(';')).commit, "你；");
        assert_eq!(typ(&mut e, ",."), "，。", "idle punctuation");
    }

    #[test]
    fn comma_and_period_page_while_composing() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        let words: Vec<String> = (0..13).map(|i| format!("w{i}")).collect();
        let w: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        answer(&mut e, &w, &[3; 13]);
        e.key(Key::Char('.'));
        assert_eq!(e.view().words[0], "w6");
        assert!(e.view().prev_page);
        e.key(Key::Char(','));
        assert_eq!(e.view().words[0], "w0");
    }

    #[test]
    fn paging_past_loaded_requests_more() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        let words: Vec<String> = (0..13).map(|i| format!("w{i}")).collect();
        let w: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        answer(&mut e, &w, &[3; 13]); // 13 >= num: keep 12, more = true
        e.key(Key::PgDn);
        e.key(Key::PgDn);
        assert_eq!(e.query().unwrap().num, 25);
    }

    #[test]
    fn digit_beyond_page_is_swallowed() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        answer(&mut e, &["你"], &[3]);
        let out = e.key(Key::Char('7'));
        assert!(out.swallow && out.commit.is_empty());
        assert!(e.composing());
    }

    #[test]
    fn caret_moves_inside_composition() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "neihou");
        e.key(Key::Left);
        e.key(Key::Left);
        e.key(Key::Left);
        assert_eq!(e.query().unwrap().text, "nei");
        e.key(Key::Home);
        assert_eq!(e.query().unwrap().text, "neihou", "caret at front looks up everything");
        e.key(Key::Char('x'));
        assert_eq!(e.view().comp, "xneihou");
    }

    #[test]
    fn uppercase_first_letter_is_raw() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "Hello");
        assert!(e.query().is_none());
        assert_eq!(e.key(Key::Space).commit, "Hello ");
    }

    #[test]
    fn waits_when_answer_missing() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "sik");
        let out = e.key(Key::Space);
        assert!(out.wait && out.swallow);
        let q = e.query().unwrap();
        e.failed(&q);
        assert_eq!(e.key(Key::Space).commit, "sik");
    }

    #[test]
    fn stale_answers_are_ignored() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "ne");
        let old = e.query().unwrap();
        typ(&mut e, "i");
        assert!(!e.answer(&old, cands(&["呢"], &[2])));
    }

    #[test]
    fn cangjie_shows_radicals_and_keeps_star() {
        let mut e = eng("zh-hant-t-i0-cangjie-1987");
        typ(&mut e, "on*");
        assert_eq!(e.view().comp, "人弓*");
        assert_eq!(e.query().unwrap().text, "on*");
        assert_eq!(e.key(Key::Enter).commit, "人弓*");
    }

    #[test]
    fn english_mode_and_toggles() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "ab");
        assert_eq!(e.toggle_lang(), "ab", "Shift tap commits raw letters");
        assert_eq!(e.key(Key::Char('a')), Out::default(), "English mode passes letters");
        e.toggle_width();
        assert_eq!(e.key(Key::Char('a')).commit, "ａ");
        e.toggle_lang();
        e.toggle_punct();
        assert_eq!(e.key(Key::Char(',')).commit, "，", "full-width comma with Chinese punctuation off");
    }

    #[test]
    fn apostrophe_is_a_letter_once_composing() {
        let mut e = eng("yue-hant-t-i0-und");
        assert_eq!(e.key(Key::Char('\'')).commit, "「", "idle: Chinese quote");
        typ(&mut e, "gam'jat");
        assert_eq!(e.query().unwrap().text, "|「,gam'jat", "the quote is context, the apostrophe a letter");
    }

    #[test]
    fn context_is_sent_for_context_tools() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        answer(&mut e, &["你"], &[3]);
        e.key(Key::Space);
        typ(&mut e, "hou");
        assert_eq!(e.query().unwrap().text, "|你,hou");
        let mut w = eng("zh-t-i0-wubi-1986");
        typ(&mut w, "a");
        answer(&mut w, &["工"], &[1]);
        w.key(Key::Space);
        typ(&mut w, "b");
        assert_eq!(w.query().unwrap().text, "b", "wubi sends no context");
    }
}

/// Findings from the independent review against the extension's code.
#[cfg(test)]
mod review {
    use super::*;
    use crate::tools::find;

    fn eng(itc: &str) -> Engine {
        Engine::new(find(itc).unwrap(), DEFAULT_STATE)
    }
    fn typ(e: &mut Engine, s: &str) -> String {
        s.chars().map(|c| e.key(Key::Char(c)).commit).collect()
    }
    fn ans(e: &mut Engine, n: usize) {
        let q = e.query().unwrap();
        let words: Vec<String> = (0..n).map(|i| format!("w{i}")).collect();
        let len = q.text.rsplit(',').next().unwrap().chars().count();
        assert!(e.answer(&q, Cands { lens: vec![len; n], ann: vec![String::new(); n], words }));
    }

    #[test]
    fn held_comma_commits_then_punctuates() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        assert!(e.key(Key::Char(',')).wait);
        ans(&mut e, 2);
        assert_eq!(e.resume(Key::Char(',')).commit, "w0，");
    }

    #[test]
    fn held_space_commits_first_candidate_whole() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "ngodei");
        assert!(e.key(Key::Space).wait);
        let q = e.query().unwrap();
        e.answer(&q, Cands { words: vec!["我".into()], lens: vec![3], ann: vec![String::new()] });
        assert_eq!(e.resume(Key::Space).commit, "我");
        assert!(!e.composing(), "the unmatched letters are dropped, like the delayed commit");
    }

    #[test]
    fn arrows_page_in_tools_without_caret() {
        let mut e = eng("hi-t-i0-und");
        typ(&mut e, "namaste");
        ans(&mut e, 12);
        assert_eq!(e.view().words[5], "namaste", "Bw: the typed text ends the first page");
        e.key(Key::Right);
        assert_eq!(e.view().words[0], "w5");
        e.key(Key::End);
        assert_eq!(e.view().hl, Some(e.view().words.len() - 1));
        e.key(Key::Home);
        assert_eq!(e.view().hl, Some(0));
    }

    #[test]
    fn digit_on_empty_list_is_swallowed() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "xq");
        ans(&mut e, 0);
        let out = e.key(Key::Char('2'));
        assert!(out.swallow && out.commit.is_empty() && e.composing());
    }

    #[test]
    fn extra_composition_chars() {
        let mut ru = eng("ru-t-i0-und");
        assert!(ru.key(Key::Char('[')).swallow && ru.composing());
        let mut hi = eng("hi-t-i0-und");
        hi.key(Key::Char('5'));
        assert!(hi.composing(), "digit starts a Hindi composition");
        let mut hi = eng("hi-t-i0-und");
        typ(&mut hi, "ka");
        ans(&mut hi, 3);
        assert_eq!(hi.key(Key::Char('2')).commit, "w1 ", "after a letter, digits select");
        let mut ar = eng("ar-t-i0-und");
        typ(&mut ar, "a1");
        ar.key(Key::Space);
        assert_eq!(ar.view().comp, "a1 ");
    }

    #[test]
    fn traditional_pinyin_quotes() {
        let mut e = eng("zh-hant-t-i0-pinyin");
        assert_eq!(e.key(Key::Char('\'')).commit, "「");
    }

    #[test]
    fn caret_noop_keeps_highlight() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "nei");
        ans(&mut e, 4);
        e.key(Key::Down);
        e.key(Key::Right);
        assert_eq!(e.view().hl, Some(1));
        assert!(e.query().is_none());
    }

    #[test]
    fn trailing_space_on_every_commit() {
        let mut e = eng("hi-t-i0-und");
        typ(&mut e, "xq");
        ans(&mut e, 0);
        assert_eq!(e.key(Key::Space).commit, "xq ");
        assert_eq!(eng("am-t-i0-und").key(Key::Char('.')).commit, "። ");
        let mut y = eng("yue-hant-t-i0-und");
        assert_eq!(y.key(Key::Char('.')).commit, "。", "Chinese tools add no space");
    }

    #[test]
    fn native_digits_and_danda() {
        assert_eq!(eng("bn-t-i0-und").key(Key::Char('3')).commit, "৩ ");
        assert_eq!(eng("ur-t-i0-und").key(Key::Char('?')).commit, "؟ ");
        assert_eq!(eng("ne-t-i0-und").key(Key::Char('|')).commit, "। ");
        assert_eq!(eng("ti-t-i0-und").key(Key::Char(',')).commit, "፣ ");
    }

    #[test]
    fn raw_mode_digits_and_caret() {
        let mut e = eng("yue-hant-t-i0-und");
        typ(&mut e, "Hi");
        assert!(e.key(Key::Char('5')).commit.is_empty());
        e.key(Key::Left);
        e.key(Key::Char('x'));
        assert_eq!(e.view().comp, "Hxi");
    }
}
