#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::ui::layout::{to_rect, Rect2};
use crate::ui::theme::{METRICS, THEME};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, RoundRect,
    SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, FW_NORMAL, HDC, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetForegroundWindow,
    GetSystemMetrics, GetWindowRect, KillTimer, RegisterClassExW, SetTimer, SetWindowPos,
    ShowWindow, CS_HREDRAW, CS_VREDRAW, HWND_NOTOPMOST, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_LBUTTONUP,
    WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// 内容区左右留白
const PAD: i32 = 16;
const PAD_TOP: i32 = 12;
const PAD_BOTTOM: i32 = 12;
/// 徽标 / 标题所在的首行
const HEAD_ROW_H: i32 = 20;
/// 正文行
const MSG_H: i32 = 20;
/// 行与行之间
const GAP_ROW: i32 = 4;
/// 底部电量条
const BAR_H: i32 = 8;
/// 左侧等级色条（视觉锚点，不计入内容区宽度）
const ACCENT_W: i32 = 4;
/// 徽标固定宽度（"⚡ 低电量预警" 最长文本的稳定占位）
const BADGE_W: i32 = 112;
/// 右上角关闭指示
const CLOSE_W: i32 = 18;

pub const TOAST_WIDTH: i32 = 340;
/// 高度由内容反推：留白 + 首行 + 正文 + 电量条 + 留白
pub const TOAST_HEIGHT: i32 =
    PAD_TOP + HEAD_ROW_H + GAP_ROW + MSG_H + GAP_ROW + BAR_H + PAD_BOTTOM;

/// Toast 布局树。窗口尺寸固定，但所有元素坐标仍由 `toast_layout()` 统一给出。
pub struct ToastLayout {
    pub surface: Rect2,
    /// 左侧等级色条
    pub accent: Rect2,
    pub badge: Rect2,
    pub title: Rect2,
    pub close: Rect2,
    pub message: Rect2,
    pub bar: Rect2,
}

pub fn toast_layout(w: i32, h: i32) -> ToastLayout {
    let surface = Rect2::new(0, 0, w, h);
    let accent = Rect2::new(0, 0, ACCENT_W, h);

    let badge = Rect2::new(PAD, PAD_TOP, BADGE_W, HEAD_ROW_H);
    let close = Rect2::new(w - PAD - CLOSE_W, PAD_TOP - 2, CLOSE_W, HEAD_ROW_H + 4);
    let title = Rect2::new(
        badge.right() + METRICS.gap_tight,
        PAD_TOP,
        close.x - METRICS.gap_tight - (badge.right() + METRICS.gap_tight),
        HEAD_ROW_H,
    );
    let message = Rect2::new(PAD, PAD_TOP + HEAD_ROW_H + GAP_ROW, w - 2 * PAD, MSG_H);
    let bar = Rect2::new(
        PAD,
        PAD_TOP + HEAD_ROW_H + GAP_ROW + MSG_H + GAP_ROW,
        w - 2 * PAD,
        BAR_H,
    );

    ToastLayout { surface, accent, badge, title, close, message, bar }
}

// 颜色取自设计令牌层（src/ui/theme.rs），本文件不再定义色值。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,     // 提示: < 50%
    Warning,  // 警告: < 30%
    Critical, // 危险: < 10%
}

impl ToastLevel {
    pub fn badge_text(&self) -> &'static str {
        match self {
            ToastLevel::Info => "ℹ️ 电量提示",
            ToastLevel::Warning => "⚠️ 低电量预警",
            ToastLevel::Critical => "🚨 极低电量危险",
        }
    }

    pub fn color(&self) -> COLORREF {
        unsafe {
            match self {
                ToastLevel::Info => THEME.charging,
                ToastLevel::Warning => THEME.warn,
                ToastLevel::Critical => THEME.danger,
            }
        }
    }
}

pub struct ToastData {
    pub title: String,
    pub message: String,
    pub level: ToastLevel,
    pub percent: u8,
}

static mut ACTIVE_TOAST: Option<HWND> = None;
static mut TOAST_STATE: Option<Arc<Mutex<ToastData>>> = None;

pub struct ToastNotification;

