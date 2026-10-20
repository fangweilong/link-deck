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

const TOAST_WIDTH: i32 = 340;
const TOAST_HEIGHT: i32 = 82;

const CLR_BG: COLORREF = 0x001B1816; // #16181B dark surface
const CLR_BORDER: COLORREF = 0x003A322C; // #2C323A border
const CLR_TEXT: COLORREF = 0x00F4F4F4;
const CLR_MUTED: COLORREF = 0x00B0A8A0;

const CLR_INFO: COLORREF = 0x00D4B606; // Cyan/Blue
const CLR_WARNING: COLORREF = 0x000B9EF5; // Orange/Yellow
const CLR_CRITICAL: COLORREF = 0x004444EF; // Red

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
        match self {
            ToastLevel::Info => CLR_INFO,
            ToastLevel::Warning => CLR_WARNING,
            ToastLevel::Critical => CLR_CRITICAL,
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
            let bg_brush = CreateSolidBrush(CLR_BG);
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
                // Colored left accent bar
                let level_clr = level.color();
                let bar_brush = CreateSolidBrush(level_clr);
                let bar_rect = RECT {
                    left: 0,
                    top: 0,
                    right: 4,
                    bottom: h,
                };
                FillRect(mem_dc, &bar_rect, bar_brush);
                DeleteObject(bar_brush);

                // Rounded outline
                let pen = CreatePen(PS_SOLID as i32, 1, CLR_BORDER);
                let old_pen = SelectObject(mem_dc, pen);
                let null_brush = windows_sys::Win32::Graphics::Gdi::GetStockObject(5); // NULL_BRUSH
                let old_brush = SelectObject(mem_dc, null_brush);
                RoundRect(mem_dc, 0, 0, w, h, 8, 8);
                SelectObject(mem_dc, old_pen);
                SelectObject(mem_dc, old_brush);
                DeleteObject(pen);

                // Badge Text
                let f_badge = create_toast_font(12, true);
                let old_f = SelectObject(mem_dc, f_badge);
                SetTextColor(mem_dc, level_clr);
                let mut b_rect = RECT {
                    left: 14,
                    top: 10,
                    right: 130,
                    bottom: 28,
                };
                draw_str(mem_dc, level.badge_text(), &mut b_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Title (Device Name)
                let f_title = create_toast_font(13, true);
                SelectObject(mem_dc, f_title);
                SetTextColor(mem_dc, CLR_TEXT);
                let mut t_rect = RECT {
                    left: 135,
                    top: 10,
                    right: w - 24,
                    bottom: 28,
                };
                draw_str(mem_dc, &title, &mut t_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Message Text
                let f_msg = create_toast_font(12, false);
                SelectObject(mem_dc, f_msg);
                SetTextColor(mem_dc, CLR_MUTED);
                let mut m_rect = RECT {
                    left: 14,
                    top: 32,
                    right: w - 24,
                    bottom: 52,
                };
                draw_str(mem_dc, &message, &mut m_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Mini Progress Bar
                let bar_w = w - 28;
                let bar_x = 14;
                let bar_y = 60;
                let bar_h = 5;

                let empty_bar = RECT {
                    left: bar_x,
                    top: bar_y,
                    right: bar_x + bar_w,
                    bottom: bar_y + bar_h,
                };
                let eb_brush = CreateSolidBrush(CLR_BORDER);
                FillRect(mem_dc, &empty_bar, eb_brush);
                DeleteObject(eb_brush);

                let fill_w = ((pct as f32 / 100.0) * bar_w as f32).round() as i32;
                if fill_w > 0 {
                    let fill_bar = RECT {
                        left: bar_x,
                        top: bar_y,
                        right: bar_x + fill_w.min(bar_w),
                        bottom: bar_y + bar_h,
                    };
                    let fb_brush = CreateSolidBrush(level_clr);
                    FillRect(mem_dc, &fill_bar, fb_brush);
                    DeleteObject(fb_brush);
                }

                // Close cross indicator on top right
                SetTextColor(mem_dc, CLR_MUTED);
                let mut c_rect = RECT {
                    left: w - 22,
                    top: 8,
                    right: w - 6,
                    bottom: 24,
                };
                draw_str(mem_dc, "✕", &mut c_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

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
