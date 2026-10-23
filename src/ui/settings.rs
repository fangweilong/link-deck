//! 偏好设置页：一次布局同时驱动绘制与命中测试。
//!
//! 旧实现把每一行的 y 坐标分别硬编码在绘制和点击两处，改一处忘一处就会错位；
//! 这里改为 `layout()` 产出带坐标的卡片树，绘制与点击读同一份结果。

use super::panel::{CLR_ACCENT, CLR_CARD, CLR_CARD_BORDER, CLR_MUTED, CLR_TEXT};
use super::panel_render::render_choice_pill;
use crate::config::{AccentTheme, DeviceFilter, DeviceSort, UiDensity};
use crate::ui::floating_ball::{create_clear_font, draw_text_clean};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, Ellipse, FillRect, HDC, RoundRect, SelectObject,
    SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, PS_SOLID,
};

/// 顶部粘性导航条（不随内容滚动）
pub const NAV_Y: i32 = 68;
pub const NAV_H: i32 = 26;
/// 滚动裁剪区上沿：导航条之下
pub const CONTENT_TOP: i32 = 104;

const CARD_MARGIN: i32 = 16;
const CARD_PAD: i32 = 14;
const CARD_GAP: i32 = 12;
const ROW_H: i32 = 26;
const ROW_GAP: i32 = 8;
const SLIDER_H: i32 = 46;
const PILL_GAP: i32 = 8;

/// 滑块与导航条使用的命中 id
pub const SLIDER_ID: i32 = 55;
const NAV_ID_BASE: i32 = 160;

pub const OPACITY_MIN: u8 = 40;
pub const OPACITY_MAX: u8 = 100;

const NAV_LABELS: [(&str, i32); 5] = [
    ("界面", 74),
    ("悬浮球", 86),
    ("通知", 74),
    ("设备", 74),
    ("系统", 74),
];

