//! Google Input Tools for any Windows app: a low-level keyboard hook feeds the
//! extension's composition logic (engine.rs), Google's Input Tools API supplies
//! candidates, SendInput types the result. See requirement.md.
#![windows_subsystem = "windows"]

mod config;
mod engine;
mod gdi;
mod google;
mod popup;
mod prefs;
mod statusbar;
mod tools;

use config::{Config, ALT, CTRL, SHIFT};
use engine::{Engine, Key, Out, CHINESE, FULL, PUNCT};
use gdi::{rgb, w};
use google::{Cands, Query};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::Duration;
use tools::Tool;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const WM_CANDS: u32 = WM_APP + 2; // lparam: Box<Lookup> from a fetch thread
const WM_MENU_AT_CARET: u32 = WM_APP + 3;
const WM_RENDER: u32 = WM_APP + 4;
const WM_CLICK_ELSEWHERE: u32 = WM_APP + 5;
const TIMER_WAIT: usize = 1;
const MAGIC: usize = 0x4749_4E50; // dwExtraInfo on our own SendInput events, so the hook skips them

const ID_OFF: usize = 1;
const ID_STATUS: usize = 2;
const ID_OPTIONS: usize = 3;
const ID_SHORTCUTS: usize = 4;
const ID_EXIT: usize = 5;
const ID_MINE: usize = 1000; // + index into cfg.tools

type Lookup = (Query, Option<Cands>);

#[derive(Clone, Copy)]
enum Cmd {
    Activate,
    Next,
    Revert,
    Toggle,
}

struct App {
    hwnd: HWND,
    popup: HWND,
    status: HWND,
    cfg: Config,
    eng: Engine,
    fg: HWND, // window the composition belongs to
    cache: HashMap<Query, Cands>,
    pending: HashSet<Query>,
    swallowed: HashSet<u32>,
    last_down: u32,
    waiting: Option<(Key, u32)>, // a key held until its candidates arrive
    queue: Vec<(u32, bool)>,     // keys typed meanwhile (vk, down)
    render_posted: bool,
    menu_pending: bool, // Activate pressed: open the menu once its modifiers are released
    icon: HICON,
    hooks: [HHOOK; 2],
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Hooks, window procs and the menu loop all run on the main thread and can nest
/// (a hook call while TrackPopupMenu pumps), so a busy borrow is skipped, never panics.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn mods() -> u8 {
    (if held(VK_SHIFT) { SHIFT } else { 0 }) | (if held(VK_CONTROL) { CTRL } else { 0 }) | (if held(VK_MENU) { ALT } else { 0 })
}

/// Any of Shift/Ctrl/Alt/Win still down, not counting `vk` (whose key-up is being handled).
fn other_mods_held(vk: u32) -> bool {
    let fam = |l: VIRTUAL_KEY, r: VIRTUAL_KEY, g: VIRTUAL_KEY| [l, r, g].contains(&(vk as VIRTUAL_KEY));
    let groups = [(VK_LSHIFT, VK_RSHIFT, VK_SHIFT), (VK_LCONTROL, VK_RCONTROL, VK_CONTROL), (VK_LMENU, VK_RMENU, VK_MENU), (VK_LWIN, VK_RWIN, VK_LWIN)];
    groups.iter().any(|&(l, r, g)| !fam(l, r, g) && (held(l) || held(r)))
}

fn is_modifier(vk: u32) -> bool {
    [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN, VK_CAPITAL]
        .contains(&(vk as VIRTUAL_KEY))
}

/// The character this key types in the foreground app's keyboard layout (Shift/Caps applied).
fn typed_char(vk: u32) -> Option<char> {
    unsafe {
        let mut state = [0u8; 256];
        if held(VK_SHIFT) {
            state[VK_SHIFT as usize] = 0x80;
        }
        state[VK_CAPITAL as usize] = (GetKeyState(VK_CAPITAL as i32) & 1) as u8;
        let layout = GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), null_mut()));
        let mut buf = [0u16; 4];
        let scan = MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, layout);
        // flag 4: don't touch the keyboard's dead-key state
        let n = ToUnicodeEx(vk, scan, state.as_ptr(), buf.as_mut_ptr(), 4, 4, layout);
        if n == 1 && buf[0] >= 0x20 {
            char::from_u32(buf[0] as u32)
        } else {
            None
        }
    }
}

