//! 五种停靠形态的绘制：贴边拉手 / 圆球 / 圆环 / 竖向胶囊 / 小徽章。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;
use super::pixels::*;

pub(crate) unsafe fn render_tab_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
    hover: bool,
) {
    let active_list = get_carousel_devices(devices);
    let total_devs = active_list.len();
    let cur_dev = if total_devs > 0 {
        active_list.get(carousel_index % total_devs).copied()
    } else {
        None
    };

    let (pct, charging, chg_source, online, kind) = if let Some(d) = cur_dev {
        (
            d.battery.percentage,
            d.battery.charging,
            d.battery.charging_source,
            d.battery.online,
            d.kind,
        )
    } else {
        (None, false, crate::devices::ChargingSource::None, false, DeviceKind::Other)
    };

    let accent = if charging {
        let pulse = 0.82 + 0.18 * pulse_phase.sin();
        let g = (210.0 + 45.0 * pulse_phase.sin()).clamp(165.0, 255.0) as u8;
        let b = (255.0 * pulse.clamp(0.85, 1.0)) as u8;
        (0u8, g, b)
    } else if let Some(p) = pct {
        if p >= 50 {
            (16u8, 185u8, 129u8)
        } else if p >= 20 {
            (245u8, 158u8, 11u8)
        } else {
            (239u8, 68u8, 68u8)
        }
    } else {
        (80u8, 92u8, 108u8)
    };

    let bg = if hover { (0x1Eu8, 0x22u8, 0x28u8) } else { (0x16u8, 0x19u8, 0x1Eu8) };
    let border_rgb = blend(bg, accent, if hover { 0.70 } else { 0.45 });
    let track_rgb = (0x2Cu8, 0x32u8, 0x3Au8);

    let x0 = (w - TAB_W) as f32;
    let x1 = w as f32;
    let y0 = 1.0f32;
    let y1 = h as f32 - 1.0;

    // 左侧独立的竖向电量条：文字永远压在深色底上，任何档位都清晰
    let gauge_x0 = x0 + 4.0;
    let gauge_x1 = gauge_x0 + 3.0;
    let gauge_y0 = 7.0;
    let gauge_y1 = h as f32 - 7.0;
    let level_y = gauge_y1 - (gauge_y1 - gauge_y0) * (pct.unwrap_or(0).min(100) as f32 / 100.0);

    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut inside = 0.0f32;
            let mut edge = 0.0f32;
            let mut gauge = 0.0f32;
            let mut filled = 0.0f32;

            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    if tab_inside(px, py, x0, x1, y0, y1, TAB_RADIUS) {
                        inside += 1.0;
                        // 右边界贴着屏幕，不描边，避免出现“断口”
                        if !tab_inside(
                            px,
                            py,
                            x0 + 1.0,
                            x1,
                            y0 + 1.0,
                            y1 - 1.0,
                            (TAB_RADIUS - 1.0).max(0.0),
                        ) {
                            edge += 1.0;
                        }
                        if px >= gauge_x0 && px <= gauge_x1 && py >= gauge_y0 && py <= gauge_y1 {
                            gauge += 1.0;
                            if py >= level_y {
                                filled += 1.0;
                            }
                        }
                    }
                }
            }

            if inside <= 0.0 {
                continue;
            }

            let alpha = inside / n_sub;
            let bar = blend(track_rgb, accent, filled / gauge.max(1.0));
            let rgb = blend(
                blend(bg, bar, gauge / n_sub),
                border_rgb,
                edge / n_sub,
            );
            let r = (rgb.0 as f32 * alpha) as u32;
            let g = (rgb.1 as f32 * alpha) as u32;
            let b = (rgb.2 as f32 * alpha) as u32;
            pixels[(y * w + x) as usize] = (r << 16) | (g << 8) | b;
        }
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

    // 设备图标 / 电量数字 / 轮播点：与完整球体同一套内容，只是排成竖条
    let icon_size = (11.0 * font_scale).round().clamp(10.0, 12.0) as i32;
    let font_icon = create_clear_font(icon_size, true);
    let old_font = SelectObject(hdc, font_icon);
    let (icon_str, icon_clr) = if charging || chg_source.is_charging() {
        ("⚡", THEME.charging)
    } else {
        let sym = match kind {
            DeviceKind::Keyboard => "⌨",
            DeviceKind::Mouse => "🖱",
            DeviceKind::Headset => "🎧",
            _ => "📱",
        };
        (sym, THEME.muted)
    };
    SetTextColor(hdc, icon_clr);
    let mut icon_rect = RECT {
        left: (w - TAB_W + 9) as i32,
        top: 6,
        right: w,
        bottom: 24,
    };
    draw_text_clean(hdc, icon_str, &mut icon_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let pct_size = (10.0 * font_scale).round().clamp(9.0, 10.0) as i32;
    let font_pct = create_clear_font(pct_size, true);
    SelectObject(hdc, font_pct);
    SetTextColor(hdc, if online { THEME.text } else { THEME.muted });
    let pct_str = if let Some(p) = pct {
        format!("{}", p)
    } else if online {
        "ON".to_string()
    } else {
        "--".to_string()
    };
    SetTextColor(hdc, if online { THEME.text } else { THEME.muted });
    let mut pct_rect = RECT {
        left: (w - TAB_W + 9) as i32,
        top: 25,
        right: w,
        bottom: 43,
    };
    draw_text_clean(hdc, &pct_str, &mut pct_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_pct);
    DeleteObject(font_icon);

    if total_devs >= 2 {
        let cur_idx = carousel_index % total_devs;
        for i in 0..total_devs.min(3) {
            let (dot_clr, dot_r) = if i == cur_idx {
                (THEME.charging, 2)
            } else {
                (THEME.muted, 1)
            };
            draw_dot(hdc, w - 5, 46 + (i as i32) * 4, dot_r, dot_clr);
        }
    }
}

