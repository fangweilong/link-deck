//! 托盘与菜单命令：菜单回调、命令分发、自启/轮播/字号开关、改名与退出。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::*;
use crate::ui::panel::LivePrefs;
use crate::ui::tray::{
    IDM_APPLY_UPDATE, IDM_CHECK_UPDATE, IDM_EXIT, IDM_FONT_SCALE_100, IDM_FONT_SCALE_115,
    IDM_FONT_SCALE_130, IDM_POLL_15M, IDM_POLL_1M, IDM_POLL_30M, IDM_POLL_30S, IDM_POLL_5M,
    IDM_POLL_60M, IDM_REFRESH, IDM_SHOW_PANEL, IDM_SHOW_SETTINGS, IDM_TOGGLE_AUTOSTART,
    IDM_TOGGLE_BALL,
};

/// `WM_TRAY_CALLBACK` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_tray_callback(hwnd: HWND, _msg: u32, _wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let event = (lparam & 0xFFFF) as u32;
        if event == WM_RBUTTONUP {
            let (ball_on, auto_start, font_scale, poll_ms) = if let Some(app_arc) = &APP_INSTANCE {
                let cfg = &app_arc.lock().unwrap().config;
                (cfg.enable_floating_ball, cfg.auto_start, cfg.font_scale, cfg.poll_interval_ms)
            } else {
                (false, false, 1.15, 3000)
            };

            // 菜单是非模态的：选中后由菜单自己把 WM_COMMAND 回投给本窗口
            TrayIcon::show_context_menu(hwnd, ball_on, auto_start, font_scale, poll_ms);
        } else if event == WM_LBUTTONUP {
            PostMessageW(hwnd, WM_COMMAND, IDM_SHOW_PANEL as _, 0);
        }
        0
}

/// `WM_COMMAND` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_command(hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let cmd_id = (wparam & 0xFFFF) as usize;
        match cmd_id {
            IDM_SHOW_PANEL => {
                if let Some(app_arc) = &APP_INSTANCE {
                    let app = app_arc.lock().unwrap();
                    if let Some(panel) = &app.panel {
                        panel.toggle();
                    }
                }
            }
            IDM_SHOW_SETTINGS => {
                if let Some(app_arc) = &APP_INSTANCE {
                    let app = app_arc.lock().unwrap();
                    if let Some(panel) = &app.panel {
                        panel.show_settings();
                    }
                }
            }
            IDM_TOGGLE_BALL => {
                toggle_floating_ball();
            }
            IDM_FONT_SCALE_100 => {
                set_font_scale(1.00);
            }
            IDM_FONT_SCALE_115 => {
                set_font_scale(1.15);
            }
            IDM_FONT_SCALE_130 => {
                set_font_scale(1.30);
            }
            IDM_POLL_30S => {
                set_poll_interval(30_000);
            }
            IDM_POLL_1M => {
                set_poll_interval(60_000);
            }
            IDM_POLL_5M => {
                set_poll_interval(300_000);
            }
            IDM_POLL_15M => {
                set_poll_interval(900_000);
            }
            IDM_POLL_30M => {
                set_poll_interval(1_800_000);
            }
            IDM_POLL_60M => {
                set_poll_interval(3_600_000);
            }
            IDM_REFRESH => {
                if let Some(app_arc) = &APP_INSTANCE {
                    let app = app_arc.lock().unwrap();
                    if let Some(panel) = &app.panel {
                        panel.set_refresh_status("正在刷新设备…");
                    }
                    let result = app.manager.refresh_with_result();
                    let snaps = app.manager.get_snapshots();
                    let ball_on = app.config.enable_floating_ball;
                    if let Some(fb) = &app.floating_ball {
                        if ball_on { fb.update_devices(&snaps); }
                    }
                    if let Some(panel) = &app.panel {
                        panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
                        panel.set_refresh_status(&format!("刷新完成 · {} 台设备 · 新增 {} · 变化 {}", result.total, result.added, result.changed));
                    }
                }
            }
            IDM_CHECK_UPDATE => {
                let host = hwnd as isize;
                update::spawn_check(move || {
                    PostMessageW(host as HWND, WM_UPDATE_STATUS, 0, 0);
                });
                if let Some(app_arc) = &APP_INSTANCE {
                    if let Some(panel) = &app_arc.lock().unwrap().panel {
                        panel.invalidate();
                    }
                }
            }
            IDM_APPLY_UPDATE => {
                let host = hwnd as isize;
                update::spawn_apply(move || {
                    PostMessageW(host as HWND, WM_UPDATE_STATUS, 0, 0);
                });
                if let Some(app_arc) = &APP_INSTANCE {
                    if let Some(panel) = &app_arc.lock().unwrap().panel {
                        panel.invalidate();
                    }
                }
            }
            IDM_TOGGLE_AUTOSTART => {
                toggle_autostart();
            }
            IDM_EXIT => {
                PostQuitMessage(0);
            }
            _ => {}
        }
        0
}

