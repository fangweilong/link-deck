//! ==============================================================================
//! 厂商模块: 华硕 ROG (玩家国度) / TUF Gaming
//! 协议类型: Armoury Crate (奥创) 私有 2.4G 无线 / USB 有线专有协议
//! 通信机制: 64字节 Feature Report (鼠标 0x1207 / 键盘 0x1201)，支持 Omni 统一接收器
//! 代表设备: 龙鳞 Ace (Harpe Ace)、月刃 (Keris)、战刃 (Gladius III)、夜魔 (Azoth)、魔导士 (Falchion) 等
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const ASUS_VID: u16 = 0x0B05;

pub struct AsusRogDriver;

impl AsusRogDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let asus_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| d.vendor_id == ASUS_VID)
            .collect();

        for dev_info in asus_devs {
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

        // Report IDs: 0x00 (standard direct endpoint), 0x03 (Omni Receiver shared endpoint)
        let report_ids = [0x00u8, 0x03u8];

        // 1. Try Mouse Battery Query: [report_id, 0x12, 0x07]
        for &rid in &report_ids {
            let mut req = [0u8; 64];
            req[0] = rid;
            req[1] = 0x12;
            req[2] = 0x07;

            if dev.write_report(&req).is_ok() {
                let mut resp = [0u8; 64];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    // Check signature: resp[1] == 0x12 && resp[2] == 0x07 (or shifted by 1 for Omni)
                    let (matched, pct, charging) = if resp[1] == 0x12 && resp[2] == 0x07 {
                        (true, resp[5], resp[10] > 0)
                    } else if resp[2] == 0x12 && resp[3] == 0x07 {
                        (true, resp[6], resp[11] > 0)
                    } else {
                        (false, 0, false)
                    };

                    if matched && pct > 0 && pct <= 100 {
                        let hw_name = dev.get_product_string();
                        let name = lookup_asus_rog_name(dev_info.product_id, hw_name.as_deref());
                        return Some(DeviceSnapshot {
                            id: format!("asus_rog_{:04x}", dev_info.product_id),
                            name,
                            kind: DeviceKind::Mouse,
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

        // 2. Try Keyboard Battery Query: [report_id, 0x12, 0x01]
        for &rid in &report_ids {
            let mut req = [0u8; 64];
            req[0] = rid;
            req[1] = 0x12;
            req[2] = 0x01;

            if dev.write_report(&req).is_ok() {
                let mut resp = [0u8; 64];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    let (matched, pct, charging) = if resp[1] == 0x12 && resp[2] == 0x01 {
                        (true, resp[6], resp[9] == 1)
                    } else if resp[2] == 0x12 && resp[3] == 0x01 {
                        (true, resp[7], resp[10] == 1)
                    } else {
                        (false, 0, false)
                    };

                    if matched && pct > 0 && pct <= 100 {
                        let hw_name = dev.get_product_string();
                        let name = lookup_asus_rog_name(dev_info.product_id, hw_name.as_deref());
                        return Some(DeviceSnapshot {
                            id: format!("asus_rog_{:04x}", dev_info.product_id),
                            name,
                            kind: DeviceKind::Keyboard,
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

pub fn lookup_asus_rog_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("ASUS")
            && !trimmed.eq_ignore_ascii_case("ROG")
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return if trimmed.to_lowercase().starts_with("rog") || trimmed.to_lowercase().starts_with("asus") {
                trimmed.to_string()
            } else {
                format!("ROG {}", trimmed)
            };
        }
    }

    match pid {
        0x1A19 | 0x1A1A => "ROG Harpe Ace Aim Lab Edition 无线鼠标".to_string(),
        0x1A38 | 0x1A39 => "ROG Harpe Ace Mini 无线鼠标".to_string(),
        0x1A73 | 0x1A74 => "ROG Harpe II Ace 无线鼠标".to_string(),
        0x1994 | 0x1995 => "ROG Keris Wireless 无线鼠标".to_string(),
        0x1A1D | 0x1A1E => "ROG Keris Wireless AimPoint 无线鼠标".to_string(),
        0x1A7E | 0x1A7F => "ROG Keris II Ace 无线鼠标".to_string(),
        0x1958 | 0x1959 => "ROG Gladius III Wireless 无线鼠标".to_string(),
        0x1A17 | 0x1A18 => "ROG Gladius III Wireless AimPoint".to_string(),
        0x195A | 0x195B => "ROG Spatha X 双模无线鼠标".to_string(),
        0x193B | 0x193C => "ROG Chakram 无线游戏鼠标".to_string(),
        0x1A1B | 0x1A1C => "ROG Chakram X 无线游戏鼠标".to_string(),
        0x1A25 | 0x1A26 => "ROG Azoth (夜魔) 75% 无线机械键盘".to_string(),
        0x1A92 | 0x1A93 => "ROG Azoth Extreme 旗舰无线键盘".to_string(),
        0x195D | 0x195E => "ROG Falchion 65% 无线机械键盘".to_string(),
        0x1A27 | 0x1A28 => "ROG Falchion Ace HFX 磁轴键盘".to_string(),
        0x193D | 0x193E => "ROG Claymore II 无线机械键盘".to_string(),
        0x1A15 | 0x1A16 => "ROG Strix Scope II 96 Wireless".to_string(),
        _ => format!("ASUS ROG 无线外设 (0x{:04X})", pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_asus_rog_name() {
        assert_eq!(
            lookup_asus_rog_name(0x1A19, None),
            "ROG Harpe Ace Aim Lab Edition 无线鼠标"
        );
        assert_eq!(
            lookup_asus_rog_name(0x1A25, None),
            "ROG Azoth (夜魔) 75% 无线机械键盘"
        );
        assert_eq!(
            lookup_asus_rog_name(0x195D, None),
            "ROG Falchion 65% 无线机械键盘"
        );
        assert_eq!(
            lookup_asus_rog_name(0x8888, Some("Gladius II")),
            "ROG Gladius II"
        );
    }
}
