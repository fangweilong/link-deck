//! //! 窗口框架消息：无边框窗口的缩放边、拖动区、最小尺寸与关闭。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

/// `WM_NCCALCSIZE` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_nc_calc_size(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if wparam == 0 {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let params = lparam as *mut NCCALCSIZE_PARAMS;
        if params.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let mut wr: RECT = mem::zeroed();
        GetWindowRect(hwnd, &mut wr);
        if IsZoomed(hwnd) != 0 {
            // 最大化时窗口矩形比工作区大出一圈看不见的边框，内缩回来，否则内容溢出屏幕
            let bx = GetSystemMetrics(SM_CXSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
            let by = GetSystemMetrics(SM_CYSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
            (*params).rgrc[0] = RECT {
                left: wr.left + bx,
                top: wr.top + by,
                right: wr.right - bx,
                bottom: wr.bottom - by,
            };
        } else {
            (*params).rgrc[0] = wr;
        }
        0
}

/// `WM_NCHITTEST` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_nc_hit_test(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let base = DefWindowProcW(hwnd, msg, wparam, lparam);
        if base != HTCLIENT as LRESULT {
            return base;
        }
        let mut pt = POINT {
            x: (lparam & 0xFFFF) as i16 as i32,
            y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
        };
        ScreenToClient(hwnd, &mut pt);
        let mut client: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let (w, h) = (client.right, client.bottom);

        if IsZoomed(hwnd) == 0 {
            let b = titlebar::RESIZE_BORDER;
            let left = pt.x < b;
            let right = pt.x >= w - b;
            let top = pt.y < b;
            let bottom = pt.y >= h - b;
            // 四角优先，否则拖角时会变成只拖一条边
            match (top, bottom, left, right) {
                (true, _, true, _) => return HTTOPLEFT as LRESULT,
                (true, _, _, true) => return HTTOPRIGHT as LRESULT,
                (_, true, true, _) => return HTBOTTOMLEFT as LRESULT,
                (_, true, _, true) => return HTBOTTOMRIGHT as LRESULT,
                (true, _, _, _) => return HTTOP as LRESULT,
                (_, true, _, _) => return HTBOTTOM as LRESULT,
                (_, _, true, _) => return HTLEFT as LRESULT,
                (_, _, _, true) => return HTRIGHT as LRESULT,
                _ => {}
            }
        }

        let bar = titlebar::layout(w);
        // 拖动区判定与「按钮区留 HTCLIENT」是同一件事，交给 titlebar 一份实现，
        // 免得这里和命中测试各写一遍边界条件
        if titlebar::is_drag_area(&bar, pt.x, pt.y) {
            return HTCAPTION as LRESULT;
        }
        HTCLIENT as LRESULT
}

/// `WM_GETMINMAXINFO` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_get_minmax_info(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let info = lparam as *mut MINMAXINFO;
        if !info.is_null() {
            (*info).ptMinTrackSize.x = MIN_PANEL_WIDTH;
            (*info).ptMinTrackSize.y = MIN_PANEL_HEIGHT;
        }
        0
}

/// `WM_SIZE` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_size(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if wparam != 1 {
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        }
        0
}

/// `WM_CLOSE` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_close(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if let Some(state_arc) = &PANEL_STATE {
            let mut state = state_arc.lock().unwrap();
            state.visible = false;
        }
        ShowWindow(hwnd, SW_HIDE);
        0
}

