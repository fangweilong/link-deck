//! //! 鼠标与滚动输入：悬停、按下、抬起（含点击派发）、滚轮与滚动条。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

/// `WM_VSCROLL` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_vscroll(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let code = (wparam & 0xFFFF) as i32;
        let thumb_pos = ((wparam >> 16) & 0xFFFF) as i32;
        if let Some(state_arc) = &PANEL_STATE {
            let mut state = state_arc.lock().unwrap();
            let mut client: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let viewport = dashboard::viewport_height(client.bottom - client.top);
            let on_devices = state.active_tab == 0;
            let step = dashboard::card_geometry(state.ui_density).1;
            let line_step = if on_devices { step / 3 } else { 48 };
            let max_scroll = if on_devices {
                device_max_scroll(hwnd, &state)
            } else {
                settings_max_scroll(hwnd, &state)
            };
            let mut pos = if on_devices { state.scroll_y } else { state.settings_scroll_y };
            match code {
                x if x == SB_LINEUP        => pos -= line_step,
                x if x == SB_LINEDOWN      => pos += line_step,
                x if x == SB_PAGEUP        => pos -= viewport,
                x if x == SB_PAGEDOWN      => pos += viewport,
                x if x == SB_TOP           => pos = 0,
                x if x == SB_BOTTOM        => pos = max_scroll,
                x if x == SB_THUMBTRACK    => pos = thumb_pos,
                x if x == SB_THUMBPOSITION => pos = thumb_pos,
                _ => {}
            }
            let clamped = pos.clamp(0, max_scroll);
            if on_devices {
                state.scroll_y = clamped;
            } else {
                state.settings_scroll_y = clamped;
            }
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        0
}

/// `WM_EXITSIZEMOVE` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_exit_size_move(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let mut r: RECT = mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let (w, h, host) = if let Some(state_arc) = &PANEL_STATE {
            let state = state_arc.lock().unwrap();
            (r.right - r.left, r.bottom - r.top, state.host_hwnd)
        } else {
            (0, 0, ptr::null_mut())
        };
        if !host.is_null() {
            PostMessageW(host, WM_SAVE_PANEL_SIZE, w as WPARAM, h as LPARAM);
        }
        0
}

/// `WM_MOUSEWHEEL` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_mouse_wheel(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let delta = ((wparam >> 16) as i16) as i32;
        if let Some(state_arc) = &PANEL_STATE {
            let mut state = state_arc.lock().unwrap();
            if state.active_tab == 0 {
                let max_scroll = device_max_scroll(hwnd, &state);
                state.scroll_y = (state.scroll_y - delta / 3).clamp(0, max_scroll);
            } else {
                let max_scroll = settings_max_scroll(hwnd, &state);
                state.settings_scroll_y = (state.settings_scroll_y - delta / 3).clamp(0, max_scroll);
            }
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
        0
}

