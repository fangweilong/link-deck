//! 面板回传的持久化消息（WM_SAVE_* / WM_SET_*）：把界面上的选择落到配置文件。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::*;
use crate::ui::panel::LivePrefs;

/// `WM_SAVE_UPDATE_PREFS` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_update_prefs(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let auto = crate::ui::panel::PANEL_STATE
            .as_ref()
            .map(|sa| sa.lock().unwrap().auto_update)
            .unwrap_or(true);
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.check_updates = auto;
            let _ = app.config.save();
        }
        0
}

/// `WM_SAVE_AUTOSTART` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_autostart(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let desired = crate::ui::panel::PANEL_STATE
            .as_ref()
            .map(|sa| sa.lock().unwrap().auto_start)
            .unwrap_or(false);
        let applied = autostart::set_enabled(desired);
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            match applied {
                Ok(()) => {
                    app.config.auto_start = desired;
                    let _ = app.config.save();
                }
                Err(err) => {
                    // 注册表写入失败：回滚开关，避免配置与系统状态不一致
                    eprintln!("设置开机自启失败: {err}");
                    let revert = app.config.auto_start;
                    if let Some(state_arc) = &crate::ui::panel::PANEL_STATE {
                        state_arc.lock().unwrap().auto_start = revert;
                    }
                    if let Some(panel) = &app.panel {
                        panel.invalidate();
                    }
                }
            }
        }
        0
}

/// `WM_SET_BALL_OPACITY` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_set_ball_opacity(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        // 拖拽过程中只应用不落盘，避免逐像素写配置文件
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.floating_ball_opacity = (wparam as u8).clamp(40, 100);
            let alpha = app.config.ball_alpha();
            if let Some(fb) = &app.floating_ball {
                fb.set_opacity(alpha);
            }
        }
        0
}

/// `WM_SET_BALL_STYLE` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_set_ball_style(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.floating_ball_style = match wparam {
                1 => crate::config::FloatingBallStyle::Orb,
                2 => crate::config::FloatingBallStyle::Ring,
                3 => crate::config::FloatingBallStyle::Pill,
                4 => crate::config::FloatingBallStyle::Badge,
                _ => crate::config::FloatingBallStyle::Tab,
            };
            let _ = app.config.save();
            if let Some(fb) = &app.floating_ball {
                fb.set_style(app.config.floating_ball_style);
            }
        }
        0
}

/// `WM_SAVE_BALL_OPACITY` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_ball_opacity(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(app_arc) = &APP_INSTANCE {
            let app = app_arc.lock().unwrap();
            let _ = app.config.save();
        }
        0
}

/// `WM_SAVE_TOAST_SETTINGS` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_toast_settings(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(state_arc) = &crate::ui::panel::PANEL_STATE {
            let (enabled, info, warn, crit, second, gaming, sound, tone) = {
                let s = state_arc.lock().unwrap();
                (s.enable_toast, s.toast_info, s.toast_warning, s.toast_critical,
                 s.toast_second, s.toast_gaming_suppression, s.enable_low_battery_sound,
                 s.alert_sound)
            };
            if let Some(app_arc) = &APP_INSTANCE {
                let mut app = app_arc.lock().unwrap();
                app.config.enable_battery_toast = enabled;
                app.config.toast_threshold_info = info;
                app.config.toast_threshold_warning = warn;
                app.config.toast_threshold_critical = crit;
                app.config.toast_threshold_second = second;
                app.config.toast_gaming_suppression = gaming;
                app.config.enable_low_battery_sound = sound;
                app.config.alert_sound = tone;
                // 手改或连续点 − 都可能造出交叉，落盘前统一修正
                app.config.normalize_toast_thresholds();
                if !sound {
                    crate::sound::stop_alarm();
                }
                let _ = app.config.save();
            }
        }
        0
}

/// `WM_SAVE_CUSTOM_THEME` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_custom_theme(_hwnd: HWND, _msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let accent = (wparam as u32) & 0x00FF_FFFF;
        let light = lparam != 0;
        unsafe { crate::ui::theme::apply_custom(accent, light) };
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.ui_style = crate::config::UiStyle::Custom;
            app.config.custom_accent = accent;
            app.config.custom_light = light;
            let _ = app.config.save();
        }
        0
}

/// `WM_SAVE_PANEL_SIZE` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_panel_size(_hwnd: HWND, _msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let w = wparam as i32;
        let h = lparam as i32;
        if w >= 480 && h >= 400 {
            if let Some(app_arc) = &APP_INSTANCE {
                let mut app = app_arc.lock().unwrap();
                app.config.set_panel_size(w, h);
                let _ = app.config.save();
            }
        }
        0
}

/// `WM_SAVE_UI_DENSITY` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_ui_density(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let density = match wparam {
            0 => crate::config::UiDensity::Compact,
            2 => crate::config::UiDensity::Spacious,
            _ => crate::config::UiDensity::Standard,
        };
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.set_ui_density(density);
            let _ = app.config.save();

            let snaps = app.manager.get_snapshots();
            let ball_on = app.config.enable_floating_ball;
            if let Some(panel) = &app.panel {
                panel.update_devices(
                    &snaps,
                    LivePrefs::from_config(&app.config).with_ball(ball_on).with_density(density),
                );
            }
        }
        0
}

/// `WM_SAVE_DASHBOARD_PREFS` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_dashboard_prefs(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let filter = match (wparam & 0xFF) as u8 {
            1 => crate::config::DeviceFilter::LowBattery,
            2 => crate::config::DeviceFilter::Charging,
            3 => crate::config::DeviceFilter::Bluetooth,
            4 => crate::config::DeviceFilter::Wireless,
            _ => crate::config::DeviceFilter::All,
        };
        let sort = match ((wparam >> 8) & 0xFF) as u8 {
            1 => crate::config::DeviceSort::Name,
            2 => crate::config::DeviceSort::Battery,
            3 => crate::config::DeviceSort::Updated,
            _ => crate::config::DeviceSort::Priority,
        };
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.device_filter = filter;
            app.config.device_sort = sort;
            let _ = app.config.save();
        }
        0
}

/// `WM_SAVE_ONBOARDING` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_onboarding(_hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.show_onboarding = false;
            let _ = app.config.save();
        }
        0
}

/// `WM_SAVE_UI_STYLE` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_save_ui_style(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        let theme = match wparam {
            1 => crate::config::UiStyle::Graphite,
            2 => crate::config::UiStyle::Aurora,
            3 => crate::config::UiStyle::Jade,
            4 => crate::config::UiStyle::Snow,
            _ => crate::config::UiStyle::Ember,
        };
        unsafe { crate::ui::theme::apply(theme); }
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.ui_style = theme;
            let _ = app.config.save();
        }
        0
}

