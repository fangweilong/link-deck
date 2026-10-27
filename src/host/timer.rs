//! 宿主定时器：轮询设备、刷新面板与悬浮球、低电量提示。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::*;
use crate::ui::panel::LivePrefs;
use super::alerts::*;

/// `WM_TIMER` 的处理体，参数与宿主窗口过程一致。
pub(crate) unsafe fn on_timer(_hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
        if wparam == 1 {
            if let Some(app_arc) = &APP_INSTANCE {
                let mut app = app_arc.lock().unwrap();
                let snaps = app.manager.get_snapshots();
                let ball_on = app.config.enable_floating_ball;
                let toast_on = app.config.enable_battery_toast;
                let t_info = app.config.toast_threshold_info;
                let t_warn = app.config.toast_threshold_warning;
                let t_crit = app.config.toast_threshold_critical;
                let t_second = app.config.toast_threshold_second;
                let sound_on = app.config.enable_low_battery_sound;
                let sound = app.config.alert_sound;

                // Multi-threshold battery toast alerts
                if toast_on {
                    check_battery_toast_alerts(&snaps, t_info, t_warn, t_crit, t_second, sound_on, sound);
                }

                // Update floating ball if enabled
                if let Some(fb) = &app.floating_ball {
                    if ball_on {
                        fb.update_devices(&snaps);
                    }
                }

                // Update management panel
                if let Some(panel) = &app.panel {
                    panel.update_devices(&snaps, LivePrefs::from_config(&app.config).with_ball(ball_on));
                }
                // Format tray tooltip
                let mut tip_lines = Vec::new();
                tip_lines.push("LinkDeck 外设电量管理".to_string());
                tip_lines.push("---------------------".to_string());

                for dev in &snaps {
                    let pct_text = if let Some(p) = dev.battery.percentage {
                        if !dev.battery.online {
                            format!("{}% [待机]", p)
                        } else {
                            format!("{}%", p)
                        }
                    } else if dev.battery.online {
                        "电量不可读".to_string()
                    } else {
                        "待机".to_string()
                    };
                    let chg_mark = dev.battery.charging_source.tray_mark(dev.battery.charging);
                    let icon = match dev.kind {
                        devices::DeviceKind::Keyboard => "⌨️",
                        devices::DeviceKind::Mouse => "🖱️",
                        devices::DeviceKind::Headset => "🎧",
                        _ => "📱",
                    };
                    tip_lines.push(format!("{} {}: {}{}", icon, dev.name, pct_text, chg_mark));
                }

                if snaps.is_empty() {
                    tip_lines.push("未检测到受支持设备".to_string());
                }

                let upd = update::snapshot();
                match upd.latest.as_deref() {
                    Some(latest) if update::is_newer(latest, update::current_version()) => {
                        tip_lines.push(format!("⬆ 新版本 v{} 可用", latest));
                    }
                    _ => {
                        tip_lines.push(format!("版本 v{}", update::current_version()));
                    }
                }

                let full_tip = tip_lines.join("\n");
                if let Some(tray) = &mut app.tray {
                    tray.update_tooltip(&full_tip);
                }
            }
        }
        0
}

