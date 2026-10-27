#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::config::FloatingBallStyle;
use crate::ui::drawer;
use crate::ui::theme::THEME;
use crate::devices::types::{DeviceKind, DeviceSnapshot};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, RoundRect,
    SelectObject, SetBkMode, SetTextColor, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
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

mod input;
mod paint;
mod pixels;
mod widgets;
mod wndproc;

pub(crate) use wndproc::*;

pub const BALL_SIZE: i32 = 48;
/// 窗口高度取收纳态拉手的高度，悬停时的完整球体居中绘制在同一个窗口里。
pub const DOCK_H: i32 = 64;
/// 贴边拉手宽度：右侧贴屏幕，左侧两个角做圆角。
const TAB_W: i32 = 26;
const TAB_RADIUS: f32 = 9.0;
/// 竖向胶囊宽度
const PILL_W: i32 = 32;
/// 小徽章的外径
const BADGE_SIZE: i32 = 36;
const CLR_COLORKEY: COLORREF = 0x00FF00FF; // Magenta colorkey for transparent edges
const TIMER_BALL_ANIM: usize = 1001;

/// 各形态的窗口尺寸。
///
/// 窗口 = 可见图形，不留透明边：分层窗口的 colorkey 区域依然参与命中测试，
/// 留白会让「点在空气上也展开抽屉」这种怪事发生。
pub fn dock_size(style: FloatingBallStyle) -> (i32, i32) {
    match style {
        FloatingBallStyle::Tab | FloatingBallStyle::Orb | FloatingBallStyle::Ring => {
            (BALL_SIZE, DOCK_H)
        }
        FloatingBallStyle::Pill => (PILL_W, DOCK_H),
        FloatingBallStyle::Badge => (BADGE_SIZE, BADGE_SIZE),
    }
}

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

