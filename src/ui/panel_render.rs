#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::DeviceSnapshot;
use super::panel::*;
use windows_sys::Win32::Foundation::{COLORREF, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, FillRect, HDC, RoundRect, SelectObject, SetTextColor,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, PS_SOLID,
};

pub(crate) unsafe fn render_choice_pill(hdc: HDC, x: i32, y: i32, w: i32, h: i32, text: &str, active: bool, hover: bool, font_scale: f32) {
    let (bg, border_clr, text_clr) = if active {
        (THEME.btn_active, THEME.accent, THEME.text)
    } else if hover {
        (THEME.btn_hover, THEME.muted, THEME.text)
    } else {
        (THEME.btn, THEME.border, THEME.muted)
    };
    let brush = CreateSolidBrush(bg);
    let pen = CreatePen(PS_SOLID, 1, border_clr);
    let op = SelectObject(hdc, pen);
    let ob = SelectObject(hdc, brush);
    RoundRect(hdc, x, y, x + w, y + h, 6, 6);
    SelectObject(hdc, ob); DeleteObject(brush); SelectObject(hdc, op); DeleteObject(pen);
    let font_pill = create_clear_font((12.0 * font_scale).round() as i32, active);
    let old_font = SelectObject(hdc, font_pill);
    SetTextColor(hdc, text_clr);
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old_font); DeleteObject(font_pill);
}

/// 页面切换：连体分段控件。
///
/// 两个独立 pill 并排会被读成四个按钮，合成一个槽、中间只留一条发丝线，
/// 选中段整块填强调色，才能一眼看出「二选一」。
pub(crate) unsafe fn render_segmented(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    items: &[(&str, bool)],
    hover: Option<usize>,
    font_scale: f32,
) {
    let n = items.len().max(1) as i32;
    let seg_w = w / n;

    let slot = CreateSolidBrush(THEME.btn);
    let pen = CreatePen(PS_SOLID, 1, THEME.border);
    let op = SelectObject(hdc, pen);
    let ob = SelectObject(hdc, slot);
    RoundRect(hdc, x, y, x + w, y + h, 8, 8);
    SelectObject(hdc, ob);
    DeleteObject(slot);
    SelectObject(hdc, op);
    DeleteObject(pen);

    for (i, (label, active)) in items.iter().enumerate() {
        let sx = x + i as i32 * seg_w;
        let hovered = hover == Some(i);
        if *active || hovered {
            let bg = if *active { THEME.accent } else { THEME.btn_hover };
            let brush = CreateSolidBrush(bg);
            let pen = CreatePen(PS_SOLID, 1, if *active { THEME.accent } else { THEME.border });
            let op = SelectObject(hdc, pen);
            let ob = SelectObject(hdc, brush);
            let right = if i as i32 == n - 1 { sx + seg_w - 1 } else { sx + seg_w };
            RoundRect(hdc, sx + 1, y + 1, right, y + h - 1, 7, 7);
            SelectObject(hdc, ob);
            DeleteObject(brush);
            SelectObject(hdc, op);
            DeleteObject(pen);
        }
        let f = create_clear_font((12.5 * font_scale).round() as i32, *active);
        let old_font = SelectObject(hdc, f);
        SetTextColor(hdc, if *active { THEME.accent_text } else if hovered { THEME.text } else { THEME.muted });
        let mut r = RECT { left: sx, top: y, right: sx + seg_w, bottom: y + h };
        draw_text_clean(hdc, label, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        SelectObject(hdc, old_font);
        DeleteObject(f);
    }
}

pub(crate) unsafe fn render_action_btn(hdc: HDC, x: i32, y: i32, w: i32, h: i32, text: &str, hover: bool, font_scale: f32) {
    let brush = CreateSolidBrush(if hover { THEME.btn_hover } else { THEME.btn });
    let border = CreatePen(PS_SOLID, 1, if hover { THEME.accent } else { THEME.border });
    let old_pen = SelectObject(hdc, border); let old_brush = SelectObject(hdc, brush);
    RoundRect(hdc, x, y, x + w, y + h, 8, 8);
    SelectObject(hdc, old_brush); DeleteObject(brush); SelectObject(hdc, old_pen); DeleteObject(border);
    let font = create_clear_font((13.0 * font_scale).round() as i32, true);
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, if hover { THEME.text } else { THEME.btn_text });
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old_font); DeleteObject(font);
}
/// 瓦片里大号电量数字的字号基准（比 `fs_value` 更突出，电量是这个页面的主角）
const TILE_PCT_FS: f32 = 30.0;

/// 电量状态 → (颜色, 右上角状态词, 大号主文本, 电量百分比)
unsafe fn status_of(dev: Option<&DeviceSnapshot>) -> (COLORREF, &'static str, String, Option<u8>) {
    let Some(d) = dev else {
        return (THEME.muted, "未连接", "—".to_string(), None);
    };
    let pct = d.battery.percentage;
    let charging = d.battery.charging || d.battery.charging_source.is_charging();
    if charging {
        (THEME.charging, "充电中", format!("{}%", pct.unwrap_or(100)), Some(pct.unwrap_or(100)))
    } else if !d.battery.online {
        (THEME.muted, "待机", pct.map(|p| format!("{}%", p)).unwrap_or_else(|| "—".to_string()), pct)
    } else if let Some(p) = pct {
        let c = if p >= 50 { THEME.ok } else if p >= 20 { THEME.warn } else { THEME.danger };
        let word = if p >= 50 { "良好" } else if p >= 20 { "偏低" } else { "需充电" };
        (c, word, format!("{}%", p), Some(p))
    } else {
        (THEME.ok, "在线", "在线".to_string(), None)
    }
}

