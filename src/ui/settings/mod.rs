//! 偏好设置页：**分类分页**，一次只渲染一组卡片。
//!
//! 旧实现把所有配置串成一条长滚动流，用户知道要改什么也得先滚到跟前；
//! 现在按「界面 / 悬浮球 / 通知 / 设备 / 系统」五类分页，导航即页面。
//!
//! 布局仍然只有一处真源：`layout()` 产出带坐标的卡片树，
//! 绘制、hover、命中三处读同一份结果。
//!
//! 文件按职责分开：`cards` 定义内容，`layout` 产出坐标树，
//! `hit` 做命中，`paint` 做绘制——四者读的是同一棵树。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::config::{AlertSound, DeviceFilter, DeviceSort, UiDensity, UiStyle};
use cards::build;

mod cards;
mod hit;
mod paint;

pub use hit::*;
pub use paint::*;

/// 分类导航不再单独占一条带子：它画在顶栏内部（顶栏在设置页加高到 `CONTENT_TOP`），
/// 视觉上属于同一块，内容区里也就不再重复一遍分组标题。
pub const NAV_Y: i32 = crate::ui::titlebar::TITLE_H + 62;
pub const NAV_H: i32 = 30;
/// 内容裁剪区上沿：导航条之下（含自绘标题栏的高度）
pub const CONTENT_TOP: i32 = crate::ui::titlebar::TITLE_H + 100;

const CARD_MARGIN: i32 = 16;
const CARD_PAD: i32 = 18;
/// 卡片之间留出比内边距更大的呼吸：分页之后一屏只有 2–3 张卡片，
/// 靠间距把功能隔开，而不是靠堆小标题。
const CARD_GAP: i32 = 18;
const ROW_H: i32 = 30;
const ROW_GAP: i32 = 8;
const SLIDER_H: i32 = 52;
const PILL_GAP: i32 = 8;
/// 页面顶部留白
const CONTENT_PAD: i32 = 12;

/// 滑块与导航条使用的命中 id（`PillKind::Swatch` 等控件一律避开 57）
pub const SLIDER_ID: i32 = 57;
const NAV_ID_BASE: i32 = 160;

/// 界面风格选项的起始 id：`STYLE_ID_BASE + i` 对应 `theme::ALL_STYLES[i]`，
/// 末位 `STYLE_ID_BASE + ALL_STYLES.len()` 是「自定义」。面板按同一规则派发。
pub const STYLE_ID_BASE: i32 = 130;

pub const OPACITY_MIN: u8 = 40;
pub const OPACITY_MAX: u8 = 100;

pub const NAV_LABELS: [(&str, i32); 5] = [
    ("界面", 76),
    ("悬浮球", 86),
    ("通知", 76),
    ("设备", 76),
    ("系统", 76),
];
pub const GROUP_COUNT: usize = NAV_LABELS.len();


/// 绘制设置页所需的全部状态快照。
#[derive(Clone)]
pub struct Ctx {
    pub font_scale: f32,
    pub poll_ms: u64,
    pub ball_on: bool,
    pub ball_opacity: u8,
    pub ball_style: crate::config::FloatingBallStyle,
    pub enable_toast: bool,
    pub toast_info: u8,
    pub toast_warn: u8,
    pub toast_crit: u8,
    pub toast_second: u8,
    pub toast_gaming: bool,
    pub toast_sound: bool,
    pub alert_sound: AlertSound,
    pub ui_density: UiDensity,
    pub device_filter: DeviceFilter,
    pub device_sort: DeviceSort,
    pub ui_style: UiStyle,
    pub custom_accent: u32,
    pub custom_light: bool,
    /// 当前分页（0..GROUP_COUNT）
    pub active_group: usize,
    pub auto_update: bool,
    pub auto_start: bool,
    pub update_line: String,
    pub update_available: bool,
    pub update_applying: bool,
    pub update_checking: bool,
    pub latest_version: Option<String>,
    pub config_path: String,
}