fn to_key(vk: u32) -> Option<Key> {
    Some(match vk as VIRTUAL_KEY {
        VK_SPACE => Key::Space,
        VK_RETURN => Key::Enter,
        VK_ESCAPE => Key::Esc,
        VK_BACK => Key::Back,
        VK_DELETE => Key::Delete,
        VK_TAB => Key::Tab,
        VK_LEFT => Key::Left,
        VK_RIGHT => Key::Right,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_UP => Key::Up,
        VK_DOWN => Key::Down,
        VK_PRIOR => Key::PgUp,
        VK_NEXT => Key::PgDn,
        _ if is_modifier(vk) => return None,
        _ => Key::Char(typed_char(vk)?),
    })
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: MAGIC } },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
}

fn send_text(s: &str) {
    let inputs: Vec<INPUT> =
        s.encode_utf16().flat_map(|u| [key(0, u, KEYEVENTF_UNICODE), key(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]).collect();
    send(&inputs);
}

fn tool_for(itc: &str) -> &'static Tool {
    tools::find(itc).unwrap_or_else(|| tools::find("yue-hant-t-i0-und").unwrap())
}

impl App {
    fn ours(&self) -> bool {
        let fg = unsafe { GetForegroundWindow() };
        !fg.is_null() && fg == prefs::current()
    }

