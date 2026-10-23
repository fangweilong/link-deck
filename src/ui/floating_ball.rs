#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::{DeviceKind, DeviceSnapshot};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, RoundRect,
    SelectObject, SetBkMode, SetTextColor, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
    FW_NORMAL, HDC, HFONT, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TTTOOLINFOW, WM_MOUSELEAVE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorPos, GetSystemMetrics, KillTimer,
    LoadCursorW, RegisterClassExW, SendMessageW, SetLayeredWindowAttributes, SetTimer,
    SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, LWA_ALPHA, LWA_COLORKEY,
    SM_CXSCREEN,
    SM_CYSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, WM_DESTROY,
    WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

pub const BALL_SIZE: i32 = 48;
/// 窗口高度取收纳态拉手的高度，悬停时的完整球体居中绘制在同一个窗口里。
pub const DOCK_H: i32 = 64;
/// 贴边拉手宽度：右侧贴屏幕，左侧两个角做圆角。
const TAB_W: i32 = 26;
const TAB_RADIUS: f32 = 9.0;
pub const EXPANDED_HEIGHT: i32 = 205;
const CLR_COLORKEY: COLORREF = 0x00FF00FF; // Magenta colorkey for transparent edges
const TIMER_BALL_ANIM: usize = 1001;

// The expanded drawer never drops below this alpha: its text has to stay readable
// even when the user dials the docked gauge far down.
const ALPHA_DRAWER_FLOOR: u8 = 232;

fn layered_alpha(expanded: bool, opacity: u8) -> u8 {
    if expanded {
        opacity.max(ALPHA_DRAWER_FLOOR)
    } else {
        opacity
    }
}

pub fn get_expanded_width(font_scale: f32) -> i32 {
    if font_scale >= 1.25 {
        268
    } else if font_scale >= 1.10 {
        248
    } else {
        232
    }
}

// Dark Palette
const CLR_BG: COLORREF = 0x00161412; // #121416
const CLR_BALL_BG: COLORREF = 0x00241E1A; // #1A1E24
const CLR_BALL_HOVER: COLORREF = 0x00362E28; // #282E36
const CLR_CARD_BG: COLORREF = 0x00201B18; // #181B20
const CLR_ROW_BG: COLORREF = 0x001A1715; // #15171A
const CLR_BORDER: COLORREF = 0x003A322C; // #2C323A
const CLR_TEXT: COLORREF = 0x00F4F4F4;
const CLR_TEXT_MUTED: COLORREF = 0x00A8A098;
const CLR_GREEN: COLORREF = 0x0081B910; // Emerald Green
const CLR_YELLOW: COLORREF = 0x000B9EF5; // Amber
const CLR_RED: COLORREF = 0x004444EF; // Crimson
const CLR_CYAN: COLORREF = 0x00D4B606; // Cyan Charging
const CLR_ACCENT: COLORREF = 0x00E08030;

pub struct FloatingBallWindow {
    pub hwnd: HWND,
}

static mut BALL_STATE: Option<Arc<Mutex<BallStateData>>> = None;

pub struct BallStateData {
    pub devices: Vec<DeviceSnapshot>,
    pub enabled: bool,
    pub expanded: bool,
    pub font_scale: f32,
    pub dragging: bool,
    pub drag_start_cursor: POINT,
    pub drag_start_win: POINT,
    pub cur_y: i32,
    pub hover: bool,
    pub hover_btn: i32,
    /// Layered-window alpha (0-255) chosen by the user for the docked gauge.
    pub opacity: u8,
    pub host_hwnd: HWND,
    pub hwnd: HWND,
    pub tooltip_hwnd: HWND,
    pub carousel_index: usize,
    pub anim_ticks: u32,
    pub pulse_phase: f32,
}

impl FloatingBallWindow {
    pub fn new(
        host_hwnd: HWND,
        initial_enabled: bool,
        font_scale: f32,
        opacity: u8,
    ) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class_name: Vec<u16> = "LinkDeck_FloatingBallWidget\0".encode_utf16().collect();

            let mut wcx: WNDCLASSEXW = mem::zeroed();
            wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
            wcx.style = CS_HREDRAW | CS_VREDRAW;
            wcx.lpfnWndProc = Some(ball_wnd_proc);
            wcx.hInstance = instance;
            wcx.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
            wcx.lpszClassName = class_name.as_ptr();

