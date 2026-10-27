//! 面板上的各类「动作」：切换风格 / 阈值步进 / 打开目录 / 重命名单等写状态与投递消息的入口。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;


pub(crate) unsafe fn open_config_dir() {
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

pub(crate) unsafe fn dismiss_onboarding(host: HWND) {
    if let Some(state_arc) = &PANEL_STATE {
        state_arc.lock().unwrap().show_onboarding = false;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_ONBOARDING, 0, 0);
    }
}

pub(crate) unsafe fn set_panel_density(hwnd: HWND, host: HWND, density: crate::config::UiDensity) {
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
pub(crate) unsafe fn update_ball_opacity_from_x(hwnd: HWND, x: i32) {
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


pub(crate) unsafe fn set_dashboard_preferences(hwnd: HWND, host: HWND, filter: crate::config::DeviceFilter, sort: crate::config::DeviceSort) {
    if let Some(state_arc) = &PANEL_STATE {
        let mut state = state_arc.lock().unwrap();
        state.device_filter = filter;
        state.device_sort = sort;
        let max_scroll = device_max_scroll(hwnd, &state);
        state.scroll_y = state.scroll_y.min(max_scroll);
    }
    if !host.is_null() {
        // 编码值要和主进程的解码顺序一致（见 WM_SAVE_DASHBOARD_PREFS 处理）
        let filter_code = match filter {
            crate::config::DeviceFilter::All => 0,
            crate::config::DeviceFilter::LowBattery => 1,
            crate::config::DeviceFilter::Charging => 2,
            crate::config::DeviceFilter::Bluetooth => 3,
            crate::config::DeviceFilter::Wireless => 4,
        };
        let sort_code = match sort {
            crate::config::DeviceSort::Priority => 0,
            crate::config::DeviceSort::Name => 1,
            crate::config::DeviceSort::Battery => 2,
            crate::config::DeviceSort::Updated => 3,
        };
        PostMessageW(host, WM_SAVE_DASHBOARD_PREFS, (filter_code | (sort_code << 8)) as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

pub(crate) unsafe fn set_ui_style(hwnd: HWND, host: HWND, theme: crate::config::UiStyle) {
    // 自定义风格不是预设：主色与明暗来自用户选择，不能落到 palette() 的兜底
    if matches!(theme, crate::config::UiStyle::Custom) {
        let (accent, light) = if let Some(state_arc) = &PANEL_STATE {
            let mut st = state_arc.lock().unwrap();
            st.ui_style = theme;
            (st.custom_accent, st.custom_light)
        } else {
            (0xE8802A, false)
        };
        crate::ui::theme::apply_custom(accent, light);
        if !host.is_null() {
            PostMessageW(host, WM_SAVE_CUSTOM_THEME, accent as WPARAM, light as LPARAM);
        }
    } else {
        // 令牌层立即换色，本窗口下一帧就是新风格
        crate::ui::theme::apply(theme);
        if let Some(state_arc) = &PANEL_STATE {
            state_arc.lock().unwrap().ui_style = theme;
        }
        if !host.is_null() {
            PostMessageW(host, WM_SAVE_UI_STYLE, theme as WPARAM, 0);
        }
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

pub(crate) unsafe fn dispatch_settings_click(hwnd: HWND, host: HWND, id: i32) {
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
        54 => set_ball_style(hwnd, host, crate::config::FloatingBallStyle::Ring),
        55 => set_ball_style(hwnd, host, crate::config::FloatingBallStyle::Pill),
        56 => set_ball_style(hwnd, host, crate::config::FloatingBallStyle::Badge),
        60 => toggle_state(&|s: &mut PanelStateData| s.enable_toast = !s.enable_toast),
        61 => toggle_state(&|s: &mut PanelStateData| s.toast_gaming_suppression = !s.toast_gaming_suppression),
        62 => toggle_state(&|s: &mut PanelStateData| s.enable_low_battery_sound = !s.enable_low_battery_sound),
        63 => set_alert_sound(hwnd, host, crate::config::AlertSound::Voice),
        64 => set_alert_sound(hwnd, host, crate::config::AlertSound::Chime),
        65 => set_alert_sound(hwnd, host, crate::config::AlertSound::Beep),
        66 => set_alert_sound(hwnd, host, crate::config::AlertSound::Pulse),
        // 四档阈值：− / + 各一个 id，改完统一做递减修正
        70 => step_threshold(hwnd, host, ToastTier::Info, -5),
        71 => step_threshold(hwnd, host, ToastTier::Info, 5),
        72 => step_threshold(hwnd, host, ToastTier::Warn, -5),
        73 => step_threshold(hwnd, host, ToastTier::Warn, 5),
        74 => step_threshold(hwnd, host, ToastTier::Crit, -1),
        75 => step_threshold(hwnd, host, ToastTier::Crit, 1),
        76 => step_threshold(hwnd, host, ToastTier::Second, -1),
        77 => step_threshold(hwnd, host, ToastTier::Second, 1),
        80 => crate::ui::toast::ToastNotification::show(
            "罗技 MX Master 3S",
            "电量已降至 28% (警告通知测试)",
            crate::ui::toast::ToastLevel::Warning,
            28,
        ),
        81 => {
            let sound = PANEL_STATE
                .as_ref()
                .map(|sa| sa.lock().unwrap().alert_sound)
                .unwrap_or(crate::config::AlertSound::Voice);
            crate::sound::play_alert(sound);
        }
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
        // 界面风格：按 `theme::ALL_STYLES` 顺序编号，最后一位是「自定义」
        id if (settings::STYLE_ID_BASE..settings::STYLE_ID_BASE + 6).contains(&id) => {
            let idx = (id - settings::STYLE_ID_BASE) as usize;
            let style = crate::ui::theme::ALL_STYLES
                .get(idx)
                .copied()
                .unwrap_or(crate::config::UiStyle::Custom);
            set_ui_style(hwnd, host, style);
        }
        // 自定义配色：200..212 是 12 个候选主色，220/221 是明暗
        id if (200..212).contains(&id) => {
            let idx = (id - 200) as usize;
            if let Some((colour, _)) = crate::ui::theme::ACCENT_CHOICES.get(idx) {
                set_custom_palette(hwnd, host, Some(*colour), None);
            }
        }
        220 => set_custom_palette(hwnd, host, None, Some(false)),
        221 => set_custom_palette(hwnd, host, None, Some(true)),
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

/// 提示音换档：状态里记一份供试听用，落盘交给宿主。
pub(crate) unsafe fn set_alert_sound(hwnd: HWND, host: HWND, sound: crate::config::AlertSound) {
    let changed = if let Some(state_arc) = &PANEL_STATE {
        let mut st = state_arc.lock().unwrap();
        let changed = st.alert_sound != sound;
        st.alert_sound = sound;
        changed
    } else {
        false
    };
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
    }
    // 选完立即试听：让用户当场听到自己选的是什么
    if changed {
        crate::sound::play_alert(sound);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

/// 阈值档位（四档递减）
#[derive(Clone, Copy)]
pub(crate) enum ToastTier {
    Info,
    Warn,
    Crit,
    Second,
}

pub(crate) unsafe fn step_threshold(hwnd: HWND, host: HWND, tier: ToastTier, delta: i32) {
    if let Some(state_arc) = &PANEL_STATE {
        let mut st = state_arc.lock().unwrap();
        match tier {
            ToastTier::Info => st.toast_info = (st.toast_info as i32 + delta).clamp(10, 90) as u8,
            ToastTier::Warn => st.toast_warning = (st.toast_warning as i32 + delta).clamp(5, 90) as u8,
            ToastTier::Crit => st.toast_critical = (st.toast_critical as i32 + delta).clamp(1, 90) as u8,
            ToastTier::Second => {
                let v = st.toast_second as i32 + delta;
                // 0 = 关闭二次提醒，减到 0 就停住
                st.toast_second = if v <= 0 { 0 } else { v.min(60) as u8 };
            }
        }
        let (mut info, mut warn, mut crit, mut second) = (
            st.toast_info,
            st.toast_warning,
            st.toast_critical,
            st.toast_second,
        );
        crate::config::normalize_thresholds(&mut info, &mut warn, &mut crit, &mut second);
        st.toast_info = info;
        st.toast_warning = warn;
        st.toast_critical = crit;
        st.toast_second = second;
    }
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_TOAST_SETTINGS, 0, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

/// 自定义配色：改主色或明暗任一维度，都会把风格切到 `Custom` 并立刻换色。
pub(crate) unsafe fn set_custom_palette(hwnd: HWND, host: HWND, accent: Option<u32>, light: Option<bool>) {
    let (accent, light) = if let Some(state_arc) = &PANEL_STATE {
        let mut st = state_arc.lock().unwrap();
        if let Some(c) = accent {
            st.custom_accent = c;
        }
        if let Some(l) = light {
            st.custom_light = l;
        }
        st.ui_style = crate::config::UiStyle::Custom;
        (st.custom_accent, st.custom_light)
    } else {
        (accent.unwrap_or(0xE8802A), light.unwrap_or(false))
    };
    crate::ui::theme::apply_custom(accent, light);
    if !host.is_null() {
        PostMessageW(host, WM_SAVE_CUSTOM_THEME, accent as WPARAM, light as LPARAM);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

pub(crate) unsafe fn set_ball_style(hwnd: HWND, host: HWND, style: crate::config::FloatingBallStyle) {
    if let Some(state_arc) = &PANEL_STATE {
        state_arc.lock().unwrap().ball_style = style;
    }
    if !host.is_null() {
        let code = match style {
            crate::config::FloatingBallStyle::Tab => 0,
            crate::config::FloatingBallStyle::Orb => 1,
            crate::config::FloatingBallStyle::Ring => 2,
            crate::config::FloatingBallStyle::Pill => 3,
            crate::config::FloatingBallStyle::Badge => 4,
        };
        PostMessageW(host, WM_SET_BALL_STYLE, code as WPARAM, 0);
    }
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
}

pub(crate) unsafe fn set_dashboard_filter(hwnd: HWND, host: HWND, filter: crate::config::DeviceFilter) {
    let sort = PANEL_STATE
        .as_ref()
        .and_then(|sa| sa.lock().ok().map(|s| s.device_sort))
        .unwrap_or(crate::config::DeviceSort::Priority);
    set_dashboard_preferences(hwnd, host, filter, sort);
}

pub(crate) unsafe fn set_dashboard_sort(hwnd: HWND, host: HWND, sort: crate::config::DeviceSort) {
    let filter = PANEL_STATE
        .as_ref()
        .and_then(|sa| sa.lock().ok().map(|s| s.device_filter))
        .unwrap_or(crate::config::DeviceFilter::All);
    set_dashboard_preferences(hwnd, host, filter, sort);
}



