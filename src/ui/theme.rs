//! 设计令牌层：颜色 / 间距 / 圆角 / 字号。
//!
//! 规则：
//! 1. 界面颜色一律从 `THEME` 取，禁止再散落 `0x00BBGGRR` 字面量；
//! 2. 颜色一律用 `rgb(r, g, b)` 按自然顺序组装，杜绝手写 COLORREF 时字节序写反；
//! 3. 间距一律取 4 / 8 体系，从 `METRICS` 取，禁止随手写 3 / 5 / 7。

use crate::config::UiStyle;
use windows_sys::Win32::Foundation::COLORREF;

/// 按自然顺序组装 COLORREF。COLORREF 内存布局是 0x00BBGGRR，
/// 手写字面量极易写成 `#E08030` → `0x00E08030`（实际渲染成蓝色），统一走这个函数。
pub const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    r | (g << 8) | (b << 16)
}

#[derive(Clone, Copy)]
pub struct Theme {
    /// 窗口底色
    pub bg: COLORREF,
    /// 顶栏 / 表头：与 bg 同色，顶栏靠发丝线分层而不是换底色
    pub header: COLORREF,
    /// 卡片 / 悬浮层底
    pub card: COLORREF,
    /// 卡片内的次级底（行、输入框）
    pub row: COLORREF,
    /// 描边
    pub border: COLORREF,
    pub text: COLORREF,
    pub muted: COLORREF,
    /// 强调色
    pub accent: COLORREF,
    /// 强调色高亮态（hover）
    pub accent_strong: COLORREF,
    /// 强调色底上的文字
    pub accent_text: COLORREF,
    /// 高电量
    pub ok: COLORREF,
    /// 中电量
    pub warn: COLORREF,
    /// 低电量
    pub danger: COLORREF,
    /// 充电中
    pub charging: COLORREF,
    pub btn: COLORREF,
    pub btn_hover: COLORREF,
    pub btn_active: COLORREF,
    pub btn_text: COLORREF,
    /// 进度条底槽
    pub track: COLORREF,
    /// 浅色主题标记。生产代码按令牌取色不需要它，测试与自定义配色的
    /// 明暗推导会读，所以保留字段。
    #[allow(dead_code)]
    pub light: bool,
}

/// 布局度量（4 / 8 间距体系，单位：逻辑像素）
#[derive(Clone, Copy)]
pub struct Metrics {
    /// 页面左右留白
    pub pad_page: i32,
    /// 卡片内边距
    pub pad_card: i32,
    /// 卡片之间（必须 ≥ pad_card，否则卡片会视觉粘连）
    pub gap_card: i32,
    /// 同一组内相邻行
    pub gap_row: i32,
    /// 图标与文字
    pub gap_tight: i32,
    pub r_card: i32,
    pub r_btn: i32,
    pub r_pill: i32,
    pub r_bar: i32,
    /// 字号基准（再乘以用户选择的 font_scale）
    pub fs_head: f32,
    pub fs_title: f32,
    pub fs_body: f32,
    pub fs_meta: f32,
}

const METRICS_BASE: Metrics = Metrics {
    pad_page: 24,
    pad_card: 16,
    gap_card: 16,
    gap_row: 12,
    gap_tight: 8,
    r_card: 12,
    r_btn: 8,
    r_pill: 6,
    r_bar: 6,
    fs_head: 18.0,
    fs_title: 15.0,
    fs_body: 12.5,
    fs_meta: 11.5,
};

// --------------------------------------------------------------------------
// 五套预设
// --------------------------------------------------------------------------

