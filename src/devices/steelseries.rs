//! ==============================================================================
//! 厂商模块: 赛睿 (SteelSeries)
//! 协议类型: SteelSeries Engine 2.4G 无线 / USB 有线专有协议
//! 通信机制: 报文指令 0xD2/0x92 (Aerox/Prime 鼠标电量)、0xAA (Rival 650)、0x0618 (Arctis 耳机)
//! 代表设备: Aerox 3/5/9 Wireless、Prime Wireless、Rival 650、Apex Pro 无线键盘、Arctis 7/9/Pro/Nova 耳机
//! 实机验证: 未实测 — 按公开报文资料实现，待对应硬件验证
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const STEELSERIES_VID: u16 = 0x1038;

pub struct SteelSeriesDriver;

impl SteelSeriesDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let ss_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| d.vendor_id == STEELSERIES_VID)
            .collect();

        for dev_info in ss_devs {
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

        // 1. Try modern wireless mouse protocol (Aerox 3/5/9, Prime Wireless, etc.)
        // Command 0xD2 for wireless 2.4G (0x92 | 0x40), or 0x92 for wired
        for &cmd in &[0xD2u8, 0x92u8] {
            let mut req = [0u8; 64];
            req[0] = cmd;
            if dev.write_report(&req).is_ok() {
                let mut resp = [0u8; 64];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    // Response: resp[0] echo or status, resp[1] contains level + charging bit
                    let byte1 = resp[1];
                    let charging = (byte1 & 0x80) != 0;
                    let raw_lvl = byte1 & 0x7F;

                    if raw_lvl > 0 && raw_lvl <= 100 {
                        let pct = if raw_lvl <= 21 {
                            // Rivalcfg step mapping: ((val - 1) * 5)
                            ((raw_lvl.saturating_sub(1)) * 5).min(100)
                        } else {
                            raw_lvl.min(100)
                        };

                        let hw_name = dev.get_product_string();
                        let name = lookup_steelseries_name(dev_info.product_id, hw_name.as_deref());
                        let kind = classify_steelseries(dev_info.product_id, &name);

                        return Some(DeviceSnapshot {
                            id: format!("steelseries_{:04x}", dev_info.product_id),
                            name,
                            kind,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: if cmd == 0xD2 {
                                ConnectionMode::Wireless24G
                            } else {
                                ConnectionMode::Wired
                            },
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

        // 2. Try Rival 650 Wireless command: [0xAA, 0x01]
        {
            let mut req = [0u8; 64];
            req[0] = 0xAA;
            req[1] = 0x01;
            if dev.write_report(&req).is_ok() {
                let mut resp = [0u8; 64];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    let lvl = resp[0];
                    let charging = resp[2] != 0;
                    if lvl > 0 && lvl <= 100 {
                        let hw_name = dev.get_product_string();
                        let name = lookup_steelseries_name(dev_info.product_id, hw_name.as_deref());
                        return Some(DeviceSnapshot {
                            id: format!("steelseries_{:04x}", dev_info.product_id),
                            name,
                            kind: DeviceKind::Mouse,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: ConnectionMode::Wireless24G,
                            battery: BatteryState {
                                percentage: Some(lvl),
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

        // 3. Try Arctis Headset series: [0x06, 0x18]
        {
            let mut req = [0u8; 32];
            req[0] = 0x06;
            req[1] = 0x18;
            if dev.write_report(&req).is_ok() {
                let mut resp = [0u8; 32];
                if dev.read_report(&mut resp, timeout).is_ok() {
                    let lvl = resp[2];
                    if lvl > 0 && lvl <= 100 {
                        let hw_name = dev.get_product_string();
                        let name = lookup_steelseries_name(dev_info.product_id, hw_name.as_deref());
                        return Some(DeviceSnapshot {
                            id: format!("steelseries_{:04x}", dev_info.product_id),
                            name,
                            kind: DeviceKind::Headset,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: ConnectionMode::Wireless24G,
                            battery: BatteryState {
                                percentage: Some(lvl),
                                voltage_mv: None,
                                charging: false,
                                charging_source: ChargingSource::None,
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

pub fn lookup_steelseries_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("SteelSeries")
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return if trimmed.to_lowercase().starts_with("steelseries") {
                trimmed.to_string()
            } else {
                format!("SteelSeries {}", trimmed)
            };
        }
    }

    match pid {
        0x1838 => "SteelSeries Aerox 3 Wireless (2.4G)".to_string(),
        0x183A => "SteelSeries Aerox 3 Wireless (有线)".to_string(),
        0x1852 => "SteelSeries Aerox 5 Wireless (2.4G)".to_string(),
        0x1854 => "SteelSeries Aerox 5 Wireless (有线)".to_string(),
        0x185A => "SteelSeries Aerox 9 Wireless (2.4G)".to_string(),
        0x185C => "SteelSeries Aerox 9 Wireless (有线)".to_string(),
        0x1836 => "SteelSeries Prime Wireless (2.4G)".to_string(),
        0x183E => "SteelSeries Prime Wireless (有线)".to_string(),
        0x1726 => "SteelSeries Rival 650 Wireless (2.4G)".to_string(),
        0x172B => "SteelSeries Rival 650 Wireless (有线)".to_string(),
        0x1830 => "SteelSeries Rival 3 Wireless (2.4G)".to_string(),
        0x12AD => "SteelSeries Arctis 7 无线游戏耳机".to_string(),
        0x12B3 => "SteelSeries Arctis 1 Wireless 游戏耳机".to_string(),
        0x12C2 => "SteelSeries Arctis 9 无线游戏耳机".to_string(),
        0x1252 => "SteelSeries Arctis Pro Wireless 游戏耳机".to_string(),
        0x12E0 => "SteelSeries Arctis Nova 7 Wireless".to_string(),
        0x12E2 => "SteelSeries Arctis Nova Pro Wireless".to_string(),
        0x161C => "SteelSeries Apex Pro Mini Wireless 机械键盘".to_string(),
        0x1626 => "SteelSeries Apex Pro TKL Wireless 机械键盘".to_string(),
        _ => format!("SteelSeries 无线外设 (0x{:04X})", pid),
    }
}

pub fn classify_steelseries(pid: u16, name: &str) -> DeviceKind {
    let lower = name.to_lowercase();
    if lower.contains("keyboard") || lower.contains("apex") || (pid >= 0x1600 && pid <= 0x16FF) {
        DeviceKind::Keyboard
    } else if lower.contains("headset") || lower.contains("arctis") || lower.contains("nova") || (pid >= 0x1200 && pid <= 0x12FF) {
        DeviceKind::Headset
    } else {
        DeviceKind::Mouse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_steelseries_name() {
        assert_eq!(
            lookup_steelseries_name(0x1838, None),
            "SteelSeries Aerox 3 Wireless (2.4G)"
        );
        assert_eq!(
            lookup_steelseries_name(0x12AD, None),
            "SteelSeries Arctis 7 无线游戏耳机"
        );
        assert_eq!(
            lookup_steelseries_name(0x9999, Some("Custom Mouse")),
            "SteelSeries Custom Mouse"
        );
    }

    #[test]
    fn test_classify_steelseries() {
        assert_eq!(
            classify_steelseries(0x1838, "SteelSeries Aerox 3"),
            DeviceKind::Mouse
        );
        assert_eq!(
            classify_steelseries(0x1626, "SteelSeries Apex Pro"),
            DeviceKind::Keyboard
        );
        assert_eq!(
            classify_steelseries(0x12AD, "SteelSeries Arctis 7"),
            DeviceKind::Headset
        );
    }
}