impl ToastNotification {
    pub fn show(title: &str, message: &str, level: ToastLevel, percent: u8) {
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class_name: Vec<u16> = "LinkDeck_ToastNotification\0".encode_utf16().collect();

            let mut wcx: WNDCLASSEXW = mem::zeroed();
            wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
            wcx.style = CS_HREDRAW | CS_VREDRAW;
            wcx.lpfnWndProc = Some(toast_wnd_proc);
            wcx.hInstance = instance;
            wcx.lpszClassName = class_name.as_ptr();

            RegisterClassExW(&wcx);

            // Destroy previous active toast if still visible
            if let Some(prev) = ACTIVE_TOAST {
                if !prev.is_null() {
                    DestroyWindow(prev);
                }
                ACTIVE_TOAST = None;
            }

            TOAST_STATE = Some(Arc::new(Mutex::new(ToastData {
                title: title.to_string(),
                message: message.to_string(),
                level,
                percent,
            })));

            // Calculate bottom-right positioning above taskbar
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let x = screen_w - TOAST_WIDTH - 20;
            let y = screen_h - TOAST_HEIGHT - 48; // above standard taskbar

            // Window created with WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW:
            // NEVER steals focus, NEVER activates, completely transparent to active games/fullscreen!
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                ptr::null(),
                WS_POPUP,
                x,
                y,
                TOAST_WIDTH,
                TOAST_HEIGHT,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );

            if hwnd.is_null() {
                return;
            }

            ACTIVE_TOAST = Some(hwnd);

            // Check if user is in full-screen application or gaming (D3D fullscreen)
            let in_fullscreen_or_game = is_user_in_fullscreen_or_gaming();

            if in_fullscreen_or_game {
                // Constraint: "如果全屏的时候这个toast是不允许最高层级的，尤其是那种游戏的时候"
                // Do NOT use HWND_TOPMOST! Place at HWND_NOTOPMOST without taking focus!
                SetWindowPos(
                    hwnd,
                    HWND_NOTOPMOST,
                    x,
                    y,
                    TOAST_WIDTH,
                    TOAST_HEIGHT,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            } else {
                // Desktop mode: Show topmost without activating focus
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    TOAST_WIDTH,
                    TOAST_HEIGHT,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }

            ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            // Auto-close timer after 4500ms
            SetTimer(hwnd, 1, 4500, None);
        }
    }
}

/// Detects whether the user is currently in a Direct3D full-screen game, presentation,
/// or full-screen exclusive window.
pub fn is_user_in_fullscreen_or_gaming() -> bool {
    unsafe {
        // 1. Check Windows Shell User Notification State
        let mut state: i32 = 0;
        if SHQueryUserNotificationState(&mut state) == 0 {
            if state == QUNS_RUNNING_D3D_FULL_SCREEN
                || state == QUNS_PRESENTATION_MODE
                || state == QUNS_BUSY
            {
                return true;
            }
        }

        // 2. Check active foreground window dimensions
        let fg = GetForegroundWindow();
        if !fg.is_null() {
            let mut rect: RECT = mem::zeroed();
            GetWindowRect(fg, &mut rect);
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            if rect.left <= 0 && rect.top <= 0 && rect.right >= screen_w && rect.bottom >= screen_h {
                return true;
            }
        }

        false
    }
}

