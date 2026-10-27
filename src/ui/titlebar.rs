//! 自绘窗口标题栏。
//!
//! 系统标题栏（Caption）由 DWM 绘制，颜色、字体、内边距都不受我们控制，
//! 和下面的自绘界面永远差一层，看着像两块拼接。这里把窗口做成无边框，
//! 客户区铺满整窗，标题栏自己画：底色与界面同色、按钮自绘、拖动与缩放仍走系统。
//!
//! 布局与命中同样来自一份 `layout()`，绘制 / hover / 点击三处共读。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::ui::layout::Rect2;
use crate::ui::theme::THEME;
use std::ptr;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, Ellipse, FillRect, HDC, LineTo, MoveToEx,
    SelectObject, SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER, PS_SOLID,
};

/// 标题栏高度（4 的倍数）
pub const TITLE_H: i32 = 40;
/// 窗口按钮尺寸：与 Windows 11 的 46×32 接近，手感一致
pub const BTN_W: i32 = 44;
pub const BTN_H: i32 = 32;
/// 拖动 / 缩放的可抓取边距
pub const RESIZE_BORDER: i32 = 6;

/// 标题栏按钮
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BtnKind {
    Min,
    Max,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub struct TitleBar {
    /// 最小化 / 最大化（还原）/ 关闭
    pub min: Rect2,
    pub max: Rect2,
    pub close: Rect2,
}

pub fn layout(width: i32) -> TitleBar {
    let y = (TITLE_H - BTN_H) / 2;
    let close = Rect2::new(width - BTN_W, y, BTN_W, BTN_H);
    let max = Rect2::new(close.x - BTN_W, y, BTN_W, BTN_H);
    let min = Rect2::new(max.x - BTN_W, y, BTN_W, BTN_H);
    TitleBar { min, max, close }
}

impl TitleBar {
    pub fn rect_of(&self, kind: BtnKind) -> Rect2 {
        match kind {
            BtnKind::Min => self.min,
            BtnKind::Max => self.max,
            BtnKind::Close => self.close,
        }
    }
}

/// 命中窗口按钮
pub fn hit(l: &TitleBar, x: i32, y: i32) -> Option<BtnKind> {
    for kind in [BtnKind::Min, BtnKind::Max, BtnKind::Close] {
        if l.rect_of(kind).contains(x, y) {
            return Some(kind);
        }
    }
    None
}

/// 标题栏空白区（可拖动）。按钮区已排除，否则点按钮会变成拖窗。
pub fn is_drag_area(l: &TitleBar, x: i32, y: i32) -> bool {
    y >= 0 && y < TITLE_H && x >= 0 && x < l.min.x && hit(l, x, y).is_none()
}

/// hover id 段：500 起（400 段已给设备瓦片）
pub fn hover_id(kind: BtnKind) -> i32 {
    match kind {
        BtnKind::Min => 501,
        BtnKind::Max => 502,
        BtnKind::Close => 503,
    }
}

/// 画标题栏。背景由上层（顶栏）铺好，这里只画标识与按钮。
pub unsafe fn paint(
    hdc: HDC,
    lay: &TitleBar,
    hover: i32,
    maximized: bool,
    font_scale: f32,
) {
    // 左上角的标识点 + 应用名：小、弱，不跟页面标题抢
    let dot_cx = 18;
    let dot_cy = TITLE_H / 2;
    let brush = CreateSolidBrush(THEME.accent);
    let pen = CreatePen(PS_SOLID, 1, THEME.accent);
    let ob = SelectObject(hdc, brush);
    let op = SelectObject(hdc, pen);
    Ellipse(hdc, dot_cx - 4, dot_cy - 4, dot_cx + 5, dot_cy + 5);
    SelectObject(hdc, ob);
    DeleteObject(brush);
    SelectObject(hdc, op);
    DeleteObject(pen);

    let f = crate::ui::panel::create_clear_font((11.5 * font_scale).round() as i32, false);
    let old = SelectObject(hdc, f);
    SetTextColor(hdc, THEME.muted);
    let mut r = RECT {
        left: dot_cx + 12,
        top: 0,
        right: lay.min.x - 8,
        bottom: TITLE_H,
    };
    crate::ui::panel::draw_text_clean(hdc, "LinkDeck", &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old);
    DeleteObject(f);

    for kind in [BtnKind::Min, BtnKind::Max, BtnKind::Close] {
        paint_button(hdc, lay.rect_of(kind), kind, hover == hover_id(kind), maximized);
    }
}

unsafe fn paint_button(hdc: HDC, rect: Rect2, kind: BtnKind, hovered: bool, maximized: bool) {
    if hovered {
        // 关闭按钮用危险色，其余用中性提亮：位置固定，颜色传达后果
        let bg = if matches!(kind, BtnKind::Close) { THEME.danger } else { THEME.btn_hover };
        let brush = CreateSolidBrush(bg);
        let r = RECT { left: rect.x, top: rect.y, right: rect.right(), bottom: rect.bottom() };
        FillRect(hdc, &r, brush);
        DeleteObject(brush);
    }
    let fg = if hovered && matches!(kind, BtnKind::Close) { THEME.accent_text } else { THEME.text };
    let cx = rect.x + rect.w / 2;
    let cy = rect.y + rect.h / 2;
    let arm = 5;

    let brush = CreateSolidBrush(fg);
    match kind {
        BtnKind::Min => {
            let r = RECT { left: cx - arm, top: cy, right: cx + arm, bottom: cy + 1 };
            FillRect(hdc, &r, brush);
        }
        BtnKind::Max => {
            if maximized {
                // 还原态：两个叠在一起的方框
                let r = RECT { left: cx - arm, top: cy - arm + 2, right: cx + arm - 2, bottom: cy - arm + 3 };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm + 2, top: cy - arm + 2, right: cx - arm + 3, bottom: cy + arm };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm + 2, top: cy + arm - 1, right: cx + arm, bottom: cy + arm };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx + arm - 1, top: cy - arm + 2, right: cx + arm, bottom: cy + arm };
                FillRect(hdc, &r, brush);
                // 前面那层小方框
                let r = RECT { left: cx - arm + 4, top: cy - arm + 5, right: cx + arm, bottom: cy - arm + 6 };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm + 4, top: cy - arm + 5, right: cx - arm + 5, bottom: cy + arm - 1 };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm + 4, top: cy + arm - 2, right: cx + arm, bottom: cy + arm - 1 };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx + arm - 1, top: cy - arm + 5, right: cx + arm, bottom: cy + arm - 1 };
                FillRect(hdc, &r, brush);
            } else {
                let r = RECT { left: cx - arm, top: cy - arm + 1, right: cx + arm, bottom: cy - arm + 2 };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm, top: cy + arm - 1, right: cx + arm, bottom: cy + arm };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx - arm, top: cy - arm + 1, right: cx - arm + 1, bottom: cy + arm };
                FillRect(hdc, &r, brush);
                let r = RECT { left: cx + arm - 1, top: cy - arm + 1, right: cx + arm, bottom: cy + arm };
                FillRect(hdc, &r, brush);
            }
        }
        BtnKind::Close => {
            let pen = CreatePen(PS_SOLID, 1, fg);
            let old = SelectObject(hdc, pen);
            let d = arm - 1;
            MoveToEx(hdc, cx - d, cy - d, ptr::null_mut());
            LineTo(hdc, cx + d + 1, cy + d + 1);
            MoveToEx(hdc, cx + d, cy - d, ptr::null_mut());
            LineTo(hdc, cx - d - 1, cy + d + 1);
            SelectObject(hdc, old);
            DeleteObject(pen);
        }
    }
    DeleteObject(brush);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_sit_at_the_top_right_in_a_row() {
        let l = layout(900);
        assert_eq!(l.close.right(), 900, "关闭按钮贴右边缘");
        assert_eq!(l.min.x, 900 - BTN_W * 3);
        assert_eq!(l.min.y, l.close.y);
        assert_eq!(l.min.h, BTN_H);
        assert!(l.min.right() == l.max.x && l.max.right() == l.close.x, "按钮之间不能有缝");
        assert!((TITLE_H - BTN_H) / 2 >= 0, "按钮要能放进标题栏");
    }

    #[test]
    fn hit_test_matches_button_geometry() {
        let l = layout(900);
        for kind in [BtnKind::Min, BtnKind::Max, BtnKind::Close] {
            let r = l.rect_of(kind);
            assert_eq!(hit(&l, r.x + r.w / 2, r.y + r.h / 2), Some(kind));
        }
        // 标题栏左侧空白不是按钮
        assert_eq!(hit(&l, 20, TITLE_H / 2), None);
    }

    #[test]
    fn drag_area_excludes_the_buttons() {
        let l = layout(900);
        assert!(is_drag_area(&l, 200, TITLE_H / 2), "左侧空白可拖窗");
        assert!(!is_drag_area(&l, l.close.x + 2, l.close.y + 2), "按钮区不能拖窗");
        assert!(!is_drag_area(&l, 200, TITLE_H + 2), "标题栏之下不算拖窗区");
    }

    #[test]
    fn hover_ids_do_not_collide_with_the_tile_range() {
        let ids = [hover_id(BtnKind::Min), hover_id(BtnKind::Max), hover_id(BtnKind::Close)];
        assert!(ids.iter().all(|id| *id > 500));
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            assert!(seen.insert(id));
        }
    }
}
