use std::mem;
use std::ptr;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// Generates a pixel-perfect, anti-aliased native Windows HICON for LinkDeck
/// Featuring:
/// - Dark titanium circular medallion background with beveled outer rim
/// - High-tech Cyan-to-Emerald gradient Energy Ring (~300° arc with rounded caps)
/// - Razor-sharp electric-cyan lightning bolt core symbolizing power & hardware connection
/// - 32-bit ARGB alpha transparency channel for crisp display on all taskbar themes
pub unsafe fn create_linkdeck_icon(size: i32) -> HICON {
    let hdc = CreateCompatibleDC(ptr::null_mut());
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    let mut bits: *mut std::ffi::c_void = ptr::null_mut();
    let hbm_color = CreateDIBSection(
        hdc,
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        ptr::null_mut(),
        0,
    );

    if hbm_color.is_null() || bits.is_null() {
        if !hdc.is_null() {
            DeleteDC(hdc);
        }
        return ptr::null_mut();
    }

    let slice = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
    let s = size as f32;
    let cx = (s - 1.0) / 2.0;
    let cy = (s - 1.0) / 2.0;

    let r_ball = s * 0.46;
    let r_mid = s * 0.36;
    let r_thick = (s * 0.09).max(1.8);
    let r_outer = r_mid + r_thick / 2.0;
    let r_inner = r_mid - r_thick / 2.0;
    let cap_r = r_thick / 2.0;

    let sweep_rad = 1.65 * std::f32::consts::PI; // ~297° energy gauge
    let start_cap_x = cx;
    let start_cap_y = cy - r_mid;
    let end_angle = sweep_rad - std::f32::consts::FRAC_PI_2;
    let end_cap_x = cx + r_mid * end_angle.cos();
    let end_cap_y = cy + r_mid * end_angle.sin();

    // 2x2 subpixel grid for fast & ultra-smooth anti-aliased icon rendering
    let sub = [0.25f32, 0.75f32];
    let n_sub = 4.0f32;

    for y in 0..size {
        for x in 0..size {
            let mut bg_hits = 0.0f32;
            let mut rim_hits = 0.0f32;
            let mut track_hits = 0.0f32;
            let mut arc_hits = 0.0f32;
            let mut sum_angle = 0.0f32;

            for &sy in &sub {
                for &sx in &sub {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;
                    let dx = px - cx;
                    let dy = py - cy;
                    let d = (dx * dx + dy * dy).sqrt();

                    if d <= r_ball {
                        bg_hits += 1.0;
                        if d >= r_ball - 1.2 {
                            rim_hits += 1.0;
                        }
                        if d >= r_inner && d <= r_outer {
                            track_hits += 1.0;
                        }

                        let mut angle = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                        if angle < 0.0 {
                            angle += 2.0 * std::f32::consts::PI;
                        }
                        if d >= r_inner && d <= r_outer && angle <= sweep_rad {
                            arc_hits += 1.0;
                            sum_angle += angle;
                        } else {
                            let d_start = ((px - start_cap_x).powi(2) + (py - start_cap_y).powi(2)).sqrt();
                            let d_end = ((px - end_cap_x).powi(2) + (py - end_cap_y).powi(2)).sqrt();
                            if d_start <= cap_r || d_end <= cap_r {
                                arc_hits += 1.0;
                                sum_angle += if d_start <= cap_r { 0.0 } else { sweep_rad };
                            }
                        }
                    }
                }
            }

            if bg_hits <= 0.0 {
                slice[(y * size + x) as usize] = 0; // 100% transparent pixel
                continue;
            }

            let mut r = 18.0f32;
            let mut g = 22.0f32;
            let mut b = 28.0f32;

            if rim_hits > 0.0 {
                let ra = rim_hits / n_sub;
                r = r * (1.0 - ra) + 48.0 * ra;
                g = g * (1.0 - ra) + 58.0 * ra;
                b = b * (1.0 - ra) + 76.0 * ra;
            }

            if track_hits > 0.0 {
                let ta = track_hits / n_sub;
                r = r * (1.0 - ta) + 32.0 * ta;
                g = g * (1.0 - ta) + 40.0 * ta;
                b = b * (1.0 - ta) + 52.0 * ta;
            }

            if arc_hits > 0.0 {
                let aa = arc_hits / n_sub;
                let avg_angle = sum_angle / arc_hits;
                let t = (avg_angle / sweep_rad).clamp(0.0, 1.0);
                // Vibrant Electric Cyan (0, 229, 255) -> Emerald Green (16, 185, 129)
                let arc_r = 0.0 * (1.0 - t) + 16.0 * t;
                let arc_g = 229.0 * (1.0 - t) + 185.0 * t;
                let arc_b = 255.0 * (1.0 - t) + 129.0 * t;

                r = r * (1.0 - aa) + arc_r * aa;
                g = g * (1.0 - aa) + arc_g * aa;
                b = b * (1.0 - aa) + arc_b * aa;
            }

            // Central Razor Lightning Bolt Polygon
            let b_scale = s / 32.0;
            let bolt_pts = [
                (cx + 1.2 * b_scale, cy - 6.5 * b_scale),
                (cx - 4.2 * b_scale, cy + 0.5 * b_scale),
                (cx - 0.5 * b_scale, cy + 0.5 * b_scale),
                (cx - 1.8 * b_scale, cy + 7.5 * b_scale),
                (cx + 4.2 * b_scale, cy - 0.5 * b_scale),
                (cx + 0.5 * b_scale, cy - 0.5 * b_scale),
            ];

            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if point_in_polygon(px, py, &bolt_pts) {
                r = 0.0;
                g = 229.0;
                b = 255.0;
            }

            let bg_alpha = bg_hits / n_sub;
            let a = (bg_alpha * 255.0) as u32;
            let pr = (r.clamp(0.0, 255.0) * bg_alpha) as u32;
            let pg = (g.clamp(0.0, 255.0) * bg_alpha) as u32;
            let pb = (b.clamp(0.0, 255.0) * bg_alpha) as u32;

            slice[(y * size + x) as usize] = (a << 24) | (pr << 16) | (pg << 8) | pb;
        }
    }

    let hbm_mask = CreateBitmap(size, size, 1, 1, ptr::null());
    let icon_info = ICONINFO {
        fIcon: 1, // TRUE = icon
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm_color,
    };

    let hicon = CreateIconIndirect(&icon_info);

    DeleteObject(hbm_color);
    DeleteObject(hbm_mask);
    DeleteDC(hdc);

    hicon
}

fn point_in_polygon(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_linkdeck_icon() {
        unsafe {
            let icon_16 = create_linkdeck_icon(16);
            assert!(!icon_16.is_null());
            let icon_32 = create_linkdeck_icon(32);
            assert!(!icon_32.is_null());
            let icon_48 = create_linkdeck_icon(48);
            assert!(!icon_48.is_null());
        }
    }
}
