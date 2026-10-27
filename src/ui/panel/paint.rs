//! //! `WM_PAINT` 的整页绘制：顶栏、设备瓦片 / 设置页分页、底栏。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use super::*;

/// `WM_PAINT` 的处理体。参数与窗口过程保持一致，方便逐条对照。
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

        let bg_brush = CreateSolidBrush(THEME.bg);
        FillRect(mem_dc, &rect, bg_brush);
        DeleteObject(bg_brush);

        SetBkMode(mem_dc, TRANSPARENT as i32);

        let (
            raw_devices, ball_on, ball_opacity, font_scale, poll_ms, hover_btn, active_tab,
            enable_toast, toast_info, toast_warn, toast_crit, toast_gaming, toast_sound,
            ui_density, scroll_y, settings_scroll_y, refresh_status, device_filter, device_sort,
            ui_style, show_onboarding, selected_device_id, auto_update, auto_start,
        ) =
            if let Some(state_arc) = &PANEL_STATE {
                let state = state_arc.lock().unwrap();
                (state.devices.clone(), state.floating_ball_enabled, state.ball_opacity, state.font_scale,
                    state.poll_interval_ms, state.hover_btn, state.active_tab,
                    state.enable_toast, state.toast_info, state.toast_warning,
                    state.toast_critical, state.toast_gaming_suppression,
                    state.enable_low_battery_sound,
                    state.ui_density, state.scroll_y, state.settings_scroll_y, state.refresh_status.clone(),
                    state.device_filter, state.device_sort, state.ui_style, state.show_onboarding,
                    state.selected_device_id.clone(), state.auto_update, state.auto_start)
            } else {
                (Vec::new(), false, 80, 1.15, 300_000, 0, 0, true, 50, 30, 10, true, true,
                    crate::config::UiDensity::Standard, 0, 0, "就绪 · 可立即刷新".to_string(),
                    crate::config::DeviceFilter::All, crate::config::DeviceSort::Priority,
                    crate::config::UiStyle::Ember, true, None, true, false)
            };
        let raw_count = raw_devices.len();
        let devices = prepare_devices(raw_devices, device_filter, device_sort);

        // 顶栏是一整块：设备页只放标题行，设置页把分类导航也收进来
        let metrics = crate::ui::theme::METRICS;
        let dash = dashboard::layout(
            width, height, ui_density, font_scale, &devices,
            raw_count, show_onboarding, scroll_y,
        );
        let header_h = dashboard::header_height(active_tab);

        // Header Banner：与内容区同色（融合），底部一条发丝线交代边界
        let h_brush = CreateSolidBrush(THEME.header);
        let header_rect = RECT { left: 0, top: 0, right: dash.header.right(), bottom: header_h };
        FillRect(mem_dc, &header_rect, h_brush);
        DeleteObject(h_brush);
        let line = CreateSolidBrush(THEME.border);
        let line_rect = RECT { left: 0, top: header_h - 1, right: dash.header.right(), bottom: header_h };
        FillRect(mem_dc, &line_rect, line);
        DeleteObject(line);

        // Header Title：主标题是「当前页在做什么」，副标题是这一页的状态
        let (h_title, h_sub) = dashboard::header_copy(active_tab, &devices);
        let font_h1 = create_clear_font((metrics.fs_head * font_scale).round() as i32, true);
        let old_font = SelectObject(mem_dc, font_h1);
        SetTextColor(mem_dc, THEME.text);

        let mut h_title_rect = RECT {
            left: metrics.pad_page,
            top: titlebar::TITLE_H + 10,
            right: dash.tab_device.x - metrics.gap_card,
            bottom: titlebar::TITLE_H + 34,
        };
        draw_text_clean(mem_dc, &h_title, &mut h_title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // Header Subtitle
        let font_sub = create_clear_font((metrics.fs_body * font_scale).round() as i32, false);
        SelectObject(mem_dc, font_sub);
        SetTextColor(mem_dc, THEME.muted);

        let mut h_sub_rect = RECT {
            left: metrics.pad_page,
            top: titlebar::TITLE_H + 36,
            right: dash.tab_device.x - metrics.gap_card,
            bottom: titlebar::TITLE_H + 56,
        };
        draw_text_clean(mem_dc, &h_sub, &mut h_sub_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // 页面切换：连体分段控件（不再用两个独立 pill）
        let hover_seg = if hover_btn == dashboard::hover_id(dashboard::Hit::TabDevice) {
            Some(0)
        } else if hover_btn == dashboard::hover_id(dashboard::Hit::TabSettings) {
            Some(1)
        } else {
            None
        };
        render_segmented(
            mem_dc,
            dash.tab_device.x,
            dash.tab_device.y,
            dash.tab_device.w + dash.tab_settings.w,
            dash.tab_device.h,
            &[("设备监控", active_tab == 0), ("偏好设置", active_tab == 1)],
            hover_seg,
            font_scale,
        );
        // 自绘标题栏：与界面同色，按钮跟着主题走
        let bar = titlebar::layout(width);
        titlebar::paint(mem_dc, &bar, hover_btn, IsZoomed(hwnd) != 0, font_scale);

        if active_tab == 0 {
            dashboard::paint(
                mem_dc,
                &dash,
                &devices,
                font_scale,
                hover_btn,
                &refresh_status,
                selected_device_id.as_deref(),
                raw_count,
                scroll_y,
            );
        } else {
            let ctx = PANEL_STATE
                .as_ref()
                .map(|sa| settings_ctx(&sa.lock().unwrap()))
                .unwrap_or_else(fallback_settings_ctx);
            let page = settings::layout(&ctx, width);
            settings::paint_nav(mem_dc, &page, hover_btn, font_scale, ctx.active_group);
            // Cards scroll under a fixed clip; the nav strip above stays put.
            let settings_clip = CreateRectRgn(
                0,
                settings::CONTENT_TOP,
                width,
                (height - dashboard::FOOTER_H).max(settings::CONTENT_TOP + 1),
            );
            SelectClipRgn(mem_dc, settings_clip);
            settings::paint_cards(mem_dc, &page, &ctx, settings_scroll_y, hover_btn, settings::CONTENT_TOP);
            SelectClipRgn(mem_dc, ptr::null_mut());
            DeleteObject(settings_clip);
            draw_scroll_thumb(
                mem_dc,
                width,
                settings::CONTENT_TOP,
                height - dashboard::FOOTER_H,
                page.content_h,
                settings_scroll_y,
            );
            // Footer：与内容区同色，顶部一条发丝线交代边界（与设备页一致）
            let footer_y = (height - dashboard::FOOTER_H).max(0);
            let fb = CreateSolidBrush(THEME.header);
            let fr = RECT { left: 0, top: footer_y, right: width, bottom: height };
            FillRect(mem_dc, &fr, fb);
            DeleteObject(fb);
            let fl = CreateSolidBrush(THEME.border);
            let flr = RECT { left: 0, top: footer_y, right: width, bottom: footer_y + 1 };
            FillRect(mem_dc, &flr, fl);
            DeleteObject(fl);
            // Footer "返回" button is outside the scroll clip — always visible.
            render_action_btn(
                mem_dc,
                dash.btn_refresh.x,
                dash.btn_refresh.y,
                dash.btn_refresh.w,
                dash.btn_refresh.h,
                "返回设备监控",
                hover_btn == 90,
                font_scale,
            );

            // Version label has its own right-aligned slot; it never shares button bounds.
            SetTextColor(mem_dc, THEME.muted);
            let mut ver_rect = RECT {
                left: dash.version.x,
                top: dash.version.y,
                right: dash.version.right(),
                bottom: dash.version.bottom(),
            };
            draw_text_clean(mem_dc, concat!("v", env!("CARGO_PKG_VERSION"), " (Rust)"), &mut ver_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);
        }

        // Blit
        BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_font);
        DeleteObject(font_h1);
        DeleteObject(font_sub);
        // Delete fonts
        SelectObject(mem_dc, old_bmp);
        DeleteObject(mem_bmp);
        DeleteDC(mem_dc);

        EndPaint(hwnd, &mut ps);
        0
}

