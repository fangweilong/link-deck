//! ==============================================================================
//! 厂商模块: 美商海盗船 (Corsair)
//! 协议类型: Corsair Slipstream 2.4G / Bragi 统一协议与 NXP Legacy 协议
//! 通信机制: Bragi Magic 0x08 (Prop 0x0F 千分比电量、Prop 0x10 充放电状态)，NXP 0x0E 报文
//! 代表设备: 暗影 Dark Core RGB Pro/SE、卡特 Katar Pro/Elite、铁爪 Ironclaw、圣剑 Sabre、K70 PRO Mini、K100 AIR、HS70/80 耳机等
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const CORSAIR_VID: u16 = 0x1B1C;

pub struct CorsairDriver;

impl CorsairDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let corsair_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| d.vendor_id == CORSAIR_VID)
            .collect();

        for dev_info in corsair_devs {
            if let Some(snapshot) = Self::query_device(dev_info) {
                if !results.iter().any(|d: &DeviceSnapshot| d.id == snapshot.id) {
                    results.push(snapshot);
                }
            }
        }

        results
    }

    fn query_device(dev_info: &HidDeviceInfo) -> Option<DeviceSnapshot> {
        let dev = HidDevice::open(dev_info.clone(), true).ok()?;
        let timeout = Duration::from_millis(60);

        // 1. Corsair Bragi / Slipstream Protocol
        // BRAGI_MAGIC (0x08), BRAGI_GET (0x02), BRAGI_BATTERY_LEVEL (0x0F), 0x00
        let mut req_level = [0u8; 64];
        req_level[0] = 0x08;
        req_level[1] = 0x02;
        req_level[2] = 0x0F;
        req_level[3] = 0x00;

        if dev.write_report(&req_level).is_ok() {
            let mut resp_level = [0u8; 64];
            if dev.read_report(&mut resp_level, timeout).is_ok() {
                // Bragi response format: [0x08, 0x02, status_err, val_low, val_mid, val_high, ...]
                if resp_level[0] == 0x08 && resp_level[1] == 0x02 && resp_level[2] == 0x00 {
                    let permille = ((resp_level[5] as u32) << 16)
                        | ((resp_level[4] as u32) << 8)
                        | (resp_level[3] as u32);
                    let pct = if permille > 100 {
                        ((permille / 10).min(100)) as u8
                    } else {
                        permille as u8
                    };

                    if pct > 0 && pct <= 100 {
                        // Query Charging Status: BRAGI_BATTERY_STATUS (0x10)
                        let mut req_stat = [0u8; 64];
                        req_stat[0] = 0x08;
                        req_stat[1] = 0x02;
                        req_stat[2] = 0x10;
                        req_stat[3] = 0x00;

                        let charging = if dev.write_report(&req_stat).is_ok() {
                            let mut resp_stat = [0u8; 64];
                            if dev.read_report(&mut resp_stat, timeout).is_ok()
                                && resp_stat[0] == 0x08
                                && resp_stat[1] == 0x02
                                && resp_stat[2] == 0x00
                            {
                                // 1 = Charging, 3 = Fully Charged
                                resp_stat[3] == 1 || resp_stat[3] == 3
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        let hw_name = dev.get_product_string();
                        let name = lookup_corsair_name(dev_info.product_id, hw_name.as_deref());
                        let kind = classify_corsair(dev_info.product_id, &name);

                        return Some(DeviceSnapshot {
                            id: format!("corsair_{:04x}", dev_info.product_id),
                            name,
                            kind,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: ConnectionMode::Wireless24G,
                            battery: BatteryState {
                                percentage: Some(pct),
                                voltage_mv: None,
                                charging,
                                charging_source: if charging {
                                    ChargingSource::Charging
                                } else {
                                    ChargingSource::None
                                },
                                online: true,
                                last_updated: SystemTime::now(),
                            },
                            path: dev_info.path.clone(),
                        });
                    }
                }
            }
        }

        // 2. Legacy NXP / Slipstream Dongle Fallback
        // Query command: [0x00, 0x0E, 0x01, ...]
        {
            let mut req_legacy = [0u8; 64];
            req_legacy[1] = 0x0E;
            req_legacy[2] = 0x01;

            if dev.write_report(&req_legacy).is_ok() {
                let mut resp = [0u8; 64];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    if resp[1] == 0x0E && resp[2] > 0 && resp[2] <= 100 {
                        let pct = resp[2];
                        let charging = resp[3] == 1;

                        let hw_name = dev.get_product_string();
                        let name = lookup_corsair_name(dev_info.product_id, hw_name.as_deref());
                        let kind = classify_corsair(dev_info.product_id, &name);

                        return Some(DeviceSnapshot {
                            id: format!("corsair_{:04x}", dev_info.product_id),
                            name,
                            kind,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: ConnectionMode::Wireless24G,
                            battery: BatteryState {
                                percentage: Some(pct),
                                voltage_mv: None,
                                charging,
                                charging_source: if charging {
                                    ChargingSource::Charging
                                } else {
                                    ChargingSource::None
                                },
                                online: true,
                                last_updated: SystemTime::now(),
                            },
                            path: dev_info.path.clone(),
                        });
                    }
                }
            }
        }

        None
    }
}

