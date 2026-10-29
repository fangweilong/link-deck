//! 各分页的卡片内容：一行行 `Pill` / `Slider` 的描述，不含坐标。

use super::*;
use crate::config::{AlertSound, DeviceFilter, DeviceSort, UiDensity, UiStyle};
use crate::ui::theme;

fn pill_of(
    label: impl Into<String>,
    id: i32,
    active: bool,
    w: i32,
    kind: PillKind,
    color: u32,
) -> Pill {
    Pill { label: label.into(), id, active, w, kind, color }
}

/// 只读数值（点不到）
fn value(label: impl Into<String>, w: i32) -> Pill {
    pill_of(label, 0, true, w, PillKind::Value, 0)
}
/// 强调按钮（触发动作，填充强调色）
fn action(label: impl Into<String>, id: i32, w: i32) -> Pill {
    pill_of(label, id, false, w, PillKind::Action, 0)
}


/// 开关项：开 / 关两种状态，开启时整块填强调色
fn toggle(label: impl Into<String>, id: i32, on: bool, w: i32) -> Pill {
    pill_of(label, id, on, w, PillKind::Toggle, 0)
}

/// 单选项
fn pill(label: impl Into<String>, id: i32, active: bool, w: i32) -> Pill {
    pill_of(label, id, active, w, PillKind::Choice, 0)
}

/// 步进按钮（− / +）
fn step(symbol: &str, id: i32) -> Pill {
    pill_of(symbol, id, false, 40, PillKind::Step, 0)
}

/// 纯文字标签
fn label_pill(text: impl Into<String>, w: i32) -> Pill {
    pill_of(text, 0, false, w, PillKind::Label, 0)
}

/// 阈值一组：`标签  −  40%  +`。
///
/// 两两成行是硬约束（不靠自动换行）：四组塞一行会在窗口稍窄时把最后一组
/// 劈成两半，读起来像行尾多出一截。
fn step_group(text: impl Into<String>, value_text: impl Into<String>, minus: i32, plus: i32) -> Vec<Pill> {
    vec![
        label_pill(text, 72),
        step("−", minus),
        value(value_text, 60),
        step("+", plus),
    ]
}

/// 界面风格选择项：标签带一个色块符号，选中态即当前风格。
fn style_pill(style: UiStyle, id: i32, current: UiStyle) -> Pill {
    let label = format!("● {}", theme::label(style));
    pill(label, id, current == style, 82)
}

/// 自定义配色候选色块
fn swatch_pill(colour: u32, name: &str, id: i32, active: bool) -> Pill {
    pill_of(name, id, active, 84, PillKind::Swatch, colour)
}

pub(super) fn build(ctx: &Ctx) -> Vec<Card> {
    match ctx.active_group {
        0 => vec![
            style_card(ctx),
            custom_color_card(ctx),
            typography_card(ctx),
        ],
        1 => vec![ball_shape_card(ctx), ball_display_card(ctx)],
        2 => vec![toast_switch_card(ctx), threshold_card(ctx), sound_card(ctx)],
        3 => vec![device_card(ctx), poll_card(ctx)],
        _ => vec![update_card(ctx), autostart_card(ctx), config_card(ctx)],
    }
}

// ---- 0 界面 ----

fn style_card(ctx: &Ctx) -> Card {
    // 这张卡不放描述：选项本身就是「暖橙 / 冷岩 / 极光…」，再写一行说明只是噪音，
    // 省下的 22px 也让「界面」分页在默认窗口下不用滚动。
    //
    // 候选表与 id 都由 `theme::ALL_STYLES` 的顺序推导：面板里按同一个区间派发，
    // 两处不会因为漏改一项而对不上。
    let mut pills: Vec<Pill> = theme::ALL_STYLES
        .iter()
        .enumerate()
        .map(|(i, style)| style_pill(*style, STYLE_ID_BASE + i as i32, ctx.ui_style))
        .collect();
    pills.push(pill(
        "自定义配色",
        STYLE_ID_BASE + theme::ALL_STYLES.len() as i32,
        matches!(ctx.ui_style, UiStyle::Custom),
        100,
    ));
    Card {
        title: "界面风格".into(),
        desc: None,
        rows: vec![Row::Pills(pills)],
    }
}

