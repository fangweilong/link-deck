#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::{ConnectionMode, DeviceSnapshot};
use crate::ui::clipboard::copy_text_to_clipboard;
use crate::ui::panel_render::{render_action_btn, render_choice_pill, render_device_card};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    RoundRect, SelectClipRgn, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HDC,
    HFONT, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowRect, GetWindowTextW, IsDialogMessageW, LoadCursorW,
    PostMessageW, RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL, ICON_BIG, ICON_SMALL, IDC_ARROW,
    IsIconic, MINMAXINFO, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_RESTORE, SW_SHOW,
    WM_CLOSE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT, WM_SETICON,
    WM_SIZE, WM_VSCROLL, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
    WS_POPUP, WS_SYSMENU, WS_VISIBLE, SW_SHOWNORMAL,
    SB_BOTTOM, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP,
};
// No native scrollbar API: the panel draws its own compact thumb.
pub const MIN_PANEL_WIDTH: i32 = 480;
pub const MIN_PANEL_HEIGHT: i32 = 400;
pub const PANEL_WIDTH: i32 = 640;
pub const PANEL_HEIGHT: i32 = 680;
/// Logical height of the settings flow (last pill bottom + bottom padding).
pub const SETTINGS_CONTENT_HEIGHT: i32 = 980;

// Custom messages exchanged with the hidden host window.
pub const WM_SAVE_PANEL_SIZE: u32 = 0x8000 + 105;
pub const WM_SAVE_UI_DENSITY: u32 = 0x8000 + 106;
pub const WM_SAVE_DASHBOARD_PREFS: u32 = 0x8000 + 107;
pub const WM_SAVE_ONBOARDING: u32 = 0x8000 + 108;
pub const WM_SAVE_ACCENT_THEME: u32 = 0x8000 + 109;
pub const WM_SAVE_UPDATE_PREFS: u32 = 0x8000 + 110;
pub const WM_SAVE_AUTOSTART: u32 = 0x8000 + 111;
/// wParam carries the docked ball opacity in percent (40-100).
pub const WM_SET_BALL_OPACITY: u32 = 0x8000 + 112;
pub const WM_DEVICE_RENAME: u32 = 0x8000 + 101;
pub const WM_SAVE_TOAST_SETTINGS: u32 = 0x8000 + 104;

// wParam packs filter in bits 0..7 and sort in bits 8..15.
pub(super) const CLR_BG: COLORREF = 0x00161412;
pub(super) const CLR_HEADER: COLORREF = 0x00221E1A;
pub(super) const CLR_CARD: COLORREF = 0x00201B18;
pub(super) const CLR_CARD_BORDER: COLORREF = 0x003A322C;
pub(super) const CLR_TEXT: COLORREF = 0x00F4F4F4;
pub(super) const CLR_MUTED: COLORREF = 0x00B0A8A0;
pub(super) const CLR_ACCENT: COLORREF = 0x00E08030;
pub(super) const CLR_GREEN: COLORREF = 0x0081B910;
pub(super) const CLR_YELLOW: COLORREF = 0x000B9EF5;
pub(super) const CLR_RED: COLORREF = 0x004444EF;
pub(super) const CLR_CYAN: COLORREF = 0x00D4B606;
pub(super) const CLR_BTN_BG: COLORREF = 0x002C241E;
pub(super) const CLR_BTN_HOVER: COLORREF = 0x003E332A;
pub(super) const CLR_BTN_ACTIVE: COLORREF = 0x00524438;

pub struct PanelWindow {
    pub hwnd: HWND,
}

pub static mut PANEL_STATE: Option<Arc<Mutex<PanelStateData>>> = None;

pub struct PanelStateData {
    pub devices: Vec<DeviceSnapshot>,
    pub visible: bool,
    pub hover_btn: i32,
    pub floating_ball_enabled: bool,
    pub font_scale: f32,
    pub poll_interval_ms: u64,
    pub ball_opacity: u8,
    pub ui_density: crate::config::UiDensity,
    pub scroll_y: i32,
    pub settings_scroll_y: i32,
    pub refresh_status: String,
    pub next_refresh_seconds: u64,
    pub device_filter: crate::config::DeviceFilter,
    pub device_sort: crate::config::DeviceSort,
    pub accent_theme: crate::config::AccentTheme,
    pub show_onboarding: bool,
    pub selected_device_id: Option<String>,
    pub host_hwnd: HWND,
    pub active_tab: usize,
    pub enable_toast: bool,
    pub toast_info: u8,
    pub toast_warning: u8,
    pub toast_critical: u8,
    pub toast_gaming_suppression: bool,
    pub enable_low_battery_sound: bool,
    pub auto_update: bool,
    pub auto_start: bool,
}
impl PanelWindow {
    pub fn new(
        host_hwnd: HWND,
        config: &crate::config::AppConfig,
    ) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class_name: Vec<u16> = "LinkDeck_ManagementPanel\0".encode_utf16().collect();

            let mut wcx: WNDCLASSEXW = mem::zeroed();
            wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
            wcx.style = CS_HREDRAW | CS_VREDRAW;
            wcx.lpfnWndProc = Some(panel_wnd_proc);
            let big_icon = crate::ui::icon::create_linkdeck_icon(48);
            let small_icon = crate::ui::icon::create_linkdeck_icon(16);
            wcx.lpfnWndProc = Some(panel_wnd_proc);
            wcx.hInstance = instance;
            wcx.hIcon = big_icon;
            wcx.hIconSm = small_icon;
            wcx.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
            wcx.lpszClassName = class_name.as_ptr();
            RegisterClassExW(&wcx);
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let panel_width = config.panel_width.clamp(MIN_PANEL_WIDTH, screen_w.max(MIN_PANEL_WIDTH));
            let panel_height = config.panel_height.clamp(MIN_PANEL_HEIGHT, screen_h.max(MIN_PANEL_HEIGHT));
            let start_x = (screen_w - panel_width) / 2;
            let start_y = (screen_h - panel_height) / 2;

            let state_data = Arc::new(Mutex::new(PanelStateData {
                devices: Vec::new(),
                visible: false,
                hover_btn: 0,
                floating_ball_enabled: config.enable_floating_ball,
                font_scale: config.font_scale,
                poll_interval_ms: config.poll_interval_ms,
                ball_opacity: config.floating_ball_opacity,
                ui_density: config.ui_density,
                scroll_y: 0,
                settings_scroll_y: 0,
                refresh_status: "就绪 · 可立即刷新".to_string(),
                next_refresh_seconds: config.poll_interval_ms / 1000,
                device_filter: config.device_filter,
                device_sort: config.device_sort,
                accent_theme: config.accent_theme,
                show_onboarding: config.show_onboarding,
                selected_device_id: None,
                host_hwnd,
                active_tab: 0,
                enable_toast: config.enable_battery_toast,
                toast_info: config.toast_threshold_info,
                toast_warning: config.toast_threshold_warning,
                toast_critical: config.toast_threshold_critical,
                toast_gaming_suppression: config.toast_gaming_suppression,
                enable_low_battery_sound: config.enable_low_battery_sound,
                auto_update: config.check_updates,
                auto_start: config.auto_start,
            }));
            PANEL_STATE = Some(state_data);

            let title_wide: Vec<u16> = "LinkDeck - 外设控制中心\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                class_name.as_ptr(),
                title_wide.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                start_x,
                start_y,
                panel_width,
                panel_height,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            if hwnd.is_null() {
                return Err("Failed to create management panel window".into());
            }

            if !big_icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, big_icon as LPARAM);
            }
            if !small_icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, small_icon as LPARAM);
            }

            Ok(Self { hwnd })
        }
    }

    pub fn update_devices(
        &self,
        devices: &[DeviceSnapshot],
        ball_enabled: bool,
        ball_opacity: u8,
        font_scale: f32,
        poll_interval_ms: u64,
        ui_density: crate::config::UiDensity,
    ) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.devices = devices.to_vec();
                state.floating_ball_enabled = ball_enabled;
                state.font_scale = font_scale;
                state.poll_interval_ms = poll_interval_ms;
                state.ball_opacity = ball_opacity;
                state.next_refresh_seconds = poll_interval_ms / 1000;
                state.ui_density = ui_density;
                let filtered_count = prepare_devices(state.devices.clone(), state.device_filter, state.device_sort).len();
                let mut client: RECT = mem::zeroed();
                GetClientRect(self.hwnd, &mut client);
                let viewport = (client.bottom - client.top - 132).max(1);
                let step = match state.ui_density {
                    crate::config::UiDensity::Compact => 96,
                    crate::config::UiDensity::Spacious => 134,
                    crate::config::UiDensity::Standard => 114,
                };
                let content = filtered_count as i32 * step;
                state.scroll_y = state.scroll_y.min((content - viewport).max(0));
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }
    pub fn set_refresh_status(&self, status: impl Into<String>) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                state_arc.lock().unwrap().refresh_status = status.into();
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    pub fn show(&self) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.visible = true;
            }
            if IsIconic(self.hwnd) != 0 {
                ShowWindow(self.hwnd, SW_RESTORE);
            } else {
                ShowWindow(self.hwnd, SW_SHOW);
            }
            SetForegroundWindow(self.hwnd);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    /// 请求重绘（更新状态变化时由宿主调用）
    pub fn invalidate(&self) {
        unsafe {
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    pub fn show_settings(&self) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.visible = true;
                state.active_tab = 1;
            }
            if IsIconic(self.hwnd) != 0 {
                ShowWindow(self.hwnd, SW_RESTORE);
            } else {
                ShowWindow(self.hwnd, SW_SHOW);
            }
            SetForegroundWindow(self.hwnd);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    pub fn toggle(&self) {
        unsafe {
            let is_vis = if let Some(state_arc) = &PANEL_STATE {
                let state = state_arc.lock().unwrap();
                state.visible
            } else {
                false
            };

            if is_vis {
                self.hide();
            } else {
                self.show();
            }
        }
    }

    pub fn hide(&self) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.visible = false;
            }
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

