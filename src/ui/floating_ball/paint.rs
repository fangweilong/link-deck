//! 悬浮球的整帧绘制（分层窗口内存位图 + 五种形态分派）。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;
use super::widgets::*;

/// `WM_PAINT` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_paint(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let mut ps: PAINTSTRUCT = mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        let (devices, expanded, font_scale, hover, hover_btn, carousel_index, pulse_phase, style) =
            if let Some(state_arc) = &BALL_STATE {
                let state = state_arc.lock().unwrap();
                (
                    state.devices.clone(),
                    state.expanded,
                    state.font_scale,
                    state.hover,
                    state.hover_btn,
                    state.carousel_index,
                    state.pulse_phase,
                    state.style,
                )
            } else {
                (Vec::new(), false, 1.15, false, 0, 0, 0.0, FloatingBallStyle::Tab)
            };

        if !expanded {
            match style {
                FloatingBallStyle::Tab => render_tab_widget(
                    mem_dc,
                    width,
                    height,
                    &devices,
                    carousel_index,
                    pulse_phase,
                    font_scale,
                    hover,
                ),
                FloatingBallStyle::Orb => render_ball_widget(
                    mem_dc, width, height, &devices, carousel_index, pulse_phase, font_scale,
                ),
                FloatingBallStyle::Ring => render_ring_widget(
                    mem_dc, width, height, &devices, carousel_index, pulse_phase, font_scale,
                ),
                FloatingBallStyle::Pill => render_pill_widget(
                    mem_dc, width, height, &devices, carousel_index, pulse_phase, font_scale,
                ),
                FloatingBallStyle::Badge => render_badge_widget(
                    mem_dc, width, height, &devices, carousel_index, pulse_phase, font_scale,
                ),
            }
        } else {
            let lay = drawer::layout(width, height, devices.len());
            drawer::paint(mem_dc, &lay, &devices, font_scale, hover_btn);
        }

        BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_bmp);
        DeleteObject(mem_bmp);
        DeleteDC(mem_dc);

        EndPaint(hwnd, &mut ps);
        0
}