    /// true = swallow the key
    fn on_key(&mut self, vk: u32, down: bool) -> bool {
        if self.waiting.is_some() {
            self.queue.push((vk, down));
            return true;
        }
        let tool = self.eng.tool;
        if !down {
            if self.menu_pending && is_modifier(vk) && !other_mods_held(vk) {
                // AttachThreadInput (needed to take focus for the menu) shares key state;
                // doing it with Shift/Ctrl still down leaves them "stuck" in the app
                self.menu_pending = false;
                unsafe { PostMessageW(self.hwnd, WM_MENU_AT_CARET, 0, 0) };
            }
            let shift = vk == VK_LSHIFT as u32 || vk == VK_RSHIFT as u32;
            if shift && self.last_down == vk && self.cfg.shift_tap && self.cfg.on && tool.chinese && !self.ours() {
                let s = self.eng.toggle_lang();
                self.state_changed(&s);
            }
            return self.swallowed.remove(&vk);
        }
        self.last_down = vk;
        if self.ours() {
            return false; // the settings window gets raw keys, so its shortcut pickers can record ours
        }

        let m = mods();
        if !is_modifier(vk) {
            let c = &self.cfg;
            let cmd = [(c.activate, Cmd::Activate), (c.next, Cmd::Next), (c.revert, Cmd::Revert), (c.toggle, Cmd::Toggle)]
                .into_iter()
                .find(|(hk, _)| hk.matches(vk, m));
            if let Some((_, cmd)) = cmd {
                if m & ALT != 0 {
                    // Alt went down/up with nothing the app saw in between: it would open the menu bar
                    send(&[key(0xE8, 0, 0), key(0xE8, 0, KEYEVENTF_KEYUP)]);
                }
                self.command(cmd);
                return self.swallow(vk);
            }
        }
        if !self.cfg.on {
            return false;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg != self.fg {
            self.fg = fg;
            self.eng.reset();
            self.eng.forget_context();
            self.render();
        }
        if tool.chinese && vk == VK_SPACE as u32 && m == SHIFT {
            let s = self.eng.toggle_width();
            self.state_changed(&s);
            return self.swallow(vk);
        }
        if tool.chinese && vk == VK_OEM_PERIOD as u32 && m == CTRL {
            self.eng.toggle_punct();
            self.state_changed("");
            return self.swallow(vk);
        }
        if m & (CTRL | ALT) != 0 || held(VK_LWIN) || held(VK_RWIN) {
            if !is_modifier(vk) {
                // unbound shortcut: composition discarded, key goes to the app
                self.eng.reset();
                self.eng.forget_context();
                self.render();
            }
            return false;
        }
        if m == SHIFT && self.eng.composing() && matches!(vk as VIRTUAL_KEY, VK_BACK | VK_TAB | VK_DELETE | VK_PRIOR..=VK_DOWN) {
            return self.swallow(vk); // bindings need no modifiers; these keys are eaten while composing
        }
        let Some(k) = to_key(vk) else { return false };
        self.feed(k, vk)
    }

    fn feed(&mut self, k: Key, vk: u32) -> bool {
        let out = self.eng.key(k);
        if out.wait {
            // typed faster than the network: never block the hook (Windows drops slow hooks),
            // hold this key and queue the following ones until the answer or a timeout
            self.waiting = Some((k, vk));
            unsafe { SetTimer(self.hwnd, TIMER_WAIT, 2000, None) };
            self.after();
            return self.swallow(vk);
        }
        self.apply(out, vk)
    }

    fn apply(&mut self, out: Out, vk: u32) -> bool {
        if !out.commit.is_empty() {
            send_text(&out.commit);
        }
        self.after();
        out.swallow && self.swallow(vk)
    }

    fn swallow(&mut self, vk: u32) -> bool {
        if vk != 0 {
            self.swallowed.insert(vk);
        }
        true
    }

    /// After every engine step: fetch what's missing, redraw.
    fn after(&mut self) {
        if let Some(q) = self.eng.query() {
            if let Some(c) = self.cache.get(&q).cloned() {
                self.eng.answer(&q, c);
            } else if self.pending.insert(q.clone()) {
                let hwnd = self.hwnd as usize;
                std::thread::spawn(move || {
                    let r = google::fetch(&q, Duration::from_secs(5));
                    let msg: Box<Lookup> = Box::new((q, r));
                    unsafe { PostMessageW(hwnd as HWND, WM_CANDS, 0, Box::into_raw(msg) as LPARAM) };
                });
            }
        }
        self.render();
    }

    fn got(&mut self, (q, r): Lookup) {
        self.pending.remove(&q);
        match r {
            Some(c) => {
                if self.cache.len() > 2000 {
                    self.cache.clear(); // ponytail: crude bound, an LRU if it ever matters
                }
                self.cache.insert(q.clone(), c.clone());
                self.eng.answer(&q, c);
            }
            None => self.eng.failed(&q),
        }
        if self.waiting.is_some() && self.eng.query().is_none() {
            self.finish_wait();
        } else {
            self.after();
        }
    }

    /// The held key can run now (answer arrived or timed out); then replay the queued keys.
    fn finish_wait(&mut self) {
        let Some((k, vk)) = self.waiting.take() else { return };
        unsafe { KillTimer(self.hwnd, TIMER_WAIT) };
        if let Some(q) = self.eng.query() {
            self.eng.failed(&q); // timed out: the held key commits the raw letters
        }
        let out = self.eng.resume(k);
        self.apply(out, vk);
        // ponytail: replayed keys see today's modifier state, not the one at queue time
        for (vk, down) in std::mem::take(&mut self.queue) {
            if self.waiting.is_some() {
                self.queue.push((vk, down));
            } else if !self.on_key(vk, down) {
                send(&[key(vk as u16, 0, if down { 0 } else { KEYEVENTF_KEYUP })]);
            }
        }
    }

    /// Drawing (caret lookup, GDI, window moves) happens after the hook returns.
    fn render(&mut self) {
        if !self.render_posted {
            self.render_posted = true;
            unsafe { PostMessageW(self.hwnd, WM_RENDER, 0, 0) };
        }
    }

    fn draw(&mut self) {
        self.render_posted = false;
        if !self.eng.composing() {
            return popup::hide(self.popup);
        }
        let t = self.eng.tool;
        popup::show(self.popup, self.eng.view(), tools::font_for(t), t.vertical, gdi::caret_pos());
    }

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Activate if mods() == 0 => return unsafe { PostMessageW(self.hwnd, WM_MENU_AT_CARET, 0, 0); },
            Cmd::Activate => return self.menu_pending = true,
            Cmd::Next => self.cfg.next(),
            Cmd::Revert => self.cfg.revert(),
            Cmd::Toggle => self.cfg.toggle(),
        }
        self.tool_changed();
    }

    /// Current tool or on/off changed: commit what's typed, start a fresh composition.
    fn tool_changed(&mut self) {
        if self.eng.composing() {
            send_text(&self.eng.commit_raw());
        }
        self.eng = Engine::new(tool_for(&self.cfg.cur), self.cfg.state);
        self.cfg.save();
        self.set_icon();
        self.update_status();
        self.render();
    }

    fn state_changed(&mut self, committed: &str) {
        if !committed.is_empty() {
            send_text(committed);
        }
        self.cfg.state = self.eng.state;
        self.cfg.save();
        self.update_status();
        self.render();
    }

    fn update_status(&self) {
        let t = self.eng.tool;
        if self.cfg.status_bar && self.cfg.on && t.chinese {
            let pos = self.cfg.status_pos.unwrap_or_else(statusbar::default_pos);
            statusbar::show(self.status, t.label, self.eng.state, pos);
        } else {
            statusbar::hide(self.status);
        }
    }

    fn set_icon(&mut self) {
        let old = self.icon;
        self.icon = unsafe { make_icon(self.eng.tool.label, self.cfg.on) };
        self.tray(NIM_MODIFY);
        if !old.is_null() {
            unsafe { DestroyIcon(old) };
        }
    }

    fn tray(&self, op: NOTIFY_ICON_MESSAGE) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = zeroed();
            nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
            nid.uCallbackMessage = WM_TRAY;
            nid.hIcon = self.icon;
            let tip = format!("Google Input Tools — {} — {}", self.eng.tool.name, if self.cfg.on { "on" } else { "off" });
            for (d, s) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
                *d = s;
            }
            Shell_NotifyIconW(op, &nid);
        }
    }

    /// The extension's toolbar popup: my tools (click the active one to turn off), Turn off,
    /// Show/Hide status bar, Options, Keyboard shortcut settings. Plus Exit.
    fn build_menu(&self) -> HMENU {
        unsafe {
            let m = CreatePopupMenu();
            let check = |b: bool| if b { MF_CHECKED } else { 0 };
            for (i, t) in self.cfg.tools.iter().enumerate() {
                let name = tools::find(t).map_or(t.as_str(), |x| x.name);
                AppendMenuW(m, MF_STRING | check(self.cfg.on && self.cfg.cur == *t), ID_MINE + i, w(name).as_ptr());
            }
            AppendMenuW(m, MF_SEPARATOR, 0, null());
            if self.cfg.on {
                AppendMenuW(m, MF_STRING, ID_OFF, w("Turn off").as_ptr());
                if self.eng.tool.chinese {
                    let s = if self.cfg.status_bar { "Hide Status Bar" } else { "Show Status Bar" };
                    AppendMenuW(m, MF_STRING, ID_STATUS, w(s).as_ptr());
                }
                AppendMenuW(m, MF_SEPARATOR, 0, null());
            }
            AppendMenuW(m, MF_STRING, ID_OPTIONS, w("Options…").as_ptr());
            AppendMenuW(m, MF_STRING, ID_SHORTCUTS, w("Keyboard Shortcut Settings…").as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, null());
            AppendMenuW(m, MF_STRING, ID_EXIT, w("Exit").as_ptr());
            m
        }
    }

    fn menu_cmd(&mut self, id: usize) {
        match id {
            ID_OFF => self.cfg.off(),
            ID_STATUS => {
                self.cfg.status_bar = !self.cfg.status_bar;
                self.cfg.save();
                return self.update_status();
            }
            ID_OPTIONS | ID_SHORTCUTS => return unsafe { prefs::open(self.hwnd, &self.cfg) },
            ID_EXIT => return unsafe { DestroyWindow(self.hwnd); },
            _ => {
                let Some(t) = self.cfg.tools.get(id.wrapping_sub(ID_MINE)).cloned() else { return };
                if self.cfg.on && self.cfg.cur == t {
                    self.cfg.off();
                } else {
                    self.cfg.select(&t);
                    self.cfg.state |= CHINESE; // the popup forces Chinese mode on
                }
            }
        }
        self.tool_changed();
    }

    fn apply_prefs(&mut self, p: prefs::Prefs) {
        self.cfg.tools = p.tools;
        if !self.cfg.tools.contains(&self.cfg.cur) {
            self.cfg.on = false; // like the options page: current tool removed -> off
        }
        [self.cfg.activate, self.cfg.next, self.cfg.revert, self.cfg.toggle] = p.hotkeys;
        self.cfg.shift_tap = p.shift_tap;
        self.tool_changed();
    }

    fn popup_hit(&mut self, hit: usize) {
        match hit {
            popup::HIT_PREV | popup::HIT_NEXT => {
                self.eng.click_page(hit == popup::HIT_NEXT);
                self.after();
            }
            i => {
                let out = self.eng.click(i);
                self.apply(out, 0);
            }
        }
    }

    fn status_click(&mut self, bit: u8) {
        let s = match bit {
            CHINESE => self.eng.toggle_lang(),
            FULL => self.eng.toggle_width(),
            PUNCT => {
                self.eng.toggle_punct();
                String::new()
            }
            _ => return,
        };
        self.state_changed(&s);
    }
}

