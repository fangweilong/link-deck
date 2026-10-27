//! 设备重命名对话框（自绘非模态弹窗 + 自己的窗口过程）。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;


pub(crate) static mut DLG_RESULT_STRING: Option<String> = None;
pub(crate) static mut DLG_EDIT_CONTROL: HWND = ptr::null_mut();
pub(crate) static mut DLG_HOVER_BTN: i32 = 0;

pub(crate) unsafe extern "system" fn modern_rename_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            let hdc = wparam as HDC;
            SetBkColor(hdc, THEME.row);
            SetTextColor(hdc, THEME.text);
            static mut BRUSH: Option<windows_sys::Win32::Graphics::Gdi::HBRUSH> = None;
            if BRUSH.is_none() {
                BRUSH = Some(CreateSolidBrush(THEME.row));
            }
            return BRUSH.unwrap() as LRESULT;
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
            let old_bmp = SelectObject(mem_dc, mem_bmp);

            // Dark background #181B20
            let bg_brush = CreateSolidBrush(THEME.card);
            FillRect(mem_dc, &rect, bg_brush);
            DeleteObject(bg_brush);

            // Subtle border
            let pen = CreatePen(PS_SOLID, 1, THEME.border);
            let op = SelectObject(mem_dc, pen);
            let ob = SelectObject(mem_dc, windows_sys::Win32::Graphics::Gdi::GetStockObject(5 /* NULL_BRUSH */));
            RoundRect(mem_dc, 0, 0, width, height, 12, 12);
            SelectObject(mem_dc, ob);
            SelectObject(mem_dc, op);
            DeleteObject(pen);

            SetBkMode(mem_dc, TRANSPARENT as i32);

            // Title: "✏️ 自定义外设名称"
            let font_title = create_clear_font(15, true);
            let old_f = SelectObject(mem_dc, font_title);
            SetTextColor(mem_dc, THEME.text);
            let mut t_r = RECT { left: 22, top: 18, right: width - 22, bottom: 42 };
            draw_text_clean(mem_dc, "✏️ 自定义外设显示名称", &mut t_r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Hint
            let font_hint = create_clear_font(12, false);
            SelectObject(mem_dc, font_hint);
            SetTextColor(mem_dc, THEME.muted);
            let mut h_r = RECT { left: 22, top: 44, right: width - 22, bottom: 64 };
            draw_text_clean(mem_dc, "修改后将在悬浮球、控制中心及托盘同步更新 (留空重置出厂名):", &mut h_r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Draw input box outer border
            let input_pen = CreatePen(PS_SOLID, 1, THEME.border);
            let ip = SelectObject(mem_dc, input_pen);
            let ib = SelectObject(mem_dc, windows_sys::Win32::Graphics::Gdi::GetStockObject(5));
            RoundRect(mem_dc, 20, 70, width - 20, 104, 6, 6);
            SelectObject(mem_dc, ib);
            SelectObject(mem_dc, ip);
            DeleteObject(input_pen);

            // OK Button (width - 190 .. width - 110, y: 120 .. 152)
            let ok_hover = DLG_HOVER_BTN == 1;
            let ok_bg = if ok_hover { THEME.accent_strong } else { THEME.accent };
            let ok_b = CreateSolidBrush(ok_bg);
            let ok_p = CreatePen(PS_SOLID, 1, if ok_hover { THEME.accent_strong } else { THEME.accent });
            let o1 = SelectObject(mem_dc, ok_p);
            let o2 = SelectObject(mem_dc, ok_b);
            RoundRect(mem_dc, width - 190, 120, width - 105, 152, 6, 6);
            SelectObject(mem_dc, o2);
            DeleteObject(ok_b);
            SelectObject(mem_dc, o1);
            DeleteObject(ok_p);

            let font_btn = create_clear_font(13, true);
            SelectObject(mem_dc, font_btn);
            SetTextColor(mem_dc, THEME.text);
            let mut b1_r = RECT { left: width - 190, top: 120, right: width - 105, bottom: 152 };
            draw_text_clean(mem_dc, "确定保存", &mut b1_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // Cancel Button (width - 95 .. width - 20, y: 120 .. 152)
            let can_hover = DLG_HOVER_BTN == 2;
            let can_bg = if can_hover { THEME.btn_hover } else { THEME.btn };
            let can_b = CreateSolidBrush(can_bg);
            let can_p = CreatePen(PS_SOLID, 1, if can_hover { THEME.muted } else { THEME.border });
            let c1 = SelectObject(mem_dc, can_p);
            let c2 = SelectObject(mem_dc, can_b);
            RoundRect(mem_dc, width - 95, 120, width - 20, 152, 6, 6);
            SelectObject(mem_dc, c2);
            DeleteObject(can_b);
            SelectObject(mem_dc, c1);
            DeleteObject(can_p);

            SetTextColor(mem_dc, if can_hover { THEME.text } else { THEME.muted });
            let mut b2_r = RECT { left: width - 95, top: 120, right: width - 20, bottom: 152 };
            draw_text_clean(mem_dc, "取消", &mut b2_r, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_f);
            DeleteObject(font_title);
            DeleteObject(font_hint);
            DeleteObject(font_btn);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(mem_bmp);
            DeleteDC(mem_dc);

            EndPaint(hwnd, &mut ps);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;

            let mut new_h = 0;
            if y >= 120 && y <= 152 {
                if x >= width - 190 && x <= width - 105 {
                    new_h = 1;
                } else if x >= width - 95 && x <= width - 20 {
                    new_h = 2;
                }
            }
            if DLG_HOVER_BTN != new_h {
                DLG_HOVER_BTN = new_h;
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let mut rect: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = rect.right - rect.left;

            if y >= 120 && y <= 152 {
                if x >= width - 190 && x <= width - 105 {
                    // OK
                    let mut buf = [0u16; 256];
                    let len = GetWindowTextW(DLG_EDIT_CONTROL, buf.as_mut_ptr(), buf.len() as i32);
                    let res = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
                    DLG_RESULT_STRING = Some(res);
                    DestroyWindow(hwnd);
                    return 0;
                } else if x >= width - 95 && x <= width - 20 {
                    // Cancel
                    DLG_RESULT_STRING = None;
                    DestroyWindow(hwnd);
                    return 0;
                }
            }
            0
        }
        WM_CLOSE => {
            DLG_RESULT_STRING = None;
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub unsafe fn show_rename_dialog(parent_hwnd: HWND, current_name: &str) -> Option<String> {
    DLG_RESULT_STRING = None;
    DLG_HOVER_BTN = 0;

    let instance = GetModuleHandleW(ptr::null());
    let class_name: Vec<u16> = "LinkDeck_ModernRenameDlg\0".encode_utf16().collect();

    let mut wcx: WNDCLASSEXW = mem::zeroed();
    wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
    wcx.style = CS_HREDRAW | CS_VREDRAW;
    wcx.lpfnWndProc = Some(modern_rename_wnd_proc);
    wcx.hInstance = instance;
    wcx.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
    wcx.lpszClassName = class_name.as_ptr();
    RegisterClassExW(&wcx);

    let dlg_w = 420;
    let dlg_h = 175;

    let mut parent_rect: RECT = mem::zeroed();
    GetClientRect(parent_hwnd, &mut parent_rect);
    let mut pt = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    windows_sys::Win32::Graphics::Gdi::ClientToScreen(parent_hwnd, &mut pt);

    let pos_x = pt.x + (parent_rect.right - dlg_w) / 2;
    let pos_y = pt.y + (parent_rect.bottom - dlg_h) / 2;

    let dlg_title: Vec<u16> = "自定义外设名称\0".encode_utf16().collect();
    let dlg_hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        dlg_title.as_ptr(),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        pos_x,
        pos_y,
        dlg_w,
        dlg_h,
        parent_hwnd,
        ptr::null_mut(),
        instance,
        ptr::null(),
    );

    if dlg_hwnd.is_null() {
        return None;
    }

    let edit_cls: Vec<u16> = "EDIT\0".encode_utf16().collect();
    let init_txt: Vec<u16> = format!("{}\0", current_name).encode_utf16().collect();

    let font_edit = create_clear_font(15, false);

    let edit_hwnd = CreateWindowExW(
        0,
        edit_cls.as_ptr(),
        init_txt.as_ptr(),
        WS_CHILD | WS_VISIBLE | (ES_AUTOHSCROLL as u32),
        26,
        74,
        dlg_w - 52,
        24,
        dlg_hwnd,
        ptr::null_mut(),
        instance,
        ptr::null(),
    );

    DLG_EDIT_CONTROL = edit_hwnd;
    SendMessageW(edit_hwnd, WM_SETFONT, font_edit as _, 1);
    // Explicitly set text
    SetWindowTextW(edit_hwnd, init_txt.as_ptr());
    // Place cursor at the end so existing name is intact and immediately editable
    let text_len = current_name.chars().count() as isize;
    SendMessageW(edit_hwnd, 0x00B1 /* EM_SETSEL */, text_len as WPARAM, text_len as LPARAM);
    SetFocus(edit_hwnd);

    EnableWindow(parent_hwnd, 0);

    let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = mem::zeroed();
    while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
        if msg.message == WM_KEYDOWN && msg.wParam == 13 {
            // Enter key -> OK
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(DLG_EDIT_CONTROL, buf.as_mut_ptr(), buf.len() as i32);
            let res = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();
            DLG_RESULT_STRING = Some(res);
            DestroyWindow(dlg_hwnd);
            break;
        } else if msg.message == WM_KEYDOWN && msg.wParam == 27 {
            // Esc key -> Cancel
            DLG_RESULT_STRING = None;
            DestroyWindow(dlg_hwnd);
            break;
        }

        if IsDialogMessageW(dlg_hwnd, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(dlg_hwnd) == 0 {
            break;
        }
    }

    EnableWindow(parent_hwnd, 1);
    SetForegroundWindow(parent_hwnd);
    DeleteObject(font_edit);

    DLG_RESULT_STRING.take()
}

pub unsafe fn send_rename_to_host(host_hwnd: HWND, device_id: &str, new_alias: &str) {
    let payload = Box::into_raw(Box::new((device_id.to_string(), new_alias.to_string())));
    PostMessageW(host_hwnd, WM_DEVICE_RENAME, payload as WPARAM, 0);
}