// 颜色取自设计令牌层（src/ui/theme.rs），本文件不再定义色值。

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
    /// 停靠形态由用户选择，悬停不再切换形状（切换会导致鼠标反复进出窗口而闪烁）
    pub style: FloatingBallStyle,
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
        style: FloatingBallStyle,
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
            let (dock_w, dock_h) = dock_size(style);
            let start_x = screen_w - dock_w;

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
                style,
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
                dock_w,
                dock_h,
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
                    let exp_w = drawer::drawer_width(scale);
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        screen_w - exp_w,
                        cur_y,
                        exp_w,
                        drawer::DRAWER_HEIGHT,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, ptr::null(), 0);
        }
    }

    /// 切换停靠形态。不同形态的可见尺寸不同，收纳态需要连窗口一起改大小，
    /// 否则会留下可以点到的透明边。
    pub fn set_style(&self, style: FloatingBallStyle) {
        unsafe {
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.style = style;
                if state.enabled && !state.expanded {
                    let (w, h) = dock_size(style);
                    let cur_y = state.cur_y;
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        screen_w - w,
                        cur_y,
                        w,
                        h,
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
                    let (dock_w, dock_h) = dock_size(state.style);
                    SetWindowPos(
                        self.hwnd,
                        ptr::null_mut(),
                        screen_w - dock_w,
                        cur_y,
                        dock_w,
                        dock_h,
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


#[cfg(test)]
mod tests {
    use super::*;
    // 五种形态的渲染函数在 widgets 里；测试要逐个形态跑渲染，显式取用
    use super::widgets::*;
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



    /// 渲染到离屏 DIB 并返回像素，用于形状级断言。
    #[cfg(test)]
    fn render_to_pixels(hover: bool, style: FloatingBallStyle) -> Vec<u32> {
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        unsafe {
            let (w, h) = dock_size(style);
            let mut bmi: BITMAPINFO = mem::zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            bmi.bmiHeader.biHeight = -h;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB as u32;
            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let bmp = CreateDIBSection(ptr::null_mut(), &bmi, DIB_RGB_COLORS as u32, &mut bits, ptr::null_mut(), 0);
            let dc = CreateCompatibleDC(ptr::null_mut());
            let old = SelectObject(dc, bmp);
            SetBkMode(dc, TRANSPARENT as i32);
            let dev = create_test_dev("kbd", "RK108", DeviceKind::Keyboard, Some(72), false, true);
            let dev2 = create_test_dev("mouse", "X2", DeviceKind::Mouse, Some(45), false, true);
            let list = [dev, dev2];
            match style {
                FloatingBallStyle::Tab => render_tab_widget(dc, w, h, &list, 0, 0.0, 1.15, hover),
                FloatingBallStyle::Orb => render_ball_widget(dc, w, h, &list, 0, 0.0, 1.15),
                FloatingBallStyle::Ring => render_ring_widget(dc, w, h, &list, 0, 0.0, 1.15),
                FloatingBallStyle::Pill => render_pill_widget(dc, w, h, &list, 0, 0.0, 1.15),
                FloatingBallStyle::Badge => render_badge_widget(dc, w, h, &list, 0, 0.0, 1.15),
            }
            let px = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize).to_vec();
            SelectObject(dc, old);
            DeleteObject(bmp);
            DeleteDC(dc);
            px
        }
    }

    #[test]
    fn hover_never_changes_the_painted_footprint() {
        // 悬停只允许改配色：一旦形状变化，处于两个形状差集里的光标会反复进出窗口 → 闪烁。
        for style in [
            FloatingBallStyle::Tab,
            FloatingBallStyle::Orb,
            FloatingBallStyle::Ring,
            FloatingBallStyle::Pill,
            FloatingBallStyle::Badge,
        ] {
            let calm = render_to_pixels(false, style);
            let hovered = render_to_pixels(true, style);
            let mask = |px: &[u32]| -> Vec<bool> { px.iter().map(|p| *p != CLR_COLORKEY).collect() };
            assert_eq!(mask(&calm), mask(&hovered), "hover changed the painted shape of {style:?}");
            assert!(calm.iter().any(|p| *p != CLR_COLORKEY), "widget painted nothing");
        }
    }

    /// 五种形态必须真的长得不一样：可见像素占比各不相同，且都不是满屏（留白=透明区）。
    #[test]
    fn every_style_paints_a_distinct_silhouette() {
        let mut ratios: Vec<f32> = Vec::new();
        for style in [
            FloatingBallStyle::Tab,
            FloatingBallStyle::Orb,
            FloatingBallStyle::Ring,
            FloatingBallStyle::Pill,
            FloatingBallStyle::Badge,
        ] {
            let px = render_to_pixels(false, style);
            let ink = px.iter().filter(|p| **p != CLR_COLORKEY).count() as f32;
            let ratio = ink / px.len() as f32;
            assert!(ratio > 0.15, "{style:?} 几乎什么都没画 ({ratio})");
            assert!(ratio < 0.98, "{style:?} 把整个窗口涂满了 ({ratio})");
            ratios.push(ratio);
        }
        // 任意两种形态的墨迹占比都不该几乎相同
        for (i, a) in ratios.iter().enumerate() {
            for b in ratios.iter().skip(i + 1) {
                assert!((a - b).abs() > 0.01, "两种形态视觉上无法区分: {a} vs {b}");
            }
        }
    }

    /// 每个形态的窗口尺寸都必须等于可见图形，否则透明区域也会吃点击。
    #[test]
    fn dock_size_matches_the_visible_shape() {
        for style in [
            FloatingBallStyle::Tab,
            FloatingBallStyle::Orb,
            FloatingBallStyle::Ring,
            FloatingBallStyle::Pill,
            FloatingBallStyle::Badge,
        ] {
            let (w, h) = dock_size(style);
            assert!(w > 0 && h > 0, "{style:?} 尺寸非法");
            assert!(w <= BALL_SIZE && h <= DOCK_H, "{style:?} 超出绘制缓冲上限");
            let px = render_to_pixels(false, style);
            assert_eq!(px.len(), (w * h) as usize);
        }
        // 徽章最小、拉手最窄
        assert!(dock_size(FloatingBallStyle::Badge).0 < dock_size(FloatingBallStyle::Orb).0);
        assert!(dock_size(FloatingBallStyle::Pill).0 < dock_size(FloatingBallStyle::Tab).0);
    }
}