/// 设备瓦片（网格布局）。几何全部由 `dashboard::layout()` 给出，本函数只负责画：
/// 绘制与命中测试必须读同一份坐标，否则会出现「看得见点不着」。
///
/// 信息层级自上而下：设备名 → 大号电量 → 电量条 → 连接/电压。
/// 复制 / 改名收在底部一行，常态弱化、hover 才提亮，避免抢走电量的视觉重心。
pub(crate) unsafe fn render_device_tile(
    hdc: HDC,
    t: &crate::ui::dashboard::TileBox,
    title: &str,
    icon: &str,
    dev: Option<&DeviceSnapshot>,
    font_scale: f32,
    hover_card: bool,
    hover_copy: bool,
    hover_edit: bool,
    selected: bool,
) {
    let m = crate::ui::theme::METRICS;
    let (status_color, chip_text, big_text, pct) = status_of(dev);

    // 瓦片底与描边：选中用强调色加粗描边，hover 提亮底色
    let bg = if hover_card { THEME.btn } else { THEME.card };
    let border_clr = if selected {
        THEME.accent
    } else if hover_card {
        THEME.muted
    } else {
        THEME.border
    };
    let border_pen = CreatePen(PS_SOLID, if selected { 2 } else { 1 }, border_clr);
    let card_brush = CreateSolidBrush(bg);
    let old_pen = SelectObject(hdc, border_pen);
    let old_brush = SelectObject(hdc, card_brush);
    RoundRect(hdc, t.rect.x, t.rect.y, t.rect.right(), t.rect.bottom(), m.r_card, m.r_card);
    SelectObject(hdc, old_brush); DeleteObject(card_brush);
    SelectObject(hdc, old_pen); DeleteObject(border_pen);

    // 设备名
    let f_name = create_clear_font((m.fs_title * font_scale).round() as i32, true);
    let old_font = SelectObject(hdc, f_name);
    SetTextColor(hdc, THEME.text);
    let mut r = RECT { left: t.name.x, top: t.name.y, right: t.name.right(), bottom: t.name.bottom() };
    draw_text_clean(hdc, &format!("{} {}", icon, title), &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // 右上角状态词
    let f_chip = create_clear_font((m.fs_meta * font_scale).round() as i32, true);
    SelectObject(hdc, f_chip);
    SetTextColor(hdc, status_color);
    let mut r = RECT { left: t.chip.x, top: t.chip.y, right: t.chip.right(), bottom: t.chip.bottom() };
    draw_text_clean(hdc, chip_text, &mut r, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

    // 大号电量
    let f_big = create_clear_font((TILE_PCT_FS * font_scale).round() as i32, true);
    SelectObject(hdc, f_big);
    SetTextColor(hdc, status_color);
    let mut r = RECT { left: t.pct.x, top: t.pct.y, right: t.pct.right(), bottom: t.pct.bottom() };
    draw_text_clean(hdc, &big_text, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // 电量条
    let r = RECT { left: t.bar.x, top: t.bar.y, right: t.bar.right(), bottom: t.bar.bottom() };
    let track_brush = CreateSolidBrush(THEME.track);
    RoundRect(hdc, t.bar.x, t.bar.y, t.bar.right(), t.bar.bottom(), m.r_bar, m.r_bar);
    FillRect(hdc, &r, track_brush);
    DeleteObject(track_brush);
    let fill_w = (t.bar.w * pct.unwrap_or(0) as i32) / 100;
    if fill_w > 0 {
        let fg = CreateSolidBrush(status_color);
        let fr = RECT { left: t.bar.x, top: t.bar.y, right: t.bar.x + fill_w, bottom: t.bar.bottom() };
        RoundRect(hdc, t.bar.x, t.bar.y, t.bar.x + fill_w, t.bar.bottom(), m.r_bar, m.r_bar);
        FillRect(hdc, &fr, fg);
        DeleteObject(fg);
    }

    // 连接 / 电压
    let f_meta = create_clear_font((m.fs_meta * font_scale).round() as i32, false);
    SelectObject(hdc, f_meta);
    SetTextColor(hdc, THEME.muted);
    let meta_str = match dev {
        Some(d) => match d.battery.voltage_mv {
            Some(v) => format!("{} · {:.3} V", d.connection.as_str(), v as f32 / 1000.0),
            None => d.connection.as_str().to_string(),
        },
        None => "等待接入".to_string(),
    };
    let mut r = RECT { left: t.meta.x, top: t.meta.y, right: t.meta.right(), bottom: t.meta.bottom() };
    draw_text_clean(hdc, &meta_str, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // 复制 / 改名
    for (btn, hover, label) in [(t.copy, hover_copy, "复制"), (t.edit, hover_edit, "改名")] {
        let brush = CreateSolidBrush(if hover { THEME.btn_hover } else { THEME.btn });
        let pen = CreatePen(PS_SOLID, 1, if hover { THEME.accent } else { THEME.border });
        let op = SelectObject(hdc, pen);
        let ob = SelectObject(hdc, brush);
        RoundRect(hdc, btn.x, btn.y, btn.right(), btn.bottom(), m.r_pill, m.r_pill);
        SelectObject(hdc, ob); DeleteObject(brush);
        SelectObject(hdc, op); DeleteObject(pen);
        SelectObject(hdc, f_meta);
        SetTextColor(hdc, if hover { THEME.text } else { THEME.muted });
        let mut br = RECT { left: btn.x, top: btn.y, right: btn.right(), bottom: btn.bottom() };
        draw_text_clean(hdc, label, &mut br, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }

    SelectObject(hdc, old_font);
    DeleteObject(f_name); DeleteObject(f_chip); DeleteObject(f_big); DeleteObject(f_meta);
}