pub fn lookup_corsair_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("Corsair")
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return if trimmed.to_lowercase().starts_with("corsair") {
                trimmed.to_string()
            } else {
                format!("Corsair {}", trimmed)
            };
        }
    }

    match pid {
        0x1B7F | 0x1B80 => "Corsair Dark Core RGB Pro 无线鼠标".to_string(),
        0x1B81 | 0x1B82 => "Corsair Dark Core RGB Pro SE 无线鼠标".to_string(),
        0x1B90 | 0x1B91 => "Corsair Katar Pro Wireless 无线鼠标".to_string(),
        0x1BA4 | 0x1BA5 => "Corsair Katar Elite Wireless 游戏鼠标".to_string(),
        0x1B5E | 0x1B5F => "Corsair Ironclaw RGB Wireless 游戏鼠标".to_string(),
        0x1BA2 | 0x1BA3 => "Corsair Sabre RGB Pro Wireless 鼠标".to_string(),
        0x1BC4 | 0x1BC5 => "Corsair M65 RGB Ultra Wireless 游戏鼠标".to_string(),
        0x1B89 | 0x1B8A => "Corsair K70 PRO Mini Wireless 机械键盘".to_string(),
        0x1B9E | 0x1B9F => "Corsair K100 AIR Wireless 旗舰机械键盘".to_string(),
        0x1B6B | 0x1B6C => "Corsair K57 RGB Wireless 游戏键盘".to_string(),
        0x0A51 | 0x0A52 => "Corsair HS70 Wireless 游戏耳机".to_string(),
        0x0A6B | 0x0A6C => "Corsair HS80 RGB Wireless 游戏耳机".to_string(),
        0x0A55 | 0x0A56 => "Corsair Void RGB Elite Wireless 耳机".to_string(),
        _ => format!("Corsair 无线外设 (0x{:04X})", pid),
    }
}

pub fn classify_corsair(pid: u16, name: &str) -> DeviceKind {
    let lower = name.to_lowercase();
    if lower.contains("keyboard") || lower.contains("k70") || lower.contains("k100") || lower.contains("k57") {
        DeviceKind::Keyboard
    } else if lower.contains("headset") || lower.contains("hs70") || lower.contains("hs80") || lower.contains("void") {
        DeviceKind::Headset
    } else {
        DeviceKind::Mouse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_corsair_name() {
        assert_eq!(
            lookup_corsair_name(0x1B7F, None),
            "Corsair Dark Core RGB Pro 无线鼠标"
        );
        assert_eq!(
            lookup_corsair_name(0x1B89, None),
            "Corsair K70 PRO Mini Wireless 机械键盘"
        );
        assert_eq!(
            lookup_corsair_name(0x0A6B, None),
            "Corsair HS80 RGB Wireless 游戏耳机"
        );
        assert_eq!(
            lookup_corsair_name(0x8888, Some("Katar Pro")),
            "Corsair Katar Pro"
        );
    }

    #[test]
    fn test_classify_corsair() {
        assert_eq!(
            classify_corsair(0x1B7F, "Corsair Dark Core RGB Pro"),
            DeviceKind::Mouse
        );
        assert_eq!(
            classify_corsair(0x1B89, "Corsair K70 PRO Mini Wireless"),
            DeviceKind::Keyboard
        );
        assert_eq!(
            classify_corsair(0x0A6B, "Corsair HS80 Wireless"),
            DeviceKind::Headset
        );
    }
}
