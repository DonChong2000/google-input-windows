//! Settings window, modeled on the extension's options page (filter, "All input tools",
//! "Selected input tools" with add/remove/reorder) plus its keyboard-shortcut page.
//! Plain Win32 controls; msctls_hotkey32 is Windows' own shortcut picker.

use crate::config::{Config, Hotkey};
use crate::gdi::{font, force_foreground, p, w};
use crate::tools::TOOLS;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::COLOR_BTNFACE;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_PREFS: u32 = WM_APP + 10; // lparam: Box<Prefs>, sent to the main window on OK

pub struct Prefs {
    pub tools: Vec<String>,
    pub hotkeys: [Hotkey; 4], // activate, next, revert, toggle
    pub shift_tap: bool,
}

const ID_FILTER: i32 = 100;
const ID_ALL: i32 = 101;
const ID_SEL: i32 = 102;
const ID_ADD: i32 = 103;
const ID_REMOVE: i32 = 104;
const ID_UP: i32 = 105;
const ID_DOWN: i32 = 106;
const ID_HK: i32 = 110; // ..=113
const ID_SHIFT: i32 = 120;
const LABELS: [&str; 4] = ["Activate input tool", "Select next input tool", "Revert last input tool", "Toggle current input tool"];

static WND: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());
static OWNER: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());

/// The open settings window, or null. The message loop feeds it IsDialogMessage for Tab/Enter/Esc.
pub fn current() -> HWND {
    WND.load(Ordering::Relaxed)
}

#[allow(clippy::too_many_arguments)]
unsafe fn child(parent: HWND, class: &str, text: &str, style: u32, id: i32, x: i32, y: i32, cx: i32, cy: i32) -> HWND {
    let ex = if class == "EDIT" || class == "LISTBOX" { WS_EX_CLIENTEDGE } else { 0 };
    let h = CreateWindowExW(
        ex,
        w(class).as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        p(x),
        p(y),
        p(cx),
        p(cy),
        parent,
        id as isize as HMENU,
        null_mut(),
        null(),
    );
    SendMessageW(h, WM_SETFONT, font("Segoe UI", 12, 400) as WPARAM, 1);
    h
}

unsafe fn item(h: HWND, i: isize) -> Option<usize> {
    if i < 0 {
        return None;
    }
    let d = SendMessageW(h, LB_GETITEMDATA, i as usize, 0);
    (d >= 0).then_some(d as usize)
}

unsafe fn add_row(h: HWND, tool: usize, at: Option<usize>) -> isize {
    let name = w(TOOLS[tool].name);
    let i = match at {
        Some(a) => SendMessageW(h, LB_INSERTSTRING, a, name.as_ptr() as LPARAM),
        None => SendMessageW(h, LB_ADDSTRING, 0, name.as_ptr() as LPARAM),
    };
    SendMessageW(h, LB_SETITEMDATA, i as usize, tool as LPARAM);
    i
}

unsafe fn selected(hwnd: HWND) -> Vec<usize> {
    let sel = GetDlgItem(hwnd, ID_SEL);
    (0..SendMessageW(sel, LB_GETCOUNT, 0, 0)).filter_map(|i| item(sel, i)).collect()
}

/// Left list: every tool matching the filter that isn't selected yet.
unsafe fn refill(hwnd: HWND) {
    let all = GetDlgItem(hwnd, ID_ALL);
    let mut buf = [0u16; 64];
    let n = GetWindowTextW(GetDlgItem(hwnd, ID_FILTER), buf.as_mut_ptr(), 64);
    let f = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
    let chosen = selected(hwnd);
    SendMessageW(all, LB_RESETCONTENT, 0, 0);
    for (i, t) in TOOLS.iter().enumerate() {
        if !chosen.contains(&i) && (t.name.to_lowercase().contains(&f) || t.itc.contains(&f)) {
            add_row(all, i, None);
        }
    }
}

