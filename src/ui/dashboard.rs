#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]

//! 设备监控页（dashboard）布局 —— 网格瓦片版。
//!
//! 旧实现是纵向单列卡片，设备一多就变成一条长列表，电量被淹没在文字里。
//! 这里改为**自适应网格瓦片**：每台设备一块瓷砖，自上而下是
//! `设备名 → 大号电量 → 电量条 → 连接/电压`，电量是视觉主角。
//!
//! 列数由窗宽反推（最小瓦片宽 240，最多 3 列），行步进 = 瓦片高 + gap_card。
//! `layout()` 产出带坐标的布局树，绘制与命中测试读同一份结果。

use crate::config::UiDensity;
use crate::devices::types::{ConnectionMode, DeviceSnapshot};
use crate::ui::panel::{create_clear_font, draw_text_clean};
use crate::ui::panel_render::{render_action_btn, render_device_tile};
pub use crate::ui::layout::Rect2;
use crate::ui::theme::{METRICS, THEME};
use crate::ui::titlebar;
use std::ptr;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    CreateRectRgn, CreateSolidBrush, DeleteObject, FillRect, SelectClipRgn, SelectObject,
    SetTextColor, HDC, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
};

/// 顶栏高度（8 的倍数）
pub const HEADER_H: i32 = 64;
/// 底栏高度
pub const FOOTER_H: i32 = 64;
/// 顶栏右侧的页面切换（连体分段控件），两段等宽
const SEG_W: i32 = 192;
const SEG_H: i32 = 32;
/// 底栏按钮高度
const BTN_H: i32 = 36;

/// 瓦片最小宽度：低于此值就少放一列（避免文字挤成一团）
const MIN_TILE_W: i32 = 240;
/// 最多 3 列，再多瓦片就窄得放不下设备名了
const MAX_COLS: i32 = 3;
/// 瓦片底部两个小按钮（复制 / 改名）
const TILE_BTN_W: i32 = 44;
const TILE_BTN_H: i32 = 22;
/// 右上角状态词（良好 / 偏低 / 需充电 / 充电中 / 待机）
const CHIP_W: i32 = 48;

/// 瓦片内部的纵向节奏。各档位的 `(各段之和 + pad_bottom)` 必须等于 `h`。
struct TileSpec {
    pad: i32,
    name_h: i32,
    gap_name: i32,
    pct_h: i32,
    gap_bar: i32,
    bar_h: i32,
    gap_meta: i32,
    meta_h: i32,
    h: i32,
}

fn tile_spec(d: UiDensity) -> TileSpec {
    match d {
        UiDensity::Compact => TileSpec {
            pad: 12, name_h: 18, gap_name: 2, pct_h: 32,
            gap_bar: 6, bar_h: 8, gap_meta: 6, meta_h: 20, h: 120,
        },
        UiDensity::Standard => TileSpec {
            pad: 16, name_h: 20, gap_name: 4, pct_h: 40,
            gap_bar: 8, bar_h: 8, gap_meta: 8, meta_h: 24, h: 144,
        },
        UiDensity::Spacious => TileSpec {
            pad: 20, name_h: 22, gap_name: 6, pct_h: 48,
            gap_bar: 10, bar_h: 8, gap_meta: 10, meta_h: 24, h: 168,
        },
    }
}

/// 单块设备瓦片。几何全部由 `layout()` 给出，渲染函数不再自己算坐标。
#[derive(Clone, Copy)]
pub struct TileBox {
    pub rect: Rect2,
    pub name: Rect2,
    pub chip: Rect2,
    pub pct: Rect2,
    pub bar: Rect2,
    pub meta: Rect2,
    pub copy: Rect2,
    pub edit: Rect2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    TabDevice,
    TabSettings,
    /// 瓦片本体（选中）
    Card(usize),
    /// 瓦片上的「复制」
    Copy(usize),
    /// 瓦片上的「改名」
    Edit(usize),
    Refresh,
    ShowAll,
    Start,
}

/// 顶栏高度：设备页只有标题行，设置页把分类导航也收进顶栏（融合成一条）。
pub fn header_height(active_tab: usize) -> i32 {
    if active_tab == 1 {
        crate::ui::settings::CONTENT_TOP
    } else {
        titlebar::TITLE_H + HEADER_H
    }
}

