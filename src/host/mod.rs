//! 宿主窗口（隐藏的托盘宿主）：窗口过程 + 定时器 + 命令 + 持久化。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::*;

mod alerts;
mod commands;
mod persist;
mod timer;

pub(crate) use commands::*;
pub(crate) use persist::*;
pub(crate) use timer::*;

use crate::ui::tray::WM_TRAY_CALLBACK;

pub(crate) unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => on_timer(hwnd, msg, wparam, lparam),
        WM_TRAY_CALLBACK => on_tray_callback(hwnd, msg, wparam, lparam),
        WM_COMMAND => on_command(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_DEVICE_RENAME => on_device_rename(hwnd, msg, wparam, lparam),
        WM_LINKDECK_ACTIVATE => on_activate(hwnd, msg, wparam, lparam),
        WM_UPDATE_STATUS => on_update_status(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_UPDATE_PREFS => on_save_update_prefs(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_AUTOSTART => on_save_autostart(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SET_BALL_OPACITY => on_set_ball_opacity(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SET_BALL_STYLE => on_set_ball_style(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_BALL_OPACITY => on_save_ball_opacity(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_TOAST_SETTINGS => on_save_toast_settings(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_CUSTOM_THEME => on_save_custom_theme(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_PANEL_SIZE => on_save_panel_size(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_UI_DENSITY => on_save_ui_density(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_DASHBOARD_PREFS => on_save_dashboard_prefs(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_ONBOARDING => on_save_onboarding(hwnd, msg, wparam, lparam),
        crate::ui::panel::WM_SAVE_UI_STYLE => on_save_ui_style(hwnd, msg, wparam, lparam),
        WM_DESTROY => on_destroy(hwnd, msg, wparam, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