pub unsafe fn open(owner: HWND, cfg: &Config) {
    let existing = current();
    if !existing.is_null() {
        force_foreground(existing);
        return;
    }
    OWNER.store(owner, Ordering::Relaxed);
    let icc = INITCOMMONCONTROLSEX { dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_HOTKEY_CLASS };
    InitCommonControlsEx(&icc);

    let cls = w("GoogleInputPrefs");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_BTNFACE + 1) as isize as _,
        lpszClassName: cls.as_ptr(),
        ..zeroed()
    };
    RegisterClassW(&wc); // fails harmlessly on reopen
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let mut r = RECT { left: 0, top: 0, right: p(600), bottom: p(540) };
    AdjustWindowRectEx(&mut r, style, 0, 0);
    let (cw, ch) = (r.right - r.left, r.bottom - r.top);
    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let title = w("Google Input Tools options");
    let hwnd = CreateWindowExW(0, cls.as_ptr(), title.as_ptr(), style, (sw - cw) / 2, (sh - ch) / 2, cw, ch, null_mut(), null_mut(), null_mut(), null());
    WND.store(hwnd, Ordering::Relaxed);

    let lb = WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32;
    child(hwnd, "STATIC", "All input tools (type to filter)", 0, -1, 12, 10, 250, 18);
    let filter = child(hwnd, "EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL as u32, ID_FILTER, 12, 30, 250, 24);
    child(hwnd, "LISTBOX", "", lb, ID_ALL, 12, 60, 250, 260);
    child(hwnd, "BUTTON", "Add  ›", WS_TABSTOP, ID_ADD, 272, 110, 76, 28);
    child(hwnd, "BUTTON", "‹  Remove", WS_TABSTOP, ID_REMOVE, 272, 146, 76, 28);
    child(hwnd, "BUTTON", "Up", WS_TABSTOP, ID_UP, 272, 210, 76, 28);
    child(hwnd, "BUTTON", "Down", WS_TABSTOP, ID_DOWN, 272, 246, 76, 28);
    child(hwnd, "STATIC", "Selected input tools", 0, -1, 358, 10, 230, 18);
    let sel = child(hwnd, "LISTBOX", "", lb, ID_SEL, 358, 30, 230, 290);
    for t in &cfg.tools {
        if let Some(i) = TOOLS.iter().position(|x| x.itc == t) {
            add_row(sel, i, None);
        }
    }
    refill(hwnd);

    child(hwnd, "STATIC", "Keyboard shortcuts", 0, -1, 12, 334, 300, 18);
    let hks = [cfg.activate, cfg.next, cfg.revert, cfg.toggle];
    for (i, (label, hk)) in LABELS.iter().zip(hks).enumerate() {
        let y = 358 + i as i32 * 30;
        child(hwnd, "STATIC", label, 0, -1, 12, y + 4, 190, 20);
        let h = child(hwnd, "msctls_hotkey32", "", WS_BORDER | WS_TABSTOP, ID_HK + i as i32, 210, y, 200, 24);
        SendMessageW(h, HKM_SETHOTKEY, hk.to_word(), 0);
    }
    let shift = "Tap Shift to switch Chinese / English (Chinese tools)";
    let cb = child(hwnd, "BUTTON", shift, WS_TABSTOP | BS_AUTOCHECKBOX as u32, ID_SHIFT, 12, 480, 390, 22);
    SendMessageW(cb, BM_SETCHECK, if cfg.shift_tap { BST_CHECKED as usize } else { 0 }, 0);
    child(hwnd, "BUTTON", "OK", WS_TABSTOP | BS_DEFPUSHBUTTON as u32, IDOK as i32, 412, 500, 84, 28);
    child(hwnd, "BUTTON", "Cancel", WS_TABSTOP, IDCANCEL as i32, 504, 500, 84, 28);

    ShowWindow(hwnd, SW_SHOW);
    SetFocus(filter);
    force_foreground(hwnd);
}

unsafe fn read(hwnd: HWND) -> Prefs {
    let hk = |i: i32| Hotkey::from_word(SendMessageW(GetDlgItem(hwnd, ID_HK + i), HKM_GETHOTKEY, 0, 0) as usize);
    Prefs {
        tools: selected(hwnd).into_iter().map(|i| TOOLS[i].itc.to_string()).collect(),
        hotkeys: [hk(0), hk(1), hk(2), hk(3)],
        shift_tap: SendMessageW(GetDlgItem(hwnd, ID_SHIFT), BM_GETCHECK, 0, 0) == BST_CHECKED as isize,
    }
}

unsafe fn command(hwnd: HWND, id: i32, code: u32) {
    let (all, sel) = (GetDlgItem(hwnd, ID_ALL), GetDlgItem(hwnd, ID_SEL));
    let cur = |h: HWND| SendMessageW(h, LB_GETCURSEL, 0, 0);
    match (id, code) {
        (ID_FILTER, EN_CHANGE) => refill(hwnd),
        (ID_ADD, _) | (ID_ALL, LBN_DBLCLK) => {
            if let Some(t) = item(all, cur(all)) {
                let i = add_row(sel, t, None);
                SendMessageW(sel, LB_SETCURSEL, i as usize, 0);
                refill(hwnd);
            }
        }
        (ID_REMOVE, _) | (ID_SEL, LBN_DBLCLK) => {
            let i = cur(sel);
            if i >= 0 {
                SendMessageW(sel, LB_DELETESTRING, i as usize, 0);
                refill(hwnd);
            }
        }
        (ID_UP | ID_DOWN, _) => {
            let i = cur(sel);
            let to = if id == ID_UP { i - 1 } else { i + 1 };
            let n = SendMessageW(sel, LB_GETCOUNT, 0, 0);
            if let (Some(t), true) = (item(sel, i), to >= 0 && to < n) {
                SendMessageW(sel, LB_DELETESTRING, i as usize, 0);
                add_row(sel, t, Some(to as usize));
                SendMessageW(sel, LB_SETCURSEL, to as usize, 0);
            }
        }
        (1, _) => {
            // IDOK
            let prefs = Box::new(read(hwnd));
            PostMessageW(OWNER.load(Ordering::Relaxed), WM_PREFS, 0, Box::into_raw(prefs) as LPARAM);
            DestroyWindow(hwnd);
        }
        (2, _) => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            command(hwnd, (wp & 0xffff) as i32, (wp >> 16) as u32);
            0
        }
        WM_DESTROY => {
            WND.store(null_mut(), Ordering::Relaxed);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