            RegisterClassExW(&wcx);

            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);

            let start_y = (screen_h as f32 * 0.75) as i32;
            let start_x = screen_w - BALL_SIZE;

            let state_data = Arc::new(Mutex::new(BallStateData {
                devices: Vec::new(),
                enabled: initial_enabled,
                expanded: false,
                font_scale,
                dragging: false,
                drag_start_cursor: POINT { x: 0, y: 0 },
                drag_start_win: POINT {
                    x: start_x,
                    y: start_y,
                },
                cur_y: start_y,
                hover: false,
                hover_btn: 0,
                opacity,
                host_hwnd,
                hwnd: ptr::null_mut(),
                tooltip_hwnd: ptr::null_mut(),
                carousel_index: 0,
                anim_ticks: 0,
                pulse_phase: 0.0,
            }));
            BALL_STATE = Some(state_data);

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                class_name.as_ptr(),
                "LinkDeck Floating Ball\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
                WS_POPUP,
                start_x,
                start_y,
                BALL_SIZE,
                DOCK_H,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );

            if hwnd.is_null() {
                return Err("Failed to create floating ball window".into());
            }

            set_ball_alpha(hwnd, layered_alpha(false, opacity));

            // Create Tooltip
            let tooltip_hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                "tooltips_class32\0".encode_utf16().collect::<Vec<_>>().as_ptr(),
                ptr::null(),
                WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                0, 0, 0, 0,
                hwnd,
                ptr::null_mut(),
                instance,
                ptr::null(),
            );

            if !tooltip_hwnd.is_null() {
                let mut ti: TTTOOLINFOW = mem::zeroed();
                ti.cbSize = mem::size_of::<TTTOOLINFOW>() as u32;
                ti.uFlags = TTF_SUBCLASS | TTF_IDISHWND;
                ti.hwnd = hwnd;
                ti.uId = hwnd as usize;
                let mut default_tip: Vec<u16> = "LinkDeck 能量环悬浮球\0".encode_utf16().collect();
                ti.lpszText = default_tip.as_mut_ptr();
                SendMessageW(tooltip_hwnd, TTM_ADDTOOLW, 0, &ti as *const _ as LPARAM);
            }

            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.hwnd = hwnd;
                state.tooltip_hwnd = tooltip_hwnd;
            }

            if initial_enabled {
                SetTimer(hwnd, TIMER_BALL_ANIM, 100, None);
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            } else {
                ShowWindow(hwnd, SW_HIDE);
            }

            Ok(Self { hwnd })
        }
    }

    pub fn update_devices(&self, devices: &[DeviceSnapshot]) {
        unsafe {
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.devices = devices.to_vec();
                let active = get_carousel_devices(&state.devices);
                if !active.is_empty() && state.carousel_index >= active.len() {
                    state.carousel_index = 0;
                }
                update_tooltip_for_current_device(&state);
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    pub fn set_font_scale(&self, scale: f32) {
        unsafe {
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.font_scale = scale;
                let cur_y = state.cur_y;
                let is_exp = state.expanded;

                if is_exp {
                    let exp_w = get_expanded_width(scale);
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        screen_w - exp_w,
                        cur_y,
                        exp_w,
                        EXPANDED_HEIGHT,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    /// Applies a user-chosen docked opacity. The drawer keeps its readable floor.
    pub fn set_opacity(&self, opacity: u8) {
        unsafe {
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.opacity = opacity;
                set_ball_alpha(self.hwnd, layered_alpha(state.expanded, opacity));
            }
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        unsafe {
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.enabled = enabled;
                state.expanded = false;

                if enabled {
                    let cur_y = state.cur_y;
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        screen_w - BALL_SIZE,
                        cur_y,
                        BALL_SIZE,
                        DOCK_H,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    set_ball_alpha(self.hwnd, layered_alpha(false, state.opacity));
                    SetTimer(self.hwnd, TIMER_BALL_ANIM, 100, None);
                    ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                } else {
                    KillTimer(self.hwnd, TIMER_BALL_ANIM);
                    ShowWindow(self.hwnd, SW_HIDE);
                }
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        unsafe {
            if let Some(state_arc) = &BALL_STATE {
                let state = state_arc.lock().unwrap();
                state.enabled
            } else {
                false
            }
        }
    }
}

pub fn get_carousel_devices(devices: &[DeviceSnapshot]) -> Vec<&DeviceSnapshot> {
    let active: Vec<&DeviceSnapshot> = devices
        .iter()
        .filter(|d| d.battery.percentage.is_some() || d.battery.online || d.battery.charging)
        .collect();
    if !active.is_empty() {
        active
    } else {
        devices.iter().collect()
    }
}

/// Sets color-key transparency plus a uniform opacity for the painted pixels.
pub unsafe fn set_ball_alpha(hwnd: HWND, alpha: u8) {
    SetLayeredWindowAttributes(hwnd, CLR_COLORKEY, alpha, LWA_COLORKEY | LWA_ALPHA);
}

pub unsafe fn update_ball_tooltip(tooltip_hwnd: HWND, target_hwnd: HWND, text: &str) {
    if tooltip_hwnd.is_null() || target_hwnd.is_null() {
        return;
    }
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let mut ti: TTTOOLINFOW = mem::zeroed();
    ti.cbSize = mem::size_of::<TTTOOLINFOW>() as u32;
    ti.hwnd = target_hwnd;
    ti.uId = target_hwnd as usize;
    ti.lpszText = wide.as_mut_ptr();
    SendMessageW(tooltip_hwnd, TTM_UPDATETIPTEXTW, 0, &ti as *const _ as LPARAM);
}

pub unsafe fn update_tooltip_for_current_device(state: &BallStateData) {
    if state.tooltip_hwnd.is_null() || state.hwnd.is_null() {
        return;
    }
    let active = get_carousel_devices(&state.devices);
    let cur_dev = if !active.is_empty() {
        active.get(state.carousel_index % active.len()).copied()
    } else {
        None
    };

    let text = if let Some(d) = cur_dev {
        let chg_str = d.battery.charging_source.display_badge(d.battery.charging);
        let pct_str = d
            .battery
            .percentage
            .map(|p| format!("{}%", p))
            .unwrap_or_else(|| {
                if d.battery.online {
                    "在线".to_string()
                } else {
                    "待机省电模式".to_string()
                }
            });
        let volt_str = d
            .battery
            .voltage_mv
            .map(|mv| format!(" ({} mV)", mv))
            .unwrap_or_default();
        format!(
            "{} · 电量: {} {}{}\n[鼠标滚轮切换外设 · 点击展开详情]",
            d.name, pct_str, chg_str, volt_str
        )
    } else {
        "LinkDeck 贴边能量环悬浮球\n[等待外设连接]".to_string()
    };
    update_ball_tooltip(state.tooltip_hwnd, state.hwnd, &text);
}

pub unsafe fn draw_text_clean(hdc: HDC, text: &str, rect: &mut RECT, format: u32) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32, rect, format);
}

pub unsafe fn create_clear_font(size: i32, bold: bool) -> HFONT {
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

unsafe extern "system" fn ball_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_BALL_ANIM);
            0
        }
        WM_TIMER => {
            if wparam == TIMER_BALL_ANIM {
                if let Some(state_arc) = &BALL_STATE {
                    let mut state = state_arc.lock().unwrap();
                    let num_devs = get_carousel_devices(&state.devices).len();

                    state.pulse_phase += 0.12;
                    if state.pulse_phase > std::f32::consts::TAU {
                        state.pulse_phase -= std::f32::consts::TAU;
                    }

                    if !state.expanded && !state.hover && !state.dragging && num_devs > 1 {
                        state.anim_ticks += 1;
                        if state.anim_ticks >= 30 {
                            state.anim_ticks = 0;
                            state.carousel_index = (state.carousel_index + 1) % num_devs;
                            update_tooltip_for_current_device(&state);
                        }
                    }
                }
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16) / 120;
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                let num_devs = get_carousel_devices(&state.devices).len();
                if num_devs > 1 {
                    if delta > 0 {
                        state.carousel_index = if state.carousel_index == 0 {
                            num_devs - 1
                        } else {
                            state.carousel_index - 1
                        };
                    } else if delta < 0 {
                        state.carousel_index = (state.carousel_index + 1) % num_devs;
                    }
                    state.anim_ticks = 0;
                    update_tooltip_for_current_device(&state);
                }
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            let (devices, expanded, font_scale, hover, hover_btn, carousel_index, pulse_phase) =
                if let Some(state_arc) = &BALL_STATE {
                    let state = state_arc.lock().unwrap();
                    (
                        state.devices.clone(),
                        state.expanded,
                        state.font_scale,
                        state.hover,
                        state.hover_btn,
                        state.carousel_index,
                        state.pulse_phase,
                    )
                } else {
                    (Vec::new(), false, 1.15, false, 0, 0, 0.0)
                };

            if !expanded {
                if hover {
                    render_ball_widget(
                        mem_dc,
                        width,
                        height,
                        &devices,
                        carousel_index,
                        pulse_phase,
                        font_scale,
                    );
                } else {
                    render_tab_widget(
                        mem_dc,
                        width,
                        height,
                        &devices,
                        carousel_index,
                        pulse_phase,
                        font_scale,
                    );
                }
            } else {
                render_expanded_drawer(mem_dc, width, height, &devices, font_scale, hover_btn);
            }

            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);

            EndPaint(hwnd, &mut ps);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let mut tme: TRACKMOUSEEVENT = mem::zeroed();
            tme.cbSize = mem::size_of::<TRACKMOUSEEVENT>() as u32;
            tme.dwFlags = TME_LEAVE;
            tme.hwndTrack = hwnd;
            TrackMouseEvent(&mut tme);

            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                if state.dragging {
                    let mut cur: POINT = mem::zeroed();
                    GetCursorPos(&mut cur);
                    let dy = cur.y - state.drag_start_cursor.y;
                    let new_y = state.drag_start_win.y + dy;

                    let screen_w = GetSystemMetrics(SM_CXSCREEN);
                    let screen_h = GetSystemMetrics(SM_CYSCREEN);
                    let clamped_y = new_y.clamp(20, screen_h - DOCK_H - 40);
                    state.cur_y = clamped_y;

                    SetWindowPos(
                        hwnd,
                        ptr::null_mut(),
                        screen_w - BALL_SIZE,
                        clamped_y,
                        BALL_SIZE,
                        DOCK_H,
                        SWP_NOACTIVATE,
                    );
                } else {
                    let mut repaint = false;
                    if !state.expanded {
                        if !state.hover {
                            state.hover = true;
                            repaint = true;
                        }
                    } else {
                        let exp_w = get_expanded_width(state.font_scale);
                        let mut new_btn = 0;
                        if x >= exp_w - 30 && x <= exp_w - 4 && y >= 6 && y <= 28 {
                            new_btn = 1; // Close arrow
                        } else if y >= EXPANDED_HEIGHT - 34 && y <= EXPANDED_HEIGHT - 6 {
                            new_btn = 2; // Open panel
                        }
                        if state.hover_btn != new_btn {
                            state.hover_btn = new_btn;
                            repaint = true;
                        }
                    }
                    if repaint {
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
            }
            0
        }
        WM_MOUSELEAVE => {
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                if state.hover || state.hover_btn != 0 {
                    state.hover = false;
                    state.hover_btn = 0;
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            let mut cur: POINT = mem::zeroed();
            GetCursorPos(&mut cur);

            let mut rect: RECT = mem::zeroed();
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);

            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                if !state.expanded {
                    state.dragging = true;
                    state.drag_start_cursor = cur;
                    state.drag_start_win = POINT {
                        x: rect.left,
                        y: rect.top,
                    };
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let mut cur: POINT = mem::zeroed();
            GetCursorPos(&mut cur);

            let mut toggle_drawer = false;
            let mut open_panel = false;

            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                if state.dragging {
                    state.dragging = false;
                    let dist = (cur.y - state.drag_start_cursor.y).abs()
                        + (cur.x - state.drag_start_cursor.x).abs();
                    if dist < 5 {
                        toggle_drawer = true;
                    }
                } else if state.expanded {
                    if state.hover_btn == 1 {
                        toggle_drawer = true;
                    } else if state.hover_btn == 2 {
                        open_panel = true;
                    } else {
                        open_panel = true;
                    }
                }
            }

            if toggle_drawer {
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                if let Some(state_arc) = &BALL_STATE {
                    let mut state = state_arc.lock().unwrap();
                    state.expanded = !state.expanded;
                    let is_exp = state.expanded;
                    let cur_y = state.cur_y;
                    let font_scale = state.font_scale;

                    let (new_w, new_h, new_x) = if is_exp {
                        let exp_w = get_expanded_width(font_scale);
                        (exp_w, EXPANDED_HEIGHT, screen_w - exp_w)
                    } else {
                        (BALL_SIZE, DOCK_H, screen_w - BALL_SIZE)
                    };

                    SetWindowPos(
                        hwnd,
                        ptr::null_mut(),
                        new_x,
                        cur_y,
                        new_w,
                        new_h,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    set_ball_alpha(hwnd, layered_alpha(is_exp, state.opacity));
                }
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }

            if open_panel {
                let host = if let Some(state_arc) = &BALL_STATE {
                    state_arc.lock().unwrap().host_hwnd
                } else {
                    ptr::null_mut()
                };
                if !host.is_null() {
                    windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                        host,
                        windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                        crate::ui::tray::IDM_SHOW_PANEL as _,
                        0,
                    );
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}


/// 左圆角、右侧齐平的贴边拉手：屏幕边就是它的天然收口，不会被看成“被裁掉的圆”。
unsafe fn tab_inside(px: f32, py: f32, x0: f32, x1: f32, y0: f32, y1: f32, r: f32) -> bool {
    if py < y0 || py > y1 || px < x0 || px > x1 {
        return false;
    }
    if px >= x0 + r {
        return true;
    }
    let cx = x0 + r;
    if py < y0 + r {
        return (px - cx).powi(2) + (py - (y0 + r)).powi(2) <= r * r;
    }
    if py > y1 - r {
        return (px - cx).powi(2) + (py - (y1 - r)).powi(2) <= r * r;
    }
    true
}

fn blend(dst: (u8, u8, u8), src: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (dst.0 as f32 * (1.0 - t) + src.0 as f32 * t) as u8,
        (dst.1 as f32 * (1.0 - t) + src.1 as f32 * t) as u8,
        (dst.2 as f32 * (1.0 - t) + src.2 as f32 * t) as u8,
    )
}

unsafe fn render_tab_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let active_list = get_carousel_devices(devices);
    let total_devs = active_list.len();
    let cur_dev = if total_devs > 0 {
        active_list.get(carousel_index % total_devs).copied()
    } else {
        None
    };

    let (pct, charging, chg_source, online, kind) = if let Some(d) = cur_dev {
        (
            d.battery.percentage,
            d.battery.charging,
            d.battery.charging_source,
            d.battery.online,
            d.kind,
        )
    } else {
        (None, false, crate::devices::ChargingSource::None, false, DeviceKind::Other)
    };

    let accent = if charging {
        let pulse = 0.82 + 0.18 * pulse_phase.sin();
        let g = (210.0 + 45.0 * pulse_phase.sin()).clamp(165.0, 255.0) as u8;
        let b = (255.0 * pulse.clamp(0.85, 1.0)) as u8;
        (0u8, g, b)
    } else if let Some(p) = pct {
        if p >= 50 {
            (16u8, 185u8, 129u8)
        } else if p >= 20 {
            (245u8, 158u8, 11u8)
        } else {
            (239u8, 68u8, 68u8)
        }
    } else {
        (80u8, 92u8, 108u8)
    };

    let bg = (0x16u8, 0x19u8, 0x1Eu8);
    let border_rgb = blend(bg, accent, 0.45);
    let track_rgb = (0x2Cu8, 0x32u8, 0x3Au8);

    let x0 = (w - TAB_W) as f32;
    let x1 = w as f32;
    let y0 = 1.0f32;
    let y1 = h as f32 - 1.0;

    // 左侧独立的竖向电量条：文字永远压在深色底上，任何档位都清晰
    let gauge_x0 = x0 + 4.0;
    let gauge_x1 = gauge_x0 + 3.0;
    let gauge_y0 = 7.0;
    let gauge_y1 = h as f32 - 7.0;
    let level_y = gauge_y1 - (gauge_y1 - gauge_y0) * (pct.unwrap_or(0).min(100) as f32 / 100.0);

    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut inside = 0.0f32;
            let mut edge = 0.0f32;
            let mut gauge = 0.0f32;
            let mut filled = 0.0f32;

            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    if tab_inside(px, py, x0, x1, y0, y1, TAB_RADIUS) {
                        inside += 1.0;
                        // 右边界贴着屏幕，不描边，避免出现“断口”
                        if !tab_inside(
                            px,
                            py,
                            x0 + 1.0,
                            x1,
                            y0 + 1.0,
                            y1 - 1.0,
                            (TAB_RADIUS - 1.0).max(0.0),
                        ) {
                            edge += 1.0;
                        }
                        if px >= gauge_x0 && px <= gauge_x1 && py >= gauge_y0 && py <= gauge_y1 {
                            gauge += 1.0;
                            if py >= level_y {
                                filled += 1.0;
                            }
                        }
                    }
                }
            }

            if inside <= 0.0 {
                continue;
            }

            let alpha = inside / n_sub;
            let bar = blend(track_rgb, accent, filled / gauge.max(1.0));
            let rgb = blend(
                blend(bg, bar, gauge / n_sub),
                border_rgb,
                edge / n_sub,
            );
            let r = (rgb.0 as f32 * alpha) as u32;
            let g = (rgb.1 as f32 * alpha) as u32;
            let b = (rgb.2 as f32 * alpha) as u32;
            pixels[(y * w + x) as usize] = (r << 16) | (g << 8) | b;
        }
    }

    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    SetDIBitsToDevice(
        hdc,
        0,
        0,
        w as u32,
        h as u32,
        0,
        0,
        0,
        h as u32,
        pixels.as_ptr() as _,
        &bmi,
        DIB_RGB_COLORS,
    );

    SetBkMode(hdc, TRANSPARENT as i32);

    // 设备图标 / 电量数字 / 轮播点：与完整球体同一套内容，只是排成竖条
    let icon_size = (11.0 * font_scale).round().clamp(10.0, 12.0) as i32;
    let font_icon = create_clear_font(icon_size, true);
    let old_font = SelectObject(hdc, font_icon);
    let (icon_str, icon_clr) = if charging || chg_source.is_charging() {
        ("⚡", CLR_CYAN)
    } else {
        let sym = match kind {
            DeviceKind::Keyboard => "⌨",
            DeviceKind::Mouse => "🖱",
            DeviceKind::Headset => "🎧",
            _ => "📱",
        };
        (sym, CLR_TEXT_MUTED)
    };
    SetTextColor(hdc, icon_clr);
    let mut icon_rect = RECT {
        left: (w - TAB_W + 9) as i32,
        top: 6,
        right: w,
        bottom: 24,
    };
    draw_text_clean(hdc, icon_str, &mut icon_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let pct_size = (10.0 * font_scale).round().clamp(9.0, 10.0) as i32;
    let font_pct = create_clear_font(pct_size, true);
    SelectObject(hdc, font_pct);
    SetTextColor(hdc, if online { CLR_TEXT } else { CLR_TEXT_MUTED });
    let pct_str = if let Some(p) = pct {
        format!("{}", p)
    } else if online {
        "ON".to_string()
    } else {
        "--".to_string()
    };
    SetTextColor(hdc, if online { CLR_TEXT } else { CLR_TEXT_MUTED });
    let mut pct_rect = RECT {
        left: (w - TAB_W + 9) as i32,
        top: 25,
        right: w,
        bottom: 43,
    };
    draw_text_clean(hdc, &pct_str, &mut pct_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_pct);
    DeleteObject(font_icon);

    if total_devs >= 2 {
        let cur_idx = carousel_index % total_devs;
        for i in 0..total_devs.min(3) {
            let (dot_clr, dot_r) = if i == cur_idx {
                (CLR_CYAN, 2)
            } else {
                (CLR_TEXT_MUTED, 1)
            };
            draw_dot(hdc, w - 5, 46 + (i as i32) * 4, dot_r, dot_clr);
        }
    }
}