fn custom_color_card(ctx: &Ctx) -> Card {
    let custom = matches!(ctx.ui_style, UiStyle::Custom);
    let mut swatches = Vec::new();
    for (i, (colour, name)) in theme::ACCENT_CHOICES.iter().enumerate() {
        swatches.push(swatch_pill(
            *colour,
            name,
            200 + i as i32,
            custom && ctx.custom_accent == *colour,
        ));
    }
    Card {
        title: "自定义配色".into(),
        desc: Some("只挑一个主色，其余中性色由程序派生，不会出现看不清的组合。".into()),
        rows: vec![
            Row::Pills(swatches),
            Row::Pills(vec![
                pill("深色底", 220, custom && !ctx.custom_light, 84),
                pill("浅色底", 221, custom && ctx.custom_light, 84),
            ]),
        ],
    }
}

fn typography_card(ctx: &Ctx) -> Card {
    let s100 = ctx.font_scale <= 1.05;
    let s115 = ctx.font_scale > 1.05 && ctx.font_scale <= 1.22;
    let s130 = ctx.font_scale > 1.22;
    Card {
        title: "文字与密度".into(),
        desc: Some("字号决定可读性，密度决定卡片之间留多少呼吸。".into()),
        rows: vec![
            Row::Pills(vec![
                pill("100% 标准", 40, s100, 92),
                pill("115% 适中", 41, s115, 92),
                pill("130% 放大", 42, s130, 92),
            ]),
            Row::Pills(vec![
                pill("紧凑布局", 100, matches!(ctx.ui_density, UiDensity::Compact), 90),
                pill("标准布局", 101, matches!(ctx.ui_density, UiDensity::Standard), 90),
                pill("宽松布局", 102, matches!(ctx.ui_density, UiDensity::Spacious), 90),
            ]),
        ],
    }
}

// ---- 1 悬浮球 ----

fn ball_shape_card(ctx: &Ctx) -> Card {
    use crate::config::FloatingBallStyle::*;
    let shapes = [
        (Tab, 52),
        (Orb, 53),
        (Ring, 54),
        (Pill, 55),
        (Badge, 56),
    ];
    let mut pills = Vec::new();
    for (style, id) in shapes {
        pills.push(pill(
            style.label(),
            id,
            ctx.ball_style == style,
            100,
        ));
    }
    Card {
        title: "停靠形态".into(),
        desc: Some(format!(
            "五种形态，悬停不切换形状。当前：{} · {}。",
            ctx.ball_style.label(),
            ctx.ball_style.hint()
        )),
        rows: vec![Row::Pills(pills)],
    }
}

fn ball_display_card(ctx: &Ctx) -> Card {
    Card {
        title: "显示与不透明度".into(),
        desc: Some("关闭后只保留托盘入口；不透明度只影响收纳态，展开抽屉始终可读。".into()),
        rows: vec![
            Row::Pills(vec![toggle(
                if ctx.ball_on { "● 悬浮球已开启" } else { "○ 悬浮球已关闭" },
                50,
                ctx.ball_on,
                176,
            )]),
            Row::Slider,
        ],
    }
}

// ---- 2 通知 ----

fn toast_switch_card(ctx: &Ctx) -> Card {
    Card {
        title: "低电量提醒".into(),
        desc: Some("跌破阈值时右下角弹窗；全屏游戏或演示时自动让路。".into()),
        rows: vec![Row::Pills(vec![
            toggle(
                if ctx.enable_toast { "● 弹窗已开" } else { "○ 弹窗已关" },
                60,
                ctx.enable_toast,
                132,
            ),
            toggle("游戏免打扰", 61, ctx.toast_gaming, 132),
            toggle(
                if ctx.toast_sound { "● 提示音已开" } else { "○ 提示音已关" },
                62,
                ctx.toast_sound,
                142,
            ),
            pill("试发一条", 80, false, 96),
        ])],
    }
}

