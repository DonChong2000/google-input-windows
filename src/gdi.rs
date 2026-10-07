//! Small Win32/GDI helpers shared by the windows, plus finding the text caret on screen.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, transmute, zeroed};
use std::ptr::null_mut;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Accessibility::AccessibleObjectFromWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | g << 8 | b << 16
}

pub fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
}

pub fn inside(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

thread_local! {
    static FONTS: RefCell<HashMap<(&'static str, i32, i32), HFONT>> = RefCell::new(HashMap::new());
    static SCALE: Cell<f32> = const { Cell::new(0.0) };
}

/// CSS px -> device px
pub fn p(px: i32) -> i32 {
    let mut s = SCALE.get();
    if s == 0.0 {
        s = unsafe {
            let dc = GetDC(null_mut());
            let d = GetDeviceCaps(dc, LOGPIXELSY as _);
            ReleaseDC(null_mut(), dc);
            d as f32 / 96.0
        };
        SCALE.set(s);
    }
    (px as f32 * s).round() as i32
}

pub fn font(face: &'static str, px: i32, weight: i32) -> HFONT {
    FONTS.with(|f| {
        *f.borrow_mut().entry((face, px, weight)).or_insert_with(|| unsafe {
            CreateFontW(
                -p(px),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                0,
                w(face).as_ptr(),
            )
        })
    })
}

pub unsafe fn text_w(dc: HDC, f: HFONT, s: &str) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let old = SelectObject(dc, f as _);
    let ws: Vec<u16> = s.encode_utf16().collect();
    let mut r = rect(0, 0, 0, 0);
    DrawTextW(dc, ws.as_ptr(), ws.len() as i32, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
    SelectObject(dc, old);
    r.right - r.left
}

/// Width measured on the screen DC (for layout before painting).
pub fn measure(f: HFONT, s: &str) -> i32 {
    unsafe {
        let dc = GetDC(null_mut());
        let w = text_w(dc, f, s);
        ReleaseDC(null_mut(), dc);
        w
    }
}

pub unsafe fn draw_text(dc: HDC, f: HFONT, s: &str, mut r: RECT, color: u32, flags: u32) {
    if s.is_empty() {
        return; // DrawTextW reads the (dangling) pointer of an empty Vec even with length 0
    }
    let old = SelectObject(dc, f as _);
    SetTextColor(dc, color);
    let ws: Vec<u16> = s.encode_utf16().collect();
    DrawTextW(dc, ws.as_ptr(), ws.len() as i32, &mut r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | flags);
    SelectObject(dc, old);
}

pub unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    let b = CreateSolidBrush(color);
    FillRect(dc, &r, b);
    DeleteObject(b as _);
}

pub unsafe fn frame(dc: HDC, r: RECT, color: u32) {
    let b = CreateSolidBrush(color);
    FrameRect(dc, &r, b);
    DeleteObject(b as _);
}

pub unsafe fn triangle(dc: HDC, r: RECT, up: bool, color: u32) {
    let (cx, cy, d) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2, p(4));
    let pts = if up {
        [POINT { x: cx - d, y: cy + d / 2 }, POINT { x: cx + d, y: cy + d / 2 }, POINT { x: cx, y: cy - d / 2 - 1 }]
    } else {
        [POINT { x: cx - d, y: cy - d / 2 }, POINT { x: cx + d, y: cy - d / 2 }, POINT { x: cx, y: cy + d / 2 + 1 }]
    };
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, color);
    let (ob, op) = (SelectObject(dc, brush as _), SelectObject(dc, pen as _));
    Polygon(dc, pts.as_ptr(), 3);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    DeleteObject(brush as _);
    DeleteObject(pen as _);
}

