//! 面板的派生状态：筛选排序、滚动上限、设置页快照。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;


pub(crate) fn prepare_devices(
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
            // 排序分层：欠电最前 → 充电中 → 其它 → 离线，同层按名字
            let key = |d: &DeviceSnapshot| {
                let tier = if !d.battery.online {
                    3
                } else if d.battery.percentage.map(|p| p < 30).unwrap_or(false) {
                    0
                } else if d.battery.charging {
                    1
                } else {
                    2
                };
                (tier, d.name.to_lowercase())
            };
            key(a).cmp(&key(b))
        }
    });
    devices
}

/// Maximum device-tab scroll offset. Uses the filtered list so it always
/// matches what is actually painted.
pub(crate) unsafe fn device_max_scroll(hwnd: HWND, state: &PanelStateData) -> i32 {
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let viewport = dashboard::viewport_height(client.bottom - client.top);
    let step = dashboard::card_geometry(state.ui_density).1;
    let count = prepare_devices(state.devices.clone(), state.device_filter, state.device_sort).len();
    ((count as i32 * step) - viewport).max(0)
}

/// Settings content is a fixed-height flow; viewport excludes header and footer.
/// 自绘滚动条：轨道 + 滑块，设备页与设置页共用同一套几何。
pub(crate) unsafe fn draw_scroll_thumb(
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
    let track_brush = CreateSolidBrush(THEME.border);
    let track = RECT { left: track_x, top, right: track_x + 4, bottom };
    FillRect(hdc, &track, track_brush);
    DeleteObject(track_brush);

    let thumb_h = ((viewport_h * viewport_h) / content_h).max(24);
    let max_thumb_y = viewport_h - thumb_h;
    let max_scroll = (content_h - viewport_h).max(1);
    let thumb_y = top + ((scroll_y * max_thumb_y) / max_scroll);
    let thumb_brush = CreateSolidBrush(THEME.accent);
    let thumb = RECT { left: track_x - 1, top: thumb_y, right: track_x + 5, bottom: thumb_y + thumb_h };
    FillRect(hdc, &thumb, thumb_brush);
    DeleteObject(thumb_brush);
}

pub(crate) unsafe fn settings_max_scroll(hwnd: HWND, state: &PanelStateData) -> i32 {
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let width = client.right - client.left;
    let viewport = (client.bottom - client.top - dashboard::FOOTER_H - settings::CONTENT_TOP).max(1);
    let content = settings::layout(&settings_ctx(state), width).content_h;
    (content - viewport).max(0)
}

pub(crate) fn update_status_line(state: &crate::update::UpdateState) -> String {
    let current = crate::update::current_version();
    if state.applying {
        return "正在下载并替换程序，完成后会自动重启…".to_string();
    }
    if state.checking {
        return "正在检查 GitHub Releases…".to_string();
    }
    if let Some(err) = &state.error {
        return format!("更新失败: {}（当前 v{}）", err, current);
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
/// 设置页命中结果
pub(crate) enum SettingsHit {
    /// 左侧导航分组
    Nav(usize),
    /// 底栏「返回设备监控」
    Back,
    /// 卡片内的控件 id
    Control(i32),
}

/// 设置页命中测试，hover 与点击共用（绘制 / hover / 点击三处同源）。
///
/// 顺序与裁剪都必须和绘制一致：
/// 1. 导航条固定在 `CONTENT_TOP` 之上，不随滚动移动；
/// 2. 返回按钮在滚动区**之外**，必须先于内容判定 —— 否则视口外的控件会吃掉它的点击；
/// 3. 内容只在裁剪区 `[CONTENT_TOP, height - FOOTER_H)` 内可点，与 `SelectClipRgn` 的范围一致。
pub(crate) fn settings_hit_at(
    page: &settings::Page,
    back_btn: &dashboard::Rect2,
    height: i32,
    scroll_y: i32,
    x: i32,
    y: i32,
) -> Option<SettingsHit> {
    if let Some(g) = settings::nav_hit(page, x, y) {
        return Some(SettingsHit::Nav(g));
    }
    if back_btn.contains(x, y) {
        return Some(SettingsHit::Back);
    }
    let view_h = (height - dashboard::FOOTER_H - settings::CONTENT_TOP).max(1);
    settings::hit_test_at(page, x, y, scroll_y, settings::CONTENT_TOP, view_h).map(SettingsHit::Control)
}

pub(crate) fn settings_ctx(state: &PanelStateData) -> settings::Ctx {
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
        toast_second: state.toast_second,
        toast_gaming: state.toast_gaming_suppression,
        toast_sound: state.enable_low_battery_sound,
        alert_sound: state.alert_sound,
        ui_density: state.ui_density,
        device_filter: state.device_filter,
        device_sort: state.device_sort,
        ui_style: state.ui_style,
        custom_accent: state.custom_accent,
        custom_light: state.custom_light,
        active_group: state.settings_group.min(settings::GROUP_COUNT - 1),
        auto_update: state.auto_update,
        auto_start: state.auto_start,
        update_line: update_status_line(&upd),
        update_available: latest.is_some(),
        update_applying: upd.applying,
        update_checking: upd.checking,
        latest_version: latest,
        config_path: crate::config::AppConfig::config_path().display().to_string(),
    }
}

pub(crate) fn fallback_settings_ctx() -> settings::Ctx {
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
        toast_second: config.toast_threshold_second,
        toast_gaming: config.toast_gaming_suppression,
        toast_sound: config.enable_low_battery_sound,
        alert_sound: config.alert_sound,
        ui_density: config.ui_density,
        device_filter: config.device_filter,
        device_sort: config.device_sort,
        ui_style: config.ui_style,
        custom_accent: config.custom_accent,
        custom_light: config.custom_light,
        active_group: 0,
        auto_update: config.check_updates,
        auto_start: false,
        update_line: "当前版本 · 可手动检查更新".to_string(),
        update_available: false,
        update_applying: false,
        update_checking: false,
        latest_version: None,
        config_path: crate::config::AppConfig::config_path().display().to_string(),
    }
}