/// `WM_DEVICE_RENAME` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_device_rename(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let ptr = wparam as *mut (String, String);
        if !ptr.is_null() {
            let (dev_id, new_name) = *Box::from_raw(ptr);
            if let Some(app_arc) = &APP_INSTANCE {
                let mut app = app_arc.lock().unwrap();
                app.config.set_alias(&dev_id, &new_name);
                let _ = app.config.save();
                app.manager.set_aliases(app.config.device_aliases.clone());

                // Refresh snapshots and UI immediately
                let snaps = app.manager.get_snapshots();
                let ball_on = app.config.enable_floating_ball;

                if let Some(fb) = &app.floating_ball {
                    if ball_on {
                        fb.update_devices(&snaps);
                    }
                }
                if let Some(panel) = &app.panel {
                    panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
                }
            }
        }
        0
}

/// `WM_LINKDECK_ACTIVATE` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_activate(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(app_arc) = &APP_INSTANCE {
            let app = app_arc.lock().unwrap();
            let snaps = app.manager.get_snapshots();
            let ball_on = app.config.enable_floating_ball;
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
                panel.show();
            }
        }
        0
}

/// `WM_UPDATE_STATUS` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_update_status(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(app_arc) = &APP_INSTANCE {
            if let Some(panel) = &app_arc.lock().unwrap().panel {
                panel.invalidate();
            }
        }
        0
}

/// `WM_DESTROY` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_destroy(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        PostQuitMessage(0);
        0
}


pub(crate) fn toggle_autostart() {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let next = {
                let app = app_arc.lock().unwrap();
                !app.config.auto_start
            };
            match autostart::set_enabled(next) {
                Ok(()) => {
                    let mut app = app_arc.lock().unwrap();
                    app.config.auto_start = next;
                    let _ = app.config.save();
                    if let Some(state_arc) = &crate::ui::panel::PANEL_STATE {
                        state_arc.lock().unwrap().auto_start = next;
                    }
                    if let Some(panel) = &app.panel {
                        panel.invalidate();
                    }
                }
                Err(err) => eprintln!("切换开机自启失败: {err}"),
            }
        }
    }
}

pub(crate) fn toggle_floating_ball() {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.enable_floating_ball = !app.config.enable_floating_ball;
            let enabled = app.config.enable_floating_ball;
            let _ = app.config.save();

            if let Some(fb) = &app.floating_ball {
                fb.set_enabled(enabled);
            }

            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(enabled));
            }
        }
    }
}

pub(crate) fn set_poll_interval(ms: u64) {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.poll_interval_ms = ms;
            let _ = app.config.save();
            app.manager.set_poll_interval_ms(ms);
            let ball_on = app.config.enable_floating_ball;
            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
            }
        }
    }
}

pub(crate) fn set_font_scale(scale: f32) {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.font_scale = scale;
            let ball_on = app.config.enable_floating_ball;

            if let Some(fb) = &app.floating_ball {
                fb.set_font_scale(scale);
            }

            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
            }
        }
    }
}