/// 顶栏文案：主标题是「当前页在做什么」，副标题给这一页的状态，
/// 两页共用同一个顶栏，页面里不再重复写一遍标题。
pub fn header_copy(active_tab: usize, devices: &[DeviceSnapshot]) -> (String, String) {
    if active_tab == 1 {
        // 副标题不再复述分类名（分类就在下面一行导航里），只说这一页的规则
        return (
            "偏好设置".to_string(),
            "5 组配置 · 修改后立即生效".to_string(),
        );
    }
    let low = devices
        .iter()
        .filter(|d| !d.battery.charging && d.battery.online && d.battery.percentage.map_or(false, |p| p < 20))
        .count();
    if devices.is_empty() {
        (
            "外设控制中心".to_string(),
            "状态栏常驻 · 贴边悬浮球 · 2.4G/蓝牙多协议电量管理".to_string(),
        )
    } else if low > 0 {
        (
            "外设控制中心".to_string(),
            format!("{} 台设备在线 · {} 台需要充电", devices.len(), low),
        )
    } else {
        (
            "外设控制中心".to_string(),
            format!("{} 台设备在线 · 电量充足", devices.len()),
        )
    }
}

/// 滚动可视区高度：整窗高度减去自绘标题栏、顶栏、底栏与顶栏下方留白。
pub fn viewport_height(window_h: i32) -> i32 {
    (window_h - titlebar::TITLE_H - HEADER_H - FOOTER_H - METRICS.gap_tight).max(1)
}

/// 瓦片高度与步进（步进 = 高度 + gap_card，保证瓦片间距 ≥ 瓦片内边距）
pub fn card_geometry(density: UiDensity) -> (i32, i32) {
    let h = match density {
        UiDensity::Compact => 120,
        UiDensity::Standard => 144,
        UiDensity::Spacious => 168,
    };
    (h, h + METRICS.gap_card)
}

/// 列数由可用宽度反推，保证每列不窄于 `MIN_TILE_W`
pub fn grid_cols(width: i32) -> i32 {
    let usable = (width - METRICS.pad_page * 2).max(MIN_TILE_W);
    ((usable + METRICS.gap_card) / (MIN_TILE_W + METRICS.gap_card)).clamp(1, MAX_COLS)
}

pub struct DashLayout {
    pub width: i32,
    pub height: i32,
    pub header: Rect2,
    pub tab_device: Rect2,
    pub tab_settings: Rect2,
    /// 滚动裁剪区
    pub content_top: i32,
    pub content_bottom: i32,
    pub tiles: Vec<TileBox>,
    /// 网格列数
    pub cols: i32,
    pub tile_w: i32,
    pub card_h: i32,
    pub card_step: i32,
    pub content_h: i32,
    pub footer: Rect2,
    pub btn_refresh: Rect2,
    pub status: Rect2,
    pub version: Rect2,
    /// 筛选后为空时的「显示全部设备」
    pub btn_show_all: Option<Rect2>,
    /// 首次运行引导面板
    pub onboarding: Option<Rect2>,
    pub btn_start: Option<Rect2>,
    pub empty_title: Option<Rect2>,
    pub empty_tip: Option<Rect2>,
}

