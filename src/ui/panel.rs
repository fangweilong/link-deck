#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::{ConnectionMode, DeviceSnapshot};
use crate::ui::clipboard::copy_text_to_clipboard;
use crate::ui::panel_render::{render_action_btn, render_choice_pill, render_device_card};
use crate::ui::settings;
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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowRect, GetWindowTextW, IsDialogMessageW, LoadCursorW,
    PostMessageW, RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL, ICON_BIG, ICON_SMALL, IDC_ARROW,
    IsIconic, MINMAXINFO, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_RESTORE, SW_SHOW,
    WM_CLOSE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT,
    WM_SETICON,
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
/// 拖拽结束才落盘，避免拖拽过程中逐像素写文件。
pub const WM_SAVE_BALL_OPACITY: u32 = 0x8000 + 113;
/// wParam: 0 = 贴边拉手，1 = 完整圆球。
pub const WM_SET_BALL_STYLE: u32 = 0x8000 + 114;
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
    pub ball_style: crate::config::FloatingBallStyle,
    pub ui_density: crate::config::UiDensity,
    pub scroll_y: i32,
    pub settings_scroll_y: i32,
    pub slider_drag: bool,
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
                ball_style: config.floating_ball_style,
                ui_density: config.ui_density,
                scroll_y: 0,
                settings_scroll_y: 0,
                slider_drag: false,
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
        ball_style: crate::config::FloatingBallStyle,
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
                state.ball_style = ball_style;
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