/// Menu outside any App borrow: TrackPopupMenu runs a modal loop that keeps calling our hooks.
unsafe fn show_menu(at: POINT, restore: HWND) {
    let Some((hwnd, menu)) = with_app(|a| (a.hwnd, a.build_menu())) else { return };
    gdi::force_foreground(hwnd);
    let id = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, at.x, at.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if id != 0 {
        with_app(|a| a.menu_cmd(id as usize));
    }
    if !restore.is_null() && prefs::current().is_null() {
        SetForegroundWindow(restore);
    }
}

unsafe fn make_icon(label: &str, on: bool) -> HICON {
    let sz = GetSystemMetrics(SM_CXSMICON);
    let sdc = GetDC(null_mut());
    let dc = CreateCompatibleDC(sdc);
    let color = CreateCompatibleBitmap(sdc, sz, sz);
    let zeros = vec![0u8; (sz as usize).div_ceil(16) * 2 * sz as usize];
    let mask = CreateBitmap(sz, sz, 1, 1, zeros.as_ptr() as _);
    let old = SelectObject(dc, color as _);
    let r = gdi::rect(0, 0, sz, sz);
    gdi::fill(dc, r, if on { rgb(26, 115, 232) } else { rgb(110, 110, 110) });
    SetBkMode(dc, TRANSPARENT as _);
    let px = if label.chars().count() > 1 { 9 } else { 13 } * sz / 16;
    let face = w("Microsoft JhengHei UI");
    let f = CreateFontW(-px, 0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, 0, face.as_ptr());
    gdi::draw_text(dc, f, label, r, rgb(255, 255, 255), DT_CENTER);
    SelectObject(dc, old);
    let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&ii);
    DeleteObject(f as _);
    DeleteObject(color as _);
    DeleteObject(mask as _);
    DeleteDC(dc);
    ReleaseDC(null_mut(), sdc);
    icon
}