/// 暖橙：暗色暖调，默认风格
const EMBER: Theme = Theme {
    bg: rgb(0x14, 0x12, 0x10),
    header: rgb(0x14, 0x12, 0x10),
    card: rgb(0x1C, 0x18, 0x15),
    row: rgb(0x17, 0x13, 0x10),
    border: rgb(0x34, 0x2C, 0x25),
    text: rgb(0xF2, 0xEE, 0xE9),
    muted: rgb(0xA7, 0x9E, 0x93),
    accent: rgb(0xE8, 0x80, 0x2A),
    accent_strong: rgb(0xF5, 0x9A, 0x4C),
    accent_text: rgb(0x1A, 0x12, 0x0A),
    ok: rgb(0x35, 0xC4, 0x6A),
    warn: rgb(0xF0, 0xA3, 0x2A),
    danger: rgb(0xEF, 0x53, 0x50),
    charging: rgb(0x25, 0xC9, 0xE8),
    btn: rgb(0x2A, 0x23, 0x1E),
    btn_hover: rgb(0x3A, 0x30, 0x28),
    btn_active: rgb(0x4C, 0x40, 0x34),
    btn_text: rgb(0xF2, 0xEE, 0xE9),
    track: rgb(0x2A, 0x23, 0x1E),
    light: false,
};

/// 冷岩：暗色中性蓝灰，最低干扰
const GRAPHITE: Theme = Theme {
    bg: rgb(0x12, 0x14, 0x17),
    header: rgb(0x12, 0x14, 0x17),
    card: rgb(0x19, 0x1C, 0x21),
    row: rgb(0x14, 0x17, 0x1B),
    border: rgb(0x2C, 0x31, 0x38),
    text: rgb(0xE7, 0xEB, 0xF0),
    muted: rgb(0x98, 0xA2, 0xAE),
    accent: rgb(0x4C, 0x8D, 0xFF),
    accent_strong: rgb(0x74, 0xA9, 0xFF),
    accent_text: rgb(0x0A, 0x12, 0x20),
    ok: rgb(0x34, 0xD3, 0x99),
    warn: rgb(0xFB, 0xBF, 0x24),
    danger: rgb(0xF8, 0x71, 0x71),
    charging: rgb(0x22, 0xD3, 0xEE),
    btn: rgb(0x22, 0x26, 0x2C),
    btn_hover: rgb(0x2E, 0x34, 0x3C),
    btn_active: rgb(0x3B, 0x43, 0x4D),
    btn_text: rgb(0xE7, 0xEB, 0xF0),
    track: rgb(0x22, 0x26, 0x2C),
    light: false,
};

/// 极光：暗色紫青，对比最强
const AURORA: Theme = Theme {
    bg: rgb(0x11, 0x12, 0x1B),
    header: rgb(0x11, 0x12, 0x1B),
    card: rgb(0x18, 0x1A, 0x27),
    row: rgb(0x14, 0x15, 0x1F),
    border: rgb(0x2E, 0x31, 0x48),
    text: rgb(0xEA, 0xEA, 0xF6),
    muted: rgb(0x9C, 0x9D, 0xBA),
    accent: rgb(0x9C, 0x6B, 0xFF),
    accent_strong: rgb(0xB7, 0x8D, 0xFF),
    accent_text: rgb(0x12, 0x0A, 0x20),
    ok: rgb(0x4A, 0xDE, 0x80),
    warn: rgb(0xFB, 0xBF, 0x24),
    danger: rgb(0xFB, 0x71, 0x85),
    charging: rgb(0x2D, 0xD4, 0xBF),
    btn: rgb(0x23, 0x25, 0x3A),
    btn_hover: rgb(0x2F, 0x32, 0x50),
    btn_active: rgb(0x3D, 0x41, 0x66),
    btn_text: rgb(0xEA, 0xEA, 0xF6),
    track: rgb(0x23, 0x25, 0x3A),
    light: false,
};

/// 竹青：暗色墨绿，长时间注视最柔和
const JADE: Theme = Theme {
    bg: rgb(0x10, 0x15, 0x12),
    header: rgb(0x10, 0x15, 0x12),
    card: rgb(0x16, 0x1E, 0x19),
    row: rgb(0x12, 0x18, 0x14),
    border: rgb(0x28, 0x36, 0x2E),
    text: rgb(0xE5, 0xEF, 0xE7),
    muted: rgb(0x93, 0xA7, 0x97),
    accent: rgb(0x34, 0xC7, 0x7B),
    accent_strong: rgb(0x5B, 0xD9, 0x95),
    accent_text: rgb(0x08, 0x17, 0x0E),
    ok: rgb(0x4A, 0xDE, 0x80),
    warn: rgb(0xF5, 0xC5, 0x42),
    danger: rgb(0xEF, 0x53, 0x50),
    charging: rgb(0x22, 0xD3, 0xEE),
    btn: rgb(0x1E, 0x28, 0x22),
    btn_hover: rgb(0x29, 0x36, 0x2D),
    btn_active: rgb(0x35, 0x46, 0x3A),
    btn_text: rgb(0xE5, 0xEF, 0xE7),
    track: rgb(0x1E, 0x28, 0x22),
    light: false,
};

