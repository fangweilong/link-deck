#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::{ConnectionMode, DeviceKind, DeviceSnapshot};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, RoundRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, FW_BOLD, FW_NORMAL, HDC, HFONT, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowTextW, IsDialogMessageW, LoadCursorW, PostMessageW,
    RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, ES_AUTOHSCROLL, ICON_BIG, ICON_SMALL, IDC_ARROW, SM_CXSCREEN,
    SM_CYSCREEN, SW_HIDE, SW_SHOW, WM_CLOSE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_SETICON,
    WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

pub const PANEL_WIDTH: i32 = 530;
pub const PANEL_HEIGHT: i32 = 572;

// Custom message for renaming device alias from GUI: wParam = &id, lParam = &new_name
pub const WM_DEVICE_RENAME: u32 = 0x8000 + 101;
// Color theme
const CLR_BG: COLORREF = 0x00161412; // #121416
const CLR_HEADER: COLORREF = 0x00221E1A; // #1A1E22
const CLR_CARD: COLORREF = 0x00201B18; // #181B20
const CLR_CARD_BORDER: COLORREF = 0x003A322C; // #2C323A
const CLR_TEXT: COLORREF = 0x00F4F4F4;
const CLR_MUTED: COLORREF = 0x00908880;
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

static mut PANEL_STATE: Option<Arc<Mutex<PanelStateData>>> = None;

pub struct PanelStateData {
    pub devices: Vec<DeviceSnapshot>,
    pub visible: bool,
    pub hover_btn: i32,
    pub floating_ball_enabled: bool,
    pub font_scale: f32,
    pub poll_interval_ms: u64,
    pub host_hwnd: HWND,
}