fn threshold_card(ctx: &Ctx) -> Card {
    let second = if ctx.toast_second == 0 {
        "关闭".to_string()
    } else {
        format!("{}%", ctx.toast_second)
    };
    let mut row_a = step_group("提示电量", format!("{}%", ctx.toast_info), 70, 71);
    row_a.extend(step_group("警告电量", format!("{}%", ctx.toast_warn), 72, 73));
    let mut row_b = step_group("危险电量", format!("{}%", ctx.toast_crit), 74, 75);
    row_b.extend(step_group("二次提醒", second, 76, 77));
    Card {
        title: "提醒阈值".into(),
        desc: Some("四档依次降低；二次提醒是电量继续下跌后补发的一次危险提示，减到关闭即不再补发。".into()),
        rows: vec![Row::Pills(row_a), Row::Pills(row_b)],
    }
}

fn sound_card(ctx: &Ctx) -> Card {
    let mut choices = Vec::new();
    for (sound, id) in [
        (AlertSound::Voice, 63),
        (AlertSound::Chime, 64),
        (AlertSound::Beep, 65),
        (AlertSound::Pulse, 66),
    ] {
        choices.push(pill(sound.label(), id, ctx.alert_sound == sound, 88));
    }
    choices.push(pill("试听", 81, false, 68));
    Card {
        title: "提示音".into(),
        desc: Some("随危险阈值一起播放；换之前可以先试听。".into()),
        rows: vec![Row::Pills(choices)],
    }
}

// ---- 3 设备 ----

fn device_card(ctx: &Ctx) -> Card {
    Card {
        title: "设备列表".into(),
        desc: Some("筛选与排序只影响显示顺序，不改变后台扫描。".into()),
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
    }
}

fn poll_card(ctx: &Ctx) -> Card {
    Card {
        title: "通信间隔".into(),
        desc: Some("外设电量消耗慢，推荐长周期省电；随时点“立即刷新”可秒级直读。".into()),
        rows: vec![Row::Pills(vec![
            pill("30s 调试", 30, ctx.poll_ms <= 45_000, 72),
            pill("1m 快速", 31, ctx.poll_ms > 45_000 && ctx.poll_ms <= 120_000, 72),
            pill("5m 推荐", 32, ctx.poll_ms > 120_000 && ctx.poll_ms <= 600_000, 78),
            pill("15m 适中", 33, ctx.poll_ms > 600_000 && ctx.poll_ms <= 1_200_000, 78),
            pill("30m 省电", 34, ctx.poll_ms > 1_200_000 && ctx.poll_ms <= 2_700_000, 78),
            pill("1h 极省", 35, ctx.poll_ms > 2_700_000, 78),
        ])],
    }
}

// ---- 4 系统 ----

fn update_card(ctx: &Ctx) -> Card {
    let check_label = if ctx.update_checking { "正在检查…" } else { "立即检查" };
    let mut pills = vec![
        toggle(
            if ctx.auto_update { "● 自动检查更新" } else { "○ 自动检查更新" },
            140,
            ctx.auto_update,
            190,
        ),
        pill(check_label, 141, false, 110),
    ];
    if ctx.update_applying {
        pills.push(value("正在下载并升级…", 160));
    } else if ctx.update_available {
        if let Some(latest) = ctx.latest_version.as_deref() {
            pills.push(action(format!("升级到 v{}", latest), 142, 160));
        }
    }
    Card {
        title: "版本与更新".into(),
        desc: Some(ctx.update_line.clone()),
        rows: vec![Row::Pills(pills)],
    }
}

fn autostart_card(ctx: &Ctx) -> Card {
    Card {
        title: "开机自动启动".into(),
        desc: Some("写入当前用户注册表 Run 项，登录后静默常驻托盘。".into()),
        rows: vec![Row::Pills(vec![toggle(
            if ctx.auto_start { "● 开机自启：已开启" } else { "○ 开机自启：已关闭" },
            150,
            ctx.auto_start,
            200,
        )])],
    }
}

fn config_card(ctx: &Ctx) -> Card {
    Card {
        title: "配置文件目录".into(),
        desc: Some(ctx.config_path.clone()),
        rows: vec![Row::Pills(vec![pill("打开配置目录", 151, false, 160)])],
    }
}