/// 雪原：浅色主题，白天与高亮环境下更清晰
const SNOW: Theme = Theme {
    bg: rgb(0xF4, 0xF6, 0xF8),
    header: rgb(0xF4, 0xF6, 0xF8),
    card: rgb(0xFF, 0xFF, 0xFF),
    row: rgb(0xF0, 0xF2, 0xF5),
    border: rgb(0xDF, 0xE4, 0xEA),
    text: rgb(0x14, 0x18, 0x1D),
    muted: rgb(0x66, 0x70, 0x7C),
    accent: rgb(0xE0, 0x70, 0x30),
    accent_strong: rgb(0xF0, 0x89, 0x4C),
    accent_text: rgb(0xFF, 0xFF, 0xFF),
    ok: rgb(0x16, 0xA3, 0x4A),
    warn: rgb(0xD9, 0x77, 0x06),
    danger: rgb(0xDC, 0x26, 0x26),
    charging: rgb(0x08, 0x91, 0xB2),
    btn: rgb(0xF0, 0xF2, 0xF5),
    btn_hover: rgb(0xE4, 0xE8, 0xED),
    btn_active: rgb(0xD8, 0xDE, 0xE6),
    btn_text: rgb(0x14, 0x18, 0x1D),
    track: rgb(0xE4, 0xE8, 0xED),
    light: true,
};

/// 当前生效的色板。切换风格时由 `apply()` 整体替换。
pub static mut THEME: Theme = EMBER;
/// 布局度量是编译期常量：运行期不变，因此不需要可变静态。
pub const METRICS: Metrics = METRICS_BASE;

pub fn palette(style: UiStyle) -> Theme {
    match style {
        UiStyle::Ember => EMBER,
        UiStyle::Graphite => GRAPHITE,
        UiStyle::Aurora => AURORA,
        UiStyle::Jade => JADE,
        UiStyle::Snow => SNOW,
        // 自定义风格要带主色，没有主色时退回默认预设
        UiStyle::Custom => EMBER,
    }
}

fn split_rgb(c: u32) -> (u32, u32, u32) {
    ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF)
}

/// `0xRRGGBB` → `COLORREF`。自定义配色在配置里按自然顺序存，绘制前统一换算。
pub fn from_rgb24(c: u32) -> COLORREF {
    let (r, g, b) = split_rgb(c);
    rgb(r, g, b)
}

/// 该底色上应该压深字还是浅字（BT.601 感知亮度）。
pub fn text_on(color: u32) -> COLORREF {
    let (r, g, b) = split_rgb(color);
    let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if lum > 165.0 {
        rgb(0x14, 0x12, 0x10)
    } else {
        rgb(0xFF, 0xFF, 0xFF)
    }
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 * (1.0 - t) + b as f32 * t).round() as u32
}