/// Render the circular/pill Edge Floating Ball
unsafe fn render_ball_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let active_list = get_carousel_devices(devices);
    let total_devs = active_list.len();
    let cur_dev = if total_devs > 0 {
        active_list.get(carousel_index % total_devs).copied()
    } else {
        None
    };

    let (pct, charging, chg_source, online, kind) = if let Some(d) = cur_dev {
        (
            d.battery.percentage,
            d.battery.charging,
            d.battery.charging_source,
            d.battery.online,
            d.kind,
        )
    } else {
        (None, false, crate::devices::ChargingSource::None, false, DeviceKind::Other)
    };

    // 1. Prepare 32-bit DIB pixel buffer (filled with chroma-key CLR_COLORKEY for transparency)
    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let cy = (h - 1) as f32 / 2.0;

    let cx = (w - 1) as f32 / 2.0;
    let r_ball = 22.5f32;
    let r_mid = 18.5f32;
    let r_thickness = 3.6f32;
    let sweep_total = std::f32::consts::TAU;
    let arc_dir = 1.0f32;
    let r_outer = r_mid + r_thickness / 2.0;
    let r_inner = r_mid - r_thickness / 2.0;
    let cap_r = r_thickness / 2.0;

    let ball_bg_rgb = (0x28u8, 0x2Eu8, 0x36u8);
    let track_rgb = (0x28u8, 0x2Eu8, 0x38u8);

    let arc_rgb = if charging {
        // Electric Cyan with subtle breathing pulse
        let pulse_factor = 0.82 + 0.18 * pulse_phase.sin();
        let g = ((210.0 + 45.0 * pulse_phase.sin()).clamp(165.0, 255.0)) as u8;
        let b = (255.0 * pulse_factor.clamp(0.85, 1.0)) as u8;
        (0u8, g, b)
    } else if let Some(p) = pct {
        if p >= 50 {
            (16u8, 185u8, 129u8) // Emerald Green
        } else if p >= 20 {
            (245u8, 158u8, 11u8) // Amber
        } else {
            (239u8, 68u8, 68u8) // Coral Red
        }
    } else {
        (80u8, 92u8, 108u8) // Slate Gray
    };

    let eff_pct = pct.unwrap_or(0).min(100);
    let sweep_rad = (eff_pct as f32 / 100.0) * sweep_total;

    // 弧参数 0 位于 12 点，顺时针推进。
    let arc_point = |s: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + arc_dir * s;
        (cx + r_mid * a.cos(), cy + r_mid * a.sin())
    };
    let (start_cap_x, start_cap_y) = arc_point(0.0);
    let (end_cap_x, end_cap_y) = arc_point(sweep_rad);

    // 3x3 subpixel sampling grid for smooth anti-aliased borders and arcs
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut bg_hits = 0.0f32;
            let mut track_hits = 0.0f32;
            let mut arc_hits = 0.0f32;

            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let dx = px - cx;
                    let dy = py - cy;
                    let d = (dx * dx + dy * dy).sqrt();

                    if d <= r_ball {
                        bg_hits += 1.0;
                        if d >= r_inner && d <= r_outer {
                            track_hits += 1.0;
                        }
                        if eff_pct > 0 {
                            let mut angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                            if angle < 0.0 {
                                angle += 2.0 * std::f32::consts::PI;
                            }
                            if d >= r_inner && d <= r_outer && angle <= sweep_rad {
                                arc_hits += 1.0;
                            } else {
                                let d_start = ((px - start_cap_x).powi(2) + (py - start_cap_y).powi(2)).sqrt();
                                let d_end = ((px - end_cap_x).powi(2) + (py - end_cap_y).powi(2)).sqrt();
                                if d_start <= cap_r || d_end <= cap_r {
                                    arc_hits += 1.0;
                                }
                            }
                        }
                    }
                }
            }

            if bg_hits <= 0.0 {
                continue;
            }

            let mut r = ball_bg_rgb.0 as f32;
            let mut g = ball_bg_rgb.1 as f32;
            let mut b = ball_bg_rgb.2 as f32;

            if track_hits > 0.0 {
                let ta = track_hits / n_sub;
                r = r * (1.0 - ta) + track_rgb.0 as f32 * ta;
                g = g * (1.0 - ta) + track_rgb.1 as f32 * ta;
                b = b * (1.0 - ta) + track_rgb.2 as f32 * ta;
            }

            if arc_hits > 0.0 {
                let aa = arc_hits / n_sub;
                r = r * (1.0 - aa) + arc_rgb.0 as f32 * aa;
                g = g * (1.0 - aa) + arc_rgb.1 as f32 * aa;
                b = b * (1.0 - aa) + arc_rgb.2 as f32 * aa;
            }

            let bg_alpha = bg_hits / n_sub;
            let final_pixel = if bg_alpha >= 0.99 {
                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            } else {
                let border_r = (r * bg_alpha) as u32;
                let border_g = (g * bg_alpha) as u32;
                let border_b = (b * bg_alpha) as u32;
                (border_r << 16) | (border_g << 8) | border_b
            };

            pixels[(y * w + x) as usize] = final_pixel;
        }
    }

    // 2. Transfer anti-aliased ring bitmap to DC
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    SetDIBitsToDevice(
        hdc,
        0,
        0,
        w as u32,
        h as u32,
        0,
        0,
        0,
        h as u32,
        pixels.as_ptr() as _,
        &bmi,
        DIB_RGB_COLORS,
    );

    SetBkMode(hdc, TRANSPARENT as i32);

    // 3. Center Content: 设备图标 / 电量数字 / 轮播点
    let pct_str = if let Some(p) = pct {
        format!("{}", p)
    } else if online {
        "ON".to_string()
    } else {
        "--".to_string()
    };

    let font_icon_size = (11.0 * font_scale).round() as i32;
    let font_icon = create_clear_font(font_icon_size, true);
    let old_font = SelectObject(hdc, font_icon);

    let (icon_str, icon_clr) = if charging || chg_source.is_charging() {
        ("⚡", CLR_CYAN)
    } else {
        let sym = match kind {
            DeviceKind::Keyboard => "⌨",
            DeviceKind::Mouse => "🖱",
            DeviceKind::Headset => "🎧",
            _ => "📱",
        };
        (sym, CLR_TEXT_MUTED)
    };
    SetTextColor(hdc, icon_clr);
    let mut icon_rect = RECT {
        left: 0,
        top: (cy - 19.0) as i32,
        right: w,
        bottom: (cy - 6.0) as i32,
    };
    draw_text_clean(hdc, icon_str, &mut icon_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    let font_pct_size = (13.0 * font_scale).round() as i32;
    let font_pct = create_clear_font(font_pct_size, true);
    SelectObject(hdc, font_pct);
    SetTextColor(hdc, if !online { CLR_TEXT_MUTED } else { CLR_TEXT });

    let mut pct_rect = RECT {
        left: 0,
        top: (cy - 3.0) as i32,
        right: w,
        bottom: (cy + 10.0) as i32,
    };
    draw_text_clean(hdc, &pct_str, &mut pct_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_pct);
    DeleteObject(font_icon);
    // 4. Carousel Pagination Dots: vertical column on the flat edge when docked,
    //    horizontal row under the percentage when hovering.
    if total_devs >= 2 {
        let cur_idx = carousel_index % total_devs;
        let spacing = 6;
        let start_x = (w - ((total_devs as i32 - 1) * spacing)) / 2;
        let dot_y = (cy as i32) + 14;

        for i in 0..total_devs {
            let (dot_clr, dot_r) = if i == cur_idx {
                (CLR_CYAN, 2)
            } else {
                (CLR_TEXT_MUTED, 1)
            };
            draw_dot(hdc, start_x + (i as i32) * spacing, dot_y, dot_r, dot_clr);
        }
    }
}