/// Render the circular/pill Edge Floating Ball

pub(crate) unsafe fn render_ball_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let active_list = get_carousel_devices(devices);
    let total_devs = active_list.len();
    let cur_dev = if total_devs > 0 {
        active_list.get(carousel_index % total_devs).copied()
    } else {
        None
    };

    let (pct, charging, chg_source, online, kind) = if let Some(d) = cur_dev {
        (
            d.battery.percentage,
            d.battery.charging,
            d.battery.charging_source,
            d.battery.online,
            d.kind,
        )
    } else {
        (None, false, crate::devices::ChargingSource::None, false, DeviceKind::Other)
    };

    // 1. Prepare 32-bit DIB pixel buffer (filled with chroma-key CLR_COLORKEY for transparency)
    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let cy = (h - 1) as f32 / 2.0;

    let cx = (w - 1) as f32 / 2.0;
    let r_ball = 22.5f32;
    let r_mid = 18.5f32;
    let r_thickness = 3.6f32;
    let sweep_total = std::f32::consts::TAU;
    let arc_dir = 1.0f32;
    let r_outer = r_mid + r_thickness / 2.0;
    let r_inner = r_mid - r_thickness / 2.0;
    let cap_r = r_thickness / 2.0;

    let ball_bg_rgb = (0x28u8, 0x2Eu8, 0x36u8);
    let track_rgb = (0x28u8, 0x2Eu8, 0x38u8);

    let arc_rgb = if charging {
        // Electric Cyan with subtle breathing pulse
        let pulse_factor = 0.82 + 0.18 * pulse_phase.sin();
        let g = ((210.0 + 45.0 * pulse_phase.sin()).clamp(165.0, 255.0)) as u8;
        let b = (255.0 * pulse_factor.clamp(0.85, 1.0)) as u8;
        (0u8, g, b)
    } else if let Some(p) = pct {
        if p >= 50 {
            (16u8, 185u8, 129u8) // Emerald Green
        } else if p >= 20 {
            (245u8, 158u8, 11u8) // Amber
        } else {
            (239u8, 68u8, 68u8) // Coral Red
        }
    } else {
        (80u8, 92u8, 108u8) // Slate Gray
    };

    let eff_pct = pct.unwrap_or(0).min(100);
    let sweep_rad = (eff_pct as f32 / 100.0) * sweep_total;

    // 弧参数 0 位于 12 点，顺时针推进。
    let arc_point = |s: f32| {
        let a = -std::f32::consts::FRAC_PI_2 + arc_dir * s;
        (cx + r_mid * a.cos(), cy + r_mid * a.sin())
    };
    let (start_cap_x, start_cap_y) = arc_point(0.0);
    let (end_cap_x, end_cap_y) = arc_point(sweep_rad);

    // 3x3 subpixel sampling grid for smooth anti-aliased borders and arcs
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut bg_hits = 0.0f32;
            let mut track_hits = 0.0f32;
            let mut arc_hits = 0.0f32;

            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let dx = px - cx;
                    let dy = py - cy;
                    let d = (dx * dx + dy * dy).sqrt();

                    if d <= r_ball {
                        bg_hits += 1.0;
                        if d >= r_inner && d <= r_outer {
                            track_hits += 1.0;
                        }
                        if eff_pct > 0 {
                            let mut angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                            if angle < 0.0 {
                                angle += 2.0 * std::f32::consts::PI;
                            }
                            if d >= r_inner && d <= r_outer && angle <= sweep_rad {
                                arc_hits += 1.0;
                            } else {
                                let d_start = ((px - start_cap_x).powi(2) + (py - start_cap_y).powi(2)).sqrt();
                                let d_end = ((px - end_cap_x).powi(2) + (py - end_cap_y).powi(2)).sqrt();
                                if d_start <= cap_r || d_end <= cap_r {
                                    arc_hits += 1.0;
                                }
                            }
                        }
                    }
                }
            }

            if bg_hits <= 0.0 {
                continue;
            }

            let mut r = ball_bg_rgb.0 as f32;
            let mut g = ball_bg_rgb.1 as f32;
            let mut b = ball_bg_rgb.2 as f32;

            if track_hits > 0.0 {
                let ta = track_hits / n_sub;
                r = r * (1.0 - ta) + track_rgb.0 as f32 * ta;
                g = g * (1.0 - ta) + track_rgb.1 as f32 * ta;
                b = b * (1.0 - ta) + track_rgb.2 as f32 * ta;
            }

            if arc_hits > 0.0 {
                let aa = arc_hits / n_sub;
                r = r * (1.0 - aa) + arc_rgb.0 as f32 * aa;
                g = g * (1.0 - aa) + arc_rgb.1 as f32 * aa;
                b = b * (1.0 - aa) + arc_rgb.2 as f32 * aa;
            }

            let bg_alpha = bg_hits / n_sub;
            let final_pixel = if bg_alpha >= 0.99 {
                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            } else {
                let border_r = (r * bg_alpha) as u32;
                let border_g = (g * bg_alpha) as u32;
                let border_b = (b * bg_alpha) as u32;
                (border_r << 16) | (border_g << 8) | border_b
            };

            pixels[(y * w + x) as usize] = final_pixel;
        }
    }

    // 2. Transfer anti-aliased ring bitmap to DC
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
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

    // 3. Center Content: 设备图标 / 电量数字 / 轮播点
    let pct_str = if let Some(p) = pct {
        format!("{}", p)
    } else if online {
        "ON".to_string()
    } else {
        "--".to_string()
    };

    let font_icon_size = (11.0 * font_scale).round() as i32;
    let font_icon = create_clear_font(font_icon_size, true);
    let old_font = SelectObject(hdc, font_icon);

    let (icon_str, icon_clr) = if charging || chg_source.is_charging() {
        ("⚡", THEME.charging)
    } else {
        let sym = match kind {
            DeviceKind::Keyboard => "⌨",
            DeviceKind::Mouse => "🖱",
            DeviceKind::Headset => "🎧",
            _ => "📱",
        };
        (sym, THEME.muted)
    };
    SetTextColor(hdc, icon_clr);
    let mut icon_rect = RECT {
        left: 0,
        top: (cy - 19.0) as i32,
        right: w,
        bottom: (cy - 6.0) as i32,
    };
    draw_text_clean(hdc, icon_str, &mut icon_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    let font_pct_size = (13.0 * font_scale).round() as i32;
    let font_pct = create_clear_font(font_pct_size, true);
    SelectObject(hdc, font_pct);
    SetTextColor(hdc, if !online { THEME.muted } else { THEME.text });

    let mut pct_rect = RECT {
        left: 0,
        top: (cy - 3.0) as i32,
        right: w,
        bottom: (cy + 10.0) as i32,
    };
    draw_text_clean(hdc, &pct_str, &mut pct_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    SelectObject(hdc, old_font);
    DeleteObject(font_pct);
    DeleteObject(font_icon);
    // 4. Carousel Pagination Dots: vertical column on the flat edge when docked,
    //    horizontal row under the percentage when hovering.
    if total_devs >= 2 {
        let cur_idx = carousel_index % total_devs;
        let spacing = 6;
        let start_x = (w - ((total_devs as i32 - 1) * spacing)) / 2;
        let dot_y = (cy as i32) + 14;

        for i in 0..total_devs {
            let (dot_clr, dot_r) = if i == cur_idx {
                (THEME.charging, 2)
            } else {
                (THEME.muted, 1)
            };
            draw_dot(hdc, start_x + (i as i32) * spacing, dot_y, dot_r, dot_clr);
        }
    }
}

// --------------------------------------------------------------------------
// 后三种形态（圆环 / 竖向胶囊 / 小徽章）
//
// 三者共用一套取数与配色，差别只在“画什么形状”，避免每加一种形态就复制一遍
// 电量分档逻辑（分档写错一次，三个形态就一起错）。
// --------------------------------------------------------------------------


/// 圆环进度：空心环形量表，中间只留一块小底板压数字，环外全透明。
pub(crate) unsafe fn render_ring_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let g = gauge_of(devices, carousel_index);
    let accent = status_rgb(g.pct, g.charging, pulse_phase);
    let groove = (0x32u8, 0x38u8, 0x42u8);
    let plate = (0x1Bu8, 0x1Fu8, 0x26u8);

    let cx = (w - 1) as f32 / 2.0;
    let cy = (h - 1) as f32 / 2.0;
    let r_out = 22.0f32;
    let r_in = 15.5f32;
    let sweep = (g.pct.unwrap_or(0).min(100) as f32 / 100.0) * std::f32::consts::TAU;
    // 底板必须小于内圈半径，否则会横切过环带，看起来像“环被一条黑杠穿过”
    let (pw, ph) = (28.0f32, 17.0f32);

    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut arc = 0.0f32;
            let mut track = 0.0f32;
            let mut disc = 0.0f32;
            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let dx = px - cx;
                    let dy = py - cy;
                    let d = (dx * dx + dy * dy).sqrt();
                    if d >= r_in && d <= r_out {
                        let mut angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                        if angle < 0.0 {
                            angle += std::f32::consts::TAU;
                        }
                        if sweep > 0.0 && angle <= sweep {
                            arc += 1.0;
                        } else {
                            track += 1.0;
                        }
                    } else if rounded_rect_hit(
                        px,
                        py,
                        cx - pw / 2.0,
                        cy - ph / 2.0,
                        cx + pw / 2.0,
                        cy + ph / 2.0,
                        7.0,
                    ) {
                        disc += 1.0;
                    }
                }
            }
            let (rgb, cov) = if arc > 0.0 {
                (accent, arc / n_sub)
            } else if track > 0.0 {
                (groove, track / n_sub)
            } else if disc > 0.0 {
                (plate, disc / n_sub)
            } else {
                continue;
            };
            pixels[(y * w + x) as usize] = keyed(rgb, cov);
        }
    }
    blit_pixels(hdc, w, h, &pixels);

    let text = percent_text(&g);
    let f = create_clear_font((fit_size(&text, 13.0) * font_scale).round().clamp(10.0, 14.0) as i32, true);
    let old = SelectObject(hdc, f);
    SetTextColor(hdc, if g.online { THEME.text } else { THEME.muted });
    let mut r = RECT {
        left: 0,
        top: (cy - 9.0) as i32,
        right: w,
        bottom: (cy + 9.0) as i32,
    };
    draw_text_clean(hdc, &text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old);
    DeleteObject(f);
}

