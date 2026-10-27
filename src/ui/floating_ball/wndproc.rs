//! 悬浮球窗口过程。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;
use super::input::*;
use super::paint::*;

pub(crate) unsafe extern "system" fn ball_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_BALL_ANIM);
            0
        }
        WM_TIMER => {
            if wparam == TIMER_BALL_ANIM {
                if let Some(state_arc) = &BALL_STATE {
                    let mut state = state_arc.lock().unwrap();
                    let num_devs = get_carousel_devices(&state.devices).len();

                    state.pulse_phase += 0.12;
                    if state.pulse_phase > std::f32::consts::TAU {
                        state.pulse_phase -= std::f32::consts::TAU;
                    }

                    if !state.expanded && !state.hover && !state.dragging && num_devs > 1 {
                        state.anim_ticks += 1;
                        if state.anim_ticks >= 30 {
                            state.anim_ticks = 0;
                            state.carousel_index = (state.carousel_index + 1) % num_devs;
                            update_tooltip_for_current_device(&state);
                        }
                    }
                }
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_MOUSEWHEEL => on_mouse_wheel(hwnd, msg, wparam, lparam),
        WM_ERASEBKGND => 1,
        WM_PAINT => on_paint(hwnd, msg, wparam, lparam),
        WM_MOUSEMOVE => on_mouse_move(hwnd, msg, wparam, lparam),
        WM_MOUSELEAVE => on_mouse_leave(hwnd, msg, wparam, lparam),
        WM_LBUTTONDOWN => on_lbutton_down(hwnd, msg, wparam, lparam),
        WM_LBUTTONUP => on_lbutton_up(hwnd, msg, wparam, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}


