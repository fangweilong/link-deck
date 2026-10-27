//! 命中测试：把窗口坐标换算成控件 id，并做视口裁剪。

use super::*;

/// 滑块轨道上某横坐标对应的百分比。
pub fn slider_value(track: &Box2, x: i32) -> u8 {
    let span = (OPACITY_MAX - OPACITY_MIN) as i32;
    let ratio = ((x - track.x) as f32 / track.w.max(1) as f32).clamp(0.0, 1.0);
    OPACITY_MIN + (ratio * span as f32).round() as u8
}

/// 百分比在轨道上的横坐标。
pub(super) fn slider_x(track: &Box2, percent: u8) -> i32 {
    let span = (OPACITY_MAX - OPACITY_MIN) as f32;
    let ratio = ((percent.clamp(OPACITY_MIN, OPACITY_MAX) - OPACITY_MIN) as f32 / span).clamp(0.0, 1.0);
    track.x + (ratio * track.w as f32).round() as i32
}

pub fn hit_test(page: &Page, x: i32, y: i32) -> Option<i32> {
    for card in &page.cards {
        if !card.rect.contains(x, y) {
            continue;
        }
        for row in &card.rows {
            match row {
                RowBox::Pills(pills) => {
                    for p in pills {
                        // id 0 是纯展示（数值 / 标签），点它不该有任何动作
                        if p.id != 0 && p.rect.contains(x, y) {
                            return Some(p.id);
                        }
                    }
                }
                RowBox::Slider(s) => {
                    // 整行都可按下，手感比只认 18px 滑轨宽得多
                    if s.rect.contains(x, y) {
                        return Some(SLIDER_ID);
                    }
                }
            }
        }
    }
    None
}

/// 窗口坐标命中（带视口裁剪）。
///
/// 内容绘制时被裁剪在 `[view_top, view_top + view_h)` 内，范围外的卡片根本不画。
/// 命中必须做同样的裁剪，否则视口外的控件会"看不见却能点到"——
/// 底栏的「返回设备监控」位于滚动区之外，会被视口外的控件抢走点击。
pub fn hit_test_at(
    page: &Page,
    x: i32,
    y: i32,
    scroll: i32,
    view_top: i32,
    view_h: i32,
) -> Option<i32> {
    if y < view_top || y >= view_top + view_h {
        return None;
    }
    hit_test(page, x, y - view_top + scroll)
}

pub fn nav_hit(page: &Page, x: i32, y: i32) -> Option<usize> {
    page.chips.iter().find(|c| c.rect.contains(x, y)).map(|c| c.group)
}
