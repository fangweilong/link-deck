#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

//! 悬浮球展开抽屉的布局。
//!
//! 旧实现把抽屉坐标散在 `render_expanded_drawer` / `render_device_row` 与
//! `WM_MOUSEMOVE` 的 hover 判定里（`exp_w - 30`、`EXPANDED_HEIGHT - 34` 这类
//! 相对窗口尺寸的推算），窗口高度 205 也是写死的，与内容无关。
//! 这里改为 `layout()` 产出带坐标的布局树，绘制与命中读同一份结果，
//! 窗口尺寸由 `size()` 从内容反推。

use crate::devices::types::{DeviceKind, DeviceSnapshot};
use crate::ui::panel::{create_clear_font, draw_text_clean};
pub use crate::ui::layout::Rect2;
use crate::ui::layout::to_rect;
use crate::ui::theme::{METRICS, THEME};
use windows_sys::Win32::Foundation::COLORREF;
use windows_sys::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, FillRect, RoundRect, SelectObject, SetBkMode,
    SetTextColor, HDC, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    PS_SOLID, TRANSPARENT,
};

/// 抽屉最多显示的设备行数（超出部分不画，窗口高度不随之增长）
pub const MAX_ROWS: usize = 3;

/// 抽屉外留白
const PAD: i32 = 12;
/// 标题行
const TITLE_H: i32 = 20;
const TITLE_TOP: i32 = 8;
/// 标题与设备行之间
const GAP_AFTER_TITLE: i32 = 6;
/// 单行设备
const ROW_H: i32 = 38;
const ROW_GAP: i32 = 4;
/// 行区与底栏按钮之间
const GAP_BEFORE_ACTION: i32 = 8;
/// 底栏按钮
const ACTION_H: i32 = 28;
/// 右上角收起按钮
const COLLAPSE_W: i32 = 24;
/// 行内迷你电量条
const BAR_H: i32 = 4;
/// 行右侧电量文字预留宽度
const PCT_W: i32 = 104;
/// 行内左右留白
const ROW_PAD: i32 = 6;

/// 行步进 = 行高 + 行间距
const ROW_STEP: i32 = ROW_H + ROW_GAP;

/// 行区顶部（标题下方）
const ROWS_TOP: i32 = TITLE_TOP + TITLE_H + GAP_AFTER_TITLE;

/// 抽屉高度：留白 + 标题 + 行区 + 底栏按钮 + 留白，全部按 4 对齐。
pub const DRAWER_HEIGHT: i32 = ROWS_TOP + MAX_ROWS as i32 * ROW_STEP - ROW_GAP
    + GAP_BEFORE_ACTION
    + ACTION_H
    + PAD;

/// 抽屉宽度三档（4 的倍数），随字号放大保证长设备名不被截断。
pub fn drawer_width(font_scale: f32) -> i32 {
    let w = if font_scale >= 1.25 {
        268
    } else if font_scale >= 1.10 {
        248
    } else {
        232
    };
    debug_assert_eq!(w % 4, 0);
    w
}

/// 抽屉窗口尺寸（宽随字号，高固定按 3 行内容算）
pub fn size(font_scale: f32) -> (i32, i32) {
    (drawer_width(font_scale), DRAWER_HEIGHT)
}


/// 一行设备：行底 + 左侧名称 + 右侧电量文字 + 底部电量条
#[derive(Clone, Copy)]
pub struct DrawerRow {
    pub rect: Rect2,
    pub label: Rect2,
    pub pct: Rect2,
    pub bar: Rect2,
}

/// 抽屉布局树
pub struct DrawerLayout {
    /// 圆角底板（右侧多画 16px，被窗口裁掉形成贴边效果）
    pub surface: Rect2,
    pub title: Rect2,
    pub collapse: Rect2,
    pub rows: Vec<DrawerRow>,
    /// 无设备时的空态文字（占满行区高度）
    pub empty: Option<Rect2>,
    pub action: Rect2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// 右上角收起箭头
    Collapse,
    /// 底栏「打开控制中心」
    Action,
    /// 某一行设备
    Row(usize),
}