/// 在资源管理器中打开配置文件所在目录（必要时先创建）。
unsafe fn open_config_dir() {
    use std::os::windows::ffi::OsStrExt;

    let dir = crate::config::AppConfig::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let path: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    ShellExecuteW(
        ptr::null_mut(),
        operation.as_ptr(),
        path.as_ptr(),
        ptr::null(),
        ptr::null(),
        SW_SHOWNORMAL,
    );
}

unsafe fn dismiss_onboarding(host: HWND) {
    if let Some(state_arc) = &PANEL_STATE {
        state_arc.lock().unwrap().show_onboarding = false;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_ONBOARDING, 0, 0);
    }
}

unsafe fn set_panel_density(hwnd: HWND, host: HWND, density: crate::config::UiDensity) {
    if let Some(state_arc) = &PANEL_STATE {
        let mut state = state_arc.lock().unwrap();
        state.ui_density = density;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_UI_DENSITY, density as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe fn set_ball_opacity(hwnd: HWND, host: HWND, percent: u8) {
    if let Some(state_arc) = &PANEL_STATE {
        let mut state = state_arc.lock().unwrap();
        state.ball_opacity = percent;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SET_BALL_OPACITY, percent as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

fn prepare_devices(
    mut devices: Vec<DeviceSnapshot>,
    filter: crate::config::DeviceFilter,
    sort: crate::config::DeviceSort,
) -> Vec<DeviceSnapshot> {
    devices.retain(|dev| match filter {
        crate::config::DeviceFilter::All => true,
        crate::config::DeviceFilter::LowBattery => dev.battery.percentage.map(|p| p < 30).unwrap_or(false),
        crate::config::DeviceFilter::Charging => dev.battery.charging || dev.battery.charging_source.is_charging(),
        crate::config::DeviceFilter::Bluetooth => dev.connection == ConnectionMode::Bluetooth,
        crate::config::DeviceFilter::Wireless => dev.connection == ConnectionMode::Wireless24G,
    });
    devices.sort_by(|a, b| match sort {
        crate::config::DeviceSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        crate::config::DeviceSort::Battery => a.battery.percentage.unwrap_or(101).cmp(&b.battery.percentage.unwrap_or(101)),
        crate::config::DeviceSort::Updated => b.battery.last_updated.cmp(&a.battery.last_updated),
        crate::config::DeviceSort::Priority => {
            let key = |d: &DeviceSnapshot| (if !d.battery.online { 3 } else if d.battery.percentage.map(|p| p < 30).unwrap_or(false) { 0 } else if d.battery.charging { 1 } else { 2 }, d.name.to_lowercase());
            key(a).cmp(&key(b))
        }
    });
    devices
}

/// Maximum device-tab scroll offset. Uses the filtered list so it always
/// matches what is actually painted.
unsafe fn device_max_scroll(hwnd: HWND, state: &PanelStateData) -> i32 {
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let viewport = (client.bottom - client.top - 132).max(1);
    let step = match state.ui_density {
        crate::config::UiDensity::Compact => 96,
        crate::config::UiDensity::Spacious => 134,
        crate::config::UiDensity::Standard => 114,
    };
    let count = prepare_devices(state.devices.clone(), state.device_filter, state.device_sort).len();
    ((count as i32 * step) - viewport).max(0)
}

/// Settings content is a fixed-height flow; viewport excludes header and footer.
unsafe fn settings_max_scroll(hwnd: HWND) -> i32 {
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let viewport = (client.bottom - client.top - 132).max(1);
    (SETTINGS_CONTENT_HEIGHT - viewport).max(0)
}

unsafe fn set_dashboard_preferences(hwnd: HWND, host: HWND, filter: crate::config::DeviceFilter, sort: crate::config::DeviceSort) {
    if let Some(state_arc) = &PANEL_STATE {
        let mut state = state_arc.lock().unwrap();
        state.device_filter = filter;
        state.device_sort = sort;
        let max_scroll = device_max_scroll(hwnd, &state);
        state.scroll_y = state.scroll_y.min(max_scroll);
    }
    if !host.is_null() {
        let filter_code = match filter { crate::config::DeviceFilter::All => 0, crate::config::DeviceFilter::LowBattery => 1, crate::config::DeviceFilter::Charging => 2, crate::config::DeviceFilter::Bluetooth => 3, crate::config::DeviceFilter::Wireless => 4 };
        let sort_code = match sort { crate::config::DeviceSort::Priority => 0, crate::config::DeviceSort::Name => 1, crate::config::DeviceSort::Battery => 2, crate::config::DeviceSort::Updated => 3 };
        PostMessageW(host, WM_SAVE_DASHBOARD_PREFS, (filter_code | (sort_code << 8)) as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe fn set_accent_theme(hwnd: HWND, host: HWND, theme: crate::config::AccentTheme) {
    if let Some(state_arc) = &PANEL_STATE {
        state_arc.lock().unwrap().accent_theme = theme;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_ACCENT_THEME, theme as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
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



unsafe extern "system" fn panel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_GETMINMAXINFO => {
            let info = lparam as *mut MINMAXINFO;
            if !info.is_null() {
                (*info).ptMinTrackSize.x = MIN_PANEL_WIDTH;
                (*info).ptMinTrackSize.y = MIN_PANEL_HEIGHT;
            }
            0
        }
        WM_SIZE => {
            if wparam != 1 {
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_VSCROLL => {
            let code = (wparam & 0xFFFF) as i32;
            let thumb_pos = ((wparam >> 16) & 0xFFFF) as i32;
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                let mut client: RECT = mem::zeroed();
                GetClientRect(hwnd, &mut client);
                let viewport = (client.bottom - client.top - 132).max(1);
                let on_devices = state.active_tab == 0;
                let step = match state.ui_density {
                    crate::config::UiDensity::Compact => 96,
                    crate::config::UiDensity::Spacious => 134,
                    crate::config::UiDensity::Standard => 114,
                };
                let line_step = if on_devices { step / 3 } else { 48 };
                let max_scroll = if on_devices {
                    device_max_scroll(hwnd, &state)
                } else {
                    settings_max_scroll(hwnd)
                };
                let mut pos = if on_devices { state.scroll_y } else { state.settings_scroll_y };
                match code {
                    x if x == SB_LINEUP        => pos -= line_step,
                    x if x == SB_LINEDOWN      => pos += line_step,
                    x if x == SB_PAGEUP        => pos -= viewport,
                    x if x == SB_PAGEDOWN      => pos += viewport,
                    x if x == SB_TOP           => pos = 0,
                    x if x == SB_BOTTOM        => pos = max_scroll,
                    x if x == SB_THUMBTRACK    => pos = thumb_pos,
                    x if x == SB_THUMBPOSITION => pos = thumb_pos,
                    _ => {}
                }
                let clamped = pos.clamp(0, max_scroll);
                if on_devices {
                    state.scroll_y = clamped;
                } else {
                    state.settings_scroll_y = clamped;
                }
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_EXITSIZEMOVE => {
            let mut r: RECT = mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            let (w, h, host) = if let Some(state_arc) = &PANEL_STATE {
                let state = state_arc.lock().unwrap();
                (r.right - r.left, r.bottom - r.top, state.host_hwnd)
            } else {
                (0, 0, ptr::null_mut())
            };
            if !host.is_null() {
                PostMessageW(host, WM_SAVE_PANEL_SIZE, w as WPARAM, h as LPARAM);
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16) as i32;
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                if state.active_tab == 0 {
                    let max_scroll = device_max_scroll(hwnd, &state);
                    state.scroll_y = (state.scroll_y - delta / 3).clamp(0, max_scroll);
                } else {
                    let max_scroll = settings_max_scroll(hwnd);
                    state.settings_scroll_y = (state.settings_scroll_y - delta / 3).clamp(0, max_scroll);
                }
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_CLOSE => {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.visible = false;
            }
            ShowWindow(hwnd, SW_HIDE);
            0
        }
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

            let bg_brush = CreateSolidBrush(CLR_BG);
            FillRect(mem_dc, &rect, bg_brush);
            DeleteObject(bg_brush);

            // Header Banner
            let header_rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: 64,
            };
            let h_brush = CreateSolidBrush(CLR_HEADER);
            FillRect(mem_dc, &header_rect, h_brush);
            DeleteObject(h_brush);

            SetBkMode(mem_dc, TRANSPARENT as i32);

            let (raw_devices, ball_on, ball_opacity, font_scale, poll_ms, hover_btn, active_tab, enable_toast, toast_info, toast_warn, toast_crit, toast_gaming, toast_sound, ui_density, scroll_y, settings_scroll_y, refresh_status, device_filter, device_sort, accent_theme, show_onboarding, selected_device_id, auto_update, auto_start) =
                if let Some(state_arc) = &PANEL_STATE {
                    let state = state_arc.lock().unwrap();
                    (state.devices.clone(), state.floating_ball_enabled, state.ball_opacity, state.font_scale,
                        state.poll_interval_ms, state.hover_btn, state.active_tab,
                        state.enable_toast, state.toast_info, state.toast_warning,
                        state.toast_critical, state.toast_gaming_suppression,
                        state.enable_low_battery_sound,
                        state.ui_density, state.scroll_y, state.settings_scroll_y, state.refresh_status.clone(),
                        state.device_filter, state.device_sort, state.accent_theme, state.show_onboarding,
                        state.selected_device_id.clone(), state.auto_update, state.auto_start)
                } else {
                    (Vec::new(), false, 80, 1.15, 300_000, 0, 0, true, 50, 30, 10, true, true,
                        crate::config::UiDensity::Standard, 0, 0, "就绪 · 可立即刷新".to_string(),
                        crate::config::DeviceFilter::All, crate::config::DeviceSort::Priority,
                        crate::config::AccentTheme::Orange, true, None, true, false)
                };
            let raw_count = raw_devices.len();
            let devices = prepare_devices(raw_devices, device_filter, device_sort);

            // Header Title
            let font_h1 = create_clear_font((19.0 * font_scale).round() as i32, true);
            let old_font = SelectObject(mem_dc, font_h1);
            SetTextColor(mem_dc, CLR_TEXT);

            let mut h_title_rect = RECT {
                left: 20,
                top: 8,
                right: width - 230,
                bottom: 34,
            };
            draw_text_clean(mem_dc, "LinkDeck 外设控制中心", &mut h_title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Header Subtitle
            let font_sub = create_clear_font((12.5 * font_scale).round() as i32, false);
            SelectObject(mem_dc, font_sub);
            SetTextColor(mem_dc, CLR_MUTED);

            let mut h_sub_rect = RECT {
                left: 20,
                top: 36,
                right: width - 230,
                bottom: 56,
            };
            draw_text_clean(mem_dc, "状态栏常驻 · 贴边悬浮球 · 2.4G/蓝牙多协议电量管理", &mut h_sub_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Header Right: View Switch Tabs
            render_choice_pill(mem_dc, width - 215, 18, 92, 28, "📊 设备监控", active_tab == 0, hover_btn == 201, font_scale);
            render_choice_pill(mem_dc, width - 110, 18, 92, 28, "⚙️ 偏好设置", active_tab == 1, hover_btn == 202, font_scale);
            if active_tab == 0 {
                if show_onboarding && raw_count == 0 {
                    let onboarding_brush = CreateSolidBrush(CLR_CARD);
                    let onboarding_rect = RECT { left: 28, top: 106, right: width - 28, bottom: 280 };
                    FillRect(mem_dc, &onboarding_rect, onboarding_brush);
                    DeleteObject(onboarding_brush);
                    SetTextColor(mem_dc, CLR_TEXT);
                    let onboarding_font = create_clear_font((16.0 * font_scale).round() as i32, true);
                    let old_onboarding = SelectObject(mem_dc, onboarding_font);
                    let mut title = RECT { left: 48, top: 124, right: width - 48, bottom: 150 };
                    draw_text_clean(mem_dc, "欢迎使用 LinkDeck", &mut title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
                    SelectObject(mem_dc, font_sub);
                    SetTextColor(mem_dc, CLR_MUTED);
                    let mut body = RECT { left: 48, top: 158, right: width - 48, bottom: 220 };
                    draw_text_clean(mem_dc, "它会在后台读取支持的键盘、鼠标、耳机和蓝牙设备电量。\n可通过悬浮球或托盘打开控制中心。", &mut body, DT_LEFT);
                    render_action_btn(mem_dc, 48, 232, 150, 30, "开始使用", false, font_scale);
                    SelectObject(mem_dc, old_onboarding);
                    DeleteObject(onboarding_font);
                }
                // --- TAB 0: DEVICE DASHBOARD ---
                let (card_h, card_step) = match ui_density {
                    crate::config::UiDensity::Compact => (88, 96),
                    crate::config::UiDensity::Spacious => (124, 134),
                    crate::config::UiDensity::Standard => (106, 114),
                };
                let content_clip = CreateRectRgn(0, 66, width, (height - 58).max(67));
                SelectClipRgn(mem_dc, content_clip);
                for (idx, dev) in devices.iter().enumerate() {
                    let y_card = 74 + idx as i32 * card_step - scroll_y;
                    if y_card + card_h < 66 || y_card > height - 58 { continue; }
                    let protocol = match dev.connection {
                        ConnectionMode::Bluetooth => "Windows 蓝牙电池服务 (GATT / HFP)",
                        ConnectionMode::Wireless24G if dev.id.contains("rk") => "Compx 2.4G 键盘直连协议",
                        ConnectionMode::Wireless24G if dev.id.contains("logitech") => "Logitech HID++ 2.4G 接收器协议",
                        ConnectionMode::Wireless24G if dev.id.contains("razer") => "Razer Hyperspeed 2.4G 接收器协议",
                        ConnectionMode::Wireless24G if dev.id.contains("mchose") => "MCHOSE 2.4G 接收器协议",
                        ConnectionMode::Wireless24G if dev.id.contains("vxe") => "VXE / VGN 2.4G 接收器协议",
                        ConnectionMode::Wireless24G if dev.id.contains("mouse") => "Xenta / Areson 2.4G HID 协议",
                        ConnectionMode::Wireless24G => "2.4G 无线 HID 直连协议",
                        ConnectionMode::Wired => "USB 有线直连通信",
                        ConnectionMode::Unknown => "标准 HID 电量服务",
                    };
                    render_device_card(mem_dc, 20, y_card, width - 40, card_h,
                        &dev.name, dev.kind.icon(), protocol, Some(dev), font_scale,
                        hover_btn == (100 + idx as i32), hover_btn == (200 + idx as i32));
                    if selected_device_id.as_deref() == Some(dev.id.as_str()) {
                        render_action_btn(mem_dc, 20, y_card + card_h - 28, 120, 22, "已选中 · 详情", false, font_scale);
                    }
                }
                let content_h = (devices.len() as i32 * card_step).max(1);
                let viewport_h = (height - 132).max(1);
                if content_h > viewport_h {
                    let track_x = width - 10;
                    let track_brush = CreateSolidBrush(CLR_CARD_BORDER);
                    let track = RECT { left: track_x, top: 70, right: track_x + 4, bottom: height - 60 };
                    FillRect(mem_dc, &track, track_brush);
                    DeleteObject(track_brush);
                    let thumb_h = ((viewport_h * viewport_h) / content_h).max(24);
                    let max_thumb_y = viewport_h - thumb_h;
                    let max_scroll = (content_h - viewport_h).max(1);
                    let thumb_y = 70 + ((scroll_y * max_thumb_y) / max_scroll);
                    let thumb_brush = CreateSolidBrush(CLR_ACCENT);
                    let thumb = RECT { left: track_x - 1, top: thumb_y, right: track_x + 5, bottom: thumb_y + thumb_h };
                    FillRect(mem_dc, &thumb, thumb_brush);
                    DeleteObject(thumb_brush);
                }
                SelectClipRgn(mem_dc, ptr::null_mut());
                DeleteObject(content_clip);

                if devices.is_empty() {
                    let (head_line, tip_line) = if raw_count > 0 {
                        (
                            "当前筛选条件下没有匹配的设备",
                            "已扫描到设备，但被筛选器隐藏。点击“偏好设置 → 设备列表”将筛选切回“全部”。",
                        )
                    } else {
                        (
                            "未检测到已适配的 2.4G 外设或在线蓝牙设备",
                            "已适配: 罗技/雷蛇全系 · RK/狼蛛键盘 · 迈从/VXE/达尔优鼠标 · 赛技 · 全系蓝牙\n提示: 多模外设切至蓝牙模式可直接读取；可在终端运行 --protocols 查看完整清单",
                        )
                    };
                    SetTextColor(mem_dc, CLR_TEXT);
                    let font_empty_title = create_clear_font((15.0 * font_scale).round() as i32, true);
                    let old_empty_f = SelectObject(mem_dc, font_empty_title);
                    let mut emp_rect = RECT {
                        left: 20,
                        top: 130,
                        right: width - 20,
                        bottom: 156,
                    };
                    draw_text_clean(mem_dc, head_line, &mut emp_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                    SelectObject(mem_dc, old_empty_f);
                    DeleteObject(font_empty_title);

                    SetTextColor(mem_dc, CLR_MUTED);
                    let font_empty_tip = create_clear_font((12.5 * font_scale).round() as i32, false);
                    let old_tip_f = SelectObject(mem_dc, font_empty_tip);
                    let mut emp_tip_rect = RECT {
                        left: 20,
                        top: 162,
                        right: width - 20,
                        bottom: 226,
                    };
                    draw_text_clean(mem_dc, tip_line, &mut emp_tip_rect, DT_CENTER);
                    SelectObject(mem_dc, old_tip_f);
                    DeleteObject(font_empty_tip);
                    if raw_count > 0 {
                        render_action_btn(mem_dc, (width - 160) / 2, 240, 160, 32, "显示全部设备", hover_btn == 9, font_scale);
                    }
                }

                // Dashboard footer is painted after the content clip and stays visible at every height.
                let footer_rect = RECT { left: 0, top: (height - 62).max(0), right: width, bottom: height };
                let footer_brush = CreateSolidBrush(CLR_HEADER);
                FillRect(mem_dc, &footer_rect, footer_brush);
                DeleteObject(footer_brush);
                render_action_btn(mem_dc, 20, height - 48, 160, 34, "刷新设备", hover_btn == 1, font_scale);
                render_action_btn(mem_dc, 190, height - 48, 130, 34, "偏好设置", hover_btn == 2, font_scale);
                SetTextColor(mem_dc, if refresh_status.starts_with("刷新失败") { CLR_RED } else { CLR_MUTED });
                let mut status_rect = RECT { left: 340, top: height - 42, right: width - 160, bottom: height - 18 };
                draw_text_clean(mem_dc, &refresh_status, &mut status_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
            } else {
                // Clip to scrollable area; render_settings_view applies -scroll_y to every coord directly.
                let settings_clip = CreateRectRgn(0, 66, width, (height - 62).max(67));
                SelectClipRgn(mem_dc, settings_clip);
                render_settings_view(
                    mem_dc, width, height, font_scale, poll_ms, ball_on, ball_opacity,
                    enable_toast, toast_info, toast_warn, toast_crit, toast_gaming,
                    toast_sound,
                    ui_density, device_filter, device_sort, accent_theme, settings_scroll_y, hover_btn,
                    auto_update, auto_start,
                );
                SelectClipRgn(mem_dc, ptr::null_mut());
                DeleteObject(settings_clip);
                // Footer "返回" button is outside the scroll clip — always visible.
                render_action_btn(mem_dc, 20, height - 48, 150, 34, "◀ 返回设备监控", hover_btn == 90, font_scale);
            }

            // Version label has its own right-aligned slot; it never shares button bounds.
            SetTextColor(mem_dc, CLR_MUTED);
            let mut ver_rect = RECT {
                left: width - 150,
                top: height - 42,
                right: width - 20,
                bottom: height - 18,
            };
            draw_text_clean(mem_dc, concat!("v", env!("CARGO_PKG_VERSION"), " (Rust)"), &mut ver_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

            // Blit
            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_font);
            DeleteObject(font_h1);
            DeleteObject(font_sub);
            // Delete fonts
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);

            EndPaint(hwnd, &mut ps);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let active_tab = if let Some(state_arc) = &PANEL_STATE {
                state_arc.lock().unwrap().active_tab
            } else {
                0
            };

            let mut new_hover = 0;
            // Top header tabs
            if y >= 14 && y <= 48 {
                if x >= width - 215 && x <= width - 123 {
                    new_hover = 201;
                } else if x >= width - 110 && x <= width - 18 {
                    new_hover = 202;
                }
            } else if active_tab == 0 {
                if y >= height - 48 && y <= height - 14 {
                    if x >= 20 && x <= 180 {
                        new_hover = 1;
                    } else if x >= 190 && x <= 320 {
                        new_hover = 2;
                    }
                } else if y >= 240 && y <= 272
                    && x >= (width - 160) / 2 && x <= (width - 160) / 2 + 160
                    && PANEL_STATE.as_ref().is_some_and(|sa| {
                        let s = sa.lock().unwrap();
                        !s.devices.is_empty()
                            && prepare_devices(s.devices.clone(), s.device_filter, s.device_sort).is_empty()
                    })
                {
                    new_hover = 9;
                } else {
                    let (card_step, scroll_y, device_count) = if let Some(state_arc) = &PANEL_STATE {
                        let state = state_arc.lock().unwrap();
                        let step = match state.ui_density {
                            crate::config::UiDensity::Compact => 96,
                            crate::config::UiDensity::Spacious => 134,
                            crate::config::UiDensity::Standard => 114,
                        };
                        (step, state.scroll_y, state.devices.len())
                    } else { (114, 0, 0) };
                    for idx in 0..device_count {
                        let card_y = 74 + idx as i32 * card_step - scroll_y;
                        if y >= card_y + 10 && y <= card_y + 32 && x >= width - 210 && x <= width - 152 {
                            new_hover = 100 + idx as i32;
                            break;
                        }
                    }
                }
            } else {
                // Settings hover coordinates are translated by the scroll offset.
                let scroll_y = PANEL_STATE.as_ref().and_then(|state| state.lock().ok().map(|s| s.settings_scroll_y)).unwrap_or(0);
                let content_y = y + scroll_y;
                if content_y >= 115 && content_y <= 146 {
                    if x >= 20 && x <= 92 { new_hover = 30; }
                    else if x >= 98 && x <= 170 { new_hover = 31; }
                    else if x >= 176 && x <= 254 { new_hover = 32; }
                    else if x >= 260 && x <= 338 { new_hover = 33; }
                    else if x >= 344 && x <= 422 { new_hover = 34; }
                    else if x >= 428 && x <= 506 { new_hover = 35; }
                } else if content_y >= 195 && content_y <= 226 {
                    if x >= 20 && x <= 112 { new_hover = 40; }
                    else if x >= 120 && x <= 212 { new_hover = 41; }
                    else if x >= 220 && x <= 312 { new_hover = 42; }
                } else if content_y >= 275 && content_y <= 308 && x >= 20 && x <= 200 { new_hover = 50; }
                else if content_y >= 356 && content_y <= 388 {
                    if x >= 20 && x <= 180 { new_hover = 60; }
                    else if x >= 190 && x <= 380 { new_hover = 61; }
                    else if x >= 390 && x <= 600 { new_hover = 62; }
                } else if content_y >= 392 && content_y <= 424 {
                    if x >= 20 && x <= 125 { new_hover = 70; }
                    else if x >= 135 && x <= 240 { new_hover = 71; }
                    else if x >= 250 && x <= 355 { new_hover = 72; }
                    else if x >= 365 && x <= 505 { new_hover = 80; }
                    else if x >= 515 && x <= 600 { new_hover = 81; }
                } else if content_y >= 478 && content_y <= 504 {
                    if x >= 20 && x <= 110 { new_hover = 100; }
                    else if x >= 118 && x <= 208 { new_hover = 101; }
                    else if x >= 216 && x <= 306 { new_hover = 102; }
                } else if content_y >= 550 && content_y <= 576 {
                    if x >= 20 && x <= 92 { new_hover = 120; }
                    else if x >= 98 && x <= 182 { new_hover = 121; }
                    else if x >= 188 && x <= 272 { new_hover = 122; }
                    else if x >= 278 && x <= 350 { new_hover = 123; }
                    else if x >= 356 && x <= 428 { new_hover = 124; }
                    else if x >= 434 && x <= 516 { new_hover = 125; }
                } else if content_y >= 622 && content_y <= 648 {
                    if x >= 20 && x <= 110 { new_hover = 130; }
                    else if x >= 118 && x <= 208 { new_hover = 131; }
                    else if x >= 216 && x <= 306 { new_hover = 132; }
                    else if x >= 314 && x <= 404 { new_hover = 133; }
                } else if content_y >= 728 && content_y <= 754 {
                    if x >= 20 && x <= 210 { new_hover = 140; }
                    else if x >= 218 && x <= 314 { new_hover = 141; }
                    else if x >= 322 && x <= 536 && crate::update::has_update() { new_hover = 142; }
                } else if content_y >= 812 && content_y <= 840 && x >= 20 && x <= 220 {
                    new_hover = 150;
                } else if content_y >= 894 && content_y <= 922 && x >= 20 && x <= 200 {
                    new_hover = 151;
                } else if y >= height - 48 && y <= height - 14 && x >= 20 && x <= 180 { new_hover = 90; }
            }

            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                if state.hover_btn != new_hover {
                    state.hover_btn = new_hover;
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let (host, active_tab, show_onboarding, device_count) = if let Some(state_arc) = &PANEL_STATE {
                let s = state_arc.lock().unwrap();
                (s.host_hwnd, s.active_tab, s.show_onboarding, s.devices.len())
            } else {
                (ptr::null_mut(), 0, false, 0)
            };

            // 1. Top Header Tabs
            if y >= 14 && y <= 48 {
                if x >= width - 215 && x <= width - 123 {
                    if let Some(state_arc) = &PANEL_STATE {
                        let mut s = state_arc.lock().unwrap();
                        s.active_tab = 0;
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    return 0;
                } else if x >= width - 110 && x <= width - 18 {
                    if let Some(state_arc) = &PANEL_STATE {
                        let mut s = state_arc.lock().unwrap();
                        s.active_tab = 1;
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    return 0;
                }
            }

            if active_tab == 0 {
                let filtered_empty = PANEL_STATE.as_ref().is_some_and(|sa| {
                    let s = sa.lock().unwrap();
                    !s.devices.is_empty()
                        && prepare_devices(s.devices.clone(), s.device_filter, s.device_sort).is_empty()
                });
                if filtered_empty
                    && y >= 240 && y <= 272
                    && x >= (width - 160) / 2 && x <= (width - 160) / 2 + 160
                {
                    let sort = PANEL_STATE.as_ref()
                        .and_then(|sa| sa.lock().ok().map(|s| s.device_sort))
                        .unwrap_or(crate::config::DeviceSort::Priority);
                    set_dashboard_preferences(hwnd, host, crate::config::DeviceFilter::All, sort);
                } else if show_onboarding && device_count == 0 && y >= 228 && y <= 266 && x >= 40 && x <= 210 {
                    dismiss_onboarding(host);
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                } else if y >= height - 48 && y <= height - 14 {
                    if x >= 20 && x <= 180 {
                        if let Some(state_arc) = &PANEL_STATE {
                            state_arc.lock().unwrap().refresh_status = "正在刷新设备…".to_string();
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                        if !host.is_null() {
                            PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_REFRESH as _, 0);
                        }
                    } else if x >= 190 && x <= 320 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.active_tab = 1;
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                } else {
                    let mut clicked_card = None;
                    if let Some(state_arc) = &PANEL_STATE {
                        let state = state_arc.lock().unwrap();
                        let card_step = match state.ui_density {
                            crate::config::UiDensity::Compact => 96,
                            crate::config::UiDensity::Spacious => 134,
                            crate::config::UiDensity::Standard => 114,
                        };
                        for (idx, dev) in state.devices.iter().enumerate() {
                            let card_y = 74 + idx as i32 * card_step - state.scroll_y;
                            if y >= card_y + 10 && y <= card_y + 32 {
                                let copy_x = width - 274;
                                let edit_x = width - 210;
                                if x >= copy_x && x <= copy_x + 58 {
                                    let detail = format!("{}\n连接: {}\nVID/PID: {:04X}/{:04X}\n路径: {}", dev.name, dev.connection.as_str(), dev.vendor_id, dev.product_id, dev.path);
                                    let _ = copy_text_to_clipboard(hwnd, &detail);
                                    break;
                                } else if x >= edit_x && x <= edit_x + 58 {
                                    clicked_card = Some((dev.id.clone(), dev.name.clone()));
                                    break;
                                } else if x >= 20 && x <= width - 20 {
                                    if let Some(state_arc) = &PANEL_STATE {
                                        state_arc.lock().unwrap().selected_device_id = Some(dev.id.clone());
                                    }
                                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                                    break;
                                }
                            }
                        }
                    }
                    if let Some((dev_id, cur_name)) = clicked_card {
                        if let Some(new_name) = show_rename_dialog(hwnd, &cur_name) {
                            send_rename_to_host(host, &dev_id, &new_name);
                        }
                    }
                }
            } else {
                // Settings View Actions
                // All content y-coords rendered with -scroll_y; reverse here for hit-test.
                let scroll_y_hit = if let Some(sa) = &PANEL_STATE { sa.lock().unwrap().settings_scroll_y } else { 0 };
                let ey = y + scroll_y_hit;
                if ey >= 115 && ey <= 146 {
                    let cmd = if x >= 20 && x <= 92 {
                        crate::ui::tray::IDM_POLL_30S
                    } else if x >= 98 && x <= 170 {
                        crate::ui::tray::IDM_POLL_1M
                    } else if x >= 176 && x <= 254 {
                        crate::ui::tray::IDM_POLL_5M
                    } else if x >= 260 && x <= 338 {
                        crate::ui::tray::IDM_POLL_15M
                    } else if x >= 344 && x <= 422 {
                        crate::ui::tray::IDM_POLL_30M
                    } else if x >= 428 && x <= 506 {
                        crate::ui::tray::IDM_POLL_60M
                    } else {
                        0
                    };
                    if cmd > 0 && !host.is_null() {
                        PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, cmd as _, 0);
                    }
                } else if ey >= 195 && ey <= 226 {
                    let cmd = if x >= 20 && x <= 112 {
                        crate::ui::tray::IDM_FONT_SCALE_100
                    } else if x >= 120 && x <= 212 {
                        crate::ui::tray::IDM_FONT_SCALE_115
                    } else if x >= 220 && x <= 312 {
                        crate::ui::tray::IDM_FONT_SCALE_130
                    } else {
                        0
                    };
                    if cmd > 0 && !host.is_null() {
                        PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, cmd as _, 0);
                    }
                } else if ey >= 275 && ey <= 308 {
                    if x >= 20 && x <= 200 && !host.is_null() {
                        PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_TOGGLE_BALL as _, 0);
                    }
                } else if ey >= 328 && ey <= 362 {
                    let pct = if x >= 20 && x <= 90 { Some(100u8) }
                        else if x >= 96 && x <= 166 { Some(90) }
                        else if x >= 172 && x <= 242 { Some(80) }
                        else if x >= 248 && x <= 318 { Some(70) }
                        else if x >= 324 && x <= 394 { Some(60) }
                        else { None };
                    if let Some(pct) = pct {
                        set_ball_opacity(hwnd, host, pct);
                    }
                } else if ey >= 404 && ey <= 436 {
                    if x >= 20 && x <= 180 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.enable_toast = !s.enable_toast;
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 190 && x <= 380 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.toast_gaming_suppression = !s.toast_gaming_suppression;
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 390 && x <= 600 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.enable_low_battery_sound = !s.enable_low_battery_sound;
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                } else if ey >= 440 && ey <= 472 {
                    if x >= 20 && x <= 125 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.toast_info = match s.toast_info {
                                40 => 50,
                                50 => 60,
                                _ => 40,
                            };
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 135 && x <= 240 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.toast_warning = match s.toast_warning {
                                20 => 30,
                                30 => 35,
                                _ => 20,
                            };
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 250 && x <= 355 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.toast_critical = match s.toast_critical {
                                5 => 10,
                                10 => 15,
                                _ => 5,
                            };
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 365 && x <= 505 {
                        crate::ui::toast::ToastNotification::show(
                            "罗技 M720 Triathlon",
                            "电量已降至 28% (警告通知测试)",
                            crate::ui::toast::ToastLevel::Warning,
                            28,
                        );
                    } else if x >= 515 && x <= 600 {
                        crate::sound::play_low_battery_alarm();
                    }
                } else if ey >= 522 && ey <= 552 {
                    let density = if x >= 20 && x <= 110 { Some(crate::config::UiDensity::Compact) }
                        else if x >= 118 && x <= 208 { Some(crate::config::UiDensity::Standard) }
                        else if x >= 216 && x <= 306 { Some(crate::config::UiDensity::Spacious) }
                        else { None };
                    if let Some(density) = density { set_panel_density(hwnd, host, density); }
                } else if ey >= 594 && ey <= 624 {
                    let filter = if x >= 20 && x <= 92 { Some(crate::config::DeviceFilter::All) }
                        else if x >= 98 && x <= 182 { Some(crate::config::DeviceFilter::LowBattery) }
                        else if x >= 188 && x <= 272 { Some(crate::config::DeviceFilter::Charging) }
                        else if x >= 278 && x <= 350 { Some(crate::config::DeviceFilter::Bluetooth) }
                        else if x >= 356 && x <= 428 { Some(crate::config::DeviceFilter::Wireless) }
                        else { None };
                    if let Some(filter) = filter {
                        let sort = PANEL_STATE.as_ref().and_then(|state| state.lock().ok().map(|s| s.device_sort)).unwrap_or(crate::config::DeviceSort::Priority);
                        set_dashboard_preferences(hwnd, host, filter, sort);
                    }
                } else if ey >= 630 && ey <= 660 {
                    let sort = if x >= 20 && x <= 102 { Some(crate::config::DeviceSort::Priority) }
                        else if x >= 110 && x <= 192 { Some(crate::config::DeviceSort::Name) }
                        else if x >= 200 && x <= 282 { Some(crate::config::DeviceSort::Battery) }
                        else if x >= 290 && x <= 372 { Some(crate::config::DeviceSort::Updated) }
                        else { None };
                    if let Some(sort) = sort {
                        let filter = PANEL_STATE.as_ref().and_then(|state| state.lock().ok().map(|s| s.device_filter)).unwrap_or(crate::config::DeviceFilter::All);
                        set_dashboard_preferences(hwnd, host, filter, sort);
                    }
                } else if ey >= 684 && ey <= 714 {
                    let theme = if x >= 20 && x <= 110 { Some(crate::config::AccentTheme::Orange) }
                        else if x >= 118 && x <= 208 { Some(crate::config::AccentTheme::Blue) }
                        else if x >= 216 && x <= 306 { Some(crate::config::AccentTheme::Purple) }
                        else if x >= 314 && x <= 404 { Some(crate::config::AccentTheme::Green) }
                        else { None };
                    if let Some(theme) = theme { set_accent_theme(hwnd, host, theme); }
                } else if ey >= 776 && ey <= 802 {
                    if x >= 20 && x <= 210 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut st = state_arc.lock().unwrap();
                            st.auto_update = !st.auto_update;
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_UPDATE_PREFS, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    } else if x >= 218 && x <= 314 {
                        if !host.is_null() {
                            PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_CHECK_UPDATE as _, 0);
                        }
                    } else if x >= 322 && x <= 536 && crate::update::has_update() {
                        if !host.is_null() {
                            PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_APPLY_UPDATE as _, 0);
                        }
                    }
                } else if ey >= 860 && ey <= 888 {
                    if x >= 20 && x <= 220 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.auto_start = !s.auto_start;
                        }
                        if !host.is_null() {
                            PostMessageW(host, WM_SAVE_AUTOSTART, 0, 0);
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                } else if ey >= 942 && ey <= 970 {
                    if x >= 20 && x <= 200 {
                        open_config_dir();
                    }
                } else if y >= height - 48 && y <= height - 14 {
                    if x >= 20 && x <= 180 {
                        if let Some(state_arc) = &PANEL_STATE {
                            let mut s = state_arc.lock().unwrap();
                            s.active_tab = 0;
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn update_status_line(state: &crate::update::UpdateState) -> String {
    let current = crate::update::current_version();
    if state.applying {
        return "正在下载并替换程序，完成后会自动重启…".to_string();
    }
    if state.checking {
        return "正在检查 GitHub Releases…".to_string();
    }
    if let Some(err) = &state.error {
        return format!("检查失败: {}（当前 v{}）", err, current);
    }
    match state.latest.as_deref() {
        Some(latest) if crate::update::is_newer(latest, current) => format!(
            "发现新版本 v{}（当前 v{}）· 点击右侧按钮自动替换并重启",
            latest, current
        ),
        Some(_) => format!("已是最新版本（v{}）", current),
        None => format!("当前版本 v{} · 可手动检查更新", current),
    }
}

unsafe fn render_settings_view(
    mem_dc: HDC,
    width: i32,
    height: i32,
    font_scale: f32,
    poll_ms: u64,
    ball_on: bool,
    ball_opacity: u8,
    enable_toast: bool,
    toast_info: u8,
    toast_warn: u8,
    toast_crit: u8,
    toast_gaming: bool,
    toast_sound: bool,
    ui_density: crate::config::UiDensity,
    device_filter: crate::config::DeviceFilter,
    device_sort: crate::config::DeviceSort,
    accent_theme: crate::config::AccentTheme,
    scroll_y: i32,
    hover_btn: i32,
    auto_update: bool,
    auto_start: bool,
) {
    let sy = scroll_y;
    let f_title = create_clear_font((14.0 * font_scale).round() as i32, true);
    let old_f = SelectObject(mem_dc, f_title);
    SetTextColor(mem_dc, CLR_TEXT);
    let mut s1_t = RECT { left: 20, top: 76 - sy, right: width - 20, bottom: 96 - sy };
    draw_text_clean(mem_dc, "1. 定时通信间隔 (后台巡检轮询)", &mut s1_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    let f_desc = create_clear_font((12.0 * font_scale).round() as i32, false);
    SelectObject(mem_dc, f_desc);
    SetTextColor(mem_dc, CLR_MUTED);
    let mut s1_d = RECT { left: 20, top: 96 - sy, right: width - 20, bottom: 114 - sy };
    draw_text_clean(mem_dc, "外设电量消耗慢，推荐长周期以极致省电；随时点击'立即刷新'可秒级直读当前电量。", &mut s1_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    render_choice_pill(mem_dc, 20,  118 - sy, 72, 26, "30s 调试",  poll_ms <= 45_000,                           hover_btn == 30, font_scale);
    render_choice_pill(mem_dc, 98,  118 - sy, 72, 26, "1m 快速",   poll_ms > 45_000 && poll_ms <= 120_000,      hover_btn == 31, font_scale);
    render_choice_pill(mem_dc, 176, 118 - sy, 78, 26, "5m 推荐",   poll_ms > 120_000 && poll_ms <= 600_000,     hover_btn == 32, font_scale);
    render_choice_pill(mem_dc, 260, 118 - sy, 78, 26, "15m 适中",  poll_ms > 600_000 && poll_ms <= 1_200_000,   hover_btn == 33, font_scale);
    render_choice_pill(mem_dc, 344, 118 - sy, 78, 26, "30m 省电",  poll_ms > 1_200_000 && poll_ms <= 2_700_000, hover_btn == 34, font_scale);
    render_choice_pill(mem_dc, 428, 118 - sy, 78, 26, "1h 极省",   poll_ms > 2_700_000,                         hover_btn == 35, font_scale);

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut s2_t = RECT { left: 20, top: 156 - sy, right: width - 20, bottom: 176 - sy };
    draw_text_clean(mem_dc, "2. 界面显示字号 (ClearType 次像素渲染)", &mut s2_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut s2_d = RECT { left: 20, top: 176 - sy, right: width - 20, bottom: 194 - sy };
    draw_text_clean(mem_dc, "Windows 微软雅黑高分屏抗锯齿次像素渲染字体缩放比例：", &mut s2_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    let is_s100 = font_scale <= 1.05;
    let is_s115 = font_scale > 1.05 && font_scale <= 1.22;
    let is_s130 = font_scale > 1.22;
    render_choice_pill(mem_dc, 20,  198 - sy, 92, 26, "100% 标准", is_s100, hover_btn == 40, font_scale);
    render_choice_pill(mem_dc, 120, 198 - sy, 92, 26, "115% 适中", is_s115, hover_btn == 41, font_scale);
    render_choice_pill(mem_dc, 220, 198 - sy, 92, 26, "130% 放大", is_s130, hover_btn == 42, font_scale);

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut s3_t = RECT { left: 20, top: 236 - sy, right: width - 20, bottom: 256 - sy };
    draw_text_clean(mem_dc, "3. 桌面贴边悬浮球", &mut s3_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut s3_d = RECT { left: 20, top: 256 - sy, right: width - 20, bottom: 274 - sy };
    draw_text_clean(mem_dc, "吸附于屏幕边缘的半环能量仪表，悬停还原完整圆球，点击展开抽屉：", &mut s3_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    let ball_text = if ball_on { "🔘 贴边悬浮球: [已开启]" } else { "⚪ 贴边悬浮球: [已关闭]" };
    render_choice_pill(mem_dc, 20, 278 - sy, 180, 28, ball_text, ball_on, hover_btn == 50, font_scale);

    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut s3_o = RECT { left: 20, top: 310 - sy, right: width - 20, bottom: 328 - sy };
    draw_text_clean(
        mem_dc,
        &format!("悬浮球不透明度 (越低越通透，抽屉保持可读)：当前 {}%", ball_opacity),
        &mut s3_o,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    for (i, pct) in [100u8, 90, 80, 70, 60].iter().enumerate() {
        render_choice_pill(
            mem_dc, 20 + (i as i32) * 76, 332 - sy, 70, 26,
            &format!("{}%", pct), ball_opacity == *pct, hover_btn == 51 + i as i32, font_scale,
        );
    }

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut s4_t = RECT { left: 20, top: 366 - sy, right: width - 20, bottom: 386 - sy };
    draw_text_clean(mem_dc, "4. 低电量原生 Toast 弹窗通知 (多级预警)", &mut s4_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut s4_d = RECT { left: 20, top: 386 - sy, right: width - 20, bottom: 404 - sy };
    draw_text_clean(mem_dc, "跌破阈值时右下角弹出 Toast；危险阈值同步播放提示音；全屏或游戏时降级/免打扰：", &mut s4_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    let toast_text = if enable_toast { "🔔 电量弹窗: [已开启]" } else { "🔕 电量弹窗: [已关闭]" };
    render_choice_pill(mem_dc, 20,  408 - sy, 160, 26, toast_text, enable_toast, hover_btn == 60, font_scale);
    let game_text = if toast_gaming { "🎮 游戏全屏免打扰: [已开启]" } else { "🎮 游戏全屏免打扰: [已关闭]" };
    render_choice_pill(mem_dc, 190, 408 - sy, 190, 26, game_text, toast_gaming,  hover_btn == 61, font_scale);
    let sound_text = if toast_sound { "🔊 傻妞饿死了提示音: [已开启]" } else { "🔇 傻妞饿死了提示音: [已关闭]" };
    render_choice_pill(mem_dc, 390, 408 - sy, 210, 26, sound_text, toast_sound, hover_btn == 62, font_scale);
    let info_str = format!("提示: < {}%", toast_info);
    let warn_str = format!("警告: < {}%", toast_warn);
    let crit_str = format!("危险: < {}%", toast_crit);
    render_choice_pill(mem_dc, 20,  444 - sy, 105, 26, &info_str,          true, hover_btn == 70, font_scale);
    render_choice_pill(mem_dc, 135, 444 - sy, 105, 26, &warn_str,          true, hover_btn == 71, font_scale);
    render_choice_pill(mem_dc, 250, 444 - sy, 105, 26, &crit_str,          true, hover_btn == 72, font_scale);
    render_choice_pill(mem_dc, 365, 444 - sy, 140, 26, "🚀 试发一条Toast", false, hover_btn == 80, font_scale);
    render_choice_pill(mem_dc, 515, 444 - sy, 85, 26,  "🎵 试听",         false, hover_btn == 81, font_scale);

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut density_title = RECT { left: 20, top: 486 - sy, right: width - 20, bottom: 504 - sy };
    draw_text_clean(mem_dc, "5. 卡片布局密度", &mut density_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut density_desc = RECT { left: 20, top: 504 - sy, right: width - 20, bottom: 522 - sy };
    draw_text_clean(mem_dc, "紧凑适合小窗口；宽松提供更舒适的信息间距。", &mut density_desc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    render_choice_pill(mem_dc, 20,  526 - sy, 90, 26, "紧凑布局", matches!(ui_density, crate::config::UiDensity::Compact),  hover_btn == 100, font_scale);
    render_choice_pill(mem_dc, 118, 526 - sy, 90, 26, "标准布局", matches!(ui_density, crate::config::UiDensity::Standard), hover_btn == 101, font_scale);
    render_choice_pill(mem_dc, 216, 526 - sy, 90, 26, "宽松布局", matches!(ui_density, crate::config::UiDensity::Spacious), hover_btn == 102, font_scale);

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut dashboard_title = RECT { left: 20, top: 558 - sy, right: width - 20, bottom: 576 - sy };
    draw_text_clean(mem_dc, "6. 设备列表", &mut dashboard_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut dashboard_desc = RECT { left: 20, top: 576 - sy, right: width - 20, bottom: 594 - sy };
    draw_text_clean(mem_dc, "筛选和排序仅影响显示顺序，不会改变后台扫描。", &mut dashboard_desc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    render_choice_pill(mem_dc, 20,  598 - sy, 72, 26, "全部",   matches!(device_filter, crate::config::DeviceFilter::All),        hover_btn == 120, font_scale);
    render_choice_pill(mem_dc, 98,  598 - sy, 84, 26, "低电量", matches!(device_filter, crate::config::DeviceFilter::LowBattery), hover_btn == 121, font_scale);
    render_choice_pill(mem_dc, 188, 598 - sy, 84, 26, "充电中", matches!(device_filter, crate::config::DeviceFilter::Charging),   hover_btn == 122, font_scale);
    render_choice_pill(mem_dc, 278, 598 - sy, 72, 26, "蓝牙",   matches!(device_filter, crate::config::DeviceFilter::Bluetooth),  hover_btn == 123, font_scale);
    render_choice_pill(mem_dc, 356, 598 - sy, 72, 26, "2.4G",   matches!(device_filter, crate::config::DeviceFilter::Wireless),   hover_btn == 124, font_scale);
    render_choice_pill(mem_dc, 20,  634 - sy, 82, 26, "优先级", matches!(device_sort, crate::config::DeviceSort::Priority),       hover_btn == 125, font_scale);
    render_choice_pill(mem_dc, 110, 634 - sy, 82, 26, "名称",   matches!(device_sort, crate::config::DeviceSort::Name),           hover_btn == 126, font_scale);
    render_choice_pill(mem_dc, 200, 634 - sy, 82, 26, "电量",   matches!(device_sort, crate::config::DeviceSort::Battery),        hover_btn == 127, font_scale);
    render_choice_pill(mem_dc, 290, 634 - sy, 82, 26, "更新时间", matches!(device_sort, crate::config::DeviceSort::Updated),      hover_btn == 128, font_scale);

    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut theme_title = RECT { left: 20, top: 666 - sy, right: width - 20, bottom: 684 - sy };
    draw_text_clean(mem_dc, "7. 主题强调色", &mut theme_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    render_choice_pill(mem_dc, 20,  688 - sy, 90, 26, "橙色", matches!(accent_theme, crate::config::AccentTheme::Orange), hover_btn == 130, font_scale);
    render_choice_pill(mem_dc, 118, 688 - sy, 90, 26, "蓝色", matches!(accent_theme, crate::config::AccentTheme::Blue),   hover_btn == 131, font_scale);
    render_choice_pill(mem_dc, 216, 688 - sy, 90, 26, "紫色", matches!(accent_theme, crate::config::AccentTheme::Purple), hover_btn == 132, font_scale);
    render_choice_pill(mem_dc, 314, 688 - sy, 90, 26, "绿色", matches!(accent_theme, crate::config::AccentTheme::Green),  hover_btn == 133, font_scale);

    // 8. 版本与更新
    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut upd_title = RECT { left: 20, top: 734 - sy, right: width - 20, bottom: 752 - sy };
    draw_text_clean(mem_dc, "8. 版本与更新 (单一 exe 自替换)", &mut upd_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc);
    let upd = crate::update::snapshot();
    SetTextColor(mem_dc, if upd.error.is_some() { CLR_RED } else { CLR_MUTED });
    let upd_status = update_status_line(&upd);
    let mut upd_desc = RECT { left: 20, top: 752 - sy, right: width - 20, bottom: 770 - sy };
    draw_text_clean(mem_dc, &upd_status, &mut upd_desc, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    render_choice_pill(
        mem_dc, 20, 776 - sy, 190, 26,
        if auto_update { "自动检查更新: 开" } else { "自动检查更新: 关" },
        auto_update, hover_btn == 140, font_scale,
    );
    render_choice_pill(
        mem_dc, 218, 776 - sy, 96, 26,
        if upd.checking { "检查中…" } else { "检查更新" },
        false, hover_btn == 141, font_scale,
    );
    if let Some(latest) = upd.latest.as_deref().filter(|v| crate::update::is_newer(v, crate::update::current_version())) {
        render_choice_pill(
            mem_dc, 322, 776 - sy, 214, 26,
            &format!("立即升级到 v{}", latest),
            true, hover_btn == 142, font_scale,
        );
    }

    // 9. 开机自启
    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut startup_title = RECT { left: 20, top: 818 - sy, right: width - 20, bottom: 836 - sy };
    draw_text_clean(mem_dc, "9. 开机自动启动 (Windows 登录时)", &mut startup_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let mut startup_desc = RECT { left: 20, top: 836 - sy, right: width - 20, bottom: 854 - sy };
    draw_text_clean(mem_dc, "写入当前用户注册表 Run 项，登录后静默常驻托盘，不弹出主界面：", &mut startup_desc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    let startup_text = if auto_start { "🚀 开机自启: [已开启]" } else { "⚪ 开机自启: [已关闭]" };
    render_choice_pill(mem_dc, 20, 860 - sy, 200, 28, startup_text, auto_start, hover_btn == 150, font_scale);

    // 10. 配置文件目录
    SelectObject(mem_dc, f_title); SetTextColor(mem_dc, CLR_TEXT);
    let mut config_title = RECT { left: 20, top: 900 - sy, right: width - 20, bottom: 918 - sy };
    draw_text_clean(mem_dc, "10. 配置文件目录", &mut config_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(mem_dc, f_desc); SetTextColor(mem_dc, CLR_MUTED);
    let config_path = crate::config::AppConfig::config_path();
    let mut config_desc = RECT { left: 20, top: 918 - sy, right: width - 20, bottom: 936 - sy };
    draw_text_clean(mem_dc, &config_path.display().to_string(), &mut config_desc, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    render_choice_pill(mem_dc, 20, 942 - sy, 180, 28, "📁 打开配置目录", false, hover_btn == 151, font_scale);

    SelectObject(mem_dc, old_f);
    DeleteObject(f_title);
    DeleteObject(f_desc);
}




static mut DLG_RESULT_STRING: Option<String> = None;
static mut DLG_EDIT_CONTROL: HWND = ptr::null_mut();
static mut DLG_HOVER_BTN: i32 = 0;

unsafe extern "system" fn modern_rename_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            let hdc = wparam as HDC;
            SetBkColor(hdc, 0x001A1715);
            SetTextColor(hdc, CLR_TEXT);
            static mut BRUSH: Option<windows_sys::Win32::Graphics::Gdi::HBRUSH> = None;
            if BRUSH.is_none() {
                BRUSH = Some(CreateSolidBrush(0x001A1715));
            }
            return BRUSH.unwrap() as LRESULT;
        }
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

            // Dark background #181B20
            let bg_brush = CreateSolidBrush(CLR_CARD);
            FillRect(mem_dc, &rect, bg_brush);
            DeleteObject(bg_brush);

            // Subtle border
            let pen = CreatePen(PS_SOLID, 1, CLR_CARD_BORDER);
            let op = SelectObject(mem_dc, pen);
            let ob = SelectObject(mem_dc, windows_sys::Win32::Graphics::Gdi::GetStockObject(5 /* NULL_BRUSH */));
            RoundRect(mem_dc, 0, 0, width, height, 12, 12);
            SelectObject(mem_dc, ob);
            SelectObject(mem_dc, op);
            DeleteObject(pen);

            SetBkMode(mem_dc, TRANSPARENT as i32);

            // Title: "✏️ 自定义外设名称"
            let font_title = create_clear_font(15, true);
            let old_f = SelectObject(mem_dc, font_title);
            SetTextColor(mem_dc, CLR_TEXT);
            let mut t_r = RECT { left: 22, top: 18, right: width - 22, bottom: 42 };
            draw_text_clean(mem_dc, "✏️ 自定义外设显示名称", &mut t_r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Hint
            let font_hint = create_clear_font(12, false);
            SelectObject(mem_dc, font_hint);
            SetTextColor(mem_dc, CLR_MUTED);
            let mut h_r = RECT { left: 22, top: 44, right: width - 22, bottom: 64 };
            draw_text_clean(mem_dc, "修改后将在悬浮球、控制中心及托盘同步更新 (留空重置出厂名):", &mut h_r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Draw input box outer border
            let input_pen = CreatePen(PS_SOLID, 1, CLR_CARD_BORDER);
            let ip = SelectObject(mem_dc, input_pen);
            let ib = SelectObject(mem_dc, windows_sys::Win32::Graphics::Gdi::GetStockObject(5));
            RoundRect(mem_dc, 20, 70, width - 20, 104, 6, 6);
            SelectObject(mem_dc, ib);
            SelectObject(mem_dc, ip);
            DeleteObject(input_pen);

            // OK Button (width - 190 .. width - 110, y: 120 .. 152)
            let ok_hover = DLG_HOVER_BTN == 1;
            let ok_bg = if ok_hover { CLR_ACCENT } else { 0x00C06818 };
            let ok_b = CreateSolidBrush(ok_bg);
            let ok_p = CreatePen(PS_SOLID, 1, if ok_hover { 0x00FFB060 } else { CLR_ACCENT });
            let o1 = SelectObject(mem_dc, ok_p);
            let o2 = SelectObject(mem_dc, ok_b);
            RoundRect(mem_dc, width - 190, 120, width - 105, 152, 6, 6);
            SelectObject(mem_dc, o2);
            DeleteObject(ok_b);
            SelectObject(mem_dc, o1);
            DeleteObject(ok_p);

            let font_btn = create_clear_font(13, true);
            SelectObject(mem_dc, font_btn);
            SetTextColor(mem_dc, CLR_TEXT);
            let mut b1_r = RECT { left: width - 190, top: 120, right: width - 105, bottom: 152 };
            draw_text_clean(mem_dc, "确定保存", &mut b1_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // Cancel Button (width - 95 .. width - 20, y: 120 .. 152)
            let can_hover = DLG_HOVER_BTN == 2;
            let can_bg = if can_hover { CLR_BTN_HOVER } else { CLR_BTN_BG };
            let can_b = CreateSolidBrush(can_bg);
            let can_p = CreatePen(PS_SOLID, 1, if can_hover { CLR_MUTED } else { CLR_CARD_BORDER });
            let c1 = SelectObject(mem_dc, can_p);
            let c2 = SelectObject(mem_dc, can_b);
            RoundRect(mem_dc, width - 95, 120, width - 20, 152, 6, 6);
            SelectObject(mem_dc, c2);
            DeleteObject(can_b);
            SelectObject(mem_dc, c1);
            DeleteObject(can_p);

            SetTextColor(mem_dc, if can_hover { CLR_TEXT } else { CLR_MUTED });
            let mut b2_r = RECT { left: width - 95, top: 120, right: width - 20, bottom: 152 };
            draw_text_clean(mem_dc, "取消", &mut b2_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_f);
            DeleteObject(font_title);
            DeleteObject(font_hint);
            DeleteObject(font_btn);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);

            EndPaint(hwnd, &mut ps);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;

            let mut new_h = 0;
            if y >= 120 && y <= 152 {
                if x >= width - 190 && x <= width - 105 {
                    new_h = 1;
                } else if x >= width - 95 && x <= width - 20 {
                    new_h = 2;
                }
            }
            if DLG_HOVER_BTN != new_h {
                DLG_HOVER_BTN = new_h;
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;

            if y >= 120 && y <= 152 {
                if x >= width - 190 && x <= width - 105 {
                    // OK
                    let mut buf = [0u16; 256];
                    let len = GetWindowTextW(DLG_EDIT_CONTROL, buf.as_mut_ptr(), buf.len() as i32);
                    let res = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
                    DLG_RESULT_STRING = Some(res);
                    DestroyWindow(hwnd);
                    return 0;
                } else if x >= width - 95 && x <= width - 20 {
                    // Cancel
                    DLG_RESULT_STRING = None;
                    DestroyWindow(hwnd);
                    return 0;
                }
            }
            0
        }
        WM_CLOSE => {
            DLG_RESULT_STRING = None;
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub unsafe fn show_rename_dialog(parent_hwnd: HWND, current_name: &str) -> Option<String> {
    DLG_RESULT_STRING = None;
    DLG_HOVER_BTN = 0;

    let instance = GetModuleHandleW(ptr::null());
    let class_name: Vec<u16> = "LinkDeck_ModernRenameDlg\0".encode_utf16().collect();

    let mut wcx: WNDCLASSEXW = mem::zeroed();
    wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
    wcx.style = CS_HREDRAW | CS_VREDRAW;
    wcx.lpfnWndProc = Some(modern_rename_wnd_proc);
    wcx.hInstance = instance;
    wcx.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
    wcx.lpszClassName = class_name.as_ptr();
    RegisterClassExW(&wcx);

    let dlg_w = 420;
    let dlg_h = 175;

    let mut parent_rect: RECT = mem::zeroed();
    GetClientRect(parent_hwnd, &mut parent_rect);
    let mut pt = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    windows_sys::Win32::Graphics::Gdi::ClientToScreen(parent_hwnd, &mut pt);

    let pos_x = pt.x + (parent_rect.right - dlg_w) / 2;
    let pos_y = pt.y + (parent_rect.bottom - dlg_h) / 2;

    let dlg_title: Vec<u16> = "自定义外设名称\0".encode_utf16().collect();
    let dlg_hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        dlg_title.as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        pos_x,
        pos_y,
        dlg_w,
        dlg_h,
        parent_hwnd,
        ptr::null_mut(),
        instance,
        ptr::null(),
    );

    if dlg_hwnd.is_null() {
        return None;
    }

    let edit_cls: Vec<u16> = "EDIT\0".encode_utf16().collect();
    let init_txt: Vec<u16> = format!("{}\0", current_name).encode_utf16().collect();

    let font_edit = create_clear_font(15, false);

    let edit_hwnd = CreateWindowExW(
        0,
        edit_cls.as_ptr(),
        init_txt.as_ptr(),
        WS_CHILD | WS_VISIBLE | (ES_AUTOHSCROLL as u32),
        26,
        74,
        dlg_w - 52,
        24,
        dlg_hwnd,
        ptr::null_mut(),
        instance,
        ptr::null(),
    );

    DLG_EDIT_CONTROL = edit_hwnd;
    SendMessageW(edit_hwnd, WM_SETFONT, font_edit as _, 1);
    // Explicitly set text
    SetWindowTextW(edit_hwnd, init_txt.as_ptr());
    // Place cursor at the end so existing name is intact and immediately editable
    let text_len = current_name.chars().count() as isize;
    SendMessageW(edit_hwnd, 0x00B1 /* EM_SETSEL */, text_len as WPARAM, text_len as LPARAM);
    SetFocus(edit_hwnd);

    EnableWindow(parent_hwnd, 0);

    let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = mem::zeroed();
    while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
        if msg.message == WM_KEYDOWN && msg.wParam == 13 {
            // Enter key -> OK
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(DLG_EDIT_CONTROL, buf.as_mut_ptr(), buf.len() as i32);
            let res = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
            DLG_RESULT_STRING = Some(res);
            DestroyWindow(dlg_hwnd);
            break;
        } else if msg.message == WM_KEYDOWN && msg.wParam == 27 {
            // Esc key -> Cancel
            DLG_RESULT_STRING = None;
            DestroyWindow(dlg_hwnd);
            break;
        }

        if IsDialogMessageW(dlg_hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(dlg_hwnd) == 0 {
            break;
        }
    }

    EnableWindow(parent_hwnd, 1);
    SetForegroundWindow(parent_hwnd);
    DeleteObject(font_edit);

    DLG_RESULT_STRING.take()
}

pub unsafe fn send_rename_to_host(host_hwnd: HWND, device_id: &str, new_alias: &str) {
    let payload = Box::into_raw(Box::new((device_id.to_string(), new_alias.to_string())));
    PostMessageW(host_hwnd, WM_DEVICE_RENAME, payload as WPARAM, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_message_constant() {
        assert_eq!(WM_DEVICE_RENAME, 0x8000 + 101);
    }

}