/// 竖向胶囊：顶部百分比 + 下方长条进度，贴边不挡内容。
pub(crate) unsafe fn render_pill_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let g = gauge_of(devices, carousel_index);
    let accent = status_rgb(g.pct, g.charging, pulse_phase);
    let bg = (0x16u8, 0x19u8, 0x1Eu8);
    let border = blend(bg, accent, 0.55);
    let track = (0x2Cu8, 0x32u8, 0x3Au8);

    let x0 = 1.0f32;
    let x1 = w as f32 - 1.0;
    let y0 = 1.0f32;
    let y1 = h as f32 - 1.0;
    let radius = ((x1 - x0) / 2.0).min((y1 - y0) / 2.0);

    let cx = (w - 1) as f32 / 2.0;
    let bar_x0 = cx - 2.5;
    let bar_x1 = cx + 2.5;
    let bar_y0 = 31.0f32;
    let bar_y1 = h as f32 - 9.0;
    let level = bar_y1 - (bar_y1 - bar_y0) * (g.pct.unwrap_or(0).min(100) as f32 / 100.0);

    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut inside = 0.0f32;
            let mut edge = 0.0f32;
            let mut bar = 0.0f32;
            let mut filled = 0.0f32;
            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    if !rounded_rect_hit(px, py, x0, y0, x1, y1, radius) {
                        continue;
                    }
                    inside += 1.0;
                    if !rounded_rect_hit(px, py, x0 + 1.0, y0 + 1.0, x1, y1 - 1.0, (radius - 1.0).max(0.0)) {
                        edge += 1.0;
                    }
                    if px >= bar_x0 && px <= bar_x1 && py >= bar_y0 && py <= bar_y1 {
                        bar += 1.0;
                        if py >= level {
                            filled += 1.0;
                        }
                    }
                }
            }
            if inside <= 0.0 {
                continue;
            }
            let alpha = inside / n_sub;
            let bar_rgb = blend(track, accent, filled / bar.max(1.0));
            let rgb = blend(blend(bg, bar_rgb, bar / n_sub), border, edge / n_sub);
            pixels[(y * w + x) as usize] = keyed(rgb, alpha);
        }
    }
    blit_pixels(hdc, w, h, &pixels);

    let text = percent_text(&g);
    // 胶囊只有 32px 宽，三位数硬套 11px 会被裁掉两头
    let f = create_clear_font((fit_size(&text, 11.0) * font_scale).round().clamp(8.0, 12.0) as i32, true);
    let old = SelectObject(hdc, f);
    SetTextColor(hdc, if g.online { THEME.text } else { THEME.muted });
    let mut r = RECT { left: 0, top: 12, right: w, bottom: 28 };
    draw_text_clean(hdc, &text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old);
    DeleteObject(f);
}

