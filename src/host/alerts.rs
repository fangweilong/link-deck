//! 低电量弹窗与提示音的分档触发逻辑。

#![allow(unsafe_op_in_unsafe_fn, static_mut_refs)]

use crate::*;

#[derive(Default, Clone)]
pub(crate) struct AlertTracker {
    alerted_info: bool,
    alerted_warn: bool,
    alerted_crit: bool,
    /// 二次危险提醒：危险阈值已经不报，再跌到用户自定义的更低档时补一次
    alerted_second: bool,
}

pub(crate) static mut ALERT_TRACKERS: Option<std::collections::HashMap<String, AlertTracker>> = None;

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_battery_toast_alerts(
    snaps: &[devices::types::DeviceSnapshot],
    t_info: u8,
    t_warn: u8,
    t_crit: u8,
    t_second: u8,
    sound_on: bool,
    sound: crate::config::AlertSound,
) {
    unsafe {
        if ALERT_TRACKERS.is_none() {
            ALERT_TRACKERS = Some(std::collections::HashMap::new());
        }
        let trackers = ALERT_TRACKERS.as_mut().unwrap();

        for dev in snaps {
            if !dev.battery.online {
                continue;
            }
            let pct = match dev.battery.percentage {
                Some(p) => p,
                None => continue,
            };
            let charging = dev.battery.charging;

            let tracker = trackers.entry(dev.id.clone()).or_default();

            if charging || pct > t_info {
                tracker.alerted_info = false;
                tracker.alerted_warn = false;
                tracker.alerted_crit = false;
                tracker.alerted_second = false;
            } else {
                // 二次提醒在最前面判定：它比危险阈值更严格，命中时说明电池又掉了一档
                if t_second > 0 && pct <= t_second {
                    if !tracker.alerted_second {
                        tracker.alerted_second = true;
                        tracker.alerted_crit = true;
                        tracker.alerted_warn = true;
                        tracker.alerted_info = true;
                        crate::ui::toast::ToastNotification::show(
                            &dev.name,
                            &format!("电量已降到 {}%，这是最后一次提醒，请立即充电。", pct),
                            crate::ui::toast::ToastLevel::Critical,
                            pct,
                        );
                        if sound_on {
                            crate::sound::play_alert(sound);
                        }
                    }
                } else if pct <= t_crit {
                    if !tracker.alerted_crit {
                        tracker.alerted_crit = true;
                        tracker.alerted_warn = true;
                        tracker.alerted_info = true;
                        crate::ui::toast::ToastNotification::show(
                            &dev.name,
                            &format!("当前电量仅剩 {}%，已严重不足！请及时充电或更换电池。", pct),
                            crate::ui::toast::ToastLevel::Critical,
                            pct,
                        );
                        // 弹窗同时播放提示音（与弹窗共用同一次触发，绝不重复）
                        if sound_on {
                            crate::sound::play_alert(sound);
                        }
                    }
                } else if pct <= t_warn {
                    if !tracker.alerted_warn {
                        tracker.alerted_warn = true;
                        tracker.alerted_info = true;
                        crate::ui::toast::ToastNotification::show(
                            &dev.name,
                            &format!("当前电量降至 {}%，电量偏低，建议适时充电。", pct),
                            crate::ui::toast::ToastLevel::Warning,
                            pct,
                        );
                    }
                } else if pct <= t_info {
                    if !tracker.alerted_info {
                        tracker.alerted_info = true;
                        crate::ui::toast::ToastNotification::show(
                            &dev.name,
                            &format!("当前电量已消耗过半，当前剩余 {}%。", pct),
                            crate::ui::toast::ToastLevel::Info,
                            pct,
                        );
                    }
                }
            }
        }
    }
}

