//! Candidate box drawn after Google Input Tools' `ita-ppe-*` CSS: white box, 1px #cdcdcd
//! border, 6px padding, shadow; composition underlined with a 2px #54bdf0 caret; "1. word"
//! items (vertical or inline), #f1f1f1 highlight, grey " (annotation)"; 22x18 page buttons.

use crate::engine::View;
use crate::gdi::*;
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_POPUP_HIT: u32 = WM_APP + 20; // wparam: candidate index, or HIT_PREV / HIT_NEXT
pub const HIT_PREV: usize = 1000;
pub const HIT_NEXT: usize = 1001;
const INK: u32 = rgb(0x22, 0x22, 0x22);
const STALE: u32 = rgb(0x77, 0x77, 0x77); // .ita-ppe-dis-text while the next answer loads
const ANN: u32 = rgb(169, 169, 169);

struct Layout {
    view: View,
    face: &'static str,
    items: Vec<RECT>,
    prev: Option<RECT>,
    next: Option<RECT>,
}

thread_local! {
    static LAYOUT: RefCell<Option<Layout>> = const { RefCell::new(None) };
}
static OWNER: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());

fn edit_font() -> HFONT {
    font("Arial", 18, 400)
}
fn num_font() -> HFONT {
    font("Arial", 16, 400)
}

fn item_w(v: &View, face: &'static str, i: usize) -> i32 {
    let ann = v.ann.get(i).filter(|a| !a.is_empty()).map(|a| format!(" ({a})")).unwrap_or_default();
    measure(num_font(), &v.numbers[i]) + measure(font(face, 16, 400), &v.words[i]) + measure(font(face, 14, 400), &ann) + p(8)
}

/// Positions (in CSS px scaled), returns the layout and the window size.
fn layout(view: View, face: &'static str, vertical: bool) -> (Layout, i32, i32) {
    let (x0, top) = (p(7), p(7 + 32));
    let n = view.words.len();
    let nav = view.prev_page || view.next_page;
    let text: String = view.segs.clone() + &view.comp;
    let mut content = measure(edit_font(), &text) + p(6);
    let widths: Vec<i32> = (0..n).map(|i| item_w(&view, face, i)).collect();
    let mut items = vec![];
    let (mut prev, mut next) = (None, None);
    let height;
    if vertical {
        let inner = widths.iter().map(|w| w + p(4)).max().unwrap_or(0).max(content).max(if nav { p(52) } else { 0 });
        content = inner;
        for i in 0..n {
            let t = top + i as i32 * p(30) + p(2);
            items.push(rect(x0 + p(2), t, x0 + inner - p(2), t + p(26)));
        }
        if nav {
            let ny = top + n as i32 * p(30) + p(4);
            prev = Some(rect(x0 + p(2), ny, x0 + p(24), ny + p(18)));
            next = Some(rect(x0 + p(28), ny, x0 + p(50), ny + p(18)));
        }
        height = p(26) + if n > 0 { p(6) + n as i32 * p(30) } else { 0 } + if nav { p(22) } else { 0 };
    } else {
        let t = top + p(2);
        let mut x = x0 + p(2);
        for w in &widths {
            items.push(rect(x, t, x + w, t + p(26)));
            x += w + p(4);
        }
        if nav {
            let (nx, ny) = (x + p(4), t + p(4));
            prev = Some(rect(nx, ny, nx + p(22), ny + p(18)));
            next = Some(rect(nx + p(26), ny, nx + p(48), ny + p(18)));
            x = nx + p(50);
        }
        content = content.max(x - x0);
        height = p(26) + if n > 0 || nav { p(6) + p(30) } else { 0 };
    }
    let (w, h) = (content + p(14), height + p(14));
    (Layout { view, face, items, prev, next }, w, h)
}

