//! 像素级绘制原语：分层窗口的 colorkey、混色、环形/圆角命中、电量取色。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

/// 左圆角、右侧齐平的贴边拉手：屏幕边就是它的天然收口，不会被看成“被裁掉的圆”。
pub(crate) unsafe fn tab_inside(px: f32, py: f32, x0: f32, x1: f32, y0: f32, y1: f32, r: f32) -> bool {
    if py < y0 || py > y1 || px < x0 || px > x1 {
        return false;
    }
    if px >= x0 + r {
        return true;
    }
    let cx = x0 + r;
    if py < y0 + r {
        return (px - cx).powi(2) + (py - (y0 + r)).powi(2) <= r * r;
    }
    if py > y1 - r {
        return (px - cx).powi(2) + (py - (y1 - r)).powi(2) <= r * r;
    }
    true
}

pub(crate) fn blend(dst: (u8, u8, u8), src: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (dst.0 as f32 * (1.0 - t) + src.0 as f32 * t) as u8,
        (dst.1 as f32 * (1.0 - t) + src.1 as f32 * t) as u8,
        (dst.2 as f32 * (1.0 - t) + src.2 as f32 * t) as u8,
    )
}


/// 当前轮播设备的电量快照
pub(crate) struct Gauge {
    pub(crate) pct: Option<u8>,
    pub(crate) charging: bool,
    pub(crate) online: bool,
}

pub(crate) fn gauge_of(devices: &[DeviceSnapshot], carousel_index: usize) -> Gauge {
    let active = get_carousel_devices(devices);
    let cur = if active.is_empty() {
        None
    } else {
        active.get(carousel_index % active.len()).copied()
    };
    match cur {
        Some(d) => Gauge {
            pct: d.battery.percentage,
            charging: d.battery.charging || d.battery.charging_source.is_charging(),
            online: d.battery.online,
        },
        None => Gauge { pct: None, charging: false, online: false },
    }
}

/// 电量分档配色：充电中呼吸青色，其余按 50 / 20 分档。
pub(crate) fn status_rgb(pct: Option<u8>, charging: bool, phase: f32) -> (u8, u8, u8) {
    if charging {
        let pulse = 0.82 + 0.18 * phase.sin();
        let g = (210.0 + 45.0 * phase.sin()).clamp(165.0, 255.0) as u8;
        let b = (255.0 * pulse.clamp(0.85, 1.0)) as u8;
        return (0, g, b);
    }
    match pct {
        Some(p) if p >= 50 => (16, 185, 129),
        Some(p) if p >= 20 => (245, 158, 11),
        Some(_) => (239, 68, 68),
        None => (80, 92, 108),
    }
}

/// 底色上该压深字还是浅字
pub(crate) fn contrast_text(rgb: (u8, u8, u8)) -> COLORREF {
    let lum = 0.299 * rgb.0 as f32 + 0.587 * rgb.1 as f32 + 0.114 * rgb.2 as f32;
    if lum > 165.0 {
        crate::ui::theme::rgb(0x14, 0x12, 0x10)
    } else {
        crate::ui::theme::rgb(0xFF, 0xFF, 0xFF)
    }
}

pub(crate) fn rounded_rect_hit(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if px < x0 || px > x1 || py < y0 || py > y1 {
        return false;
    }
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let nx = px.clamp(x0 + r, x1 - r);
    let ny = py.clamp(y0 + r, y1 - r);
    (px - nx).powi(2) + (py - ny).powi(2) <= r * r
}

/// 亚像素采样后的像素写回：颜色乘覆盖率得到与 colorkey 的混合结果。
pub(crate) fn keyed(rgb: (u8, u8, u8), coverage: f32) -> u32 {
    let a = coverage.clamp(0.0, 1.0);
    let r = (rgb.0 as f32 * a) as u32;
    let g = (rgb.1 as f32 * a) as u32;
    let b = (rgb.2 as f32 * a) as u32;
    (r << 16) | (g << 8) | b
}

pub(crate) unsafe fn blit_pixels(hdc: HDC, w: i32, h: i32, pixels: &[u32]) {
    if w <= 0 || h <= 0 || pixels.len() < (w * h) as usize {
        return;
    }
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;
    SetDIBitsToDevice(
        hdc,
        0,
        0,
        w as u32,
        h as u32,
        0,
        0,
        0,
        h as u32,
        pixels.as_ptr() as _,
        &bmi,
        DIB_RGB_COLORS,
    );
    SetBkMode(hdc, TRANSPARENT as i32);
}

pub(crate) fn percent_text(g: &Gauge) -> String {
    match g.pct {
        Some(p) => format!("{}", p),
        None if g.online => "ON".to_string(),
        None => "--".to_string(),
    }
}

/// 窄形态里放不下大字号：三位数必须缩一档，否则会被窗口边缘裁掉半个字。
pub(crate) fn fit_size(text: &str, base: f32) -> f32 {
    if text.chars().count() >= 3 {
        base - 1.5
    } else {
        base
    }
}


pub(crate) unsafe fn draw_dot(hdc: HDC, x: i32, y: i32, r: i32, clr: COLORREF) {
    let dot_brush = CreateSolidBrush(clr);
    let old_b = SelectObject(hdc, dot_brush);
    let null_pen = CreatePen(PS_SOLID, 0, clr);
    let old_p = SelectObject(hdc, null_pen);

    RoundRect(hdc, x - r, y - r, x + r + 1, y + r + 1, 2, 2);

    SelectObject(hdc, old_p);
    DeleteObject(null_pen);
    SelectObject(hdc, old_b);
    DeleteObject(dot_brush);
}

