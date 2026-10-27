#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

pub(crate) use crate::devices::types::{ConnectionMode, DeviceSnapshot};
pub(crate) use crate::ui::clipboard::copy_text_to_clipboard;
pub(crate) use crate::ui::panel_render::{render_action_btn, render_segmented};
pub(crate) use crate::ui::dashboard;
pub(crate) use crate::ui::settings;
pub(crate) use crate::ui::titlebar;
pub(crate) use std::mem;
pub(crate) use std::ptr;
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
pub(crate) use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    RoundRect, ScreenToClient, SelectClipRgn, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HDC,
    HFONT, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
pub(crate) use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
pub(crate) use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture, SetCapture, SetFocus};
pub(crate) use windows_sys::Win32::UI::Shell::ShellExecuteW;
pub(crate) use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowRect, GetWindowTextW, IsDialogMessageW, IsZoomed,
    LoadCursorW, NCCALCSIZE_PARAMS, PostMessageW, RegisterClassExW, SendMessageW,
    SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    ES_AUTOHSCROLL, ICON_BIG, ICON_SMALL, IDC_ARROW,
    IsIconic, MINMAXINFO, SM_CXSCREEN, SM_CYSCREEN, SM_CXPADDEDBORDER, SM_CXSIZEFRAME,
    SM_CYSIZEFRAME, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW,
    HTCAPTION, HTCLIENT, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP,
    HTTOPLEFT, HTTOPRIGHT, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCPAINT, WM_NCACTIVATE,
    WM_CLOSE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT,
    WM_SETICON,
    WM_SIZE, WM_VSCROLL, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
    WS_POPUP, WS_SYSMENU, WS_VISIBLE, SW_SHOWNORMAL,
    SB_BOTTOM, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP,
};

// 面板按职责拆成子模块：状态派生 / 动作 / 窗口过程 / 重命名对话框。
// 子模块通过 `use super::*;` 共用同一套类型与常量（mod.rs 的导入一律 pub(crate)）。
mod actions;
mod chrome;
mod input;
mod paint;
mod rename;
mod state;
mod wndproc;

pub(crate) use actions::*;
pub(crate) use chrome::*;
pub(crate) use input::*;
pub(crate) use paint::*;
pub(crate) use rename::{send_rename_to_host, show_rename_dialog};
pub(crate) use state::*;
pub(crate) use wndproc::*;

pub const MIN_PANEL_WIDTH: i32 = 480;
pub const MIN_PANEL_HEIGHT: i32 = 400;
pub const PANEL_WIDTH: i32 = 640;
pub const PANEL_HEIGHT: i32 = 680;
// Custom messages exchanged with the hidden host window.
pub const WM_SAVE_PANEL_SIZE: u32 = 0x8000 + 105;
pub const WM_SAVE_UI_DENSITY: u32 = 0x8000 + 106;
pub const WM_SAVE_DASHBOARD_PREFS: u32 = 0x8000 + 107;
pub const WM_SAVE_ONBOARDING: u32 = 0x8000 + 108;
pub const WM_SAVE_UI_STYLE: u32 = 0x8000 + 109;
pub const WM_SAVE_UPDATE_PREFS: u32 = 0x8000 + 110;
pub const WM_SAVE_AUTOSTART: u32 = 0x8000 + 111;
/// wParam carries the docked ball opacity in percent (40-100).
pub const WM_SET_BALL_OPACITY: u32 = 0x8000 + 112;
/// 拖拽结束才落盘，避免拖拽过程中逐像素写文件。
pub const WM_SAVE_BALL_OPACITY: u32 = 0x8000 + 113;
/// wParam: 0..4 对应五种停靠形态（见 `FloatingBallStyle`）。
pub const WM_SET_BALL_STYLE: u32 = 0x8000 + 114;
/// wParam = 0xRRGGBB 主色，lParam = 1 表示浅色底。
pub const WM_SAVE_CUSTOM_THEME: u32 = 0x8000 + 115;
pub const WM_DEVICE_RENAME: u32 = 0x8000 + 101;
pub const WM_SAVE_TOAST_SETTINGS: u32 = 0x8000 + 104;

