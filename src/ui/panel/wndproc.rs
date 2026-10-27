//! 管理面板的窗口过程。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

pub(crate) unsafe extern "system" fn panel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        // 去掉系统标题栏：客户区铺满整窗，标题栏由 titlebar 自己画。
        // 保留 WS_OVERLAPPEDWINDOW 的意义在于最大化、贴边、阴影、缩放仍由系统处理。
        WM_NCCALCSIZE => on_nc_calc_size(hwnd, msg, wparam, lparam),
        // 非客户区没有内容：不画，避免系统在边缘补一圈标题栏底色
        WM_NCPAINT => 0,
        WM_NCACTIVATE => 1,
        WM_NCHITTEST => on_nc_hit_test(hwnd, msg, wparam, lparam),
        WM_GETMINMAXINFO => on_get_minmax_info(hwnd, msg, wparam, lparam),
        WM_SIZE => on_size(hwnd, msg, wparam, lparam),
        WM_VSCROLL => on_vscroll(hwnd, msg, wparam, lparam),
        WM_EXITSIZEMOVE => on_exit_size_move(hwnd, msg, wparam, lparam),
        WM_MOUSEWHEEL => on_mouse_wheel(hwnd, msg, wparam, lparam),
        WM_CLOSE => on_close(hwnd, msg, wparam, lparam),
        WM_PAINT => on_paint(hwnd, msg, wparam, lparam),
        WM_MOUSEMOVE => on_mouse_move(hwnd, msg, wparam, lparam),
        WM_LBUTTONDOWN => on_lbutton_down(hwnd, msg, wparam, lparam),
        WM_LBUTTONUP => on_lbutton_up(hwnd, msg, wparam, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