unsafe fn draw(dc: HDC, l: &Layout, width: i32, height: i32) {
    let all = rect(0, 0, width, height);
    fill(dc, all, rgb(255, 255, 255));
    frame(dc, all, rgb(0xcd, 0xcd, 0xcd));
    let v = &l.view;
    let (x, y) = (p(7), p(7));

    // .ita-ppe-edit: held text + letters underlined 2px, caret 2x18 #54bdf0
    let text = v.segs.clone() + &v.comp;
    let tw = text_w(dc, edit_font(), &text);
    draw_text(dc, edit_font(), &text, rect(x + p(1), y, x + p(1) + tw, y + p(20)), INK, 0);
    fill(dc, rect(x + p(1), y + p(20), x + p(1) + tw, y + p(22)), INK);
    let before: String = text.chars().take(v.caret).collect();
    let cx = x + p(1) + text_w(dc, edit_font(), &before);
    fill(dc, rect(cx, y + p(1), cx + p(2), y + p(19)), rgb(0x54, 0xbd, 0xf0));

    let ink = if v.stale { STALE } else { INK };
    for (i, r) in l.items.iter().enumerate() {
        if v.hl == Some(i) {
            fill(dc, *r, rgb(0xf1, 0xf1, 0xf1));
        }
        let mut tx = r.left + p(4);
        for (s, f, c) in [
            (v.numbers[i].clone(), num_font(), ink),
            (v.words[i].clone(), font(l.face, 16, 400), ink),
            (v.ann.get(i).filter(|a| !a.is_empty()).map(|a| format!(" ({a})")).unwrap_or_default(), font(l.face, 14, 400), ANN),
        ] {
            let w = text_w(dc, f, &s);
            draw_text(dc, f, &s, rect(tx, r.top, tx + w, r.bottom), c, 0);
            tx += w;
        }
    }
    for (r, up, on) in [(l.prev, true, v.prev_page), (l.next, false, v.next_page)] {
        if let Some(r) = r {
            nav(dc, r, up, on);
        }
    }
}

/// .ita-ppe-pgu/pgd: gray border, #f5f5f5 fill, opacity .55 (disabled .333), pre-blended onto white.
unsafe fn nav(dc: HDC, r: RECT, up: bool, enabled: bool) {
    let (bg, border, arrow) = if enabled {
        (rgb(0xf9, 0xf9, 0xf9), rgb(0xb9, 0xb9, 0xb9), rgb(0x8f, 0x8f, 0x8f))
    } else {
        (rgb(0xfc, 0xfc, 0xfc), rgb(0xd5, 0xd5, 0xd5), rgb(0xbc, 0xbc, 0xbc))
    };
    fill(dc, r, bg);
    frame(dc, r, border);
    triangle(dc, r, up, arrow);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            LAYOUT.with(|l| {
                if let Some(l) = &*l.borrow() {
                    paint(hwnd, |dc, w, h| draw(dc, l, w, h));
                } else {
                    paint(hwnd, |_, _, _| {});
                }
            });
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as _,
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32);
            let hit = LAYOUT.with(|l| {
                let l = l.borrow();
                let l = l.as_ref()?;
                if l.prev.is_some_and(|r| inside(&r, x, y)) {
                    return Some(HIT_PREV);
                }
                if l.next.is_some_and(|r| inside(&r, x, y)) {
                    return Some(HIT_NEXT);
                }
                l.items.iter().position(|r| inside(r, x, y))
            });
            if let Some(h) = hit {
                PostMessageW(OWNER.load(Ordering::Relaxed), WM_POPUP_HIT, h, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn create(hinst: HINSTANCE, owner: HWND) -> HWND {
    OWNER.store(owner, Ordering::Relaxed);
    popup_window(hinst, "GoogleInputCandidates", Some(proc), true)
}

pub fn show(hwnd: HWND, view: View, face: &'static str, vertical: bool, at: POINT) {
    let (l, w, h) = layout(view, face, vertical);
    LAYOUT.with(|x| *x.borrow_mut() = Some(l));
    let wa = work_area(at);
    let x = at.x.min(wa.right - w).max(wa.left);
    let y = if at.y + h > wa.bottom { at.y - h - p(32) } else { at.y };
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        InvalidateRect(hwnd, null(), 0);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_HIDE) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paints_without_annotations() {
        // regression: an empty annotation string crashed DrawTextW
        let words: Vec<String> = ["नाम", "नम", "नं", "नेम", "णाम", "नैं"].iter().map(|s| s.to_string()).collect();
        let v = View {
            segs: String::new(),
            comp: "nam".into(),
            caret: 3,
            numbers: (1..=6).map(|n| format!("{n}. ")).collect(),
            ann: vec![],
            hl: Some(0),
            words,
            stale: false,
            prev_page: false,
            next_page: true,
        };
        let (l, w, h) = layout(v, "Segoe UI", true);
        unsafe {
            let sdc = GetDC(null_mut());
            let dc = CreateCompatibleDC(sdc);
            let bmp = CreateCompatibleBitmap(sdc, w, h);
            SelectObject(dc, bmp as _);
            draw(dc, &l, w, h);
            eprintln!("drew {w}x{h}");
        }
    }
}