// 颜色一律取自设计令牌层（src/ui/theme.rs），本文件不再定义色值。
pub use crate::ui::theme::THEME;

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
    pub ui_style: crate::config::UiStyle,
    pub custom_accent: u32,
    pub custom_light: bool,
    /// 设置页当前分页（0..settings::GROUP_COUNT）
    pub settings_group: usize,
    pub show_onboarding: bool,
    pub selected_device_id: Option<String>,
    pub host_hwnd: HWND,
    pub active_tab: usize,
    pub enable_toast: bool,
    pub toast_info: u8,
    pub toast_warning: u8,
    pub toast_critical: u8,
    pub toast_second: u8,
    pub alert_sound: crate::config::AlertSound,
    pub toast_gaming_suppression: bool,
    pub enable_low_battery_sound: bool,
    pub auto_update: bool,
    pub auto_start: bool,
}
/// 面板每次喂数据都要带上的「当前配置快照」。
///
/// 之前是 7 个裸参数一路传，调用点长到 170 列且极易漏传；聚成结构体后
/// 调用点回到一行，`from_config` 保证默认值与配置文件同源。
#[derive(Clone, Copy, Debug)]
pub struct LivePrefs {
    pub ball_enabled: bool,
    pub ball_opacity: u8,
    pub ball_style: crate::config::FloatingBallStyle,
    pub font_scale: f32,
    pub poll_interval_ms: u64,
    pub density: crate::config::UiDensity,
}

impl LivePrefs {
    pub fn from_config(config: &crate::config::AppConfig) -> Self {
        Self {
            ball_enabled: config.enable_floating_ball,
            ball_opacity: config.floating_ball_opacity,
            ball_style: config.floating_ball_style,
            font_scale: config.font_scale,
            poll_interval_ms: config.poll_interval_ms,
            density: config.ui_density,
        }
    }

    pub fn with_ball(mut self, on: bool) -> Self {
        self.ball_enabled = on;
        self
    }

    pub fn with_density(mut self, density: crate::config::UiDensity) -> Self {
        self.density = density;
        self
    }
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
                ui_style: config.ui_style,
                custom_accent: config.custom_accent,
                custom_light: config.custom_light,
                settings_group: 0,
                show_onboarding: config.show_onboarding,
                selected_device_id: None,
                host_hwnd,
                active_tab: 0,
                enable_toast: config.enable_battery_toast,
                toast_info: config.toast_threshold_info,
                toast_warning: config.toast_threshold_warning,
                toast_critical: config.toast_threshold_critical,
                toast_second: config.toast_threshold_second,
                alert_sound: config.alert_sound,
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

    pub fn update_devices(&self, devices: &[DeviceSnapshot], prefs: LivePrefs) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.devices = devices.to_vec();
                state.floating_ball_enabled = prefs.ball_enabled;
                state.font_scale = prefs.font_scale;
                state.poll_interval_ms = prefs.poll_interval_ms;
                state.ball_opacity = prefs.ball_opacity;
                state.ball_style = prefs.ball_style;
                state.next_refresh_seconds = prefs.poll_interval_ms / 1000;
                state.ui_density = prefs.density;
                let filtered_count = prepare_devices(state.devices.clone(), state.device_filter, state.device_sort).len();
                let mut client: RECT = mem::zeroed();
                GetClientRect(self.hwnd, &mut client);
                let viewport = dashboard::viewport_height(client.bottom - client.top);
                let step = dashboard::card_geometry(state.ui_density).1;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_message_constant_stays_stable() {
        assert_eq!(WM_DEVICE_RENAME, 0x8000 + 101);
    }

    /// `LivePrefs::from_config` 必须原样搬运配置：漏一个字段就会出现
    /// 「改了不透明度但面板不变」这类静默不同步。
    #[test]
    fn live_prefs_copies_every_field_from_config() {
        let mut c = crate::config::AppConfig::default();
        c.enable_floating_ball = true;
        c.floating_ball_opacity = 66;
        c.floating_ball_style = crate::config::FloatingBallStyle::Pill;
        c.font_scale = 1.3;
        c.poll_interval_ms = 12_000;
        c.ui_density = crate::config::UiDensity::Compact;

        let prefs = LivePrefs::from_config(&c);
        assert!(prefs.ball_enabled);
        assert_eq!(prefs.ball_opacity, 66);
        assert_eq!(prefs.ball_style as u8, crate::config::FloatingBallStyle::Pill as u8);
        assert!((prefs.font_scale - 1.3).abs() < f32::EPSILON);
        assert_eq!(prefs.poll_interval_ms, 12_000);
        assert_eq!(prefs.density as u8, crate::config::UiDensity::Compact as u8);
        // with_* 只覆盖对应维度
        let tweaked = prefs.with_ball(false).with_density(crate::config::UiDensity::Spacious);
        assert!(!tweaked.ball_enabled);
        assert_eq!(tweaked.ball_opacity, 66);
    }
}