/// 计算抽屉布局。`row_count` 取设备数与 `MAX_ROWS` 的较小值；为 0 时产出行区空态。
pub fn layout(w: i32, h: i32, row_count: usize) -> DrawerLayout {
    let rows_n = row_count.min(MAX_ROWS);
    let surface = Rect2::new(0, 0, w, h);

    let collapse = Rect2::new(w - PAD - COLLAPSE_W, TITLE_TOP - 2, COLLAPSE_W, TITLE_H + 4);
    let title = Rect2::new(PAD, TITLE_TOP, collapse.x - PAD - METRICS.gap_tight, TITLE_H);

    let mut rows = Vec::with_capacity(rows_n);
    for i in 0..rows_n {
        let top = ROWS_TOP + i as i32 * ROW_STEP;
        let rect = Rect2::new(PAD - 4, top, w - 2 * (PAD - 4), ROW_H);
        let label = Rect2::new(rect.x + ROW_PAD, rect.y + 2, rect.w - PCT_W - 2 * ROW_PAD, 18);
        let pct = Rect2::new(rect.right() - PCT_W - ROW_PAD, rect.y + 2, PCT_W, 18);
        let bar = Rect2::new(rect.x + ROW_PAD, rect.bottom() - BAR_H - 6, rect.w - 2 * ROW_PAD, BAR_H);
        rows.push(DrawerRow { rect, label, pct, bar });
    }

    let empty = if rows_n == 0 {
        Some(Rect2::new(
            PAD,
            ROWS_TOP + 8,
            w - 2 * PAD,
            MAX_ROWS as i32 * ROW_STEP - ROW_GAP - 16,
        ))
    } else {
        None
    };

    let action = Rect2::new(PAD - 2, h - PAD - ACTION_H, w - 2 * (PAD - 2), ACTION_H);

    DrawerLayout { surface, title, collapse, rows, empty, action }
}

/// 命中测试。绘制、hover、点击三处共用。
pub fn hit_test(lay: &DrawerLayout, px: i32, py: i32) -> Option<Hit> {
    if lay.collapse.contains(px, py) {
        return Some(Hit::Collapse);
    }
    if lay.action.contains(px, py) {
        return Some(Hit::Action);
    }
    for (i, r) in lay.rows.iter().enumerate() {
        if r.rect.contains(px, py) {
            return Some(Hit::Row(i));
        }
    }
    None
}

/// hover 按钮 id：1 = 收起箭头，2 = 打开控制中心（保持与旧 id 兼容）
pub fn hover_id(hit: Option<Hit>) -> i32 {
    match hit {
        Some(Hit::Collapse) => 1,
        Some(Hit::Action) => 2,
        _ => 0,
    }
}

/// 行右侧的电量文字与颜色，供绘制与测试共用。
pub fn row_status(dev: &DeviceSnapshot) -> (COLORREF, String) {
    // THEME 是 static mut（切换风格时整体替换），取值需要 unsafe 块
    let (ok, warn, danger, muted, charging) =
        unsafe { (THEME.ok, THEME.warn, THEME.danger, THEME.muted, THEME.charging) };
    if dev.battery.charging || dev.battery.charging_source.is_charging() {
        (charging, format!("{}% ⚡[充电中]", dev.battery.percentage.unwrap_or(100)))
    } else if let Some(p) = dev.battery.percentage {
        if !dev.battery.online {
            (muted, format!("{}% [待机]", p))
        } else {
            let color = if p >= 50 { ok } else if p >= 20 { warn } else { danger };
            (color, format!("{}%", p))
        }
    } else if dev.battery.online {
        (ok, "在线".to_string())
    } else {
        (muted, "待机".to_string())
    }
}

pub fn kind_icon(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Keyboard => "⌨️",
        DeviceKind::Mouse => "🖱️",
        DeviceKind::Headset => "🎧",
        _ => "📱",
    }
}

/// 超长设备名截断（保留省略号）
pub fn short_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() > max_chars {
        format!("{}...", name.chars().take(max_chars.saturating_sub(3)).collect::<String>())
    } else {
        name.to_string()
    }
}