impl PanelWindow {
    pub fn new(
        host_hwnd: HWND,
        initial_ball_enabled: bool,
        font_scale: f32,
        initial_poll_interval_ms: u64,
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
                floating_ball_enabled: initial_ball_enabled,
                font_scale,
                poll_interval_ms: initial_poll_interval_ms,
                host_hwnd,
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
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
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
        size,
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

            let (devices, ball_on, font_scale, poll_ms, hover_btn) = if let Some(state_arc) = &PANEL_STATE {
                let state = state_arc.lock().unwrap();
                (
                    state.devices.clone(),
                    state.floating_ball_enabled,
                    state.font_scale,
                    state.poll_interval_ms,
                    state.hover_btn,
                )
            } else {
                (Vec::new(), false, 1.15, 3000, 0)
            };

            // Header Title
            let font_h1 = create_clear_font((19.0 * font_scale).round() as i32, true);
            let old_font = SelectObject(mem_dc, font_h1);
            SetTextColor(mem_dc, CLR_TEXT);

            let mut h_title_rect = RECT {
                left: 20,
                top: 8,
                right: width - 20,
                bottom: 34,
            };
            draw_text_clean(mem_dc, "LinkDeck 外设控制中心", &mut h_title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Header Subtitle
            let font_sub = create_clear_font((12.0 * font_scale).round() as i32, false);
            SelectObject(mem_dc, font_sub);
            SetTextColor(mem_dc, CLR_MUTED);

            let mut h_sub_rect = RECT {
                left: 20,
                top: 36,
                right: width - 20,
                bottom: 56,
            };
            draw_text_clean(mem_dc, "常驻系统状态栏 · 贴边悬浮球可配置 · 蓝牙/2.4G/有线全兼容", &mut h_sub_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Render Device Cards (up to 3 devices)
            for (idx, dev) in devices.iter().take(3).enumerate() {
                let y_card = 74 + idx as i32 * 114;
                let icon = match dev.kind {
                    DeviceKind::Keyboard => "⌨️",
                    DeviceKind::Mouse => "🖱️",
                    DeviceKind::Headset => "🎧",
                    _ => "📱",
                };
                let protocol = match dev.connection {
                    ConnectionMode::Bluetooth => "Windows 蓝牙电池服务 (HFP / GATT)",
                    ConnectionMode::Wireless24G if dev.id.contains("rk") => {
                        "Compx 2.4G 特性协议 (COL06)"
                    }
                    ConnectionMode::Wireless24G if dev.id.contains("mouse") => {
                        "Xenta / Areson 2.4G HID 协议 (COL03/04)"
                    }
                    ConnectionMode::Wireless24G => "2.4G 无线 HID 协议",
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
                SetTextColor(mem_dc, CLR_MUTED);
                let mut emp_rect = RECT {
                    left: 20,
                    top: 140,
                    right: width - 20,
                    bottom: 180,
                };
                draw_text_clean(mem_dc, "正在扫描已连接的键盘、鼠标和蓝牙设备...", &mut emp_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            }

            // Polling Interval Selector Row (at height - 124 .. height - 98)
            let font_label_sz = (12.0 * font_scale).round() as i32;
            let f_opt = create_clear_font(font_label_sz, false);
            SelectObject(mem_dc, f_opt);
            SetTextColor(mem_dc, CLR_MUTED);

            let mut p_label_rect = RECT {
                left: 20,
                top: height - 124,
                right: 95,
                bottom: height - 98,
            };
            draw_text_clean(mem_dc, "刷新频率:", &mut p_label_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let is_p1500 = poll_ms <= 2000;
            let is_p3000 = poll_ms > 2000 && poll_ms <= 6000;
            let is_p10000 = poll_ms > 6000 && poll_ms <= 20000;
            let is_p30000 = poll_ms > 20000;

            render_choice_pill(mem_dc, 98, height - 124, 76, 26, "1.5s 极速", is_p1500, hover_btn == 20);
            render_choice_pill(mem_dc, 180, height - 124, 76, 26, "3s 推荐", is_p3000, hover_btn == 21);
            render_choice_pill(mem_dc, 262, height - 124, 82, 26, "10s 省电", is_p10000, hover_btn == 22);
            render_choice_pill(mem_dc, 350, height - 124, 94, 26, "30s 长续航", is_p30000, hover_btn == 23);

            // Font Scale Selector Row (at height - 90 .. height - 64)
            let mut f_label_rect = RECT {
                left: 20,
                top: height - 90,
                right: 95,
                bottom: height - 64,
            };
            draw_text_clean(mem_dc, "字体大小:", &mut f_label_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let is_s100 = font_scale <= 1.05;
            let is_s115 = font_scale > 1.05 && font_scale <= 1.22;
            let is_s130 = font_scale > 1.22;

            render_choice_pill(mem_dc, 98, height - 90, 86, 26, "100% 标准", is_s100, hover_btn == 10);
            render_choice_pill(mem_dc, 190, height - 90, 86, 26, "115% 适中", is_s115, hover_btn == 11);
            render_choice_pill(mem_dc, 282, height - 90, 86, 26, "130% 放大", is_s130, hover_btn == 12);
            // Footer Action Buttons (at height - 50)
            render_action_btn(
                mem_dc,
                20,
                height - 48,
                140,
                34,
                "🔄 刷新外设状态",
                hover_btn == 1,
                font_scale,
            );

            let ball_btn_text = if ball_on {
                "🔘 贴边悬浮球: [已开启]"
            } else {
                "⚪ 贴边悬浮球: [已关闭]"
            };

            render_action_btn(
                mem_dc,
                170,
                height - 48,
                185,
                34,
                ball_btn_text,
                hover_btn == 2,
                font_scale,
            );

            // Version info at right
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
            DeleteObject(f_opt);
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
            let height = rect.bottom - rect.top;

            let mut new_hover = 0;
            // Polling pills: height - 124 .. height - 98
            if y >= height - 124 && y <= height - 98 {
                if x >= 98 && x <= 174 {
                    new_hover = 20;
                } else if x >= 180 && x <= 256 {
                    new_hover = 21;
                } else if x >= 262 && x <= 344 {
                    new_hover = 22;
                } else if x >= 350 && x <= 444 {
                    new_hover = 23;
                }
            } else if y >= height - 90 && y <= height - 64 {
                if x >= 98 && x <= 184 {
                    new_hover = 10;
                } else if x >= 190 && x <= 276 {
                    new_hover = 11;
                } else if x >= 282 && x <= 368 {
                    new_hover = 12;
                }
            } else if y >= height - 52 && y <= height - 12 {
                if x >= 20 && x <= 160 {
                    new_hover = 1;
                } else if x >= 170 && x <= 355 {
                    new_hover = 2;
                }
            } else {
                for idx in 0..3 {
                    let card_y = 74 + idx * 114;
                    if y >= card_y + 10 && y <= card_y + 32 {
                        let btn_x = 20 + (rect.right - rect.left - 40) - 230 - 68;
                        if x >= btn_x && x <= btn_x + 58 {
                            new_hover = 100 + idx;
                            break;
                        }
                    }
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
            let height = rect.bottom - rect.top;

            let host = if let Some(state_arc) = &PANEL_STATE {
                state_arc.lock().unwrap().host_hwnd
            } else {
                ptr::null_mut()
            };

            if !host.is_null() {
                if y >= height - 124 && y <= height - 98 {
                    let cmd = if x >= 98 && x <= 174 {
                        crate::ui::tray::IDM_POLL_1500
                    } else if x >= 180 && x <= 256 {
                        crate::ui::tray::IDM_POLL_3000
                    } else if x >= 262 && x <= 344 {
                        crate::ui::tray::IDM_POLL_10000
                    } else if x >= 350 && x <= 444 {
                        crate::ui::tray::IDM_POLL_30000
                    } else {
                        0
                    };
                    if cmd > 0 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            cmd as _,
                            0,
                        );
                    }
                } else if y >= height - 90 && y <= height - 64 {
                    if x >= 98 && x <= 184 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            crate::ui::tray::IDM_FONT_SCALE_100 as _,
                            0,
                        );
                    } else if x >= 190 && x <= 276 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            crate::ui::tray::IDM_FONT_SCALE_115 as _,
                            0,
                        );
                    } else if x >= 282 && x <= 368 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            crate::ui::tray::IDM_FONT_SCALE_130 as _,
                            0,
                        );
                    }
                } else if y >= height - 52 && y <= height - 12 {
                    if x >= 20 && x <= 160 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            crate::ui::tray::IDM_REFRESH as _,
                            0,
                        );
                    } else if x >= 170 && x <= 355 {
                        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                            host,
                            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                            crate::ui::tray::IDM_TOGGLE_BALL as _,
                            0,
                        );
                    }
                } else {
                    let mut clicked_card = None;
                    if let Some(state_arc) = &PANEL_STATE {
                        let state = state_arc.lock().unwrap();
                        for (idx, dev) in state.devices.iter().take(3).enumerate() {
                            let card_y = 74 + idx as i32 * 114;
                            if y >= card_y + 10 && y <= card_y + 32 {
                                let btn_x = 20 + (rect.right - rect.left - 40) - 230 - 68;
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
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
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
    let font_sz_title = (14.5 * font_scale).round() as i32;
    let font_bold = create_clear_font(font_sz_title, true);
    let old_font = SelectObject(hdc, font_bold);
    SetTextColor(hdc, CLR_TEXT);
    let title_full = format!("{} {}", icon, title);
    let mut t_rect = RECT {
        left: x + 16,
        top: y + 10,
        right: x + w - 230 - 75,
        bottom: y + 32,
    };
    draw_text_clean(hdc, &title_full, &mut t_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // Rename Button: [✏️ 改名]
    let btn_x = x + w - 230 - 68;
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

    let font_btn = create_clear_font((10.5 * font_scale).round() as i32, false);
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
    let font_sz_sub = (11.5 * font_scale).round() as i32;
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
        top: y + 32,
        right: x + w - 16,
        bottom: y + 50,
    };
    draw_text_clean(hdc, &meta_str, &mut m_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

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
            (" ⚡[充电中]", CLR_CYAN)
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
    let font_sz_big = (22.0 * font_scale).round() as i32;
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
        left: x + w - 230,
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
            "状态: 处于低功耗省电模式 (触动即醒)".to_string()
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
    draw_text_clean(hdc, &detail_str, &mut d_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

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

    SetTextColor(hdc, text_clr);
    let mut r = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
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

    let font_sz = (12.5 * font_scale).round() as i32;
    let font = create_clear_font(font_sz, false);
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, if hover { CLR_TEXT } else { 0x00D0C0B8 });

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