/// 绘制设置页所需的全部状态快照。
#[derive(Clone)]
pub struct Ctx {
    pub font_scale: f32,
    pub poll_ms: u64,
    pub ball_on: bool,
    pub ball_opacity: u8,
    pub enable_toast: bool,
    pub toast_info: u8,
    pub toast_warn: u8,
    pub toast_crit: u8,
    pub toast_gaming: bool,
    pub toast_sound: bool,
    pub ui_density: UiDensity,
    pub device_filter: DeviceFilter,
    pub device_sort: DeviceSort,
    pub accent_theme: AccentTheme,
    pub auto_update: bool,
    pub auto_start: bool,
    pub update_line: String,
    pub update_available: bool,
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

pub struct PillBox {
    pub rect: Box2,
    pub label: String,
    pub id: i32,
    pub active: bool,
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
    /// 每个分组首个卡片的内容坐标，用于导航跳转
    pub group_tops: Vec<i32>,
    pub content_h: i32,
    /// 透明度的可拖拽滑轨；未找到时为 None（正常情况下恒存在）
    pub slider: Box2,
}

// --------------------------------------------------------------------------
// 内容定义
// --------------------------------------------------------------------------

struct Pill {
    label: String,
    id: i32,
    active: bool,
    w: i32,
}

enum Row {
    Pills(Vec<Pill>),
    Slider,
}

struct Card {
    group: usize,
    title: String,
    desc: Option<String>,
    rows: Vec<Row>,
}

fn active_pill(label: impl Into<String>, id: i32, w: i32) -> Pill {
    Pill { label: label.into(), id, active: true, w }
}

fn pill(label: impl Into<String>, id: i32, active: bool, w: i32) -> Pill {
    Pill { label: label.into(), id, active, w }
}

fn build(ctx: &Ctx) -> Vec<Card> {
    let mut cards = Vec::new();

    // ---- 界面 ----
    let s100 = ctx.font_scale <= 1.05;
    let s115 = ctx.font_scale > 1.05 && ctx.font_scale <= 1.22;
    let s130 = ctx.font_scale > 1.22;
    cards.push(Card {
        group: 0,
        title: "显示字号".into(),
        desc: Some("微软雅黑 ClearType 次像素渲染的缩放比例。".into()),
        rows: vec![Row::Pills(vec![
            pill("100% 标准", 40, s100, 92),
            pill("115% 适中", 41, s115, 92),
            pill("130% 放大", 42, s130, 92),
        ])],
    });
    cards.push(Card {
        group: 0,
        title: "卡片布局密度".into(),
        desc: Some("紧凑适合小窗口；宽松提供更舒适的信息间距。".into()),
        rows: vec![Row::Pills(vec![
            pill("紧凑布局", 100, matches!(ctx.ui_density, UiDensity::Compact), 90),
            pill("标准布局", 101, matches!(ctx.ui_density, UiDensity::Standard), 90),
            pill("宽松布局", 102, matches!(ctx.ui_density, UiDensity::Spacious), 90),
        ])],
    });
    cards.push(Card {
        group: 0,
        title: "主题强调色".into(),
        desc: None,
        rows: vec![Row::Pills(vec![
            pill("橙色", 130, matches!(ctx.accent_theme, AccentTheme::Orange), 90),
            pill("蓝色", 131, matches!(ctx.accent_theme, AccentTheme::Blue), 90),
            pill("紫色", 132, matches!(ctx.accent_theme, AccentTheme::Purple), 90),
            pill("绿色", 133, matches!(ctx.accent_theme, AccentTheme::Green), 90),
        ])],
    });

    // ---- 悬浮球 ----
    cards.push(Card {
        group: 1,
        title: "贴边悬浮球".into(),
        desc: Some("半环能量仪表吸附屏幕边缘，悬停还原完整圆球，点击展开抽屉。".into()),
        rows: vec![
            Row::Pills(vec![pill(
                if ctx.ball_on { "🔘 已开启" } else { "⚪ 已关闭" },
                50,
                ctx.ball_on,
                150,
            )]),
            Row::Slider,
        ],
    });

    // ---- 通知 ----
    cards.push(Card {
        group: 2,
        title: "定时通信间隔".into(),
        desc: Some("外设电量消耗慢，推荐长周期省电；随时点“立即刷新”可秒级直读。".into()),
        rows: vec![Row::Pills(vec![
            pill("30s 调试", 30, ctx.poll_ms <= 45_000, 72),
            pill("1m 快速", 31, ctx.poll_ms > 45_000 && ctx.poll_ms <= 120_000, 72),
            pill("5m 推荐", 32, ctx.poll_ms > 120_000 && ctx.poll_ms <= 600_000, 78),
            pill("15m 适中", 33, ctx.poll_ms > 600_000 && ctx.poll_ms <= 1_200_000, 78),
            pill("30m 省电", 34, ctx.poll_ms > 1_200_000 && ctx.poll_ms <= 2_700_000, 78),
            pill("1h 极省", 35, ctx.poll_ms > 2_700_000, 78),
        ])],
    });
    cards.push(Card {
        group: 2,
        title: "低电量 Toast 弹窗".into(),
        desc: Some("跌破阈值时右下角弹窗；危险阈值同步提示音；全屏或游戏时免打扰。".into()),
        rows: vec![
            Row::Pills(vec![
                pill(
                    if ctx.enable_toast { "🔔 弹窗已开" } else { "🔕 弹窗已关" },
                    60,
                    ctx.enable_toast,
                    140,
                ),
                pill(
                    if ctx.toast_gaming { "🎮 游戏免打扰" } else { "🎮 允许游戏中弹窗" },
                    61,
                    ctx.toast_gaming,
                    170,
                ),
                pill(
                    if ctx.toast_sound { "🔊 提示音已开" } else { "🔇 提示音已关" },
                    62,
                    ctx.toast_sound,
                    150,
                ),
            ]),
            Row::Pills(vec![
                active_pill(format!("提示 < {}%", ctx.toast_info), 70, 100),
                active_pill(format!("警告 < {}%", ctx.toast_warn), 71, 100),
                active_pill(format!("危险 < {}%", ctx.toast_crit), 72, 100),
                pill("🚀 试发一条", 80, false, 120),
                pill("🎵 试听", 81, false, 90),
            ]),
        ],
    });

    // ---- 设备 ----
    cards.push(Card {
        group: 3,
        title: "设备列表".into(),
        desc: Some("筛选和排序只影响显示，不改变后台扫描。".into()),
        rows: vec![
            Row::Pills(vec![
                pill("全部", 120, matches!(ctx.device_filter, DeviceFilter::All), 72),
                pill("低电量", 121, matches!(ctx.device_filter, DeviceFilter::LowBattery), 84),
                pill("充电中", 122, matches!(ctx.device_filter, DeviceFilter::Charging), 84),
                pill("蓝牙", 123, matches!(ctx.device_filter, DeviceFilter::Bluetooth), 72),
                pill("2.4G", 124, matches!(ctx.device_filter, DeviceFilter::Wireless), 72),
            ]),
            Row::Pills(vec![
                pill("优先级", 125, matches!(ctx.device_sort, DeviceSort::Priority), 82),
                pill("名称", 126, matches!(ctx.device_sort, DeviceSort::Name), 82),
                pill("电量", 127, matches!(ctx.device_sort, DeviceSort::Battery), 82),
                pill("更新时间", 128, matches!(ctx.device_sort, DeviceSort::Updated), 82),
            ]),
        ],
    });

    // ---- 系统 ----
    let mut update_pills = vec![
        pill(
            if ctx.auto_update { "自动检查更新: 开" } else { "自动检查更新: 关" },
            140,
            ctx.auto_update,
            190,
        ),
        pill("立即检查", 141, false, 110),
    ];
    if ctx.update_available {
        if let Some(latest) = ctx.latest_version.as_deref() {
            update_pills.push(pill(format!("升级到 v{}", latest), 142, true, 160));
        }
    }
    cards.push(Card {
        group: 4,
        title: "版本与更新".into(),
        desc: Some(ctx.update_line.clone()),
        rows: vec![Row::Pills(update_pills)],
    });
    cards.push(Card {
        group: 4,
        title: "开机自动启动".into(),
        desc: Some("写入当前用户注册表 Run 项，登录后静默常驻托盘。".into()),
        rows: vec![Row::Pills(vec![pill(
            if ctx.auto_start { "🚀 开机自启: 已开启" } else { "⚪ 开机自启: 已关闭" },
            150,
            ctx.auto_start,
            200,
        )])],
    });
    cards.push(Card {
        group: 4,
        title: "配置文件目录".into(),
        desc: Some(ctx.config_path.clone()),
        rows: vec![Row::Pills(vec![pill("📁 打开配置目录", 151, false, 180)])],
    });

    cards
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
    let mut group_tops = vec![0i32; NAV_LABELS.len()];
    let mut seen_group = [false; NAV_LABELS.len()];
    let mut slider = Box2 { x: 0, y: 0, w: 0, h: 0 };
    let mut y = 12;

    for card in build(ctx) {
        if !seen_group[card.group] {
            seen_group[card.group] = true;
            group_tops[card.group] = y;
        }
        let card_y = y;
        let mut cy = card_y + CARD_PAD;
        let title_y = cy;
        cy += 20;
        let mut desc_y = 0;
        if card.desc.is_some() {
            cy += 2;
            desc_y = cy;
            cy += 18;
        }
        cy += 10;

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
                            ry += ROW_H + 6;
                        }
                        boxes.push(PillBox {
                            rect: Box2 { x, y: ry, w: p.w, h: ROW_H },
                            label: p.label,
                            id: p.id,
                            active: p.active,
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

    Page {
        cards,
        chips,
        group_tops,
        content_h: y + 8,
        slider,
    }
}

/// 滑块轨道上某横坐标对应的百分比。
pub fn slider_value(track: &Box2, x: i32) -> u8 {
    let span = (OPACITY_MAX - OPACITY_MIN) as i32;
    let ratio = ((x - track.x) as f32 / track.w.max(1) as f32).clamp(0.0, 1.0);
    OPACITY_MIN + (ratio * span as f32).round() as u8
}

/// 百分比在轨道上的横坐标。
fn slider_x(track: &Box2, percent: u8) -> i32 {
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
                        if p.rect.contains(x, y) {
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

pub fn nav_hit(page: &Page, x: i32, y: i32) -> Option<usize> {
    page.chips.iter().find(|c| c.rect.contains(x, y)).map(|c| c.group)
}

// --------------------------------------------------------------------------
// 绘制
// --------------------------------------------------------------------------

pub unsafe fn paint_nav(hdc: HDC, page: &Page, scroll: i32, hover: i32, font_scale: f32) {
    // 当前分组 = 内容坐标 scroll 之上最靠下的那一个
    let mut current = 0;
    for (group, top) in page.group_tops.iter().enumerate() {
        if *top <= scroll + 16 {
            current = group;
        }
    }
    for chip in &page.chips {
        let active = chip.group == current;
        render_choice_pill(
            hdc,
            chip.rect.x,
            chip.rect.y,
            chip.rect.w,
            chip.rect.h,
            chip.label,
            active,
            hover == chip.id,
            font_scale,
        );
    }
}

/// `origin` 是内容视口在窗口中的上沿，滚动量从视口顶算起。
pub unsafe fn paint_cards(hdc: HDC, page: &Page, ctx: &Ctx, scroll: i32, hover: i32, origin: i32) {
    let sy = scroll - origin;
    let f_title = create_clear_font((14.0 * ctx.font_scale).round() as i32, true);
    let f_desc = create_clear_font((12.0 * ctx.font_scale).round() as i32, false);

    for card in &page.cards {
        let bg = CreateSolidBrush(CLR_CARD);
        let border = CreatePen(PS_SOLID, 1, CLR_CARD_BORDER);
        let ob = SelectObject(hdc, bg);
        let op = SelectObject(hdc, border);
        RoundRect(
            hdc,
            card.rect.x,
            card.rect.y - sy,
            card.rect.x + card.rect.w + 1,
            card.rect.y - sy + card.rect.h + 1,
            10,
            10,
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
        SetTextColor(hdc, CLR_TEXT);
        draw_text_clean(hdc, &card.title, &mut title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        if let Some(desc) = &card.desc {
            let mut desc_rect = RECT {
                left: card.rect.x + CARD_PAD,
                top: card.desc_y - sy,
                right: card.rect.x + card.rect.w - CARD_PAD,
                bottom: card.desc_y - sy + 18,
            };
            SelectObject(hdc, f_desc);
            SetTextColor(hdc, CLR_MUTED);
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
                        render_choice_pill(
                            hdc,
                            p.rect.x,
                            p.rect.y - sy,
                            p.rect.w,
                            p.rect.h,
                            &p.label,
                            p.active,
                            hover == p.id,
                            ctx.font_scale,
                        );
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

unsafe fn paint_slider(hdc: HDC, s: &SliderBox, ctx: &Ctx, sy: i32, hover: i32) {
    let f_lbl = create_clear_font((12.0 * ctx.font_scale).round() as i32, false);
    let old = SelectObject(hdc, f_lbl);

    // 左标题 + 右数值
    SetTextColor(hdc, CLR_MUTED);
    let mut lbl = RECT {
        left: s.rect.x,
        top: s.rect.y - sy,
        right: s.rect.x + s.rect.w / 2,
        bottom: s.rect.y - sy + 18,
    };
    draw_text_clean(hdc, "不透明度", &mut lbl, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    SetTextColor(hdc, CLR_ACCENT);
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
    let bar_y = ty + s.track.h / 2 - 2;
    let bar_h = 4;

    let bg = CreateSolidBrush(0x002A2420);
    let bg_rect = RECT { left: s.track.x, top: bar_y, right: s.track.x + s.track.w, bottom: bar_y + bar_h };
    FillRect(hdc, &bg_rect, bg);
    DeleteObject(bg);

    let knob_x = slider_x(&s.track, ctx.ball_opacity);
    let fg = CreateSolidBrush(CLR_ACCENT);
    let fg_rect = RECT { left: s.track.x, top: bar_y, right: knob_x.max(s.track.x + 1), bottom: bar_y + bar_h };
    FillRect(hdc, &fg_rect, fg);
    DeleteObject(fg);

    let (knob_clr, knob_r) = if hover == SLIDER_ID {
        (CLR_TEXT, 7)
    } else {
        (0x00E0D8D0, 6)
    };
    let brush = CreateSolidBrush(knob_clr);
    let pen = CreatePen(PS_SOLID, 1, CLR_ACCENT);
    let ob = SelectObject(hdc, brush);
    let op = SelectObject(hdc, pen);
    let cy = bar_y + bar_h / 2;
    Ellipse(hdc, knob_x - knob_r, cy - knob_r, knob_x + knob_r + 1, cy + knob_r + 1);
    SelectObject(hdc, ob);
    DeleteObject(brush);
    SelectObject(hdc, op);
    DeleteObject(pen);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> Ctx {
        let c = crate::config::AppConfig::default();
        Ctx {
            font_scale: c.font_scale,
            poll_ms: c.poll_interval_ms,
            ball_on: c.enable_floating_ball,
            ball_opacity: c.floating_ball_opacity,
            enable_toast: c.enable_battery_toast,
            toast_info: c.toast_threshold_info,
            toast_warn: c.toast_threshold_warning,
            toast_crit: c.toast_threshold_critical,
            toast_gaming: c.toast_gaming_suppression,
            toast_sound: c.enable_low_battery_sound,
            ui_density: c.ui_density,
            device_filter: c.device_filter,
            device_sort: c.device_sort,
            accent_theme: c.accent_theme,
            auto_update: c.check_updates,
            auto_start: false,
            update_line: "已是最新版本".into(),
            update_available: false,
            latest_version: None,
            config_path: "C:/tmp/config.json".into(),
        }
    }

    #[test]
    fn cards_never_overlap_and_fit_the_viewport_width() {
        for width in [480, 640, 900] {
            let page = layout(&test_ctx(), width);
            for pair in page.cards.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                assert!(
                    b.rect.y >= a.rect.y + a.rect.h,
                    "card overlap at width {width}: {} vs {}",
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
                                "pill {} overflows card {} at width {width}",
                                p.label,
                                card.title
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn slider_covers_the_full_opacity_range() {
        let page = layout(&test_ctx(), 640);
        let t = &page.slider;
        assert_eq!(slider_value(t, t.x), OPACITY_MIN);
        assert_eq!(slider_value(t, t.x + t.w), OPACITY_MAX);
        assert_eq!(slider_value(t, t.x + t.w / 2), 70);
        // 越界点击夹到两端，不会把透明度推到 0
        assert_eq!(slider_value(t, t.x - 500), OPACITY_MIN);
        assert_eq!(slider_value(t, t.x + t.w + 500), OPACITY_MAX);
    }

    #[test]
    fn hit_test_resolves_controls_inside_their_cards() {
        let ctx = test_ctx();
        let page = layout(&ctx, 640);

        let toggle = page
            .cards
            .iter()
            .flat_map(|c| c.rows.iter())
            .find_map(|r| match r {
                RowBox::Pills(p) => p.iter().find(|p| p.id == 50),
                _ => None,
            })
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
}