/// 绘制整个抽屉。坐标全部来自 `lay`。
pub unsafe fn paint(
    hdc: HDC,
    lay: &DrawerLayout,
    devices: &[DeviceSnapshot],
    font_scale: f32,
    hover: i32,
) {
    // 圆角底板，右侧多画 16px 让贴边侧不留圆角
    let border_pen = CreatePen(PS_SOLID, 1, THEME.border);
    let old_pen = SelectObject(hdc, border_pen);
    let card_brush = CreateSolidBrush(THEME.card);
    let old_brush = SelectObject(hdc, card_brush);
    let s = lay.surface;
    RoundRect(hdc, s.x, s.y, s.right() + 16, s.bottom(), 14, 14);
    SelectObject(hdc, old_brush);
    DeleteObject(card_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(border_pen);

    SetBkMode(hdc, TRANSPARENT as i32);

    // 标题
    let f_title = create_clear_font((14.0 * font_scale).round() as i32, true);
    let old_font = SelectObject(hdc, f_title);
    SetTextColor(hdc, THEME.text);
    let mut r = to_rect(lay.title);
    draw_text_clean(hdc, "⚡ 外设电量与充电状态", &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // 收起箭头
    SetTextColor(hdc, if hover == 1 { THEME.accent } else { THEME.muted });
    let mut r = to_rect(lay.collapse);
    draw_text_clean(hdc, "▶", &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    // 设备行
    for (i, row) in lay.rows.iter().enumerate() {
        if let Some(dev) = devices.get(i) {
            paint_row(hdc, row, dev, font_scale);
        }
    }

    // 空态
    if let Some(e) = lay.empty {
        SetTextColor(hdc, THEME.muted);
        let mut r = to_rect(e);
        draw_text_clean(hdc, "正在检测已连接外设...", &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }

    // 底栏按钮
    let a = lay.action;
    let btn_brush = CreateSolidBrush(if hover == 2 { THEME.btn_hover } else { THEME.btn });
    let btn_border = CreatePen(PS_SOLID, 1, if hover == 2 { THEME.accent } else { THEME.border });
    let op = SelectObject(hdc, btn_border);
    let ob = SelectObject(hdc, btn_brush);
    RoundRect(hdc, a.x, a.y, a.right(), a.bottom(), METRICS.r_btn, METRICS.r_btn);
    SelectObject(hdc, ob);
    DeleteObject(btn_brush);
    SelectObject(hdc, op);
    DeleteObject(btn_border);

    let f_btn = create_clear_font((12.0 * font_scale).round() as i32, false);
    SelectObject(hdc, f_btn);
    SetTextColor(hdc, if hover == 2 { THEME.text } else { THEME.muted });
    let mut r = to_rect(a);
    draw_text_clean(hdc, "⚙️ 打开控制中心", &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(f_title);
    DeleteObject(f_btn);
}

unsafe fn paint_row(hdc: HDC, row: &DrawerRow, dev: &DeviceSnapshot, font_scale: f32) {
    let row_brush = CreateSolidBrush(THEME.row);
    let rr = to_rect(row.rect);
    FillRect(hdc, &rr, row_brush);
    DeleteObject(row_brush);

    let f_row = create_clear_font((12.5 * font_scale).round() as i32, false);
    let old_font = SelectObject(hdc, f_row);

    let label = format!("{} {}", kind_icon(dev.kind), short_name(&dev.name, 12));
    SetTextColor(hdc, THEME.text);
    let mut r = to_rect(row.label);
    draw_text_clean(hdc, &label, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    let (clr, text) = row_status(dev);
    SetTextColor(hdc, clr);
    let mut r = to_rect(row.pct);
    draw_text_clean(hdc, &text, &mut r, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(f_row);

    // 迷你电量条
    let b = row.bar;
    let bg = CreateSolidBrush(THEME.track);
    let r = to_rect(b);
    FillRect(hdc, &r, bg);
    DeleteObject(bg);

    let fill_w = (b.w * dev.battery.percentage.unwrap_or(0) as i32) / 100;
    if fill_w > 0 {
        let fg = CreateSolidBrush(clr);
        let r = to_rect(Rect2::new(b.x, b.y, fill_w.min(b.w), b.h));
        FillRect(hdc, &r, fg);
        DeleteObject(fg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode};
    use std::time::SystemTime;

    fn dev(id: &str, name: &str, kind: DeviceKind, pct: Option<u8>, charging: bool, online: bool) -> DeviceSnapshot {
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
                charging_source: if charging { ChargingSource::Charging } else { ChargingSource::None },
                online,
                last_updated: SystemTime::now(),
            },
            path: "test".to_string(),
        }
    }

    fn lay(rows: usize) -> DrawerLayout {
        let (w, h) = size(1.15);
        layout(w, h, rows)
    }

    #[test]
    fn size_is_four_aligned() {
        for s in [1.0f32, 1.15, 1.30] {
            let (w, h) = size(s);
            assert_eq!(w % 4, 0, "width {} not 4-aligned", w);
            assert_eq!(h % 4, 0, "height {} not 4-aligned", h);
        }
    }

    #[test]
    fn rows_never_overlap_and_stay_inside() {
        let (w, h) = size(1.15);
        let l = layout(w, h, MAX_ROWS);
        assert_eq!(l.rows.len(), MAX_ROWS);
        for i in 1..l.rows.len() {
            assert_eq!(l.rows[i].rect.y - l.rows[i - 1].rect.bottom(), ROW_GAP);
        }
        for r in &l.rows {
            assert!(r.rect.x >= 0 && r.rect.right() <= w, "row exceeds drawer width");
            assert!(r.rect.bottom() <= h);
            // 行内元素不得越出行边界
            assert!(r.label.x >= r.rect.x && r.label.right() <= r.rect.right());
            assert!(r.pct.right() <= r.rect.right());
            assert!(r.bar.bottom() <= r.rect.bottom());
        }
    }

    #[test]
    fn label_and_pct_never_overlap() {
        let l = lay(3);
        for r in &l.rows {
            assert!(r.label.right() <= r.pct.x, "label runs into percentage text");
        }
    }

    #[test]
    fn action_sits_below_last_row() {
        let l = lay(3);
        let last = l.rows.last().unwrap().rect;
        assert!(l.action.y >= last.bottom() + GAP_BEFORE_ACTION - ROW_GAP);
        assert!(l.action.bottom() <= size(1.15).1 - PAD + 1);
    }

    #[test]
    fn hit_matches_paint_bounds() {
        let l = lay(3);
        assert_eq!(hit_test(&l, l.collapse.x + 2, l.collapse.y + 2), Some(Hit::Collapse));
        assert_eq!(hit_test(&l, l.action.x + 4, l.action.y + 4), Some(Hit::Action));
        assert_eq!(hit_test(&l, l.rows[1].rect.x + 4, l.rows[1].rect.y + 4), Some(Hit::Row(1)));
        assert_eq!(hit_test(&l, l.title.x + 2, l.title.y + 2), None);
        // 旧实现里底栏只要 y 命中就算，x 不限；现在必须真的落在按钮上
        assert_eq!(hit_test(&l, 2, l.action.y + 2), None);
    }

    #[test]
    fn hover_id_keeps_legacy_values() {
        assert_eq!(hover_id(Some(Hit::Collapse)), 1);
        assert_eq!(hover_id(Some(Hit::Action)), 2);
        assert_eq!(hover_id(None), 0);
    }

    #[test]
    fn empty_state_takes_row_area() {
        let l = lay(0);
        assert!(l.rows.is_empty());
        let e = l.empty.expect("empty rect");
        assert!(e.y >= ROWS_TOP);
        assert!(e.bottom() <= l.action.y);
    }

    #[test]
    fn row_count_caps_at_max() {
        assert_eq!(layout(232, DRAWER_HEIGHT, 10).rows.len(), MAX_ROWS);
    }

    #[test]
    fn status_text_and_color() {
        let th = unsafe { THEME };
        let (c, t) = row_status(&dev("a", "a", DeviceKind::Keyboard, Some(80), false, true));
        assert_eq!(t, "80%");
        assert_eq!(c, th.ok);

        let (c, t) = row_status(&dev("b", "b", DeviceKind::Mouse, Some(25), false, true));
        assert_eq!(t, "25%");
        assert_eq!(c, th.warn);

        let (c, t) = row_status(&dev("c", "c", DeviceKind::Mouse, Some(8), false, true));
        assert_eq!(t, "8%");
        assert_eq!(c, th.danger);

        let (c, t) = row_status(&dev("d", "d", DeviceKind::Mouse, Some(60), false, false));
        assert_eq!(t, "60% [待机]");
        assert_eq!(c, th.muted);

        let (c, t) = row_status(&dev("e", "e", DeviceKind::Mouse, Some(50), true, true));
        assert_eq!(t, "50% ⚡[充电中]");
        assert_eq!(c, th.charging);

        let (_, t) = row_status(&dev("f", "f", DeviceKind::Headset, None, false, true));
        assert_eq!(t, "在线");
        let (_, t) = row_status(&dev("g", "g", DeviceKind::Headset, None, false, false));
        assert_eq!(t, "待机");
    }

    #[test]
    fn long_names_are_truncated() {
        assert_eq!(short_name("RK108", 12), "RK108");
        let long = short_name("Super Long Keyboard Name", 12);
        assert!(long.ends_with("..."));
        assert!(long.chars().count() <= 12);
    }
}