unsafe extern "system" fn toast_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            // Background
            let bg_brush = CreateSolidBrush(THEME.bg);
            FillRect(mem_dc, &rect, bg_brush);
            DeleteObject(bg_brush);

            SetBkMode(mem_dc, TRANSPARENT as i32);

            let data = if let Some(state_arc) = &TOAST_STATE {
                let state = state_arc.lock().unwrap();
                Some((
                    state.title.clone(),
                    state.message.clone(),
                    state.level,
                    state.percent,
                ))
            } else {
                None
            };

            if let Some((title, message, level, pct)) = data {
                let lay = toast_layout(w, h);
                let level_clr = level.color();

                // 左侧等级色条：一眼区分提示 / 预警 / 危险
                let accent_brush = CreateSolidBrush(level_clr);
                let r = to_rect(lay.accent);
                FillRect(mem_dc, &r, accent_brush);
                DeleteObject(accent_brush);

                // 外框
                let pen = CreatePen(PS_SOLID as i32, 1, THEME.border);
                let old_pen = SelectObject(mem_dc, pen);
                let null_brush = windows_sys::Win32::Graphics::Gdi::GetStockObject(5); // NULL_BRUSH
                let old_brush = SelectObject(mem_dc, null_brush);
                let sr = to_rect(lay.surface);
                RoundRect(mem_dc, sr.left, sr.top, sr.right, sr.bottom, METRICS.r_card, METRICS.r_card);
                SelectObject(mem_dc, old_pen);
                SelectObject(mem_dc, old_brush);
                DeleteObject(pen);

                // 徽标
                let f_badge = create_toast_font(12, true);
                let old_f = SelectObject(mem_dc, f_badge);
                SetTextColor(mem_dc, level_clr);
                let mut r = to_rect(lay.badge);
                draw_str(mem_dc, level.badge_text(), &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // 设备名
                let f_title = create_toast_font(13, true);
                SelectObject(mem_dc, f_title);
                SetTextColor(mem_dc, THEME.text);
                let mut r = to_rect(lay.title);
                draw_str(mem_dc, &title, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // 正文
                let f_msg = create_toast_font(12, false);
                SelectObject(mem_dc, f_msg);
                SetTextColor(mem_dc, THEME.muted);
                let mut r = to_rect(lay.message);
                draw_str(mem_dc, &message, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // 电量条：底槽 + 实际电量
                let bg_brush = CreateSolidBrush(THEME.border);
                let r = to_rect(lay.bar);
                FillRect(mem_dc, &r, bg_brush);
                DeleteObject(bg_brush);

                let fill_w = ((pct as f32 / 100.0) * lay.bar.w as f32).round() as i32;
                if fill_w > 0 {
                    let fg_brush = CreateSolidBrush(level_clr);
                    let r = to_rect(Rect2::new(lay.bar.x, lay.bar.y, fill_w.min(lay.bar.w), lay.bar.h));
                    FillRect(mem_dc, &r, fg_brush);
                    DeleteObject(fg_brush);
                }

                // 右上角关闭指示
                SetTextColor(mem_dc, THEME.muted);
                let mut r = to_rect(lay.close);
                draw_str(mem_dc, "\u{2715}", &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                SelectObject(mem_dc, old_f);
                DeleteObject(f_badge);
                DeleteObject(f_title);
                DeleteObject(f_msg);
            }

            windows_sys::Win32::Graphics::Gdi::BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, windows_sys::Win32::Graphics::Gdi::SRCCOPY);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONUP => {
            // Dismiss toast on click
            KillTimer(hwnd, 1);
            ShowWindow(hwnd, SW_HIDE);
            DestroyWindow(hwnd);
            0
        }
        WM_TIMER => {
            if wparam == 1 {
                KillTimer(hwnd, 1);
                ShowWindow(hwnd, SW_HIDE);
                DestroyWindow(hwnd);
            }
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_DESTROY => {
            ACTIVE_TOAST = None;
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_str(hdc: HDC, text: &str, rect: &mut RECT, format: u32) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32, rect, format);
}

unsafe fn create_toast_font(size: i32, bold: bool) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    CreateFontW(
        -size.abs(),
        0,
        0,
        0,
        if bold { FW_BOLD as i32 } else { FW_NORMAL as i32 },
        0,
        0,
        0,
        1, // DEFAULT_CHARSET
        0,
        0,
        5, // CLEARTYPE_QUALITY
        0,
        "Microsoft YaHei UI\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay() -> ToastLayout {
        toast_layout(TOAST_WIDTH, TOAST_HEIGHT)
    }

    #[test]
    fn window_size_is_four_aligned() {
        assert_eq!(TOAST_WIDTH % 4, 0);
        assert_eq!(TOAST_HEIGHT % 4, 0);
    }

    #[test]
    fn everything_stays_inside_the_surface() {
        let l = lay();
        let s = l.surface;
        for r in [l.accent, l.badge, l.title, l.close, l.message, l.bar] {
            assert!(r.x >= s.x && r.right() <= s.right(), "horizontal overflow: {:?}", r);
            assert!(r.y >= s.y && r.bottom() <= s.bottom(), "vertical overflow: {:?}", r);
        }
    }

    #[test]
    fn header_row_never_overlaps() {
        let l = lay();
        assert!(!l.badge.overlaps(&l.title), "badge overlaps title");
        assert!(!l.title.overlaps(&l.close), "title overlaps close button");
        assert!(!l.badge.overlaps(&l.close));
        assert_eq!(l.title.x - l.badge.right(), METRICS.gap_tight);
        assert_eq!(l.close.x - l.title.right(), METRICS.gap_tight);
    }

    #[test]
    fn content_stacks_without_collisions() {
        let l = lay();
        assert!(!l.message.overlaps(&l.bar));
        assert!(l.message.y >= l.badge.bottom());
        assert!(l.bar.y >= l.message.bottom());
        // 电量条下方留出底部留白
        assert_eq!(TOAST_HEIGHT - l.bar.bottom(), PAD_BOTTOM);
    }

    #[test]
    fn accent_bar_spans_full_height() {
        let l = lay();
        assert_eq!(l.accent.h, TOAST_HEIGHT);
        assert!(l.badge.x > l.accent.right(), "text must clear the accent bar");
    }
}