/// 一次算完整页坐标。scroll_y 直接参与计算，因此布局结果即屏幕坐标。
pub fn layout(
    width: i32,
    height: i32,
    density: UiDensity,
    font_scale: f32,
    devices: &[DeviceSnapshot],
    raw_count: usize,
    show_onboarding: bool,
    scroll_y: i32,
) -> DashLayout {
    let pad = METRICS.pad_page;
    // 客户区铺满整窗（无系统标题栏），内容整体下移自绘标题栏的高度
    let title_h = titlebar::TITLE_H;
    let content_top = title_h + HEADER_H + METRICS.gap_tight;
    let content_bottom = (height - FOOTER_H).max(content_top + 1);
    let (tile_h, tile_step) = card_geometry(density);
    let spec = tile_spec(density);

    let cols = grid_cols(width);
    let gap = METRICS.gap_card;
    let usable = (width - pad * 2).max(MIN_TILE_W);
    let tile_w = (usable - gap * (cols - 1)) / cols;
    // 底部留白 = 瓦片高减去上面各段之和（各档位都留下 ≥12 的余量）
    let pad_bottom = tile_h
        - (spec.pad + spec.name_h + spec.gap_name + spec.pct_h + spec.gap_bar + spec.bar_h
            + spec.gap_meta + spec.meta_h);

    let mut tiles = Vec::with_capacity(devices.len());
    for idx in 0..devices.len() {
        let col = idx as i32 % cols;
        let row = idx as i32 / cols;
        let x = pad + col * (tile_w + gap);
        let y = content_top + row * tile_step - scroll_y;

        let rect = Rect2::new(x, y, tile_w, tile_h);
        let inner_x = x + spec.pad;
        let inner_w = tile_w - spec.pad * 2;

        let name = Rect2::new(
            inner_x,
            y + spec.pad,
            (inner_w - CHIP_W - METRICS.gap_tight).max(1),
            spec.name_h,
        );
        let chip = Rect2::new(x + tile_w - spec.pad - CHIP_W, y + spec.pad, CHIP_W, spec.name_h);
        let pct = Rect2::new(inner_x, name.bottom() + spec.gap_name, inner_w, spec.pct_h);
        let bar = Rect2::new(inner_x, pct.bottom() + spec.gap_bar, inner_w, spec.bar_h);
        let meta = Rect2::new(
            inner_x,
            bar.bottom() + spec.gap_meta,
            (inner_w - TILE_BTN_W * 2 - METRICS.gap_tight - METRICS.gap_row).max(1),
            spec.meta_h,
        );
        let edit = Rect2::new(
            x + tile_w - spec.pad - TILE_BTN_W,
            y + tile_h - pad_bottom - TILE_BTN_H,
            TILE_BTN_W,
            TILE_BTN_H,
        );
        let copy = Rect2::new(edit.x - METRICS.gap_tight - TILE_BTN_W, edit.y, TILE_BTN_W, TILE_BTN_H);

        tiles.push(TileBox { rect, name, chip, pct, bar, meta, copy, edit });
    }
    let rows = (devices.len() as i32 + cols - 1) / cols;
    let content_h = (rows * tile_step).max(1);

    // 顶栏右侧分段控件：一整条 SEG_W，左右两半各是一段
    let tab_y = title_h + (HEADER_H - SEG_H) / 2;
    let tab_device = Rect2::new(width - pad - SEG_W, tab_y, SEG_W / 2, SEG_H);
    let tab_settings = Rect2::new(tab_device.right(), tab_y, SEG_W - SEG_W / 2, SEG_H);

    // 底栏：刷新 + 状态 + 版本号。页面切换已经收进顶栏分段控件，底栏不再重复一个入口
    let footer = Rect2::new(0, (height - FOOTER_H).max(0), width, FOOTER_H);
    let btn_y = footer.y + (FOOTER_H - BTN_H) / 2;
    let btn_refresh = Rect2::new(pad, btn_y, 160, BTN_H);
    let status = Rect2::new(
        btn_refresh.right() + METRICS.gap_row,
        footer.y + (FOOTER_H - 24) / 2,
        (width - pad - 130 - METRICS.gap_row - btn_refresh.right() - METRICS.gap_row).max(1),
        24,
    );
    let version = Rect2::new(width - pad - 130, footer.y + (FOOTER_H - 24) / 2, 130, 24);

    // 引导面板：只在完全没有设备时出现
    //
    // 首屏有一小段时间「设备列表还没刷新出来」，引导卡和空态会同时在场，两块
    // 必须上下排开（曾经两者画在同一块区域，文字直接叠成一团）。窗口被压到最
    // 小时排不下，就先让掉引导卡——它是锦上添花，空态那三行硬件说明才是必需。
    const ONBOARDING_H: i32 = 174;
    const EMPTY_TITLE_H: i32 = 26;
    const EMPTY_TIP_H: i32 = 64;
    let onboarding_top = content_top + METRICS.gap_tight;
    let empty_block_h = EMPTY_TITLE_H + METRICS.gap_row + EMPTY_TIP_H;
    let stacked_bottom = onboarding_top + ONBOARDING_H + METRICS.gap_card + empty_block_h;
    let show_ob = show_onboarding && raw_count == 0 && stacked_bottom <= content_bottom;
    let (onboarding, btn_start) = if show_ob {
        let box_rect = Rect2::new(pad, onboarding_top, width - pad * 2, ONBOARDING_H);
        let btn = Rect2::new(box_rect.x + METRICS.pad_card, box_rect.bottom() - METRICS.pad_card - BTN_H, 150, 32);
        (Some(box_rect), Some(btn))
    } else {
        (None, None)
    };

    // 空态：标题 + 说明 +（筛选导致为空时的恢复按钮）。引导卡在场时整体下移
    let empty_anchor = match onboarding {
        Some(ob) => ob.bottom() + METRICS.gap_card,
        None => content_top + 56,
    };
    let (empty_title, empty_tip, btn_show_all) = if devices.is_empty() {
        let title = Rect2::new(pad, empty_anchor, width - pad * 2, EMPTY_TITLE_H);
        let tip = Rect2::new(pad, title.bottom() + METRICS.gap_row, width - pad * 2, EMPTY_TIP_H);
        let btn = if raw_count > 0 {
            Some(Rect2::new((width - 160) / 2, tip.bottom() + METRICS.gap_card, 160, 32))
        } else {
            None
        };
        (Some(title), Some(tip), btn)
    } else {
        (None, None, None)
    };

    DashLayout {
        width,
        height,
        header: Rect2::new(0, title_h, width, HEADER_H),
        tab_device,
        tab_settings,
        content_top,
        content_bottom,
        tiles,
        cols,
        tile_w,
        card_h: tile_h,
        card_step: tile_step,
        content_h,
        footer,
        btn_refresh,
        status,
        version,
        btn_show_all,
        onboarding,
        btn_start,
        empty_title,
        empty_tip,
    }
}