/// 由「一个主色 + 明暗」派生整套色板。
///
/// 只让用户挑一个主色，其余中性色仍按预设的中性阶走，
/// 这样无论用户挑什么颜色都不会出现「文字看不清 / 卡片糊成一片」的组合。
pub fn custom_theme(accent_rgb: u32, light: bool) -> Theme {
    let (r, g, b) = split_rgb(accent_rgb);
    let accent = rgb(r, g, b);
    // 感知亮度（ITU-R BT.601）：决定强调色上的文字用黑还是白
    let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let accent_strong = rgb(mix(r, 255, 0.24), mix(g, 255, 0.24), mix(b, 255, 0.24));
    // 亮色主色配深字，暗色主色配白字
    let accent_text = if lum > 165.0 { rgb(0x14, 0x12, 0x10) } else { rgb(0xFF, 0xFF, 0xFF) };

    if light {
        Theme {
            bg: rgb(0xF4, 0xF6, 0xF8),
            header: rgb(0xF4, 0xF6, 0xF8),
            card: rgb(0xFF, 0xFF, 0xFF),
            row: rgb(0xF0, 0xF2, 0xF5),
            border: rgb(0xDF, 0xE4, 0xEA),
            text: rgb(0x14, 0x18, 0x1D),
            muted: rgb(0x66, 0x70, 0x7C),
            accent,
            accent_strong,
            accent_text,
            ok: rgb(0x16, 0xA3, 0x4A),
            warn: rgb(0xD9, 0x77, 0x06),
            danger: rgb(0xDC, 0x26, 0x26),
            charging: rgb(0x08, 0x91, 0xB2),
            btn: rgb(0xF0, 0xF2, 0xF5),
            btn_hover: rgb(0xE4, 0xE8, 0xED),
            btn_active: rgb(0xD8, 0xDE, 0xE6),
            btn_text: rgb(0x14, 0x18, 0x1D),
            track: rgb(0xE4, 0xE8, 0xED),
            light: true,
        }
    } else {
        Theme {
            bg: rgb(0x14, 0x12, 0x10),
            header: rgb(0x14, 0x12, 0x10),
            card: rgb(0x1C, 0x18, 0x15),
            row: rgb(0x17, 0x13, 0x10),
            border: rgb(0x34, 0x2C, 0x25),
            text: rgb(0xF2, 0xEE, 0xE9),
            muted: rgb(0xA7, 0x9E, 0x93),
            accent,
            accent_strong,
            accent_text,
            ok: rgb(0x35, 0xC4, 0x6A),
            warn: rgb(0xF0, 0xA3, 0x2A),
            danger: rgb(0xEF, 0x53, 0x50),
            charging: rgb(0x25, 0xC9, 0xE8),
            btn: rgb(0x2A, 0x23, 0x1E),
            btn_hover: rgb(0x3A, 0x30, 0x28),
            btn_active: rgb(0x4C, 0x40, 0x34),
            btn_text: rgb(0xF2, 0xEE, 0xE9),
            track: rgb(0x2A, 0x23, 0x1E),
            light: false,
        }
    }
}

/// 应用自定义风格：主色 + 明暗。
pub unsafe fn apply_custom(accent_rgb: u32, light: bool) {
    THEME = custom_theme(accent_rgb, light);
}

/// 切换到指定风格。所有界面下一帧自动生效（绘制时读 THEME）。
pub unsafe fn apply(style: UiStyle) {
    THEME = palette(style);
}

/// 五套预设，按设置页的展示顺序排列。
///
/// 这是「有哪些风格」的唯一来源：设置页按顺序生成选项，面板按同一顺序
/// 从 `settings::STYLE_ID_BASE` 起派发点击，两边不会漂移。
pub const ALL_STYLES: [UiStyle; 5] = [
    UiStyle::Ember,
    UiStyle::Graphite,
    UiStyle::Aurora,
    UiStyle::Jade,
    UiStyle::Snow,
];

pub fn label(style: UiStyle) -> &'static str {
    match style {
        UiStyle::Ember => "暖橙",
        UiStyle::Graphite => "冷岩",
        UiStyle::Aurora => "极光",
        UiStyle::Jade => "竹青",
        UiStyle::Snow => "雪原",
        UiStyle::Custom => "自定义",
    }
}

