#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::{ConnectionMode, DeviceKind, DeviceSnapshot};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, RoundRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
    DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HDC, HFONT, PAINTSTRUCT, PS_SOLID,
    SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowTextW, IsDialogMessageW, LoadCursorW, PostMessageW,
    RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL, ICON_BIG, ICON_SMALL, IDC_ARROW, IsIconic, SM_CXSCREEN,
    SM_CYSCREEN, SW_HIDE, SW_RESTORE, SW_SHOW, WM_CLOSE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_SETICON,
    WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

pub const PANEL_WIDTH: i32 = 530;
pub const PANEL_HEIGHT: i32 = 580;

// Custom message for renaming device alias from GUI: wParam = &id, lParam = &new_name
pub const WM_DEVICE_RENAME: u32 = 0x8000 + 101;
// Custom message for persisting toast settings to config
pub const WM_SAVE_TOAST_SETTINGS: u32 = 0x8000 + 104;
const CLR_BG: COLORREF = 0x00161412; // #121416
const CLR_HEADER: COLORREF = 0x00221E1A; // #1A1E22
const CLR_CARD: COLORREF = 0x00201B18; // #181B20
const CLR_CARD_BORDER: COLORREF = 0x003A322C; // #2C323A
const CLR_TEXT: COLORREF = 0x00F4F4F4;
const CLR_MUTED: COLORREF = 0x00B0A8A0;
const CLR_ACCENT: COLORREF = 0x00E08030; // Orange / Cyan
const CLR_GREEN: COLORREF = 0x0081B910;
const CLR_YELLOW: COLORREF = 0x000B9EF5;
const CLR_RED: COLORREF = 0x004444EF;
const CLR_CYAN: COLORREF = 0x00D4B606;
const CLR_BTN_BG: COLORREF = 0x002C241E;
const CLR_BTN_HOVER: COLORREF = 0x003E332A;
const CLR_BTN_ACTIVE: COLORREF = 0x00524438;

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
    pub host_hwnd: HWND,
    pub active_tab: usize, // 0 = Devices, 1 = Settings
    pub enable_toast: bool,
    pub toast_info: u8,
    pub toast_warning: u8,
    pub toast_critical: u8,
    pub toast_gaming_suppression: bool,
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
            let start_x = (screen_w - PANEL_WIDTH) / 2;
            let start_y = (screen_h - PANEL_HEIGHT) / 2;

            let state_data = Arc::new(Mutex::new(PanelStateData {
                devices: Vec::new(),
                visible: false,
                hover_btn: 0,
                floating_ball_enabled: config.enable_floating_ball,
                font_scale: config.font_scale,
                poll_interval_ms: config.poll_interval_ms,
                host_hwnd,
                active_tab: 0,
                enable_toast: config.enable_battery_toast,
                toast_info: config.toast_threshold_info,
                toast_warning: config.toast_threshold_warning,
                toast_critical: config.toast_threshold_critical,
                toast_gaming_suppression: config.toast_gaming_suppression,
            }));
            PANEL_STATE = Some(state_data);

            let title_wide: Vec<u16> = "LinkDeck - 外设控制中心\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                class_name.as_ptr(),
                title_wide.as_ptr(),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                start_x,
                start_y,
                PANEL_WIDTH,
                PANEL_HEIGHT,
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
        font_scale: f32,
        poll_interval_ms: u64,
    ) {
        unsafe {
            if let Some(state_arc) = &PANEL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.devices = devices.to_vec();
                state.floating_ball_enabled = ball_enabled;
                state.font_scale = font_scale;
                state.poll_interval_ms = poll_interval_ms;
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

unsafe extern "system" fn panel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
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

            let (devices, ball_on, font_scale, poll_ms, hover_btn, active_tab, enable_toast, toast_info, toast_warn, toast_crit, toast_gaming) =
                if let Some(state_arc) = &PANEL_STATE {
                    let state = state_arc.lock().unwrap();
                    (
                        state.devices.clone(),
                        state.floating_ball_enabled,
                        state.font_scale,
                        state.poll_interval_ms,
                        state.hover_btn,
                        state.active_tab,
                        state.enable_toast,
                        state.toast_info,
                        state.toast_warning,
                        state.toast_critical,
                        state.toast_gaming_suppression,
                    )
                } else {
                    (Vec::new(), false, 1.15, 300_000, 0, 0, true, 50, 30, 10, true)
                };

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
                // --- TAB 0: DEVICE DASHBOARD ---
                for (idx, dev) in devices.iter().take(3).enumerate() {
                    let y_card = 74 + idx as i32 * 114;
                    let icon = match dev.kind {
                        DeviceKind::Keyboard => "⌨️",
                        DeviceKind::Mouse => "🖱️",
                        DeviceKind::Headset => "🎧",
                        _ => "📱",
                    };
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
                    let is_hover_edit = hover_btn == (100 + idx as i32);
                    render_device_card(
                        mem_dc,
                        20,
                        y_card,
                        width - 40,
                        106,
                        &dev.name,
                        icon,
                        protocol,
                        Some(dev),
                        font_scale,
                        is_hover_edit,
                    );
                }

                if devices.is_empty() {
                    SetTextColor(mem_dc, CLR_TEXT);
                    let font_empty_title = create_clear_font((15.0 * font_scale).round() as i32, true);
                    let old_empty_f = SelectObject(mem_dc, font_empty_title);
                    let mut emp_rect = RECT {
                        left: 20,
                        top: 130,
                        right: width - 20,
                        bottom: 156,
                    };
                    draw_text_clean(mem_dc, "未检测到已适配的 2.4G 外设或在线蓝牙设备", &mut emp_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
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
                    draw_text_clean(mem_dc, "已适配: 罗技/雷蛇全系 · RK/狼蛛键盘 · 迈从/VXE/达尔优鼠标 · 赛技 · 全系蓝牙\n提示: 多模外设切至蓝牙模式可直接读取；可在终端运行 --protocols 查看完整清单", &mut emp_tip_rect, DT_CENTER);
                    SelectObject(mem_dc, old_tip_f);
                    DeleteObject(font_empty_tip);
                }

                // Dashboard Footer: Instant Refresh & Settings Button
                render_action_btn(
                    mem_dc,
                    20,
                    height - 48,
                    160,
                    34,
                    "🔄 立即刷新设备",
                    hover_btn == 1,
                    font_scale,
                );

                render_action_btn(
                    mem_dc,
                    190,
                    height - 48,
                    130,
                    34,
                    "⚙️ 偏好设置",
                    hover_btn == 2,
                    font_scale,
                );
            } else {
                // --- TAB 1: DEDICATED SETTINGS VIEW ---
                render_settings_view(
                    mem_dc,
                    width,
                    height,
                    font_scale,
                    poll_ms,
                    ball_on,
                    enable_toast,
                    toast_info,
                    toast_warn,
                    toast_crit,
                    toast_gaming,
                    hover_btn,
                );
            }

            // Version info at bottom right
            SetTextColor(mem_dc, CLR_MUTED);
            let mut ver_rect = RECT {
                left: width - 140,
                top: height - 42,
                right: width - 20,
                bottom: height - 18,
            };
            draw_text_clean(mem_dc, concat!("v", env!("CARGO_PKG_VERSION"), " (Rust)"), &mut ver_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

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
                } else {
                    for idx in 0..3 {
                        let card_y = 74 + idx * 114;
                        if y >= card_y + 10 && y <= card_y + 32 {
                            let btn_x = 20 + (width - 40) - 190;
                            if x >= btn_x && x <= btn_x + 58 {
                                new_hover = 100 + idx;
                                break;
                            }
                        }
                    }
                }
            } else {
                // Settings View Hover
                if y >= 115 && y <= 146 {
                    if x >= 20 && x <= 92 { new_hover = 30; }
                    else if x >= 98 && x <= 170 { new_hover = 31; }
                    else if x >= 176 && x <= 254 { new_hover = 32; }
                    else if x >= 260 && x <= 338 { new_hover = 33; }
                    else if x >= 344 && x <= 422 { new_hover = 34; }
                    else if x >= 428 && x <= 506 { new_hover = 35; }
                } else if y >= 195 && y <= 226 {
                    if x >= 20 && x <= 112 { new_hover = 40; }
                    else if x >= 120 && x <= 212 { new_hover = 41; }
                    else if x >= 220 && x <= 312 { new_hover = 42; }
                } else if y >= 275 && y <= 308 {
                    if x >= 20 && x <= 200 { new_hover = 50; }
                } else if y >= 356 && y <= 388 {
                    if x >= 20 && x <= 180 { new_hover = 60; }
                    else if x >= 190 && x <= 380 { new_hover = 61; }
                } else if y >= 392 && y <= 424 {
                    if x >= 20 && x <= 125 { new_hover = 70; }
                    else if x >= 135 && x <= 240 { new_hover = 71; }
                    else if x >= 250 && x <= 355 { new_hover = 72; }
                    else if x >= 365 && x <= 505 { new_hover = 80; }
                } else if y >= height - 48 && y <= height - 14 {
                    if x >= 20 && x <= 180 { new_hover = 90; }
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
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;

            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let (host, active_tab) = if let Some(state_arc) = &PANEL_STATE {
                let s = state_arc.lock().unwrap();
                (s.host_hwnd, s.active_tab)
            } else {
                (ptr::null_mut(), 0)
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
                // Device Dashboard Actions
                if y >= height - 48 && y <= height - 14 {
                    if x >= 20 && x <= 180 {
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
                        for (idx, dev) in state.devices.iter().take(3).enumerate() {
                            let card_y = 74 + idx as i32 * 114;
                            if y >= card_y + 10 && y <= card_y + 32 {
                                let btn_x = 20 + (width - 40) - 190;
                                if x >= btn_x && x <= btn_x + 58 {
                                    clicked_card = Some((dev.id.clone(), dev.name.clone()));
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
                if y >= 115 && y <= 146 {
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
                } else if y >= 195 && y <= 226 {
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
                } else if y >= 275 && y <= 308 {
                    if x >= 20 && x <= 200 && !host.is_null() {
                        PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_TOGGLE_BALL as _, 0);
                    }
                } else if y >= 356 && y <= 388 {
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
                    }
                } else if y >= 392 && y <= 424 {
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

unsafe fn render_settings_view(
    mem_dc: HDC,
    width: i32,
    height: i32,
    font_scale: f32,
    poll_ms: u64,
    ball_on: bool,
    enable_toast: bool,
    toast_info: u8,
    toast_warn: u8,
    toast_crit: u8,
    toast_gaming: bool,
    hover_btn: i32,
) {
    SetBkMode(mem_dc, TRANSPARENT as i32);

    // Section 1: 定时通信间隔
    let f_title = create_clear_font((14.0 * font_scale).round() as i32, true);
    let old_f = SelectObject(mem_dc, f_title);
    SetTextColor(mem_dc, CLR_TEXT);
    let mut s1_t = RECT { left: 20, top: 76, right: width - 20, bottom: 96 };
    draw_text_clean(mem_dc, "1. 定时通信间隔 (后台巡检轮询)", &mut s1_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    let f_desc = create_clear_font((12.0 * font_scale).round() as i32, false);
    SelectObject(mem_dc, f_desc);
    SetTextColor(mem_dc, CLR_MUTED);
    let mut s1_d = RECT { left: 20, top: 96, right: width - 20, bottom: 114 };
    draw_text_clean(mem_dc, "外设电量消耗慢，推荐长周期以极致省电；随时点击'立即刷新'可秒级直读当前电量。", &mut s1_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    render_choice_pill(mem_dc, 20, 118, 72, 26, "30s 调试", poll_ms <= 45_000, hover_btn == 30, font_scale);
    render_choice_pill(mem_dc, 98, 118, 72, 26, "1m 快速", poll_ms > 45_000 && poll_ms <= 120_000, hover_btn == 31, font_scale);
    render_choice_pill(mem_dc, 176, 118, 78, 26, "5m 推荐", poll_ms > 120_000 && poll_ms <= 600_000, hover_btn == 32, font_scale);
    render_choice_pill(mem_dc, 260, 118, 78, 26, "15m 适中", poll_ms > 600_000 && poll_ms <= 1_200_000, hover_btn == 33, font_scale);
    render_choice_pill(mem_dc, 344, 118, 78, 26, "30m 省电", poll_ms > 1_200_000 && poll_ms <= 2_700_000, hover_btn == 34, font_scale);
    render_choice_pill(mem_dc, 428, 118, 78, 26, "1h 极省", poll_ms > 2_700_000, hover_btn == 35, font_scale);

    // Section 2: 界面显示字号
    SelectObject(mem_dc, f_title);
    SetTextColor(mem_dc, CLR_TEXT);
    let mut s2_t = RECT { left: 20, top: 156, right: width - 20, bottom: 176 };
    draw_text_clean(mem_dc, "2. 界面显示字号 (ClearType 次像素渲染)", &mut s2_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    SelectObject(mem_dc, f_desc);
    SetTextColor(mem_dc, CLR_MUTED);
    let mut s2_d = RECT { left: 20, top: 176, right: width - 20, bottom: 194 };
    draw_text_clean(mem_dc, "Windows 微软雅黑高分屏抗锯齿次像素渲染字体缩放比例：", &mut s2_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    let is_s100 = font_scale <= 1.05;
    let is_s115 = font_scale > 1.05 && font_scale <= 1.22;
    let is_s130 = font_scale > 1.22;
    render_choice_pill(mem_dc, 20, 198, 92, 26, "100% 标准", is_s100, hover_btn == 40, font_scale);
    render_choice_pill(mem_dc, 120, 198, 92, 26, "115% 适中", is_s115, hover_btn == 41, font_scale);
    render_choice_pill(mem_dc, 220, 198, 92, 26, "130% 放大", is_s130, hover_btn == 42, font_scale);

    // Section 3: 桌面贴边悬浮球
    SelectObject(mem_dc, f_title);
    SetTextColor(mem_dc, CLR_TEXT);
    let mut s3_t = RECT { left: 20, top: 236, right: width - 20, bottom: 256 };
    draw_text_clean(mem_dc, "3. 桌面贴边悬浮球", &mut s3_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    SelectObject(mem_dc, f_desc);
    SetTextColor(mem_dc, CLR_MUTED);
    let mut s3_d = RECT { left: 20, top: 256, right: width - 20, bottom: 274 };
    draw_text_clean(mem_dc, "吸附于屏幕边缘的微型动态能量环胶囊，滚轮切台，悬停滑出抽屉：", &mut s3_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    let ball_text = if ball_on { "🔘 贴边悬浮球: [已开启]" } else { "⚪ 贴边悬浮球: [已关闭]" };
    render_choice_pill(mem_dc, 20, 278, 180, 28, ball_text, ball_on, hover_btn == 50, font_scale);

    // Section 4: 低电量 Toast 弹窗通知
    SelectObject(mem_dc, f_title);
    SetTextColor(mem_dc, CLR_TEXT);
    let mut s4_t = RECT { left: 20, top: 318, right: width - 20, bottom: 338 };
    draw_text_clean(mem_dc, "4. 低电量原生 Toast 弹窗通知 (多级预警)", &mut s4_t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    SelectObject(mem_dc, f_desc);
    SetTextColor(mem_dc, CLR_MUTED);
    let mut s4_d = RECT { left: 20, top: 338, right: width - 20, bottom: 356 };
    draw_text_clean(mem_dc, "外设电量跌破阈值时在屏幕右下角弹出 Toast 提醒；全屏或玩游戏时降级/免打扰：", &mut s4_d, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    let toast_text = if enable_toast { "🔔 电量弹窗: [已开启]" } else { "🔕 电量弹窗: [已关闭]" };
    render_choice_pill(mem_dc, 20, 360, 160, 26, toast_text, enable_toast, hover_btn == 60, font_scale);

    let game_text = if toast_gaming { "🎮 游戏全屏免打扰: [已开启]" } else { "🎮 游戏全屏免打扰: [已关闭]" };
    render_choice_pill(mem_dc, 190, 360, 190, 26, game_text, toast_gaming, hover_btn == 61, font_scale);

    // Row 2: Threshold buttons
    let info_str = format!("提示: < {}%", toast_info);
    let warn_str = format!("警告: < {}%", toast_warn);
    let crit_str = format!("危险: < {}%", toast_crit);
    render_choice_pill(mem_dc, 20, 396, 105, 26, &info_str, true, hover_btn == 70, font_scale);
    render_choice_pill(mem_dc, 135, 396, 105, 26, &warn_str, true, hover_btn == 71, font_scale);
    render_choice_pill(mem_dc, 250, 396, 105, 26, &crit_str, true, hover_btn == 72, font_scale);
    render_choice_pill(mem_dc, 365, 396, 140, 26, "🚀 试发一条Toast", false, hover_btn == 80, font_scale);

    // Bottom action: Back to devices view
    render_action_btn(
        mem_dc,
        20,
        height - 48,
        150,
        34,
        "◀ 返回设备监控",
        hover_btn == 90,
        font_scale,
    );

    SelectObject(mem_dc, old_f);
    DeleteObject(f_title);
    DeleteObject(f_desc);
}
unsafe fn render_device_card(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    title: &str,
    icon: &str,
    protocol: &str,
    dev: Option<&DeviceSnapshot>,
    font_scale: f32,
    hover_edit: bool,
) {
    let border_pen = CreatePen(PS_SOLID, 1, CLR_CARD_BORDER);
    let old_pen = SelectObject(hdc, border_pen);
    let card_brush = CreateSolidBrush(CLR_CARD);
    let old_brush = SelectObject(hdc, card_brush);

    RoundRect(hdc, x, y, x + w, y + h, 12, 12);

    SelectObject(hdc, old_brush);
    DeleteObject(card_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(border_pen);

    // Card Title
    let font_sz_title = (15.0 * font_scale).round() as i32;
    let font_bold = create_clear_font(font_sz_title, true);
    let old_font = SelectObject(hdc, font_bold);
    SetTextColor(hdc, CLR_TEXT);
    let title_full = format!("{} {}", icon, title);
    let mut t_rect = RECT {
        left: x + 16,
        top: y + 10,
        right: x + w - 196,
        bottom: y + 32,
    };
    draw_text_clean(hdc, &title_full, &mut t_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // Rename Button: [✏️ 改名]
    let btn_x = x + w - 190;
    let btn_y = y + 11;
    let btn_w = 58;
    let btn_h = 20;

    let edit_bg = if hover_edit { CLR_BTN_HOVER } else { CLR_BTN_BG };
    let edit_brush = CreateSolidBrush(edit_bg);
    let edit_pen = CreatePen(PS_SOLID, 1, if hover_edit { CLR_ACCENT } else { CLR_CARD_BORDER });
    let old_p = SelectObject(hdc, edit_pen);
    let old_b = SelectObject(hdc, edit_brush);

    RoundRect(hdc, btn_x, btn_y, btn_x + btn_w, btn_y + btn_h, 4, 4);

    SelectObject(hdc, old_b);
    DeleteObject(edit_brush);
    SelectObject(hdc, old_p);
    DeleteObject(edit_pen);

    let font_btn = create_clear_font((11.5 * font_scale).round() as i32, false);
    SelectObject(hdc, font_btn);
    SetTextColor(hdc, if hover_edit { CLR_TEXT } else { CLR_MUTED });
    let mut btn_r = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };
    draw_text_clean(hdc, "✏️ 改名", &mut btn_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old_font);
    DeleteObject(font_btn);

    // Protocol badge / connection info
    let font_sz_sub = (12.5 * font_scale).round() as i32;
    let font_normal = create_clear_font(font_sz_sub, false);
    SelectObject(hdc, font_normal);
    SetTextColor(hdc, CLR_MUTED);

    let (conn_str, vid_pid) = if let Some(d) = dev {
        (
            d.connection.as_str(),
            if d.vendor_id > 0 {
                format!("VID 0x{:04X} : PID 0x{:04X}", d.vendor_id, d.product_id)
            } else {
                "系统蓝牙栈".to_string()
            },
        )
    } else {
        ("未连接", "等待接入...".to_string())
    };

    let meta_str = format!("模式: {} | {} | {}", conn_str, vid_pid, protocol);
    let mut m_rect = RECT {
        left: x + 16,
        top: y + 33,
        right: x + w - 16,
        bottom: y + 52,
    };
    draw_text_clean(hdc, &meta_str, &mut m_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // Battery & status section
    let (pct, mv, charging, online) = if let Some(d) = dev {
        (
            d.battery.percentage,
            d.battery.voltage_mv,
            d.battery.charging,
            d.battery.online,
        )
    } else {
        (None, None, false, false)
    };

    let (chg_tag, status_color) = if let Some(d) = dev {
        if d.battery.charging || d.battery.charging_source.is_charging() {
            (" ⚡", CLR_CYAN)
        } else if !d.battery.online {
            (" [待机]", CLR_MUTED)
        } else {
            let clr = if let Some(p) = pct {
                if p >= 50 { CLR_GREEN } else if p >= 20 { CLR_YELLOW } else { CLR_RED }
            } else {
                CLR_MUTED
            };
            ("", clr)
        }
    } else {
        ("", CLR_MUTED)
    };
    // Big Battery Percentage Display
    let font_sz_big = (24.0 * font_scale).round() as i32;
    let font_big = create_clear_font(font_sz_big, true);
    SelectObject(hdc, font_big);
    SetTextColor(hdc, status_color);

    let p_str = if let Some(p) = pct {
        format!("{}%{}", p, chg_tag)
    } else if online {
        "在线".to_string()
    } else {
        "待机休眠".to_string()
    };

    let mut p_rect = RECT {
        left: x + w - 160,
        top: y + 8,
        right: x + w - 16,
        bottom: y + 42,
    };
    draw_text_clean(hdc, &p_str, &mut p_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

    // Voltage readout / status detail
    SelectObject(hdc, font_normal);
    let detail_str = if let Some(d) = dev {
        if d.battery.charging || d.battery.charging_source.is_charging() {
            if let Some(v) = mv {
                format!("供电: 正在充电中 · 电池端电压 {} mV ({:.3} V)", v, v as f32 / 1000.0)
            } else {
                "供电: USB 接口供电 · 持续充电中".to_string()
            }
        } else if let Some(v) = mv {
            format!("放电: 锂电池电压 {} mV ({:.3} V) · 放电曲线平稳", v, v as f32 / 1000.0)
        } else if online {
            "状态: 正常运行 · 已就绪".to_string()
        } else {
            "状态: 处于低功耗省电休眠模式 (触动即醒)".to_string()
        }
    } else {
        "状态: 等待外设接入".to_string()
    };
    SetTextColor(hdc, CLR_MUTED);
    let mut d_rect = RECT {
        left: x + 16,
        top: y + 56,
        right: x + w - 16,
        bottom: y + 76,
    };
    draw_text_clean(hdc, &detail_str, &mut d_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // Battery Progress Bar
    let bar_x = x + 16;
    let bar_y = y + 82;
    let bar_w = w - 32;
    let bar_h = 10;

    let bar_bg = CreateSolidBrush(0x00322A26);
    let bg_r = RECT {
        left: bar_x,
        top: bar_y,
        right: bar_x + bar_w,
        bottom: bar_y + bar_h,
    };
    RoundRect(hdc, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, 6, 6);
    FillRect(hdc, &bg_r, bar_bg);
    DeleteObject(bar_bg);

    let fill_pct = pct.unwrap_or(0) as i32;
    let fill_w = (bar_w * fill_pct) / 100;
    if fill_w > 0 {
        let bar_fg = CreateSolidBrush(status_color);
        let fg_r = RECT {
            left: bar_x,
            top: bar_y,
            right: bar_x + fill_w,
            bottom: bar_y + bar_h,
        };
        RoundRect(hdc, bar_x, bar_y, bar_x + fill_w, bar_y + bar_h, 6, 6);
        FillRect(hdc, &fg_r, bar_fg);
        DeleteObject(bar_fg);
    }

    SelectObject(hdc, old_font);
    DeleteObject(font_bold);
    DeleteObject(font_normal);
    DeleteObject(font_big);
}

unsafe fn render_choice_pill(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    active: bool,
    hover: bool,
    font_scale: f32,
) {
    let (bg, border_clr, text_clr) = if active {
        (CLR_BTN_ACTIVE, CLR_ACCENT, CLR_TEXT)
    } else if hover {
        (CLR_BTN_HOVER, CLR_MUTED, CLR_TEXT)
    } else {
        (CLR_BTN_BG, CLR_CARD_BORDER, CLR_MUTED)
    };

    let brush = CreateSolidBrush(bg);
    let pen = CreatePen(PS_SOLID, 1, border_clr);
    let op = SelectObject(hdc, pen);
    let ob = SelectObject(hdc, brush);

    RoundRect(hdc, x, y, x + w, y + h, 6, 6);

    SelectObject(hdc, ob);
    DeleteObject(brush);
    SelectObject(hdc, op);
    DeleteObject(pen);

    let font_sz = (12.0 * font_scale).round() as i32;
    let font_pill = create_clear_font(font_sz, active);
    let old_font = SelectObject(hdc, font_pill);

    SetTextColor(hdc, text_clr);
    let mut r = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_pill);
}

unsafe fn render_action_btn(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    hover: bool,
    font_scale: f32,
) {
    let btn_bg = if hover { CLR_BTN_HOVER } else { CLR_BTN_BG };
    let brush = CreateSolidBrush(btn_bg);
    let border = CreatePen(PS_SOLID, 1, if hover { CLR_ACCENT } else { CLR_CARD_BORDER });

    let old_pen = SelectObject(hdc, border);
    let old_brush = SelectObject(hdc, brush);

    RoundRect(hdc, x, y, x + w, y + h, 8, 8);

    SelectObject(hdc, old_brush);
    DeleteObject(brush);
    SelectObject(hdc, old_pen);
    DeleteObject(border);

    let font_sz = (13.0 * font_scale).round() as i32;
    let font = create_clear_font(font_sz, true);
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, if hover { CLR_TEXT } else { 0x00E0D8D0 });

    let mut r = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font);
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