/// `WM_MOUSEMOVE` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_mouse_move(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let x = (lparam & 0xFFFF) as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i32;

        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let dragging_slider = PANEL_STATE
            .as_ref()
            .map(|sa| sa.lock().unwrap().slider_drag)
            .unwrap_or(false);
        if dragging_slider {
            update_ball_opacity_from_x(hwnd, x);
            return 0;
        }

        let active_tab = if let Some(state_arc) = &PANEL_STATE {
            state_arc.lock().unwrap().active_tab
        } else {
            0
        };

        // hover 命中一律走布局的 hit_test，绘制与命中同源
        let new_hover = if let Some(state_arc) = &PANEL_STATE {
            let st = state_arc.lock().unwrap();
            let devices = prepare_devices(st.devices.clone(), st.device_filter, st.device_sort);
            let lay = dashboard::layout(
                width, height, st.ui_density, st.font_scale, &devices,
                st.devices.len(), st.show_onboarding, st.scroll_y,
            );
            // 标题栏按钮永远在最上层的命中优先级
            let bar = titlebar::layout(width);
            if let Some(kind) = titlebar::hit(&bar, x, y) {
                titlebar::hover_id(kind)
            // 顶栏两个标签在设备页与设置页都存在，先判（否则设置页下无 hover 高亮）
            } else if lay.tab_device.contains(x, y) {
                dashboard::hover_id(dashboard::Hit::TabDevice)
            } else if lay.tab_settings.contains(x, y) {
                dashboard::hover_id(dashboard::Hit::TabSettings)
            } else if st.active_tab == 0 {
                dashboard::hit_test(&lay, x, y).map(dashboard::hover_id).unwrap_or(0)
            } else {
                let page = settings::layout(&settings_ctx(&st), width);
                match settings_hit_at(&page, &lay.btn_refresh, height, st.settings_scroll_y, x, y) {
                    Some(SettingsHit::Nav(g)) => 160 + g as i32,
                    Some(SettingsHit::Back) => 90,
                    Some(SettingsHit::Control(id)) => id,
                    None => 0,
                }
            }
        } else {
            0
        };

        if let Some(state_arc) = &PANEL_STATE {
            let mut state = state_arc.lock().unwrap();
            if state.hover_btn != new_hover {
                state.hover_btn = new_hover;
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
        0
}

/// `WM_LBUTTONDOWN` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_lbutton_down(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let x = (lparam & 0xFFFF) as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i32;

        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;

        let height = rect.bottom - rect.top;
        let on_slider = PANEL_STATE.as_ref().is_some_and(|sa| {
            let st = sa.lock().unwrap();
            if st.active_tab != 1 {
                return false;
            }
            let page = settings::layout(&settings_ctx(&st), width);
            let view_h = (height - dashboard::FOOTER_H - settings::CONTENT_TOP).max(1);
            // 与绘制同源：滚动区之外（底栏）按下不算按在滑块上
            settings::hit_test_at(&page, x, y, st.settings_scroll_y, settings::CONTENT_TOP, view_h)
                == Some(settings::SLIDER_ID)
        });
        if on_slider {
            SetCapture(hwnd);
            if let Some(sa) = &PANEL_STATE {
                sa.lock().unwrap().slider_drag = true;
            }
            update_ball_opacity_from_x(hwnd, x);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// `WM_LBUTTONUP` 的处理体。参数与窗口过程保持一致，方便逐条对照。
pub(crate) unsafe fn on_lbutton_up(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let x = (lparam & 0xFFFF) as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i32;

        let slider_released = PANEL_STATE
            .as_ref()
            .map(|sa| {
                let mut st = sa.lock().unwrap();
                let was = st.slider_drag;
                st.slider_drag = false;
                was
            })
            .unwrap_or(false);
        if slider_released {
            ReleaseCapture();
            if let Some(sa) = &PANEL_STATE {
                let host = sa.lock().unwrap().host_hwnd;
                if !host.is_null() {
                    PostMessageW(host, WM_SAVE_BALL_OPACITY, 0, 0);
                }
            }
            return 0;
        }

        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // 自绘标题栏按钮：最小化 / 最大化还原 / 关闭
        let bar = titlebar::layout(width);
        if let Some(kind) = titlebar::hit(&bar, x, y) {
            match kind {
                titlebar::BtnKind::Min => {
                    ShowWindow(hwnd, SW_MINIMIZE);
                }
                titlebar::BtnKind::Max => {
                    if IsZoomed(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                }
                titlebar::BtnKind::Close => {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                }
            }
            return 0;
        }

        let (host, active_tab, show_onboarding, device_count) = if let Some(state_arc) = &PANEL_STATE {
            let s = state_arc.lock().unwrap();
            (s.host_hwnd, s.active_tab, s.show_onboarding, s.devices.len())
        } else {
            (ptr::null_mut(), 0, false, 0)
        };

        // 命中一律由布局给出：绘制 / hover / 点击三处读同一份坐标
        let (hit, dash_devices, back_btn) = if let Some(state_arc) = &PANEL_STATE {
            let st = state_arc.lock().unwrap();
            let devices = prepare_devices(st.devices.clone(), st.device_filter, st.device_sort);
            let lay = dashboard::layout(
                width, height, st.ui_density, st.font_scale, &devices,
                st.devices.len(), st.show_onboarding, st.scroll_y,
            );
            (dashboard::hit_test(&lay, x, y), devices, lay.btn_refresh)
        } else {
            (None, Vec::new(), dashboard::Rect2::new(0, 0, 0, 0))
        };

        // 1. Top Header Tabs
        match hit {
            Some(dashboard::Hit::TabDevice) | Some(dashboard::Hit::TabSettings) => {
                let target = if matches!(hit, Some(dashboard::Hit::TabDevice)) { 0 } else { 1 };
                if target != active_tab {
                    if let Some(state_arc) = &PANEL_STATE {
                        state_arc.lock().unwrap().active_tab = target;
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
                return 0;
            }
            _ => {}
        }

        if active_tab == 0 {
            match hit {
                Some(dashboard::Hit::Copy(idx)) => {
                    if let Some(dev) = dash_devices.get(idx) {
                        let detail = format!(
                            "{}\n连接: {}\nVID/PID: {:04X}/{:04X}\n路径: {}",
                            dev.name,
                            dev.connection.as_str(),
                            dev.vendor_id,
                            dev.product_id,
                            dev.path
                        );
                        let _ = copy_text_to_clipboard(hwnd, &detail);
                    }
                }
                Some(dashboard::Hit::Edit(idx)) => {
                    if let Some(dev) = dash_devices.get(idx) {
                        let (dev_id, cur_name) = (dev.id.clone(), dev.name.clone());
                        if let Some(new_name) = show_rename_dialog(hwnd, &cur_name) {
                            send_rename_to_host(host, &dev_id, &new_name);
                        }
                    }
                }
                Some(dashboard::Hit::Card(idx)) => {
                    if let Some(dev) = dash_devices.get(idx) {
                        if let Some(state_arc) = &PANEL_STATE {
                            state_arc.lock().unwrap().selected_device_id = Some(dev.id.clone());
                        }
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
                Some(dashboard::Hit::ShowAll) => {
                    let sort = PANEL_STATE.as_ref()
                        .and_then(|sa| sa.lock().ok().map(|s| s.device_sort))
                        .unwrap_or(crate::config::DeviceSort::Priority);
                    set_dashboard_preferences(hwnd, host, crate::config::DeviceFilter::All, sort);
                }
                Some(dashboard::Hit::Start) => {
                    dismiss_onboarding(host);
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
                Some(dashboard::Hit::Refresh) => {
                    if let Some(state_arc) = &PANEL_STATE {
                        state_arc.lock().unwrap().refresh_status = "正在刷新设备…".to_string();
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                    if !host.is_null() {
                        PostMessageW(host, windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND, crate::ui::tray::IDM_REFRESH as _, 0);
                    }
                }
                _ => {}
            }
        } else {
            // 设置页：导航条跳转 + 底栏返回 + 卡片内控件派发（与 hover 共用 settings_hit_at）
            let (scroll_y, ctx) = PANEL_STATE.as_ref()
                .map(|sa| { let st = sa.lock().unwrap(); (st.settings_scroll_y, settings_ctx(&st)) })
                .unwrap_or((0, fallback_settings_ctx()));
            let page = settings::layout(&ctx, width);
            match settings_hit_at(&page, &back_btn, height, scroll_y, x, y) {
                Some(SettingsHit::Nav(group)) => {
                    // 分页切换：内容整体替换，滚动一律归零
                    if let Some(sa) = &PANEL_STATE {
                        let mut st = sa.lock().unwrap();
                        st.settings_group = group.min(settings::GROUP_COUNT - 1);
                        st.settings_scroll_y = 0;
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
                Some(SettingsHit::Back) => {
                    if let Some(state_arc) = &PANEL_STATE {
                        state_arc.lock().unwrap().active_tab = 0;
                    }
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 0);
                }
                Some(SettingsHit::Control(id)) => dispatch_settings_click(hwnd, host, id),
                None => {}
            }
        }
        0
}