/// 命中测试。顺序即优先级：标签 → 底栏 → 空态/引导 → 瓦片。
pub fn hit_test(l: &DashLayout, x: i32, y: i32) -> Option<Hit> {
    if l.tab_device.contains(x, y) {
        return Some(Hit::TabDevice);
    }
    if l.tab_settings.contains(x, y) {
        return Some(Hit::TabSettings);
    }
    if l.btn_refresh.contains(x, y) {
        return Some(Hit::Refresh);
    }
    if let Some(b) = l.btn_start {
        if b.contains(x, y) {
            return Some(Hit::Start);
        }
    }
    if let Some(b) = l.btn_show_all {
        if b.contains(x, y) {
            return Some(Hit::ShowAll);
        }
    }
    for (idx, t) in l.tiles.iter().enumerate() {
        if y < l.content_top || y > l.content_bottom {
            break;
        }
        if t.copy.contains(x, y) {
            return Some(Hit::Copy(idx));
        }
        if t.edit.contains(x, y) {
            return Some(Hit::Edit(idx));
        }
        if t.rect.contains(x, y) {
            return Some(Hit::Card(idx));
        }
    }
    None
}

/// 命中结果 → panel 状态里的 hover id。
/// 瓦片本体用 400 段（设置页已占用到 317，300 段会撞车）。
pub fn hover_id(hit: Hit) -> i32 {
    match hit {
        Hit::TabDevice => 201,
        Hit::TabSettings => 202,
        Hit::Edit(i) => 100 + i as i32,
        Hit::Copy(i) => 200 + i as i32,
        Hit::Card(i) => 400 + i as i32,
        Hit::Refresh => 1,
        Hit::ShowAll => 9,
        Hit::Start => 8,
    }
}

pub fn protocol_label(dev: &DeviceSnapshot) -> &'static str {
    match dev.connection {
        ConnectionMode::Bluetooth => "Windows 蓝牙电池服务 (GATT / HFP)",
        ConnectionMode::Wireless24G if dev.id.contains("rk") => "Compx 2.4G 键盘直连协议",
        ConnectionMode::Wireless24G if dev.id.contains("logitech") => "Logitech HID++ 2.4G 接收器协议",
        ConnectionMode::Wireless24G if dev.id.contains("razer") => "Razer Hyperspeed 2.4G 接收器协议",
        ConnectionMode::Wireless24G if dev.id.contains("mchose") => "MCHOSE 2.4G 接收器协议",
        ConnectionMode::Wireless24G if dev.id.contains("vxe") => "VXE / VGN 2.4G 接收器协议",
        ConnectionMode::Wireless24G if dev.id.contains("mouse") => "Xenta / Areson 2.4G HID 协议",
        ConnectionMode::Wireless24G => "2.4G 无线 HID 直连协议",
        ConnectionMode::Wired => "USB 有线直连通信",
        ConnectionMode::Unknown => "标准 HID 电量服务",
    }
}

fn fs(base: f32, font_scale: f32) -> i32 {
    (base * font_scale).round() as i32
}