unsafe extern "system" fn key_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lp as *const KBDLLHOOKSTRUCT);
        if k.dwExtraInfo != MAGIC {
            let down = wp == WM_KEYDOWN as usize || wp == WM_SYSKEYDOWN as usize;
            if with_app(|a| a.on_key(k.vkCode, down)).unwrap_or(false) {
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

/// A click anywhere but our windows moves the caret: the composition is discarded.
unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wp as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
        let pt = (*(lp as *const MSLLHOOKSTRUCT)).pt;
        let ours = with_app(|a| (a.hwnd, a.popup, a.status));
        if let Some((main, pop, status)) = ours {
            let hit = WindowFromPoint(pt);
            if hit != pop && hit != status {
                PostMessageW(main, WM_CLICK_ELSEWHERE, 0, 0);
            }
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lp & 0xffff) as u32 {
                WM_LBUTTONUP => drop(with_app(|a| a.command(Cmd::Toggle))),
                WM_RBUTTONUP => {
                    let mut pt: POINT = zeroed();
                    GetCursorPos(&mut pt);
                    show_menu(pt, null_mut());
                }
                _ => {}
            }
        }
        WM_CANDS => {
            let r = Box::from_raw(lp as *mut Lookup);
            with_app(|a| a.got(*r));
        }
        WM_RENDER => drop(with_app(|a| a.draw())),
        WM_TIMER if wp == TIMER_WAIT => drop(with_app(|a| a.finish_wait())),
        WM_CLICK_ELSEWHERE => drop(with_app(|a| {
            if a.waiting.is_none() {
                a.eng.reset();
                a.eng.forget_context();
                a.render();
            }
        })),
        WM_MENU_AT_CARET => show_menu(gdi::caret_pos(), GetForegroundWindow()),
        popup::WM_POPUP_HIT => drop(with_app(|a| a.popup_hit(wp))),
        statusbar::WM_STATUS_CLICK => drop(with_app(|a| a.status_click(wp as u8))),
        statusbar::WM_STATUS_MOVED => drop(with_app(|a| {
            a.cfg.status_pos = Some((wp as i32, lp as i32));
            a.cfg.save();
        })),
        prefs::WM_PREFS => {
            let pr = Box::from_raw(lp as *mut prefs::Prefs);
            with_app(|a| a.apply_prefs(*pr));
        }
        WM_DESTROY => PostQuitMessage(0),
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}