/// Double-buffered WM_PAINT: `draw` gets a memory DC and the client size.
pub unsafe fn paint(hwnd: HWND, draw: impl FnOnce(HDC, i32, i32)) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let dc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old = SelectObject(dc, bmp as _);
    SetBkMode(dc, TRANSPARENT as _);
    draw(dc, rc.right, rc.bottom);
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, dc, 0, 0, SRCCOPY);
    SelectObject(dc, old);
    DeleteObject(bmp as _);
    DeleteDC(dc);
    EndPaint(hwnd, &ps);
}

/// Topmost, never-activated popup window class + window.
pub unsafe fn popup_window(hinst: HINSTANCE, class: &str, proc: WNDPROC, shadow: bool) -> HWND {
    let cls = w(class);
    let wc = WNDCLASSW {
        style: if shadow { CS_DROPSHADOW } else { 0 },
        lpfnWndProc: proc,
        hInstance: hinst,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: cls.as_ptr(),
        ..zeroed()
    };
    RegisterClassW(&wc);
    CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        cls.as_ptr(),
        std::ptr::null(),
        WS_POPUP,
        0,
        0,
        10,
        10,
        null_mut(),
        null_mut(),
        hinst,
        std::ptr::null(),
    )
}

pub fn work_area(at: POINT) -> RECT {
    unsafe {
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST), &mut mi);
        mi.rcWork
    }
}

/// SetForegroundWindow is refused to background processes (we are one: input arrives via the hook),
/// unless our thread shares input state with the current foreground thread.
pub unsafe fn force_foreground(hwnd: HWND) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    let fg = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
    let me = GetCurrentThreadId();
    if fg != 0 && fg != me {
        AttachThreadInput(me, fg, 1);
        SetForegroundWindow(hwnd);
        AttachThreadInput(me, fg, 0);
    } else {
        SetForegroundWindow(hwnd);
    }
}

/// Where to put the box: system caret, else the MSAA caret (Chrome, Electron...), else the mouse.
pub fn caret_pos() -> POINT {
    unsafe {
        let mut g: GUITHREADINFO = zeroed();
        g.cbSize = size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut g) != 0 && !g.hwndCaret.is_null() {
            let mut pt = POINT { x: g.rcCaret.left, y: g.rcCaret.bottom };
            ClientToScreen(g.hwndCaret, &mut pt);
            return POINT { x: pt.x, y: pt.y + p(8) };
        }
        if let Some(pt) = msaa_caret() {
            return pt;
        }
        let mut pt: POINT = zeroed();
        GetCursorPos(&mut pt);
        POINT { x: pt.x, y: pt.y + p(20) } // ponytail: last resort, near the mouse
    }
}

#[repr(C)]
struct Variant {
    vt: u16,
    reserved: [u16; 3],
    val: i64,
    pad: i64,
}

unsafe fn msaa_caret() -> Option<POINT> {
    const IID_IACCESSIBLE: GUID = GUID::from_u128(0x618736e0_3c3d_11cf_810c_00aa00389b71);
    let mut obj: *mut c_void = null_mut();
    if AccessibleObjectFromWindow(GetForegroundWindow(), OBJID_CARET as u32, &IID_IACCESSIBLE, &mut obj) != 0 || obj.is_null() {
        return None;
    }
    // IUnknown(3) + IDispatch(4) + 15 IAccessible getters -> accLocation is slot 22
    let vtbl = *(obj as *const *const usize);
    type AccLocation = unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32, *mut i32, *mut i32, Variant) -> i32;
    type Release = unsafe extern "system" fn(*mut c_void) -> u32;
    let acc_location: AccLocation = transmute(*vtbl.add(22));
    let release: Release = transmute(*vtbl.add(2));
    let (mut x, mut y, mut wd, mut h) = (0, 0, 0, 0);
    let child_self = Variant { vt: 3, reserved: [0; 3], val: 0, pad: 0 }; // VT_I4 CHILDID_SELF
    let hr = acc_location(obj, &mut x, &mut y, &mut wd, &mut h, child_self);
    release(obj);
    (hr == 0 && (x != 0 || y != 0)).then(|| POINT { x, y: y + h + p(8) })
}