#[derive(Clone, Copy)]
pub struct Box2 {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Box2 {
    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

/// 控件的视觉语义。三者长得一样时用户分不清“开关”和“单选”，
/// 这里按语义分派配色与文字对齐。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillKind {
    /// 单选：选中项描边
    Choice,
    /// 开关：开则整块填强调色
    Toggle,
    /// 强调按钮：强调色填充
    Action,
    /// 数值：只读展示（`id == 0`，点不到）
    Value,
    /// 步进按钮：− / +
    Step,
    /// 色块：底色即候选主色
    Swatch,
    /// 纯文字标签：无底无框
    Label,
}

pub struct PillBox {
    pub rect: Box2,
    pub label: String,
    pub id: i32,
    pub active: bool,
    pub kind: PillKind,
    /// 仅 `Swatch` 使用：0xRRGGBB
    pub color: u32,
}

pub struct SliderBox {
    pub rect: Box2,
    /// 滑轨所在的一条水平带，点击/拖拽都按它换算百分比
    pub track: Box2,
}

pub enum RowBox {
    Pills(Vec<PillBox>),
    Slider(SliderBox),
}

pub struct CardBox {
    pub rect: Box2,
    pub title: String,
    pub title_y: i32,
    pub desc: Option<String>,
    pub desc_y: i32,
    pub rows: Vec<RowBox>,
}

pub struct Chip {
    pub rect: Box2,
    pub label: &'static str,
    pub group: usize,
    pub id: i32,
}

pub struct Page {
    pub cards: Vec<CardBox>,
    pub chips: Vec<Chip>,
    pub content_h: i32,
    /// 透明度的可拖拽滑轨；不在悬浮球分页时为 None（0 尺寸）
    pub slider: Box2,
}

// --------------------------------------------------------------------------
// 内容定义：`cards` 产出的中间形态，尚未带坐标
// --------------------------------------------------------------------------

/// 一行控件的内容描述。`w` 与 `kind` 决定它排出来长什么样，
/// 真正的坐标由下面的 `layout()` 分配。
pub(super) struct Pill {
    pub(super) label: String,
    pub(super) id: i32,
    pub(super) active: bool,
    pub(super) w: i32,
    pub(super) kind: PillKind,
    pub(super) color: u32,
}

pub(super) enum Row {
    Pills(Vec<Pill>),
    Slider,
}

pub(super) struct Card {
    pub(super) title: String,
    pub(super) desc: Option<String>,
    pub(super) rows: Vec<Row>,
}

// --------------------------------------------------------------------------
// 布局
// --------------------------------------------------------------------------

pub fn layout(ctx: &Ctx, width: i32) -> Page {
    let card_x = CARD_MARGIN;
    let card_w = (width - CARD_MARGIN * 2).max(240);
    let inner_x = card_x + CARD_PAD;
    let inner_right = card_x + card_w - CARD_PAD;
    let inner_w = (inner_right - inner_x).max(120);

    let mut cards = Vec::new();
    let mut slider = Box2 { x: 0, y: 0, w: 0, h: 0 };
    let mut y = CONTENT_PAD;

    for card in build(ctx) {
        let card_y = y;
        let mut cy = card_y + CARD_PAD;
        let title_y = cy;
        cy += 20;
        let mut desc_y = 0;
        if card.desc.is_some() {
            cy += 4;
            desc_y = cy;
            cy += 18;
        }
        cy += 12;

        let mut rows = Vec::new();
        for row in card.rows {
            match row {
                Row::Pills(pills) => {
                    let mut boxes = Vec::new();
                    let mut x = inner_x;
                    let mut ry = cy;
                    for p in pills {
                        if x + p.w > inner_right && x > inner_x {
                            x = inner_x;
                            ry += ROW_H + 8;
                        }
                        boxes.push(PillBox {
                            rect: Box2 { x, y: ry, w: p.w, h: ROW_H },
                            label: p.label,
                            id: p.id,
                            active: p.active,
                            kind: p.kind,
                            color: p.color,
                        });
                        x += p.w + PILL_GAP;
                    }
                    let used = ry - cy + ROW_H;
                    rows.push(RowBox::Pills(boxes));
                    cy += used + ROW_GAP;
                }
                Row::Slider => {
                    let track = Box2 { x: inner_x, y: cy + 24, w: inner_w, h: 18 };
                    slider = track;
                    rows.push(RowBox::Slider(SliderBox {
                        rect: Box2 { x: inner_x, y: cy, w: inner_w, h: SLIDER_H },
                        track,
                    }));
                    cy += SLIDER_H + ROW_GAP;
                }
            }
        }
        cy -= ROW_GAP;
        let card_h = cy + CARD_PAD - card_y;

        cards.push(CardBox {
            rect: Box2 { x: card_x, y: card_y, w: card_w, h: card_h },
            title: card.title,
            title_y,
            desc: card.desc,
            desc_y,
            rows,
        });
        y = card_y + card_h + CARD_GAP;
    }

    let mut chips = Vec::new();
    let mut cx = CARD_MARGIN;
    for (group, (label, w)) in NAV_LABELS.iter().enumerate() {
        chips.push(Chip {
            rect: Box2 { x: cx, y: NAV_Y, w: *w, h: NAV_H },
            label,
            group,
            id: NAV_ID_BASE + group as i32,
        });
        cx += w + PILL_GAP;
    }

    let content_h = (y - CARD_GAP + CONTENT_PAD).max(CONTENT_PAD * 2);
    Page { cards, chips, content_h, slider }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme;

    fn test_ctx() -> Ctx {
        let c = crate::config::AppConfig::default();
        Ctx {
            font_scale: c.font_scale,
            poll_ms: c.poll_interval_ms,
            ball_on: c.enable_floating_ball,
            ball_opacity: c.floating_ball_opacity,
            ball_style: c.floating_ball_style,
            enable_toast: c.enable_battery_toast,
            toast_info: c.toast_threshold_info,
            toast_warn: c.toast_threshold_warning,
            toast_crit: c.toast_threshold_critical,
            toast_second: c.toast_threshold_second,
            toast_gaming: c.toast_gaming_suppression,
            toast_sound: c.enable_low_battery_sound,
            alert_sound: c.alert_sound,
            ui_density: c.ui_density,
            device_filter: c.device_filter,
            device_sort: c.device_sort,
            ui_style: c.ui_style,
            custom_accent: c.custom_accent,
            custom_light: c.custom_light,
            active_group: 0,
            auto_update: c.check_updates,
            auto_start: false,
            update_line: "已是最新版本".into(),
            update_available: false,
            update_applying: false,
            update_checking: false,
            latest_version: None,
            config_path: "C:/tmp/config.json".into(),
        }
    }

    fn all_pills(page: &Page) -> Vec<&PillBox> {
        page.cards
            .iter()
            .flat_map(|c| c.rows.iter())
            .filter_map(|r| match r {
                RowBox::Pills(p) => Some(p.iter()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn cards_never_overlap_and_fit_the_viewport_width() {
        for group in 0..GROUP_COUNT {
            for width in [480, 640, 900] {
                let mut ctx = test_ctx();
                ctx.active_group = group;
                let page = layout(&ctx, width);
                assert!(!page.cards.is_empty(), "分组 {group} 没有渲染任何卡片");
                for pair in page.cards.windows(2) {
                    let (a, b) = (&pair[0], &pair[1]);
                    assert!(
                        b.rect.y >= a.rect.y + a.rect.h,
                        "分组 {group} 卡片重叠 @ {width}: {} vs {}",
                        a.title,
                        b.title
                    );
                }
                let last = page.cards.last().unwrap();
                assert!(page.content_h > last.rect.y + last.rect.h);
                for card in &page.cards {
                    assert!(card.rect.x + card.rect.w <= width);
                    for row in &card.rows {
                        if let RowBox::Pills(pills) = row {
                            for p in pills {
                                assert!(
                                    p.rect.x + p.rect.w <= card.rect.x + card.rect.w,
                                    "控件 {} 溢出卡片 {} @ {width}",
                                    p.label,
                                    card.title
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// 分类导航必须落在顶栏内（顶栏在设置页加高到 CONTENT_TOP），
    /// 否则它又会变成独立的一条带子，和顶栏重复。
    #[test]
    fn nav_lives_inside_the_header_band() {
        assert!(NAV_Y >= crate::ui::dashboard::HEADER_H, "导航要落在标题行之下");
        assert!(NAV_Y + NAV_H <= CONTENT_TOP, "导航要收在顶栏裁剪线之上");
        let page = layout(&test_ctx(), 640);
        for chip in &page.chips {
            assert_eq!(chip.rect.y, NAV_Y);
            assert!(chip.rect.y + chip.rect.h <= CONTENT_TOP);
        }
        assert_eq!(page.chips.len(), NAV_LABELS.len());
        for (i, a) in page.chips.iter().enumerate() {
            for b in page.chips.iter().skip(i + 1) {
                assert!(
                    a.rect.x + a.rect.w <= b.rect.x || b.rect.x + b.rect.w <= a.rect.x,
                    "导航项 {} 重叠", a.label
                );
            }
        }
    }

    /// 分页的核心不变量：一次只渲染一个分组，切换分组内容必须真的变。
    #[test]
    fn only_the_active_group_is_laid_out() {
        let mut seen: Vec<String> = Vec::new();
        for group in 0..GROUP_COUNT {
            let mut ctx = test_ctx();
            ctx.active_group = group;
            let page = layout(&ctx, 640);
            let titles: Vec<String> = page.cards.iter().map(|c| c.title.clone()).collect();
            assert!(!titles.is_empty(), "分组 {group} 为空");
            let joined = titles.join("/");
            assert!(!seen.contains(&joined), "分组 {group} 与之前的分组内容完全相同");
            seen.push(joined);
            // 卡片从顶部留白开始排，不再依赖滚动位置
            assert_eq!(page.cards[0].rect.y, CONTENT_PAD);
        }
    }

    /// 控件 id 是全窗口共用的，跨分组重复会让 hover / 点击串台。
    #[test]
    fn every_control_id_is_unique_across_all_pages() {
        let mut ids: Vec<i32> = Vec::new();
        for group in 0..GROUP_COUNT {
            let mut ctx = test_ctx();
            ctx.active_group = group;
            let page = layout(&ctx, 640);
            for p in all_pills(&page) {
                if p.id == 0 {
                    continue;
                }
                assert!(!ids.contains(&p.id), "控件 id {} 重复（分组 {group}）", p.id);
                assert_ne!(p.id, SLIDER_ID, "控件 id 与滑块 id 冲突（分组 {group}）");
                ids.push(p.id);
            }
            for chip in &page.chips {
                assert!(!ids.contains(&chip.id), "导航 id {} 与控件冲突", chip.id);
            }
        }
    }

    #[test]
    fn slider_covers_the_full_opacity_range() {
        let mut ctx = test_ctx();
        ctx.active_group = 1;
        let page = layout(&ctx, 640);
        let t = &page.slider;
        assert!(t.w > 0, "悬浮球分页必须有滑轨");
        assert_eq!(slider_value(t, t.x), OPACITY_MIN);
        assert_eq!(slider_value(t, t.x + t.w), OPACITY_MAX);
        assert_eq!(slider_value(t, t.x + t.w / 2), 70);
        assert_eq!(slider_value(t, t.x - 500), OPACITY_MIN);
        assert_eq!(slider_value(t, t.x + t.w + 500), OPACITY_MAX);
    }

    /// 滑轨只属于悬浮球分页：切到别的分页不能留下一个“幽灵滑块”。
    #[test]
    fn slider_exists_only_on_the_ball_page() {
        for group in 0..GROUP_COUNT {
            let mut ctx = test_ctx();
            ctx.active_group = group;
            let page = layout(&ctx, 640);
            if group == 1 {
                assert!(page.slider.w > 0);
            } else {
                assert_eq!(page.slider.w, 0);
            }
        }
    }

    #[test]
    fn hit_test_resolves_controls_inside_their_cards() {
        let mut ctx = test_ctx();
        ctx.active_group = 1;
        let page = layout(&ctx, 640);

        let toggle = all_pills(&page)
            .into_iter()
            .find(|p| p.id == 50)
            .expect("ball toggle pill");
        let cx = toggle.rect.x + toggle.rect.w / 2;
        let cy = toggle.rect.y + toggle.rect.h / 2;
        assert_eq!(hit_test(&page, cx, cy), Some(50));

        assert_eq!(
            hit_test(&page, page.slider.x + 4, page.slider.y + page.slider.h / 2),
            Some(SLIDER_ID)
        );
        // 卡片之间的空隙不命中任何控件
        assert_eq!(hit_test(&page, 2, page.cards[0].rect.y), None);
    }

    /// 只读数值（id 0）不能响应点击，否则点阈值会派发一个不存在的动作。
    #[test]
    fn read_only_values_never_capture_clicks() {
        let mut ctx = test_ctx();
        ctx.active_group = 2;
        let page = layout(&ctx, 640);
        let values: Vec<&PillBox> = all_pills(&page).into_iter().filter(|p| p.id == 0).collect();
        assert!(!values.is_empty(), "阈值分页必须有只读数值");
        for v in values {
            assert_eq!(
                hit_test(&page, v.rect.x + v.rect.w / 2, v.rect.y + v.rect.h / 2),
                None,
                "只读控件 {} 不该被点到",
                v.label
            );
        }
    }

    /// 四档阈值 + 提示音 + 二次提醒都必须有可点的步进/选项按钮。
    #[test]
    fn alert_controls_are_all_reachable() {
        let mut ctx = test_ctx();
        ctx.active_group = 2;
        let page = layout(&ctx, 640);
        for id in [70, 71, 72, 73, 74, 75, 76, 77, 63, 64, 65, 66, 80, 81] {
            let p = all_pills(&page)
                .into_iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("缺少控件 id {id}"));
            assert_eq!(
                hit_test(&page, p.rect.x + p.rect.w / 2, p.rect.y + p.rect.h / 2),
                Some(id)
            );
        }
    }

    /// 风格选项的 id 必须按 `theme::ALL_STYLES` 的顺序排：面板按同一规则
    /// 用 `STYLE_ID_BASE + i` 派发，两边一旦错位就会「点暖橙切极光」。
    #[test]
    fn style_option_ids_follow_the_all_styles_order() {
        let page = layout(&test_ctx(), 640);
        let pills = all_pills(&page);
        for (i, style) in theme::ALL_STYLES.iter().enumerate() {
            let id = STYLE_ID_BASE + i as i32;
            let pill = pills
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("缺少风格控件 id {id}"));
            assert!(
                pill.label.contains(theme::label(*style)),
                "id {id} 应当是 {}, 实际 {}",
                theme::label(*style),
                pill.label
            );
        }
        // 末位留给「自定义配色」
        let custom_id = STYLE_ID_BASE + theme::ALL_STYLES.len() as i32;
        assert!(pills.iter().any(|p| p.id == custom_id), "缺少自定义配色入口");
    }

    /// 自定义配色入口：12 个色块 + 深浅两档，全部可点。
    #[test]
    fn custom_palette_exposes_every_choice() {
        let page = layout(&test_ctx(), 640);
        let pills = all_pills(&page);
        for id in (200..212).chain(220..=221) {
            assert!(pills.iter().any(|p| p.id == id), "缺少配色控件 id {id}");
        }
        // 色块数量与候选色板一致
        let swatches = pills.iter().filter(|p| matches!(p.kind, PillKind::Swatch)).count();
        assert_eq!(swatches, theme::ACCENT_CHOICES.len());
    }

    fn page_with_pill(pill_y: i32) -> Page {
        Page {
            cards: vec![CardBox {
                rect: Box2 { x: 0, y: pill_y - 8, w: 400, h: 40 },
                title: "测试卡片".into(),
                title_y: pill_y - 8,
                desc: None,
                desc_y: 0,
                rows: vec![RowBox::Pills(vec![PillBox {
                    rect: Box2 { x: 10, y: pill_y, w: 100, h: 24 },
                    label: "测试".into(),
                    id: 777,
                    active: false,
                    kind: PillKind::Choice,
                    color: 0,
                }])],
            }],
            chips: vec![],
            content_h: pill_y + 200,
            slider: Box2 { x: 0, y: 0, w: 0, h: 0 },
        }
    }

    /// 回归：绘制把内容裁剪在滚动视口内，命中必须做同样的裁剪。
    /// 否则视口外的控件会“看不见却能点到”，把底栏「返回设备监控」的点击吃掉
    /// （表现为：点返回按钮没反应，必须滚动一段距离才点得到）。
    #[test]
    fn hit_test_at_clips_to_the_visible_scroll_view() {
        // 坐标全部相对视口上沿，避免窗口加了标题栏后测试又要手改数字
        let view_top = CONTENT_TOP;
        let view_h = 392;

        // 视口内：内容 y=306 → 屏幕 y = view_top + 306
        let page = page_with_pill(300);
        assert_eq!(hit_test(&page, 20, 306), Some(777));
        assert_eq!(hit_test_at(&page, 20, view_top + 306, 0, view_top, view_h), Some(777));

        // 视口外：内容 y=406 → 屏幕 y 超出视口底部，已被裁剪，不该命中
        let page = page_with_pill(400);
        assert_eq!(hit_test(&page, 20, 406), Some(777), "内容坐标下该控件确实存在");
        assert_eq!(
            hit_test_at(&page, 20, view_top + 406, 0, view_top, view_h),
            None,
            "视口外的控件不得被点到"
        );

        // 底栏按钮所在区域（屏幕 y 超出视口）不得命中任何内容
        assert_eq!(hit_test_at(&page, 20, view_top + view_h + 24, 0, view_top, view_h), None);

        // 滚动到可视区后，同一个控件应当可以点到（内容 y=406 → 屏幕 view_top + 306）
        assert_eq!(hit_test_at(&page, 20, view_top + 306, 100, view_top, view_h), Some(777));
    }

    #[test]
    fn update_card_upgrade_button_is_action_and_clickable() {
        let mut ctx = test_ctx();
        ctx.active_group = 4; // 系统
        ctx.update_available = true;
        ctx.latest_version = Some("0.8.0".into());

        let page = layout(&ctx, 640);
        let pills = all_pills(&page);
        let upgrade_btn = pills.iter().find(|p| p.label == "升级到 v0.8.0")
            .expect("必须包含升级按钮");

        assert_eq!(upgrade_btn.id, 142, "升级按钮 ID 必须为 142");
        assert_eq!(upgrade_btn.kind, PillKind::Action, "升级按钮类型必须为 Action");

        // 验证命中测试
        let hit = hit_test(&page, upgrade_btn.rect.x + 10, upgrade_btn.rect.y + 10);
        assert_eq!(hit, Some(142), "点击升级按钮区域必须命中 ID 142");

        // 升级中状态
        let mut applying_ctx = ctx.clone();
        applying_ctx.update_applying = true;
        let applying_page = layout(&applying_ctx, 640);
        let applying_pills = all_pills(&applying_page);
        let applying_btn = applying_pills.iter().find(|p| p.label == "正在下载并升级…")
            .expect("必须展示升级中提示按钮");
        assert_eq!(applying_btn.id, 0);
        assert_eq!(applying_btn.kind, PillKind::Value);
        let applying_hit = hit_test(&applying_page, applying_btn.rect.x + 10, applying_btn.rect.y + 10);
        assert_eq!(applying_hit, None, "升级中按钮不可点击");
    }
}
