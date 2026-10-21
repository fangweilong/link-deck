#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

use crate::devices::types::DeviceSnapshot;
use super::panel::*;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{CreatePen, CreateSolidBrush, DeleteObject, FillRect, HDC, RoundRect, SelectObject, SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, PS_SOLID};

pub(super) unsafe fn render_choice_pill(hdc: HDC, x: i32, y: i32, w: i32, h: i32, text: &str, active: bool, hover: bool, font_scale: f32) {
    let (bg, border_clr, text_clr) = if active { (CLR_BTN_ACTIVE, CLR_ACCENT, CLR_TEXT) } else if hover { (CLR_BTN_HOVER, CLR_MUTED, CLR_TEXT) } else { (CLR_BTN_BG, CLR_CARD_BORDER, CLR_MUTED) };
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

pub(super) unsafe fn render_action_btn(hdc: HDC, x: i32, y: i32, w: i32, h: i32, text: &str, hover: bool, font_scale: f32) {
    let brush = CreateSolidBrush(if hover { CLR_BTN_HOVER } else { CLR_BTN_BG });
    let border = CreatePen(PS_SOLID, 1, if hover { CLR_ACCENT } else { CLR_CARD_BORDER });
    let old_pen = SelectObject(hdc, border); let old_brush = SelectObject(hdc, brush);
    RoundRect(hdc, x, y, x + w, y + h, 8, 8);
    SelectObject(hdc, old_brush); DeleteObject(brush); SelectObject(hdc, old_pen); DeleteObject(border);
    let font = create_clear_font((13.0 * font_scale).round() as i32, true);
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, if hover { CLR_TEXT } else { 0x00E0D8D0 });
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    draw_text_clean(hdc, text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old_font); DeleteObject(font);
}
pub(super) unsafe fn render_device_card(
    hdc: HDC, x: i32, y: i32, w: i32, h: i32, title: &str, icon: &str,
    protocol: &str, dev: Option<&DeviceSnapshot>, font_scale: f32,
    hover_edit: bool, hover_copy: bool,
) {
    let border_pen = CreatePen(PS_SOLID, 1, CLR_CARD_BORDER);
    let old_pen = SelectObject(hdc, border_pen);
    let card_brush = CreateSolidBrush(CLR_CARD);
    let old_brush = SelectObject(hdc, card_brush);
    RoundRect(hdc, x, y, x + w, y + h, 12, 12);
    SelectObject(hdc, old_brush); DeleteObject(card_brush); SelectObject(hdc, old_pen); DeleteObject(border_pen);
    let font_bold = create_clear_font((15.0 * font_scale).round() as i32, true);
    let old_font = SelectObject(hdc, font_bold);
    SetTextColor(hdc, CLR_TEXT);
    let title_full = format!("{} {}", icon, title);
    let mut t_rect = RECT { left: x + 16, top: y + 10, right: x + w - 196, bottom: y + 32 };
    draw_text_clean(hdc, &title_full, &mut t_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    let btn_y = y + 11; let btn_w = 58; let btn_h = 20;
    let copy_x = x + w - 254; let edit_x = x + w - 190;
    for (button_x, hover, label) in [(copy_x, hover_copy, "📋 复制"), (edit_x, hover_edit, "✏️ 改名")] {
        let brush = CreateSolidBrush(if hover { CLR_BTN_HOVER } else { CLR_BTN_BG });
        let pen = CreatePen(PS_SOLID, 1, if hover { CLR_ACCENT } else { CLR_CARD_BORDER });
        let old_p = SelectObject(hdc, pen); let old_b = SelectObject(hdc, brush);
        RoundRect(hdc, button_x, btn_y, button_x + btn_w, btn_y + btn_h, 4, 4);
        SelectObject(hdc, old_b); DeleteObject(brush); SelectObject(hdc, old_p); DeleteObject(pen);
        let font_btn = create_clear_font((11.5 * font_scale).round() as i32, false);
        SelectObject(hdc, font_btn); SetTextColor(hdc, if hover { CLR_TEXT } else { CLR_MUTED });
        let mut btn_r = RECT { left: button_x, top: btn_y, right: button_x + btn_w, bottom: btn_y + btn_h };
        draw_text_clean(hdc, label, &mut btn_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        SelectObject(hdc, old_font); DeleteObject(font_btn);
    }
    let font_normal = create_clear_font((12.5 * font_scale).round() as i32, false);
    SelectObject(hdc, font_normal); SetTextColor(hdc, CLR_MUTED);
    let (conn_str, vid_pid) = if let Some(d) = dev { (d.connection.as_str(), if d.vendor_id > 0 { format!("VID 0x{:04X} : PID 0x{:04X}", d.vendor_id, d.product_id) } else { "系统蓝牙栈".to_string() }) } else { ("未连接", "等待接入...".to_string()) };
    let meta_str = format!("模式: {} | {} | {}", conn_str, vid_pid, protocol);
    let mut m_rect = RECT { left: x + 16, top: y + 33, right: x + w - 16, bottom: y + 52 };
    draw_text_clean(hdc, &meta_str, &mut m_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    let (pct, mv, online) = dev.map(|d| (d.battery.percentage, d.battery.voltage_mv, d.battery.online)).unwrap_or((None, None, false));
    let status_color = dev.map(|d| if d.battery.charging || d.battery.charging_source.is_charging() { CLR_CYAN } else if !d.battery.online { CLR_MUTED } else { pct.map(|p| if p >= 50 { CLR_GREEN } else if p >= 20 { CLR_YELLOW } else { CLR_RED }).unwrap_or(CLR_MUTED) }).unwrap_or(CLR_MUTED);
    let font_big = create_clear_font((24.0 * font_scale).round() as i32, true);
    SelectObject(hdc, font_big); SetTextColor(hdc, status_color);
    let p_str = pct.map(|p| format!("{}%", p)).unwrap_or_else(|| if online { "在线".to_string() } else { "待机休眠".to_string() });
    let mut p_rect = RECT { left: x + w - 160, top: y + 8, right: x + w - 16, bottom: y + 42 };
    draw_text_clean(hdc, &p_str, &mut p_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, font_normal); SetTextColor(hdc, CLR_MUTED);
    let detail_str = mv.map(|v| format!("电池端电压 {} mV ({:.3} V)", v, v as f32 / 1000.0)).unwrap_or_else(|| if online { "状态: 正常运行 · 已就绪".to_string() } else { "状态: 处于低功耗省电休眠模式".to_string() });
    let compact = h < 100; let detail_top = if compact { y + 48 } else { y + 56 }; let bar_y = if compact { y + h - 14 } else { y + 82 }; let bar_h = if compact { 7 } else { 10 };
    let mut d_rect = RECT { left: x + 16, top: detail_top, right: x + w - 16, bottom: bar_y - 3 };
    draw_text_clean(hdc, &detail_str, &mut d_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    let bar_x = x + 16; let bar_w = (w - 32).max(1); let bar_bg = CreateSolidBrush(0x00322A26);
    let bg_r = RECT { left: bar_x, top: bar_y, right: bar_x + bar_w, bottom: bar_y + bar_h };
    RoundRect(hdc, bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, 6, 6); FillRect(hdc, &bg_r, bar_bg); DeleteObject(bar_bg);
    let fill_w = (bar_w * pct.unwrap_or(0) as i32) / 100;
    if fill_w > 0 { let bar_fg = CreateSolidBrush(status_color); let fg_r = RECT { left: bar_x, top: bar_y, right: bar_x + fill_w, bottom: bar_y + bar_h }; RoundRect(hdc, bar_x, bar_y, bar_x + fill_w, bar_y + bar_h, 6, 6); FillRect(hdc, &fg_r, bar_fg); DeleteObject(bar_fg); }
    SelectObject(hdc, old_font); DeleteObject(font_bold); DeleteObject(font_normal); DeleteObject(font_big);
}
