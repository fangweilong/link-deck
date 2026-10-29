//! 参数页绘制：分类导航条、卡片、控件、滑块。

use super::hit::slider_x;
use super::*;
use crate::ui::floating_ball::{create_clear_font, draw_text_clean};
use crate::ui::theme::{self, THEME};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, Ellipse, FillRect, HDC, RoundRect, SelectObject,
    SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    PS_SOLID,
};

// --------------------------------------------------------------------------
// 绘制
// --------------------------------------------------------------------------

pub unsafe fn paint_nav(hdc: HDC, page: &Page, hover: i32, font_scale: f32, active_group: usize) {
    // 选中态直接来自分页状态：分页后导航就是页面本身，不再靠滚动位置反推
    for chip in &page.chips {
        let active = chip.group == active_group;
        let hovered = hover == chip.id;

        let f = create_clear_font((13.0 * font_scale).round() as i32, active);
        let old = SelectObject(hdc, f);
        SetTextColor(hdc, if active || hovered { THEME.text } else { THEME.muted });
        let mut r = RECT {
            left: chip.rect.x,
            top: chip.rect.y,
            right: chip.rect.x + chip.rect.w,
            bottom: chip.rect.y + chip.rect.h - 4,
        };
        draw_text_clean(hdc, chip.label, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        SelectObject(hdc, old);
        DeleteObject(f);

        if active || hovered {
            let clr = if active { THEME.accent } else { THEME.border };
            let bar = CreateSolidBrush(clr);
            let w = if active { chip.rect.w - 16 } else { chip.rect.w - 24 };
            let left = chip.rect.x + (chip.rect.w - w) / 2;
            let r = RECT {
                left,
                top: chip.rect.y + chip.rect.h - 5,
                right: left + w,
                bottom: chip.rect.y + chip.rect.h - 3,
            };
            FillRect(hdc, &r, bar);
            DeleteObject(bar);
        }
    }
}

/// `origin` 是内容视口在窗口中的上沿，滚动量从视口顶算起。
pub unsafe fn paint_cards(hdc: HDC, page: &Page, ctx: &Ctx, scroll: i32, hover: i32, origin: i32) {
    let sy = scroll - origin;
    let f_title = create_clear_font((15.0 * ctx.font_scale).round() as i32, true);
    let f_desc = create_clear_font((12.5 * ctx.font_scale).round() as i32, false);

    for card in &page.cards {
        let bg = CreateSolidBrush(THEME.card);
        let border = CreatePen(PS_SOLID, 1, THEME.border);
        let ob = SelectObject(hdc, bg);
        let op = SelectObject(hdc, border);
        RoundRect(
            hdc,
            card.rect.x,
            card.rect.y - sy,
            card.rect.x + card.rect.w + 1,
            card.rect.y - sy + card.rect.h + 1,
            12,
            12,
        );
        SelectObject(hdc, ob);
        DeleteObject(bg);
        SelectObject(hdc, op);
        DeleteObject(border);

        let mut title_rect = RECT {
            left: card.rect.x + CARD_PAD,
            top: card.title_y - sy,
            right: card.rect.x + card.rect.w - CARD_PAD,
            bottom: card.title_y - sy + 20,
        };
        SelectObject(hdc, f_title);
        SetTextColor(hdc, THEME.text);
        draw_text_clean(hdc, &card.title, &mut title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        if let Some(desc) = &card.desc {
            let mut desc_rect = RECT {
                left: card.rect.x + CARD_PAD,
                top: card.desc_y - sy,
                right: card.rect.x + card.rect.w - CARD_PAD,
                bottom: card.desc_y - sy + 18,
            };
            SelectObject(hdc, f_desc);
            SetTextColor(hdc, THEME.muted);
            draw_text_clean(
                hdc,
                desc,
                &mut desc_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
        }

        for row in &card.rows {
            match row {
                RowBox::Pills(pills) => {
                    for p in pills {
                        paint_pill(hdc, p, hover == p.id && p.id != 0, ctx.font_scale, sy);
                    }
                }
                RowBox::Slider(s) => paint_slider(hdc, s, ctx, sy, hover),
            }
        }
    }

    SelectObject(hdc, f_desc);
    DeleteObject(f_desc);
    SelectObject(hdc, f_title);
    DeleteObject(f_title);
}

/// 按语义配色：开关开启整块填强调色，数值项强调色文字，单选项只有选中项描边。
unsafe fn paint_pill(hdc: HDC, p: &PillBox, hover: bool, font_scale: f32, sy: i32) {
    let y = p.rect.y - sy;

    match p.kind {
        PillKind::Label => {
            let f = create_clear_font((12.5 * font_scale).round() as i32, false);
            let old = SelectObject(hdc, f);
            SetTextColor(hdc, THEME.muted);
            let mut r = RECT {
                left: p.rect.x + 2,
                top: y,
                right: p.rect.x + p.rect.w,
                bottom: y + p.rect.h,
            };
            draw_text_clean(hdc, &p.label, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            SelectObject(hdc, old);
            DeleteObject(f);
        }
        PillKind::Swatch => {
            let fill = theme::from_rgb24(p.color);
            let ring = if p.active {
                THEME.text
            } else if hover {
                THEME.muted
            } else {
                THEME.border
            };
            let brush = CreateSolidBrush(fill);
            let pen = CreatePen(PS_SOLID, if p.active { 2 } else { 1 }, ring);
            let ob = SelectObject(hdc, brush);
            let op = SelectObject(hdc, pen);
            RoundRect(hdc, p.rect.x, y, p.rect.x + p.rect.w, y + p.rect.h, 8, 8);
            SelectObject(hdc, ob);
            DeleteObject(brush);
            SelectObject(hdc, op);
            DeleteObject(pen);

            let f = create_clear_font((12.5 * font_scale).round() as i32, p.active);
            let old = SelectObject(hdc, f);
            SetTextColor(hdc, theme::text_on(p.color));
            let mut r = RECT {
                left: p.rect.x,
                top: y,
                right: p.rect.x + p.rect.w,
                bottom: y + p.rect.h,
            };
            draw_text_clean(hdc, &p.label, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            SelectObject(hdc, old);
            DeleteObject(f);
        }
        _ => {
            let on = p.active;
            let (bg, edge, fg) = match (p.kind, on, hover) {
                (PillKind::Toggle, true, true) => (THEME.accent_strong, THEME.accent_strong, THEME.accent_text),
                (PillKind::Toggle, true, false) => (THEME.accent, THEME.accent, THEME.accent_text),
                (PillKind::Action, _, true) => (THEME.accent_strong, THEME.accent_strong, THEME.accent_text),
                (PillKind::Action, _, false) => (THEME.accent, THEME.accent, THEME.accent_text),
                (PillKind::Value, _, _) => (THEME.btn, THEME.accent, THEME.accent),
                (PillKind::Step, _, true) => (THEME.btn_active, THEME.accent, THEME.text),
                (PillKind::Step, _, false) => (THEME.btn, THEME.border, THEME.text),
                (PillKind::Choice, true, _) => (THEME.btn_active, THEME.accent, THEME.text),
                (_, _, true) => (THEME.btn_hover, THEME.muted, THEME.text),
                (_, _, false) => (THEME.btn, THEME.border, THEME.muted),
            };
            let brush = CreateSolidBrush(bg);
            let pen = CreatePen(PS_SOLID, 1, edge);
            let ob = SelectObject(hdc, brush);
            let op = SelectObject(hdc, pen);
            RoundRect(hdc, p.rect.x, y, p.rect.x + p.rect.w, y + p.rect.h, 8, 8);
            SelectObject(hdc, ob);
            DeleteObject(brush);
            SelectObject(hdc, op);
            DeleteObject(pen);

            let bold = on || hover || matches!(p.kind, PillKind::Action);
            let f = create_clear_font((12.5 * font_scale).round() as i32, bold);
            let old = SelectObject(hdc, f);
            SetTextColor(hdc, fg);
            let mut r = RECT {
                left: p.rect.x,
                top: y,
                right: p.rect.x + p.rect.w,
                bottom: y + p.rect.h,
            };
            draw_text_clean(hdc, &p.label, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            SelectObject(hdc, old);
            DeleteObject(f);
        }
    }
}

unsafe fn paint_slider(hdc: HDC, s: &SliderBox, ctx: &Ctx, sy: i32, hover: i32) {
    let f_lbl = create_clear_font((12.0 * ctx.font_scale).round() as i32, false);
    let old = SelectObject(hdc, f_lbl);

    // 左标题 + 右数值
    SetTextColor(hdc, THEME.muted);
    let mut lbl = RECT {
        left: s.rect.x,
        top: s.rect.y - sy,
        right: s.rect.x + s.rect.w / 2,
        bottom: s.rect.y - sy + 18,
    };
    draw_text_clean(hdc, "不透明度", &mut lbl, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    SetTextColor(hdc, THEME.accent);
    let mut val = RECT {
        left: s.rect.x + s.rect.w / 2,
        top: s.rect.y - sy,
        right: s.rect.x + s.rect.w,
        bottom: s.rect.y - sy + 18,
    };
    draw_text_clean(
        hdc,
        &format!("{}%", ctx.ball_opacity),
        &mut val,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
    );
    SelectObject(hdc, old);
    DeleteObject(f_lbl);

    let ty = s.track.y - sy;
    let bar_h = 6;
    let bar_y = ty + (s.track.h - bar_h) / 2;

    // 底槽：圆角条，比扁平方块更像现代滑块
    let bg = CreateSolidBrush(THEME.track);
    let bg_rect = RECT { left: s.track.x, top: bar_y, right: s.track.x + s.track.w, bottom: bar_y + bar_h };
    RoundRect(hdc, s.track.x, bar_y, s.track.x + s.track.w, bar_y + bar_h, 3, 3);
    FillRect(hdc, &bg_rect, bg);
    DeleteObject(bg);

    let knob_x = slider_x(&s.track, ctx.ball_opacity);
    let fill_right = knob_x.max(s.track.x + 3);
    let fg = CreateSolidBrush(THEME.accent);
    let fg_rect = RECT { left: s.track.x, top: bar_y, right: fill_right, bottom: bar_y + bar_h };
    RoundRect(hdc, s.track.x, bar_y, fill_right, bar_y + bar_h, 3, 3);
    FillRect(hdc, &fg_rect, fg);
    DeleteObject(fg);

    let (knob_clr, knob_r) = if hover == SLIDER_ID {
        (THEME.text, 7)
    } else {
        (THEME.btn_text, 6)
    };
    let brush = CreateSolidBrush(knob_clr);
    let pen = CreatePen(PS_SOLID, 1, THEME.accent);
    let ob = SelectObject(hdc, brush);
    let op = SelectObject(hdc, pen);
    let cy = bar_y + bar_h / 2;
    Ellipse(hdc, knob_x - knob_r, cy - knob_r, knob_x + knob_r + 1, cy + knob_r + 1);
    SelectObject(hdc, ob);
    DeleteObject(brush);
    SelectObject(hdc, op);
    DeleteObject(pen);
}
