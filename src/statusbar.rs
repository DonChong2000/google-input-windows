//! The floating status bar Chinese tools get (`ita-kd-statusbar`): #eee box, drag grip,
//! tool icon, then three buttons for 中/En, half/full width, and Chinese/English punctuation.

use crate::engine::{CHINESE, FULL, PUNCT};
use crate::gdi::*;
use std::cell::Cell;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_STATUS_CLICK: u32 = WM_APP + 30; // wparam: the state bit to flip
pub const WM_STATUS_MOVED: u32 = WM_APP + 31; // wparam/lparam: x, y

// grip 14, icon 33 (.ita-kd-small), buttons 54 (.ita-kd-icon-button min-width); 27 tall
const COLS: [(i32, u8); 5] = [(14, 0), (33, 0), (54, CHINESE), (54, FULL), (54, PUNCT)];
const H: i32 = 27;

thread_local! {
    static LABEL: Cell<&'static str> = const { Cell::new("") };
    static STATE: Cell<u8> = const { Cell::new(0) };
    static DRAG: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}
static OWNER: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());

fn col_at(x: i32) -> usize {
    let mut left = 0;
    for (i, (w, _)) in COLS.iter().enumerate() {
        left += p(*w);
        if x < left {
            return i;
        }
    }
    COLS.len() - 1
}

unsafe fn draw(dc: HDC, w: i32, h: i32) {
    fill(dc, rect(0, 0, w, h), rgb(0xee, 0xee, 0xee));
    let state = STATE.get();
    let icon = rgb(0x6d, 0x6d, 0x6d); // black at the sprite's .54 opacity on #eee
    let mut x = 0;
    for (i, (cw, bit)) in COLS.iter().enumerate() {
        let r = rect(x, 0, x + p(*cw), h);
        match i {
            0 => {
                // grip: two columns of dots
                for row in 0..4 {
                    for col in 0..2 {
                        let (dx, dy) = (r.left + p(4) + col * p(4), p(7) + row * p(4));
                        fill(dc, rect(dx, dy, dx + p(2), dy + p(2)), rgb(0xaa, 0xaa, 0xaa));
                    }
                }
            }
            1 => draw_text(dc, font("Microsoft JhengHei UI", 14, 700), LABEL.get(), r, icon, DT_CENTER),
            _ => {
                let on = state & bit != 0;
                let s = match *bit {
                    CHINESE if on => "中",
                    CHINESE => "En",
                    FULL if on => "●",
                    FULL => "☽",
                    _ if on => "°,",
                    _ => "·,",
                };
                draw_text(dc, font("Microsoft JhengHei UI", 15, 700), s, r, icon, DT_CENTER);
            }
        }
        x = r.right;
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (x, y) = ((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32);
    match msg {
        WM_PAINT => {
            paint(hwnd, |dc, w, h| draw(dc, w, h));
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as _,
        WM_LBUTTONDOWN => {
            let col = col_at(x);
            if col < 2 {
                DRAG.set(Some((x, y)));
                SetCapture(hwnd);
            } else {
                PostMessageW(OWNER.load(Ordering::Relaxed), WM_STATUS_CLICK, COLS[col].1 as usize, 0);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some((dx, dy)) = DRAG.get() {
                let mut pt: POINT = zeroed();
                GetCursorPos(&mut pt);
                SetWindowPos(hwnd, null_mut(), pt.x - dx, pt.y - dy, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            0
        }
        WM_LBUTTONUP => {
            if DRAG.take().is_some() {
                ReleaseCapture();
                let mut r: RECT = zeroed();
                GetWindowRect(hwnd, &mut r);
                PostMessageW(OWNER.load(Ordering::Relaxed), WM_STATUS_MOVED, r.left as usize, r.top as isize);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn create(hinst: HINSTANCE, owner: HWND) -> HWND {
    OWNER.store(owner, Ordering::Relaxed);
    popup_window(hinst, "GoogleInputStatusBar", Some(proc), true)
}

pub fn width() -> i32 {
    COLS.iter().map(|(w, _)| p(*w)).sum()
}

/// Default spot: bottom-right of the primary work area, 50px in (`dy`).
pub fn default_pos() -> (i32, i32) {
    let wa = work_area(POINT { x: 0, y: 0 });
    (wa.right - width() - p(50), wa.bottom - p(H) - p(50))
}

pub fn show(hwnd: HWND, label: &'static str, state: u8, pos: (i32, i32)) {
    LABEL.set(label);
    STATE.set(state);
    unsafe {
        let (w, h) = (width(), p(H));
        SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, p(6), p(6)), 1);
        SetWindowPos(hwnd, HWND_TOPMOST, pos.0, pos.1, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        InvalidateRect(hwnd, null(), 0);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_HIDE) };
}
