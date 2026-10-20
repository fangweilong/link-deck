//! ==============================================================================
//! 厂商模块: 雷蛇 (Razer)
//! 协议类型: Razer HyperSpeed 2.4G 无线 / USB 有线专有协议
//! 通信机制: OpenRazer 90字节 Class 0x07 标准双向报文 (CMD 0x80 电量 / CMD 0x84 充电)
//! 代表设备: 毒蝰 (Viper) 系列、炼狱蝰蛇 (DeathAdder) 系列、巴塞利斯蛇 (Basilisk) 系列、眼镜蛇 (Cobra Pro) 等
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const RAZER_VID: u16 = 0x1532;

pub struct RazerDriver;

impl RazerDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let razer_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                d.vendor_id == RAZER_VID
                    && (d.output_len >= 90 || d.feature_len >= 90 || (d.usage_page == 0x0001 && d.usage == 0x0002))
            })
            .collect();

        for dev_info in razer_devs {
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

        // Transaction IDs: 0x1F (newer mice), 0x3F (older wireless dongles), 0xFF (universal fallback)
        let tx_ids = [0x1Fu8, 0x3F, 0xFF];

        for &tx_id in &tx_ids {
            // 1. Query Battery Level: Class 0x07, Command 0x80
            let mut req = [0u8; 91]; // Report ID 0x00 + 90 bytes payload
            req[0] = 0x00; // Report ID
            req[1] = 0x00; // Status
            req[2] = tx_id; // Transaction ID
            req[3] = 0x00;
            req[4] = 0x00;
            req[5] = 0x00;
            req[6] = 0x02; // Data size
            req[7] = 0x07; // Class: Battery
            req[8] = 0x80; // Command: Get Battery Level
            req[9] = 0x00;

            let mut crc = 0u8;
            for b in &req[2..89] {
                crc ^= *b;
            }
            req[89] = crc;

            let _ = dev.write_report(&req);

            let mut resp = [0u8; 91];
            if dev.read_report(&mut resp, timeout).is_ok() {
                if (resp[7] == 0x07 && resp[8] == 0x80) || (resp[6] == 0x07 && resp[7] == 0x80) {
                    let raw_lvl = if resp[7] == 0x07 { resp[10] } else { resp[9] };
                    let pct = ((raw_lvl as f32 / 255.0) * 100.0).round() as u8;

                    // 2. Query Charging Status: Class 0x07, Command 0x84
                    let mut req_chg = [0u8; 91];
                    req_chg[2] = tx_id;
                    req_chg[6] = 0x02;
                    req_chg[7] = 0x07;
                    req_chg[8] = 0x84;
                    let mut crc_chg = 0u8;
                    for b in &req_chg[2..89] {
                        crc_chg ^= *b;
                    }
                    req_chg[89] = crc_chg;

                    let _ = dev.write_report(&req_chg);
                    let mut resp_chg = [0u8; 91];
                    let charging = if dev.read_report(&mut resp_chg, timeout).is_ok() {
                        if (resp_chg[7] == 0x07 && resp_chg[8] == 0x84) || (resp_chg[6] == 0x07 && resp_chg[7] == 0x84) {
                            let chg_val = if resp_chg[7] == 0x07 { resp_chg[10] } else { resp_chg[9] };
                            chg_val == 1
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    let hw_name = dev.get_product_string();
                    let name = lookup_razer_name(dev_info.product_id, hw_name.as_deref());

                    return Some(DeviceSnapshot {
                        id: format!("razer_{:04x}", dev_info.product_id),
                        name,
                        kind: DeviceKind::Mouse,
                        vendor_id: dev_info.vendor_id,
                        product_id: dev_info.product_id,
                        connection: ConnectionMode::Wireless24G,
                        battery: BatteryState {
                            percentage: if pct > 0 && pct <= 100 { Some(pct) } else { None },
                            voltage_mv: None,
                            charging,
                            charging_source: if charging { ChargingSource::Charging } else { ChargingSource::None },
                            online: true,
                            last_updated: SystemTime::now(),
                        },
                        path: dev_info.path.clone(),
                    });
                }
            }
        }

        None
    }
}

pub fn lookup_razer_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
            && !trimmed.eq_ignore_ascii_case("Razer Device")
        {
            return format!("Razer {}", trimmed);
        }
    }

    match pid {
        0x00B6 => "Razer DeathAdder V3 Pro (炼狱蝰蛇 V3 专业版)".to_string(),
        0x00B8 => "Razer DeathAdder V3 HyperSpeed (炼狱蝰蛇 V3 极速版)".to_string(),
        0x007C => "Razer DeathAdder V2 Pro (炼狱蝰蛇 V2 专业版)".to_string(),
        0x00A5 => "Razer Viper V2 Pro (毒蝰 V2 专业版)".to_string(),
        0x00C2 => "Razer Viper V3 Pro (毒蝰 V3 专业版)".to_string(),
        0x007A | 0x007B => "Razer Viper Ultimate (毒蝰终极版)".to_string(),
        0x00AA => "Razer Basilisk V3 Pro (巴塞利斯蛇 V3 专业版)".to_string(),
        0x0088 => "Razer Basilisk Ultimate (巴塞利斯蛇终极版)".to_string(),
        0x00AF => "Razer Cobra Pro (眼镜蛇专业版)".to_string(),
        0x008F => "Razer Naga Pro (那伽梵蛇专业版)".to_string(),
        0x00B3 => "Razer Naga V2 HyperSpeed (那伽梵蛇 V2 极速版)".to_string(),
        0x0094 => "Razer Orochi V2 (八岐大蛇 V2)".to_string(),
        _ => format!("Razer 无线外设 (0x{:04X})", pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_razer_name() {
        let v2 = lookup_razer_name(0x00A5, None);
        assert!(v2.contains("毒蝰 V2"));

        let da3 = lookup_razer_name(0x00B6, None);
        assert!(da3.contains("炼狱蝰蛇 V3"));

        let hw = lookup_razer_name(0x00A5, Some("Viper V2 Pro"));
        assert_eq!(hw, "Razer Viper V2 Pro");
    }
}