/// 自定义配色的候选主色（12 色）。设置页直接拿它画色块。
pub const ACCENT_CHOICES: [(u32, &str); 12] = [
    (0xE8802A, "暖橙"),
    (0xF2C14E, "琥珀"),
    (0xE0574C, "珊瑚"),
    (0xD9537E, "玫红"),
    (0x9C6BFF, "紫罗兰"),
    (0x6C7BFF, "靛蓝"),
    (0x4C8DFF, "天蓝"),
    (0x25C9E8, "青碧"),
    (0x34C77B, "松绿"),
    (0x8FBF3F, "草绿"),
    (0xB0B7C3, "石墨灰"),
    (0xF0EDE6, "暖白"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_packs_in_colorref_order() {
        // #E08030 必须渲染成橙色，而不是把字节序写反成蓝色
        assert_eq!(rgb(0xE0, 0x80, 0x30), 0x0030_80E0);
        assert_eq!(rgb(0xFF, 0xFF, 0xFF), 0x00FF_FFFF);
    }

    #[test]
    fn every_palette_is_distinct() {
        let mut seen = std::collections::HashSet::new();
        for s in ALL_STYLES {
            let t = palette(s);
            assert!(seen.insert(t.accent), "强调色重复: {:?}", s);
            assert!(seen.insert(t.bg), "底色重复: {:?}", s);
        }
    }

    #[test]
    fn header_blends_into_the_background() {
        // 融合约定：顶栏不再是一块独立色带，靠发丝线分层。
        // 谁要是想让顶栏重新显出"色块感"，请先改这条测试并说明理由。
        for s in ALL_STYLES {
            assert_eq!(palette(s).header, palette(s).bg, "{:?} 顶栏底色必须与窗口底色一致", s);
        }
    }

    #[test]
    fn custom_theme_keeps_the_user_accent_exactly() {
        let t = custom_theme(0x4C8DFF, false);
        assert_eq!(t.accent, rgb(0x4C, 0x8D, 0xFF), "主色必须原样使用");
        assert!(!t.light);
        // 顶栏与底色一致（融合约定对自定义风格同样成立）
        assert_eq!(t.header, t.bg);
        // 亮主色配深字，暗主色配白字
        let bright = custom_theme(0xF2C14E, false);
        assert_eq!(bright.accent_text, rgb(0x14, 0x12, 0x10));
        let deep = custom_theme(0x34225C, false);
        assert_eq!(deep.accent_text, rgb(0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn custom_theme_light_variant_has_dark_text() {
        let t = custom_theme(0xE8802A, true);
        assert!(t.light);
        fn lum(c: COLORREF) -> u32 {
            (c & 0xFF) + ((c >> 8) & 0xFF) + ((c >> 16) & 0xFF)
        }
        assert!(lum(t.text) < lum(t.bg), "浅色底必须配深色字");
        assert_eq!(t.header, t.bg);
    }

    #[test]
    fn accent_choices_are_unique_and_readable() {
        let mut seen = std::collections::HashSet::new();
        for (c, name) in ACCENT_CHOICES {
            assert!(seen.insert(c), "候选色重复: {name}");
            let t = custom_theme(c, false);
            // 每个候选色都必须能读出强调色上的文字
            assert_ne!(t.accent_text, t.accent, "{name} 的文字与底色相同");
        }
    }

    #[test]
    fn light_theme_has_dark_text() {
        let t = palette(UiStyle::Snow);
        assert!(t.light);
        // 浅色主题：文字必须比底色暗
        fn lum(c: COLORREF) -> u32 {
            (c & 0xFF) + ((c >> 8) & 0xFF) + ((c >> 16) & 0xFF)
        }
        assert!(lum(t.text) < lum(t.bg));
        assert!(lum(t.accent_text) > lum(t.accent) || t.accent_text == 0x00FF_FFFF);
    }

    #[test]
    fn dark_themes_have_light_text() {
        fn lum(c: COLORREF) -> u32 {
            (c & 0xFF) + ((c >> 8) & 0xFF) + ((c >> 16) & 0xFF)
        }
        for s in ALL_STYLES {
            let t = palette(s);
            if t.light {
                continue;
            }
            assert!(lum(t.text) > lum(t.bg), "{:?} 文字不够亮", s);
            assert!(lum(t.muted) > lum(t.bg), "{:?} 次要文字不够亮", s);
        }
    }

    #[test]
    fn spacing_follows_eight_grid() {
        let m = METRICS_BASE;
        for v in [m.pad_page, m.pad_card, m.gap_card, m.gap_row, m.gap_tight] {
            assert_eq!(v % 4, 0, "间距 {} 不在 4/8 体系内", v);
        }
        assert!(m.gap_card >= m.pad_card, "卡片间距必须 ≥ 卡片内边距");
    }
}