/// 按鼠标横坐标更新透明度：面板立即重绘，主程序同步应用到悬浮球（不落盘）。
unsafe fn update_ball_opacity_from_x(hwnd: HWND, x: i32) {
    let mut rect: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;
    if let Some(state_arc) = &PANEL_STATE {
        let (pct, host, changed) = {
            let mut st = state_arc.lock().unwrap();
            let page = settings::layout(&settings_ctx(&st), width);
            let pct = settings::slider_value(&page.slider, x);
            let changed = pct != st.ball_opacity;
            st.ball_opacity = pct;
            (pct, st.host_hwnd, changed)
        };
        if changed && !host.is_null() {
            PostMessageW(host, WM_SET_BALL_OPACITY, pct as WPARAM, 0);
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
    }
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
/// 自绘滚动条：轨道 + 滑块，设备页与设置页共用同一套几何。
unsafe fn draw_scroll_thumb(
    hdc: HDC,
    width: i32,
    top: i32,
    bottom: i32,
    content_h: i32,
    scroll_y: i32,
) {
    let viewport_h = (bottom - top).max(1);
    if content_h <= viewport_h {
        return;
    }
    let track_x = width - 10;
    let track_brush = CreateSolidBrush(CLR_CARD_BORDER);
    let track = RECT { left: track_x, top, right: track_x + 4, bottom };
    FillRect(hdc, &track, track_brush);
    DeleteObject(track_brush);

    let thumb_h = ((viewport_h * viewport_h) / content_h).max(24);
    let max_thumb_y = viewport_h - thumb_h;
    let max_scroll = (content_h - viewport_h).max(1);
    let thumb_y = top + ((scroll_y * max_thumb_y) / max_scroll);
    let thumb_brush = CreateSolidBrush(CLR_ACCENT);
    let thumb = RECT { left: track_x - 1, top: thumb_y, right: track_x + 5, bottom: thumb_y + thumb_h };
    FillRect(hdc, &thumb, thumb_brush);
    DeleteObject(thumb_brush);
}

unsafe fn settings_max_scroll(hwnd: HWND, state: &PanelStateData) -> i32 {
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let width = client.right - client.left;
    let viewport = (client.bottom - client.top - 62 - settings::CONTENT_TOP).max(1);
    let content = settings::layout(&settings_ctx(state), width).content_h;
    (content - viewport).max(0)
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
                    settings_max_scroll(hwnd, &state)
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
                    let max_scroll = settings_max_scroll(hwnd, &state);
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
                draw_scroll_thumb(mem_dc, width, 70, height - 60, content_h, scroll_y);
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
                let ctx = PANEL_STATE
                    .as_ref()
                    .map(|sa| settings_ctx(&sa.lock().unwrap()))
                    .unwrap_or_else(fallback_settings_ctx);
                let page = settings::layout(&ctx, width);
                settings::paint_nav(
                    mem_dc,
                    &page,
                    settings_scroll_y,
                    hover_btn,
                    font_scale,
                    (height - 62 - settings::CONTENT_TOP).max(1),
                );
                // Cards scroll under a fixed clip; the nav strip above stays put.
                let settings_clip = CreateRectRgn(
                    0,
                    settings::CONTENT_TOP,
                    width,
                    (height - 62).max(settings::CONTENT_TOP + 1),
                );
                SelectClipRgn(mem_dc, settings_clip);
                settings::paint_cards(mem_dc, &page, &ctx, settings_scroll_y, hover_btn, settings::CONTENT_TOP);
                SelectClipRgn(mem_dc, ptr::null_mut());
                DeleteObject(settings_clip);
                draw_scroll_thumb(
                    mem_dc,
                    width,
                    settings::CONTENT_TOP,
                    height - 62,
                    page.content_h,
                    settings_scroll_y,
                );
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

            let dragging_slider = PANEL_STATE
                .as_ref()
                .map(|sa| sa.lock().unwrap().slider_drag)
                .unwrap_or(false);
            if dragging_slider {
                update_ball_opacity_from_x(hwnd, x);
                return 0;
            }

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
                // 设置页：导航条固定，卡片内容随滚动偏移。
                let (scroll_y, ctx) = PANEL_STATE.as_ref()
                    .map(|sa| { let st = sa.lock().unwrap(); (st.settings_scroll_y, settings_ctx(&st)) })
                    .unwrap_or((0, fallback_settings_ctx()));
                let page = settings::layout(&ctx, width);
                if let Some(group) = settings::nav_hit(&page, x, y) {
                    new_hover = 160 + group as i32;
                } else if let Some(id) = settings::hit_test(&page, x, y - settings::CONTENT_TOP + scroll_y) {
                    new_hover = id;
                } else if y >= height - 48 && y <= height - 14 && x >= 20 && x <= 180 {
                    new_hover = 90;
                }
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
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;

            let on_slider = PANEL_STATE.as_ref().is_some_and(|sa| {
                let st = sa.lock().unwrap();
                if st.active_tab != 1 {
                    return false;
                }
                let page = settings::layout(&settings_ctx(&st), width);
                settings::hit_test(&page, x, y - settings::CONTENT_TOP + st.settings_scroll_y) == Some(settings::SLIDER_ID)
            });
            if on_slider {
                SetCapture(hwnd);
                if let Some(sa) = &PANEL_STATE {
                    sa.lock().unwrap().slider_drag = true;
                }
                update_ball_opacity_from_x(hwnd, x);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let slider_released = PANEL_STATE
                .as_ref()
                .map(|sa| {
                    let mut st = sa.lock().unwrap();
                    let was = st.slider_drag;
                    st.slider_drag = false;
                    was
                })
                .unwrap_or(false);
            if slider_released {
                ReleaseCapture();
                if let Some(sa) = &PANEL_STATE {
                    let host = sa.lock().unwrap().host_hwnd;
                    if !host.is_null() {
                        PostMessageW(host, WM_SAVE_BALL_OPACITY, 0, 0);
                    }
                }
                return 0;
            }

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
                // 设置页：导航条跳转 + 卡片内控件派发。
                let (scroll_y, ctx) = PANEL_STATE.as_ref()
                    .map(|sa| { let st = sa.lock().unwrap(); (st.settings_scroll_y, settings_ctx(&st)) })
                    .unwrap_or((0, fallback_settings_ctx()));
                let page = settings::layout(&ctx, width);
                if let Some(group) = settings::nav_hit(&page, x, y) {
                    let max_scroll = PANEL_STATE.as_ref()
                        .map(|sa| { let st = sa.lock().unwrap(); settings_max_scroll(hwnd, &st) })
                        .unwrap_or(0);
                    let target = page.group_tops.get(group).copied().unwrap_or(0) - 8;
                    if let Some(sa) = &PANEL_STATE {
                        sa.lock().unwrap().settings_scroll_y = target.clamp(0, max_scroll);
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                } else if let Some(id) = settings::hit_test(&page, x, y - settings::CONTENT_TOP + scroll_y) {
                    dispatch_settings_click(hwnd, host, id);
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

/// Settings-page snapshot for the layout engine.
fn settings_ctx(state: &PanelStateData) -> settings::Ctx {
    let upd = crate::update::snapshot();
    let latest = upd
        .latest
        .clone()
        .filter(|v| crate::update::is_newer(v, crate::update::current_version()));
    settings::Ctx {
        font_scale: state.font_scale,
        poll_ms: state.poll_interval_ms,
        ball_on: state.floating_ball_enabled,
        ball_opacity: state.ball_opacity,
        ball_style: state.ball_style,
        enable_toast: state.enable_toast,
        toast_info: state.toast_info,
        toast_warn: state.toast_warning,
        toast_crit: state.toast_critical,
        toast_gaming: state.toast_gaming_suppression,
        toast_sound: state.enable_low_battery_sound,
        ui_density: state.ui_density,
        device_filter: state.device_filter,
        device_sort: state.device_sort,
        accent_theme: state.accent_theme,
        auto_update: state.auto_update,
        auto_start: state.auto_start,
        update_line: update_status_line(&upd),
        update_available: latest.is_some(),
        latest_version: latest,
        config_path: crate::config::AppConfig::config_path().display().to_string(),
    }
}

fn fallback_settings_ctx() -> settings::Ctx {
    let config = crate::config::AppConfig::default();
    settings::Ctx {
        font_scale: config.font_scale,
        poll_ms: config.poll_interval_ms,
        ball_on: config.enable_floating_ball,
        ball_opacity: config.floating_ball_opacity,
        ball_style: config.floating_ball_style,
        enable_toast: config.enable_battery_toast,
        toast_info: config.toast_threshold_info,
        toast_warn: config.toast_threshold_warning,
        toast_crit: config.toast_threshold_critical,
        toast_gaming: config.toast_gaming_suppression,
        toast_sound: config.enable_low_battery_sound,
        ui_density: config.ui_density,
        device_filter: config.device_filter,
        device_sort: config.device_sort,
        accent_theme: config.accent_theme,
        auto_update: config.check_updates,
        auto_start: false,
        update_line: "当前版本 · 可手动检查更新".to_string(),
        update_available: false,
        latest_version: None,
        config_path: crate::config::AppConfig::config_path().display().to_string(),
    }
}

unsafe fn dispatch_settings_click(hwnd: HWND, host: HWND, id: i32) {
    let send_cmd = |cmd: usize| {
        if !host.is_null() {
            PostMessageW(
                host,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                cmd as _,
                0,
            );
        }
    };
    let toggle_state = |f: &dyn Fn(&mut PanelStateData)| {
        if let Some(state_arc) = &PANEL_STATE {
            f(&mut state_arc.lock().unwrap());
        }
        if !host.is_null() {
            PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
    };

    match id {
        30 => send_cmd(crate::ui::tray::IDM_POLL_30S),
        31 => send_cmd(crate::ui::tray::IDM_POLL_1M),
        32 => send_cmd(crate::ui::tray::IDM_POLL_5M),
        33 => send_cmd(crate::ui::tray::IDM_POLL_15M),
        34 => send_cmd(crate::ui::tray::IDM_POLL_30M),
        35 => send_cmd(crate::ui::tray::IDM_POLL_60M),
        40 => send_cmd(crate::ui::tray::IDM_FONT_SCALE_100),
        41 => send_cmd(crate::ui::tray::IDM_FONT_SCALE_115),
        42 => send_cmd(crate::ui::tray::IDM_FONT_SCALE_130),
        50 => send_cmd(crate::ui::tray::IDM_TOGGLE_BALL),
        52 => set_ball_style(hwnd, host, crate::config::FloatingBallStyle::Tab),
        53 => set_ball_style(hwnd, host, crate::config::FloatingBallStyle::Orb),
        60 => toggle_state(&|s: &mut PanelStateData| s.enable_toast = !s.enable_toast),
        61 => toggle_state(&|s: &mut PanelStateData| s.toast_gaming_suppression = !s.toast_gaming_suppression),
        62 => toggle_state(&|s: &mut PanelStateData| s.enable_low_battery_sound = !s.enable_low_battery_sound),
        70 => toggle_state(&|s: &mut PanelStateData| {
            s.toast_info = match s.toast_info { 40 => 50, 50 => 60, _ => 40 };
        }),
        71 => toggle_state(&|s: &mut PanelStateData| {
            s.toast_warning = match s.toast_warning { 20 => 30, 30 => 35, _ => 20 };
        }),
        72 => toggle_state(&|s: &mut PanelStateData| {
            s.toast_critical = match s.toast_critical { 5 => 10, 10 => 15, _ => 5 };
        }),
        80 => crate::ui::toast::ToastNotification::show(
            "罗技 M720 Triathlon",
            "电量已降至 28% (警告通知测试)",
            crate::ui::toast::ToastLevel::Warning,
            28,
        ),
        81 => crate::sound::play_low_battery_alarm(),
        100 => set_panel_density(hwnd, host, crate::config::UiDensity::Compact),
        101 => set_panel_density(hwnd, host, crate::config::UiDensity::Standard),
        102 => set_panel_density(hwnd, host, crate::config::UiDensity::Spacious),
        120 => set_dashboard_filter(hwnd, host, crate::config::DeviceFilter::All),
        121 => set_dashboard_filter(hwnd, host, crate::config::DeviceFilter::LowBattery),
        122 => set_dashboard_filter(hwnd, host, crate::config::DeviceFilter::Charging),
        123 => set_dashboard_filter(hwnd, host, crate::config::DeviceFilter::Bluetooth),
        124 => set_dashboard_filter(hwnd, host, crate::config::DeviceFilter::Wireless),
        125 => set_dashboard_sort(hwnd, host, crate::config::DeviceSort::Priority),
        126 => set_dashboard_sort(hwnd, host, crate::config::DeviceSort::Name),
        127 => set_dashboard_sort(hwnd, host, crate::config::DeviceSort::Battery),
        128 => set_dashboard_sort(hwnd, host, crate::config::DeviceSort::Updated),
        130 => set_accent_theme(hwnd, host, crate::config::AccentTheme::Orange),
        131 => set_accent_theme(hwnd, host, crate::config::AccentTheme::Blue),
        132 => set_accent_theme(hwnd, host, crate::config::AccentTheme::Purple),
        133 => set_accent_theme(hwnd, host, crate::config::AccentTheme::Green),
        140 => {
            if let Some(state_arc) = &PANEL_STATE {
                let mut st = state_arc.lock().unwrap();
                st.auto_update = !st.auto_update;
            }
            if !host.is_null() {
                PostMessageW(host, WM_SAVE_UPDATE_PREFS, 0, 0);
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        }
        141 => send_cmd(crate::ui::tray::IDM_CHECK_UPDATE),
        142 => {
            if crate::update::has_update() {
                send_cmd(crate::ui::tray::IDM_APPLY_UPDATE);
            }
        }
        150 => {
            if let Some(state_arc) = &PANEL_STATE {
                let mut st = state_arc.lock().unwrap();
                st.auto_start = !st.auto_start;
            }
            if !host.is_null() {
                PostMessageW(host, WM_SAVE_AUTOSTART, 0, 0);
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        }
        151 => open_config_dir(),
        _ => {}
    }
}

unsafe fn set_ball_style(hwnd: HWND, host: HWND, style: crate::config::FloatingBallStyle) {
    if let Some(state_arc) = &PANEL_STATE {
        state_arc.lock().unwrap().ball_style = style;
    }
    if !host.is_null() {
        let code = if matches!(style, crate::config::FloatingBallStyle::Tab) { 0 } else { 1 };
        PostMessageW(host, WM_SET_BALL_STYLE, code as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

unsafe fn set_dashboard_filter(hwnd: HWND, host: HWND, filter: crate::config::DeviceFilter) {
    let sort = PANEL_STATE
        .as_ref()
        .and_then(|sa| sa.lock().ok().map(|s| s.device_sort))
        .unwrap_or(crate::config::DeviceSort::Priority);
    set_dashboard_preferences(hwnd, host, filter, sort);
}

unsafe fn set_dashboard_sort(hwnd: HWND, host: HWND, sort: crate::config::DeviceSort) {
    let filter = PANEL_STATE
        .as_ref()
        .and_then(|sa| sa.lock().ok().map(|s| s.device_filter))
        .unwrap_or(crate::config::DeviceFilter::All);
    set_dashboard_preferences(hwnd, host, filter, sort);
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