/// 整页绘制。坐标全部来自 `lay`，本函数不再出现魔法数字。
pub unsafe fn paint(
    hdc: HDC,
    lay: &DashLayout,
    devices: &[DeviceSnapshot],
    font_scale: f32,
    hover_btn: i32,
    refresh_status: &str,
    selected_id: Option<&str>,
    raw_count: usize,
    scroll_y: i32,
) {
    // ---- 引导面板（不随滚动裁剪） ----
    if let Some(ob) = lay.onboarding {
        let brush = CreateSolidBrush(THEME.card);
        let r = RECT { left: ob.x, top: ob.y, right: ob.right(), bottom: ob.bottom() };
        FillRect(hdc, &r, brush);
        DeleteObject(brush);
        SetTextColor(hdc, THEME.text);
        let f = create_clear_font(fs(METRICS.fs_title, font_scale), true);
        let old = SelectObject(hdc, f);
        let mut t = RECT {
            left: ob.x + METRICS.pad_card,
            top: ob.y + METRICS.pad_card,
            right: ob.right() - METRICS.pad_card,
            bottom: ob.y + METRICS.pad_card + 26,
        };
        draw_text_clean(hdc, "欢迎使用 LinkDeck", &mut t, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
        let f_sub = create_clear_font(fs(METRICS.fs_body, font_scale), false);
        SelectObject(hdc, f_sub);
        SetTextColor(hdc, THEME.muted);
        let mut b = RECT {
            left: ob.x + METRICS.pad_card,
            top: t.bottom + METRICS.gap_row,
            right: ob.right() - METRICS.pad_card,
            bottom: ob.bottom() - METRICS.pad_card - BTN_H - METRICS.gap_row,
        };
        draw_text_clean(hdc, "它会在后台读取支持的键盘、鼠标、耳机和蓝牙设备电量。\n可通过悬浮球或托盘打开控制中心。", &mut b, DT_LEFT);
        SelectObject(hdc, f);
        SelectObject(hdc, old);
        DeleteObject(f_sub);
        DeleteObject(f);
        if let Some(btn) = lay.btn_start {
            render_action_btn(hdc, btn.x, btn.y, btn.w, btn.h, "开始使用", hover_btn == hover_id(Hit::Start), font_scale);
        }
    }

    // ---- 瓦片网格（滚动裁剪） ----
    let clip = CreateRectRgn(0, lay.content_top, lay.width, lay.content_bottom.max(lay.content_top + 1));
    SelectClipRgn(hdc, clip);
    for (idx, dev) in devices.iter().enumerate() {
        let t = match lay.tiles.get(idx) {
            Some(t) => t,
            None => break,
        };
        if t.rect.bottom() < lay.content_top || t.rect.y > lay.content_bottom {
            continue;
        }
        render_device_tile(
            hdc,
            t,
            &dev.name,
            dev.kind.icon(),
            Some(dev),
            font_scale,
            hover_btn == hover_id(Hit::Card(idx)),
            hover_btn == hover_id(Hit::Copy(idx)),
            hover_btn == hover_id(Hit::Edit(idx)),
            selected_id == Some(dev.id.as_str()),
        );
    }
    crate::ui::panel::draw_scroll_thumb(hdc, lay.width, lay.content_top + 4, lay.content_bottom - 4, lay.content_h, scroll_y);
    SelectClipRgn(hdc, ptr::null_mut());
    DeleteObject(clip);

    // ---- 空态 ----
    if let (Some(t), Some(tip)) = (lay.empty_title, lay.empty_tip) {
        let (head_line, tip_line) = if raw_count > 0 {
            (
                "当前筛选条件下没有匹配的设备",
                "已扫描到设备，但被筛选器隐藏。点击“偏好设置 → 设备列表”将筛选切回“全部”。",
            )
        } else {
            (
                "未检测到已适配的 2.4G 外设或在线蓝牙设备",
                "已适配: 罗技/雷蛇全系 · RK/狼蛛键盘 · 迈从/VXE/达尔优鼠标 · 赛技 · 全系蓝牙\n注意: 标注未实测的私有协议尚未用真实硬件验证，运行 --protocols 查看\n提示: 多模外设切至蓝牙模式可直接读取",
            )
        };
        SetTextColor(hdc, THEME.text);
        let f_title = create_clear_font(fs(METRICS.fs_title, font_scale), true);
        let old_t = SelectObject(hdc, f_title);
        let mut tr = RECT { left: t.x, top: t.y, right: t.right(), bottom: t.bottom() };
        draw_text_clean(hdc, head_line, &mut tr, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        SelectObject(hdc, old_t);
        DeleteObject(f_title);

        SetTextColor(hdc, THEME.muted);
        let f_tip = create_clear_font(fs(METRICS.fs_body, font_scale), false);
        let old_p = SelectObject(hdc, f_tip);
        let mut pr = RECT { left: tip.x, top: tip.y, right: tip.right(), bottom: tip.bottom() };
        draw_text_clean(hdc, tip_line, &mut pr, DT_CENTER);
        SelectObject(hdc, old_p);
        DeleteObject(f_tip);

        if let Some(btn) = lay.btn_show_all {
            render_action_btn(hdc, btn.x, btn.y, btn.w, btn.h, "显示全部设备", hover_btn == hover_id(Hit::ShowAll), font_scale);
        }
    }

    // ---- 底栏：与内容区同色，顶部一条发丝线交代边界 ----
    let footer_brush = CreateSolidBrush(THEME.header);
    let fr = RECT { left: lay.footer.x, top: lay.footer.y, right: lay.footer.right(), bottom: lay.footer.bottom() };
    FillRect(hdc, &fr, footer_brush);
    DeleteObject(footer_brush);
    let line = CreateSolidBrush(THEME.border);
    let lr = RECT { left: 0, top: lay.footer.y, right: lay.footer.right(), bottom: lay.footer.y + 1 };
    FillRect(hdc, &lr, line);
    DeleteObject(line);
    render_action_btn(
        hdc,
        lay.btn_refresh.x,
        lay.btn_refresh.y,
        lay.btn_refresh.w,
        lay.btn_refresh.h,
        "刷新设备",
        hover_btn == hover_id(Hit::Refresh),
        font_scale,
    );
    SetTextColor(hdc, if refresh_status.starts_with("刷新失败") { THEME.danger } else { THEME.muted });
    let mut sr = RECT { left: lay.status.x, top: lay.status.y, right: lay.status.right(), bottom: lay.status.bottom() };
    draw_text_clean(hdc, refresh_status, &mut sr, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

    // ---- 版本号 ----
    SetTextColor(hdc, THEME.muted);
    let mut vr = RECT { left: lay.version.x, top: lay.version.y, right: lay.version.right(), bottom: lay.version.bottom() };
    draw_text_clean(hdc, concat!("v", env!("CARGO_PKG_VERSION"), " (Rust)"), &mut vr, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_640() -> DashLayout {
        layout(640, 680, UiDensity::Standard, 1.15, &[], 0, false, 0)
    }

    fn devs(n: usize) -> Vec<DeviceSnapshot> {
        vec![DeviceSnapshot::default(); n]
    }

    #[test]
    fn tile_step_keeps_gap_at_least_padding() {
        for d in [UiDensity::Compact, UiDensity::Standard, UiDensity::Spacious] {
            let (h, step) = card_geometry(d);
            assert_eq!(step - h, METRICS.gap_card);
            assert!(step - h >= METRICS.pad_card, "瓦片间距必须 ≥ 瓦片内边距");
            assert_eq!(h % 4, 0, "瓦片高度不在 4/8 体系内");
        }
    }

    #[test]
    fn tile_parts_sum_to_the_tile_height() {
        for d in [UiDensity::Compact, UiDensity::Standard, UiDensity::Spacious] {
            let s = tile_spec(d);
            let used = s.pad + s.name_h + s.gap_name + s.pct_h + s.gap_bar + s.bar_h
                + s.gap_meta + s.meta_h;
            let pad_bottom = s.h - used;
            assert!(pad_bottom >= 12, "{:?} 底部留白只剩 {}", d, pad_bottom);
        }
    }

    #[test]
    fn column_count_follows_the_window_width() {
        assert_eq!(grid_cols(480), 1, "窄窗口单列");
        assert_eq!(grid_cols(640), 2, "默认宽度两列");
        assert_eq!(grid_cols(1000), 3, "宽窗口三列");
        assert_eq!(grid_cols(400), 1, "再窄也不塌成一列以下");
    }

    #[test]
    fn tiles_form_a_grid_without_overlap() {
        let l = layout(640, 680, UiDensity::Standard, 1.15, &devs(5), 5, false, 0);
        assert_eq!(l.cols, 2);
        assert_eq!(l.tiles.len(), 5);
        // 前两个在同一行，第三个换行
        assert_eq!(l.tiles[0].rect.y, l.tiles[1].rect.y);
        assert_eq!(l.tiles[2].rect.y - l.tiles[0].rect.bottom(), METRICS.gap_card);
        assert_eq!(l.tiles[1].rect.x - l.tiles[0].rect.right(), METRICS.gap_card);
        assert_eq!(l.tiles[2].rect.x, METRICS.pad_page);

        for (i, a) in l.tiles.iter().enumerate() {
            for b in l.tiles.iter().skip(i + 1) {
                assert!(!a.rect.overlaps(&b.rect), "瓦片 {} 与后面的重叠", i);
            }
            assert!(a.rect.x >= METRICS.pad_page);
            assert!(a.rect.right() <= 640 - METRICS.pad_page, "瓦片超出右边界");
        }
    }

    #[test]
    fn tile_contents_stack_top_to_bottom() {
        let l = layout(640, 680, UiDensity::Standard, 1.15, &devs(1), 1, false, 0);
        let t = &l.tiles[0];
        assert!(t.name.y >= t.rect.y);
        assert!(t.pct.y >= t.name.bottom());
        assert!(t.bar.y >= t.pct.bottom());
        assert!(t.meta.y >= t.bar.bottom());
        assert!(t.meta.bottom() <= t.rect.bottom(), "内容溢出瓦片底部");
        // 名称与右上角状态词不重叠
        assert!(!t.name.overlaps(&t.chip));
        // 说明文字不能压到底部按钮
        assert!(t.meta.right() <= t.copy.x, "说明文字与复制按钮重叠");
    }

    #[test]
    fn tile_buttons_sit_inside_their_tile() {
        let l = layout(480, 680, UiDensity::Compact, 1.15, &devs(1), 1, false, 0);
        let t = &l.tiles[0];
        assert!(t.rect.contains(t.copy.x, t.copy.y));
        assert!(t.rect.contains(t.edit.right(), t.edit.bottom()));
        assert!(t.copy.right() < t.edit.x, "复制与改名不能重叠");
    }

    #[test]
    fn first_launch_onboarding_and_empty_state_do_not_overlap() {
        // 首次启动：引导面板 + 空态同时出现，两块必须上下排开
        let l = layout(983, 720, UiDensity::Standard, 1.15, &[], 0, true, 0);
        let ob = l.onboarding.expect("首次启动应当有引导面板");
        let title = l.empty_title.expect("没有设备时应当有空态标题");
        let tip = l.empty_tip.expect("没有设备时应当有空态说明");
        assert!(!ob.overlaps(&title), "引导面板压住了空态标题");
        assert!(!ob.overlaps(&tip), "引导面板压住了空态说明");
        assert!(title.y >= ob.bottom(), "空态应当排在引导面板下方");
        assert!(tip.y >= title.bottom(), "空态说明应当在标题下方");
        assert!(tip.bottom() <= l.content_bottom, "空态被挤到视口外面");
        // 放弃引导后（show_onboarding = false）空态回到内容区顶部
        let bare = layout(983, 720, UiDensity::Standard, 1.15, &[], 0, false, 0);
        assert!(bare.onboarding.is_none());
        let bare_title = bare.empty_title.unwrap();
        assert!(bare_title.y < title.y, "没有引导面板时空态应当更靠上");
        assert!(bare_title.y >= bare.content_top, "空态不能跑到顶栏里");
    }

    #[test]
    fn short_window_drops_the_onboarding_card_instead_of_clipping_the_empty_state() {
        // 最小窗口高度 400：引导卡 + 空态排不下，应当让掉引导卡，而不是让文字溢出到页脚
        for h in [crate::ui::panel::MIN_PANEL_HEIGHT, 430, 460] {
            let l = layout(480, h, UiDensity::Standard, 1.15, &[], 0, true, 0);
            assert!(l.btn_start.is_none(), "高度 {h} 下引导卡应当让位");
            assert!(l.onboarding.is_none());
            let tip = l.empty_tip.expect("空态说明必须还在");
            assert!(tip.bottom() <= l.content_bottom, "高度 {h} 下空态溢出内容区");
        }
        // 高度够时引导卡要回来
        let tall = layout(480, 640, UiDensity::Standard, 1.15, &[], 0, true, 0);
        assert!(tall.onboarding.is_some());
    }

    #[test]
    fn hit_test_matches_the_painted_geometry() {
        let l = layout(640, 680, UiDensity::Standard, 1.15, &devs(3), 3, false, 0);
        for (i, t) in l.tiles.iter().enumerate() {
            assert_eq!(hit_test(&l, t.copy.x + 2, t.copy.y + 2), Some(Hit::Copy(i)));
            assert_eq!(hit_test(&l, t.edit.x + 2, t.edit.y + 2), Some(Hit::Edit(i)));
            assert_eq!(hit_test(&l, t.rect.x + 8, t.rect.y + t.rect.h - 8), Some(Hit::Card(i)));
        }
    }

    #[test]
    fn tiles_scrolled_out_of_the_viewport_are_not_clickable() {
        let l = layout(640, 680, UiDensity::Standard, 1.15, &devs(1), 1, false, 200);
        let t = &l.tiles[0];
        assert!(t.rect.y < l.content_top);
        assert_eq!(hit_test(&l, t.copy.x + 2, l.content_top + 2), None);
    }

    #[test]
    fn tabs_and_footer_buttons_are_hittable() {
        let l = layout_640();
        assert_eq!(hit_test(&l, l.tab_device.x + 4, l.tab_device.y + 4), Some(Hit::TabDevice));
        assert_eq!(hit_test(&l, l.tab_settings.x + 4, l.tab_settings.y + 4), Some(Hit::TabSettings));
        assert_eq!(hit_test(&l, l.btn_refresh.x + 4, l.btn_refresh.y + 4), Some(Hit::Refresh));
    }

    /// 分段控件的两段必须严丝合缝：有缝就读成两个按钮，重叠则命中打架
    #[test]
    fn page_switch_is_one_continuous_segment() {
        let l = layout_640();
        assert_eq!(l.tab_device.right(), l.tab_settings.x, "两段之间不能留缝");
        assert_eq!(l.tab_device.y, l.tab_settings.y);
        assert_eq!(l.tab_device.h, l.tab_settings.h);
        assert_eq!(hit_test(&l, l.tab_device.right() - 2, l.tab_device.y + 4), Some(Hit::TabDevice));
        assert_eq!(hit_test(&l, l.tab_settings.x + 2, l.tab_settings.y + 4), Some(Hit::TabSettings));
    }

    /// 设置页把分类导航收进顶栏，顶栏因此变高；设备页保持一行标题
    #[test]
    fn header_grows_when_the_settings_nav_is_folded_into_it() {
        // 顶栏高度含自绘标题栏，因为客户区铺满整窗
        assert_eq!(header_height(0), titlebar::TITLE_H + HEADER_H);
        assert_eq!(header_height(1), crate::ui::settings::CONTENT_TOP);
        assert!(header_height(1) > header_height(0));
    }

    /// 自绘标题栏占掉窗口顶部：内容不能压上去，否则会挡住窗口按钮
    #[test]
    fn content_starts_below_the_custom_title_bar() {
        let l = layout_640();
        assert_eq!(l.header.y, titlebar::TITLE_H);
        assert!(l.content_top >= titlebar::TITLE_H + HEADER_H);
        assert_eq!(l.tab_device.y, titlebar::TITLE_H + (HEADER_H - 32) / 2);
    }

    /// 顶栏文案随页面切换，两页不重复同一个标题
    #[test]
    fn header_copy_follows_the_active_page() {
        let (dev_title, _) = header_copy(0, &devs(2));
        let (set_title, _) = header_copy(1, &devs(2));
        assert_ne!(dev_title, set_title);
        assert!(dev_title.contains("控制中心"));
        assert_eq!(set_title, "偏好设置");
        // 有设备时副标题给设备数，不再是固定的广告语
        let (_, sub) = header_copy(0, &devs(3));
        assert!(sub.contains("3 台"), "副标题应反映实际设备数: {}", sub);
    }

    #[test]
    fn status_and_version_never_overlap() {
        let l = layout_640();
        assert!(l.status.right() <= l.version.x, "状态文字与版本号重叠");
    }

    #[test]
    fn hover_ids_stay_distinct() {
        let mut seen = std::collections::HashSet::new();
        for h in [
            Hit::TabDevice, Hit::TabSettings, Hit::Refresh,
            Hit::ShowAll, Hit::Start, Hit::Copy(0), Hit::Edit(0), Hit::Card(0),
        ] {
            assert!(seen.insert(hover_id(h)), "hover id 冲突: {:?}", h);
        }
        // 瓦片本体用 400 段，不能撞上设置页已占用的 id（设置页用到 317）
        assert!(hover_id(Hit::Card(0)) > 320);
    }

    #[test]
    fn every_geometry_value_follows_the_four_grid() {
        let l = layout_640();
        for v in [
            l.card_h, l.card_step, l.tab_device.w, l.tab_device.h,
            l.btn_refresh.h, l.tab_settings.h, l.content_top,
        ] {
            assert_eq!(v % 4, 0, "{} 不在 4/8 体系内", v);
        }
    }
}