/// 小徽章：一个纯色圆点 + 百分比，尺寸最小、最不打扰。
pub(crate) unsafe fn render_badge_widget(
    hdc: HDC,
    w: i32,
    h: i32,
    devices: &[DeviceSnapshot],
    carousel_index: usize,
    pulse_phase: f32,
    font_scale: f32,
) {
    let g = gauge_of(devices, carousel_index);
    let fill = status_rgb(g.pct, g.charging, pulse_phase);
    let ring = blend(fill, (0x00, 0x00, 0x00), 0.28);

    let cx = (w - 1) as f32 / 2.0;
    let cy = (h - 1) as f32 / 2.0;
    let r_ball = (cx - 1.0).min(cy - 1.0);

    let mut pixels = vec![CLR_COLORKEY; (w * h) as usize];
    let sub = [0.2f32, 0.5f32, 0.8f32];
    let n_sub = 9.0f32;

    for y in 0..h {
        for x in 0..w {
            let mut inside = 0.0f32;
            let mut edge = 0.0f32;
            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                    if d <= r_ball {
                        inside += 1.0;
                        if d >= r_ball - 1.2 {
                            edge += 1.0;
                        }
                    }
                }
            }
            if inside <= 0.0 {
                continue;
            }
            let alpha = inside / n_sub;
            let rgb = blend(fill, ring, edge / n_sub);
            pixels[(y * w + x) as usize] = keyed(rgb, alpha);
        }
    }
    blit_pixels(hdc, w, h, &pixels);

    let text = percent_text(&g);
    let f = create_clear_font((fit_size(&text, 11.0) * font_scale).round().clamp(9.0, 12.0) as i32, true);
    let old = SelectObject(hdc, f);
    SetTextColor(hdc, contrast_text(fill));
    let mut r = RECT {
        left: 0,
        top: (cy - 8.0) as i32,
        right: w,
        bottom: (cy + 8.0) as i32,
    };
    draw_text_clean(hdc, &text, &mut r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(hdc, old);
    DeleteObject(f);
}