fn main() {
    unsafe {
        let name = w("GoogleInputTools-Windows");
        CreateMutexW(null(), 1, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDPIAware();
        CoInitializeEx(null(), COINIT_APARTMENTTHREADED as _);
        let hinst = GetModuleHandleW(null());

        let cls = w("GoogleInputMain");
        let wc = WNDCLASSW { lpfnWndProc: Some(main_proc), hInstance: hinst, lpszClassName: cls.as_ptr(), ..zeroed() };
        RegisterClassW(&wc);
        // a real (never shown) top-level window: tray menus need one that can take foreground
        let title = w("Google Input Tools");
        let hwnd = CreateWindowExW(0, cls.as_ptr(), title.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());

        let cfg = Config::load();
        let mut app = App {
            hwnd,
            popup: popup::create(hinst, hwnd),
            status: statusbar::create(hinst, hwnd),
            eng: Engine::new(tool_for(&cfg.cur), cfg.state),
            cfg,
            fg: null_mut(),
            cache: HashMap::new(),
            pending: HashSet::new(),
            swallowed: HashSet::new(),
            last_down: 0,
            waiting: None,
            queue: Vec::new(),
            render_posted: false,
            menu_pending: false,
            icon: null_mut(),
            hooks: [null_mut(); 2],
        };
        app.icon = make_icon(app.eng.tool.label, app.cfg.on);
        app.tray(NIM_ADD);
        app.update_status();
        app.hooks = [SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), hinst, 0), SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), hinst, 0)];
        APP.with(|a| *a.borrow_mut() = Some(app));

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            let pw = prefs::current();
            if !pw.is_null() && IsDialogMessageW(pw, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        with_app(|a| {
            for h in a.hooks {
                UnhookWindowsHookEx(h);
            }
            a.tray(NIM_DELETE);
        });
    }
}