unsafe fn draw_dot(hdc: HDC, x: i32, y: i32, r: i32, clr: COLORREF) {
    let dot_brush = CreateSolidBrush(clr);
    let old_b = SelectObject(hdc, dot_brush);
    let null_pen = CreatePen(PS_SOLID, 0, clr);
    let old_p = SelectObject(hdc, null_pen);

    RoundRect(hdc, x - r, y - r, x + r + 1, y + r + 1, 2, 2);

    SelectObject(hdc, old_p);
    DeleteObject(null_pen);
    SelectObject(hdc, old_b);
    DeleteObject(dot_brush);
}

/// Render the expanded side drawer
unsafe fn render_expanded_drawer(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    font_scale: f32,
    hover_btn: i32,
) {
    let bg_brush = CreateSolidBrush(CLR_COLORKEY);
    let full_rect = RECT {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    FillRect(hdc, &full_rect, bg_brush);
    DeleteObject(bg_brush);

    let border_pen = CreatePen(PS_SOLID, 1, CLR_BORDER);
    let old_pen = SelectObject(hdc, border_pen);
    let card_brush = CreateSolidBrush(CLR_CARD_BG);
    let old_brush = SelectObject(hdc, card_brush);

    RoundRect(hdc, 0, 0, w + 16, h, 14, 14);

    SelectObject(hdc, old_brush);
    DeleteObject(card_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(border_pen);

    SetBkMode(hdc, TRANSPARENT as i32);

    // Title Font
    let font_size_h = (14.0 * font_scale).round() as i32;
    let font_header = create_clear_font(font_size_h, true);
    let old_font = SelectObject(hdc, font_header);
    SetTextColor(hdc, CLR_TEXT);

    let mut h_rect = RECT {
        left: 12,
        top: 8,
        right: w - 36,
        bottom: 28,
    };
    draw_text_clean(hdc, "⚡ 外设电量与充电状态", &mut h_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // Collapse arrow button
    SetTextColor(hdc, if hover_btn == 1 { CLR_ACCENT } else { CLR_TEXT_MUTED });
    let mut arr_rect = RECT {
        left: w - 28,
        top: 8,
        right: w - 6,
        bottom: 28,
    };
    draw_text_clean(hdc, "▶", &mut arr_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    // Render device rows (up to 3 devices, Keyboard, Mouse, Bluetooth)
    let mut y_row = 34;
    for dev in devices.iter().take(3) {
        render_device_row(hdc, 8, y_row, w - 16, 38, dev, font_scale);
        y_row += 42;
    }

    if devices.is_empty() {
        SetTextColor(hdc, CLR_TEXT_MUTED);
        let mut emp_rect = RECT {
            left: 12,
            top: 50,
            right: w - 12,
            bottom: 80,
        };
        draw_text_clean(hdc, "正在检测已连接外设...", &mut emp_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }

    // Bottom Action Button: "⚙️ 打开管理面板"
    let btn_bg = if hover_btn == 2 { 0x003A3028 } else { 0x002C241E };
    let btn_brush = CreateSolidBrush(btn_bg);
    let btn_border = CreatePen(PS_SOLID, 1, if hover_btn == 2 { CLR_ACCENT } else { CLR_BORDER });
    let op = SelectObject(hdc, btn_border);
    let ob = SelectObject(hdc, btn_brush);

    RoundRect(hdc, 10, h - 34, w - 10, h - 8, 8, 8);

    SelectObject(hdc, ob);
    DeleteObject(btn_brush);
    SelectObject(hdc, op);
    DeleteObject(btn_border);

    let font_size_btn = (12.0 * font_scale).round() as i32;
    let font_btn = create_clear_font(font_size_btn, false);
    SelectObject(hdc, font_btn);

    SetTextColor(hdc, if hover_btn == 2 { CLR_TEXT } else { CLR_TEXT_MUTED });
    let mut b_rect = RECT {
        left: 10,
        top: h - 34,
        right: w - 10,
        bottom: h - 8,
    };
    draw_text_clean(hdc, "⚙️ 打开控制中心", &mut b_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_header);
    DeleteObject(font_btn);
}

unsafe fn render_device_row(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    dev: &DeviceSnapshot,
    font_scale: f32,
) {
    let row_bg = CreateSolidBrush(CLR_ROW_BG);
    let r_rect = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    FillRect(hdc, &r_rect, row_bg);
    DeleteObject(row_bg);

    let icon = match dev.kind {
        DeviceKind::Keyboard => "⌨️",
        DeviceKind::Mouse => "🖱️",
        DeviceKind::Headset => "🎧",
        _ => "📱",
    };

    let short_name = if dev.name.chars().count() > 12 {
        format!("{}...", dev.name.chars().take(10).collect::<String>())
    } else {
        dev.name.clone()
    };

    let font_size_row = (12.5 * font_scale).round() as i32;
    let font_row = create_clear_font(font_size_row, false);
    let old_font = SelectObject(hdc, font_row);

    // Left Device Label
    SetTextColor(hdc, CLR_TEXT);
    let label = format!("{} {}", icon, short_name);
    let mut l_rect = RECT {
        left: x + 6,
        top: y + 2,
        right: x + w - 105,
        bottom: y + 20,
    };
    draw_text_clean(hdc, &label, &mut l_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // Right Percentage Text
    let (status_color, text_pct) = if dev.battery.charging || dev.battery.charging_source.is_charging() {
        (
            CLR_CYAN,
            format!("{}% ⚡[充电中]", dev.battery.percentage.unwrap_or(100)),
        )
    } else if let Some(p) = dev.battery.percentage {
        let (color, txt) = if !dev.battery.online {
            (CLR_TEXT_MUTED, format!("{}% [待机]", p))
        } else {
            let color = if p >= 50 {
                CLR_GREEN
            } else if p >= 20 {
                CLR_YELLOW
            } else {
                CLR_RED
            };
            (color, format!("{}%", p))
        };
        (color, txt)
    } else if dev.battery.online {
        (CLR_GREEN, "在线".to_string())
    } else {
        (CLR_TEXT_MUTED, "待机".to_string())
    };
    SetTextColor(hdc, status_color);
    let mut p_rect = RECT {
        left: x + w - 110,
        top: y + 2,
        right: x + w - 6,
        bottom: y + 20,
    };
    draw_text_clean(hdc, &text_pct, &mut p_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_row);

    // Mini battery progress bar
    let bar_x = x + 6;
    let bar_y = y + 24;
    let bar_w = w - 12;
    let bar_h = 4;

    let bar_bg = CreateSolidBrush(0x002A2420);
    let bg_rect = RECT {
        left: bar_x,
        top: bar_y,
        right: bar_x + bar_w,
        bottom: bar_y + bar_h,
    };
    FillRect(hdc, &bg_rect, bar_bg);
    DeleteObject(bar_bg);

    let fill_pct = dev.battery.percentage.unwrap_or(0) as i32;
    let fill_w = (bar_w * fill_pct) / 100;
    if fill_w > 0 {
        let bar_fg = CreateSolidBrush(status_color);
        let fg_rect = RECT {
            left: bar_x,
            top: bar_y,
            right: bar_x + fill_w,
            bottom: bar_y + bar_h,
        };
        FillRect(hdc, &fg_rect, bar_fg);
        DeleteObject(bar_fg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::types::{BatteryState, ConnectionMode, DeviceKind, DeviceSnapshot};
    use std::time::SystemTime;

    fn create_test_dev(id: &str, name: &str, kind: DeviceKind, pct: Option<u8>, charging: bool, online: bool) -> DeviceSnapshot {
        DeviceSnapshot {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            vendor_id: 0x1234,
            product_id: 0x5678,
            connection: ConnectionMode::Wireless24G,
            battery: BatteryState {
                percentage: pct,
                voltage_mv: Some(4200),
                charging,
                charging_source: if charging { crate::devices::types::ChargingSource::Charging } else { crate::devices::types::ChargingSource::None },
                online,
                last_updated: SystemTime::now(),
            },
            path: "test_path".to_string(),
        }
    }

    #[test]
    fn test_carousel_filter() {
        let kbd = create_test_dev("kbd", "RK108 Keyboard", DeviceKind::Keyboard, Some(100), true, true);
        let mouse_asleep = create_test_dev("mouse", "Saiggi Mouse", DeviceKind::Mouse, None, false, false);
        let bt_headset = create_test_dev("bt", "P6 Headset", DeviceKind::Headset, Some(80), false, true);

        let all = vec![kbd.clone(), mouse_asleep.clone(), bt_headset.clone()];
        let carousel = get_carousel_devices(&all);

        // Should pick the 2 active devices (keyboard charging & headset online)
        assert_eq!(carousel.len(), 2);
        assert_eq!(carousel[0].id, "kbd");
        assert_eq!(carousel[1].id, "bt");
        assert!(carousel[0].battery.charging);
    }

    #[test]
    fn test_carousel_fallback() {
        let mouse_asleep = create_test_dev("mouse", "Saiggi Mouse", DeviceKind::Mouse, None, false, false);
        let all = vec![mouse_asleep.clone()];
        let carousel = get_carousel_devices(&all);

        // Fallback to all when none active
        assert_eq!(carousel.len(), 1);
        assert_eq!(carousel[0].id, "mouse");
    }


}
