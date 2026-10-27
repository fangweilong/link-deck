//! 悬浮球的鼠标输入：悬停展开、滚轮切换设备、拖拽停靠、点击开面板。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

/// `WM_MOUSEWHEEL` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_mouse_wheel(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let delta = ((wparam >> 16) as i16) / 120;
        if let Some(state_arc) = &BALL_STATE {
            let mut state = state_arc.lock().unwrap();
            let num_devs = get_carousel_devices(&state.devices).len();
            if num_devs > 1 {
                if delta > 0 {
                    state.carousel_index = if state.carousel_index == 0 {
                        num_devs - 1
                    } else {
                        state.carousel_index - 1
                    };
                } else if delta < 0 {
                    state.carousel_index = (state.carousel_index + 1) % num_devs;
                }
                state.anim_ticks = 0;
                update_tooltip_for_current_device(&state);
            }
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        0
}

/// `WM_MOUSEMOVE` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_mouse_move(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let x = (lparam & 0xFFFF) as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i32;

        let mut tme: TRACKMOUSEEVENT = mem::zeroed();
        tme.cbSize = mem::size_of::<TRACKMOUSEEVENT>() as u32;
        tme.dwFlags = TME_LEAVE;
        tme.hwndTrack = hwnd;
        TrackMouseEvent(&mut tme);

        if let Some(state_arc) = &BALL_STATE {
            let mut state = state_arc.lock().unwrap();
            if state.dragging {
                let mut cur: POINT = mem::zeroed();
                GetCursorPos(&mut cur);
                let dy = cur.y - state.drag_start_cursor.y;
                let new_y = state.drag_start_win.y + dy;

                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let (dock_w, dock_h) = dock_size(state.style);
                let clamped_y = new_y.clamp(20, screen_h - dock_h - 40);
                state.cur_y = clamped_y;

                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    screen_w - dock_w,
                    clamped_y,
                    dock_w,
                    dock_h,
                    SWP_NOACTIVATE,
                );
            } else {
                let mut repaint = false;
                if !state.expanded {
                    if !state.hover {
                        state.hover = true;
                        repaint = true;
                    }
                } else {
                    let (dw, dh) = drawer::size(state.font_scale);
                    let lay = drawer::layout(dw, dh, state.devices.len());
                    let new_btn = drawer::hover_id(drawer::hit_test(&lay, x, y));
                    if state.hover_btn != new_btn {
                        state.hover_btn = new_btn;
                        repaint = true;
                    }
                }
                if repaint {
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        0
}

/// `WM_MOUSELEAVE` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_mouse_leave(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if let Some(state_arc) = &BALL_STATE {
            let mut state = state_arc.lock().unwrap();
            if state.hover || state.hover_btn != 0 {
                state.hover = false;
                state.hover_btn = 0;
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
        0
}

/// `WM_LBUTTONDOWN` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_lbutton_down(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        SetCapture(hwnd);
        let mut cur: POINT = mem::zeroed();
        GetCursorPos(&mut cur);

        let mut rect: RECT = mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);

        if let Some(state_arc) = &BALL_STATE {
            let mut state = state_arc.lock().unwrap();
            if !state.expanded {
                state.dragging = true;
                state.drag_start_cursor = cur;
                state.drag_start_win = POINT {
                    x: rect.left,
                    y: rect.top,
                };
            }
        }
        0
}

/// `WM_LBUTTONUP` 的处理体，参数与窗口过程一致。
pub(crate) unsafe fn on_lbutton_up(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        ReleaseCapture();
        let mut cur: POINT = mem::zeroed();
        GetCursorPos(&mut cur);

        let mut toggle_drawer = false;
        let mut open_panel = false;

        if let Some(state_arc) = &BALL_STATE {
            let mut state = state_arc.lock().unwrap();
            if state.dragging {
                state.dragging = false;
                let dist = (cur.y - state.drag_start_cursor.y).abs()
                    + (cur.x - state.drag_start_cursor.x).abs();
                if dist < 5 {
                    toggle_drawer = true;
                }
            } else if state.expanded {
                if state.hover_btn == 1 {
                    toggle_drawer = true;
                } else if state.hover_btn == 2 {
                    open_panel = true;
                } else {
                    open_panel = true;
                }
            }
        }

        if toggle_drawer {
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            if let Some(state_arc) = &BALL_STATE {
                let mut state = state_arc.lock().unwrap();
                state.expanded = !state.expanded;
                let is_exp = state.expanded;
                let cur_y = state.cur_y;
                let font_scale = state.font_scale;
                let style = state.style;

                let (new_w, new_h, new_x) = if is_exp {
                    let exp_w = drawer::drawer_width(font_scale);
                    (exp_w, drawer::DRAWER_HEIGHT, screen_w - exp_w)
                } else {
                    let (dock_w, dock_h) = dock_size(style);
                    (dock_w, dock_h, screen_w - dock_w)
                };

                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    new_x,
                    cur_y,
                    new_w,
                    new_h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                set_ball_alpha(hwnd, layered_alpha(is_exp, state.opacity));
            }
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        }

        if open_panel {
            let host = if let Some(state_arc) = &BALL_STATE {
                state_arc.lock().unwrap().host_hwnd
            } else {
                ptr::null_mut()
            };
            if !host.is_null() {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    host,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                    crate::ui::tray::IDM_SHOW_PANEL as _,
                    0,
                );
            }
        }
        0
}

