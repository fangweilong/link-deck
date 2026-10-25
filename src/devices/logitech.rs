//! ==============================================================================
//! 厂商模块: 罗技 (Logitech / Logitech G)
//! 协议类型: 官方 HID++ 2.0 / 1.0 双协议 (Lightspeed 2.4G 无线 / 优联 / Bolt / USB 有线)
//! 通信机制: Feature 0x1004 (Unified Battery) 与 Feature 0x1000 (Battery Status) 探针
//! 代表设备: GPW 狗屁王 1/2/3 代、G502、G304、G903、G913/G915 键盘、MX Master 3/3S 等
//! 实机验证: 已实测 — MX Master 3S
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const LOGITECH_VID: u16 = 0x046D;
const HIDPP_SW_ID: u8 = 0x05;

pub struct LogitechDriver;

impl LogitechDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        // Search for Logitech HID++ communication endpoints
        // Unifying / Lightspeed receivers and wired devices expose vendor collections (UsagePage >= 0xFF00)
        // or collections supporting HID++ long reports (Report ID 0x11, 20 bytes) or short reports (0x10, 7 bytes)
        let logi_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                d.vendor_id == LOGITECH_VID
                    && (d.usage_page >= 0xFF00 || d.output_len >= 7 || d.feature_len >= 7)
            })
            .collect();

        for dev_info in logi_devs {
            // Check direct device (0xFF) and paired wireless slots (1..=6 for Unifying / Bolt / Lightspeed)
            let slots = [1u8, 2, 3, 4, 5, 6, 0xFF];
            for &dev_idx in &slots {
                if let Some(snapshot) = Self::query_device(dev_info, dev_idx) {
                    if !results.iter().any(|d: &DeviceSnapshot| d.id == snapshot.id) {
                        results.push(snapshot);
                    }
                }
            }
        }

        results
    }

    fn query_device(dev_info: &HidDeviceInfo, dev_idx: u8) -> Option<DeviceSnapshot> {
        let dev = HidDevice::open(dev_info.clone(), true).ok()?;
        let timeout = Duration::from_millis(60);

        // 1. First probe if slot/device is responsive and query Feature 0x1004 (Unified Battery)
        let feat_1004_idx = Self::get_feature_index(&dev, dev_idx, 0x1004, timeout);

        if let Some(idx) = feat_1004_idx {
            // Call Function 1: getBatteryInfo
            let req = [
                0x11,
                dev_idx,
                idx,
                (1 << 4) | HIDPP_SW_ID,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ];
            let _ = dev.write_report(&req);

            let mut resp = [0u8; 20];
            if dev.read_report(&mut resp, timeout).is_ok()
                && resp[0] == 0x11
                && resp[1] == dev_idx
                && resp[2] == idx
            {
                let pct = resp[4];
                let status = resp[6];
                let charging = status == 1 || status == 2 || status == 3;
                let name = Self::get_device_name(&dev, dev_idx, dev_info.product_id, timeout);

                return Some(DeviceSnapshot {
                    id: format!("logitech_{:04x}_{}", dev_info.product_id, dev_idx),
                    name: format!("{} (2.4G)", name),
                    kind: classify_logitech(dev_info.product_id, &name),
                    vendor_id: dev_info.vendor_id,
                    product_id: dev_info.product_id,
                    connection: if dev_idx == 0xFF { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
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

        // 2. Query Feature 0x1000 (Battery Status - legacy models without 0x1004)
        let feat_1000_idx = Self::get_feature_index(&dev, dev_idx, 0x1000, timeout);

        if let Some(idx) = feat_1000_idx {
            // Call Function 0: getBatteryLevelStatus
            let req = [
                0x11,
                dev_idx,
                idx,
                (0 << 4) | HIDPP_SW_ID,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ];
            let _ = dev.write_report(&req);

            let mut resp = [0u8; 20];
            if dev.read_report(&mut resp, timeout).is_ok()
                && resp[0] == 0x11
                && resp[1] == dev_idx
                && resp[2] == idx
            {
                let discharge_level = resp[4];
                let status = resp[6];
                let charging = status == 1 || status == 2 || status == 3;
                let name = Self::get_device_name(&dev, dev_idx, dev_info.product_id, timeout);

                return Some(DeviceSnapshot {
                    id: format!("logitech_{:04x}_{}", dev_info.product_id, dev_idx),
                    name: format!("{} (2.4G)", name),
                    kind: classify_logitech(dev_info.product_id, &name),
                    vendor_id: dev_info.vendor_id,
                    product_id: dev_info.product_id,
                    connection: if dev_idx == 0xFF { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
                    battery: BatteryState {
                        percentage: if discharge_level > 0 && discharge_level <= 100 { Some(discharge_level) } else { None },
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

        // 3. Query HID++ 1.0 Battery Status (Register 0x07 via Short report 0x10)
        let req_v1 = [0x10, dev_idx, 0x81, 0x07, 0, 0, 0];
        let _ = dev.write_report(&req_v1);

        let mut resp_v1 = [0u8; 20];
        if dev.read_report(&mut resp_v1, timeout).is_ok()
            && resp_v1[0] == 0x10
            && resp_v1[1] == dev_idx
            && resp_v1[2] == 0x81
            && resp_v1[3] == 0x07
        {
            let raw_pct = resp_v1[4];
            let charging = (resp_v1[5] & 0x20) != 0;
            let pct = if raw_pct <= 10 { raw_pct * 10 } else { raw_pct };
            let name = Self::get_device_name(&dev, dev_idx, dev_info.product_id, timeout);

            return Some(DeviceSnapshot {
                id: format!("logitech_{:04x}_{}", dev_info.product_id, dev_idx),
                name: format!("{} (2.4G)", name),
                kind: classify_logitech(dev_info.product_id, &name),
                vendor_id: dev_info.vendor_id,
                product_id: dev_info.product_id,
                connection: if dev_idx == 0xFF { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
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

        None
    }

    /// Query feature index from HID++ 2.0 Feature 0x0000 (Root)
    fn get_feature_index(dev: &HidDevice, dev_idx: u8, feature_id: u16, timeout: Duration) -> Option<u8> {
        let req = [
            0x11,
            dev_idx,
            0x00, // Feature 0x0000 (Root)
            (0 << 4) | HIDPP_SW_ID, // Function 0 (getFeature) | software_id
            (feature_id >> 8) as u8,
            (feature_id & 0xFF) as u8,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let _ = dev.write_report(&req);

        let mut resp = [0u8; 20];
        if dev.read_report(&mut resp, timeout).is_ok()
            && resp[0] == 0x11
            && resp[1] == dev_idx
            && resp[2] == 0x00
            && (resp[3] & 0x0F) == HIDPP_SW_ID
        {
            let feat_idx = resp[4];
            if feat_idx != 0 {
                return Some(feat_idx);
            }
        }

        None
    }

    /// Retrieve friendly device name via HID++ Feature 0x0005 (DeviceName)
    fn get_device_name(dev: &HidDevice, dev_idx: u8, receiver_pid: u16, timeout: Duration) -> String {
        // First try Feature 0x0005 (DeviceName)
        if let Some(name_feat) = Self::get_feature_index(dev, dev_idx, 0x0005, timeout) {
            // Function 0 gets name length
            let req_count = [
                0x11,
                dev_idx,
                name_feat,
                (0 << 4) | HIDPP_SW_ID,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ];
            let _ = dev.write_report(&req_count);

            let mut count_resp = [0u8; 20];
            let name_len = if dev.read_report(&mut count_resp, timeout).is_ok()
                && count_resp[0] == 0x11
                && count_resp[1] == dev_idx
                && count_resp[2] == name_feat
            {
                count_resp[4] as usize
            } else {
                0
            };

            if name_len > 0 {
                // Function 1 gets device name string
                let req_name = [
                    0x11,
                    dev_idx,
                    name_feat,
                    (1 << 4) | HIDPP_SW_ID,
                    0, // charIndex 0
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ];
                let _ = dev.write_report(&req_name);

                let mut name_resp = [0u8; 20];
                if dev.read_report(&mut name_resp, timeout).is_ok()
                    && name_resp[0] == 0x11
                    && name_resp[1] == dev_idx
                    && name_resp[2] == name_feat
                {
                    let chunk_len = name_len.min(16);
                    let raw_bytes = &name_resp[4..4 + chunk_len];
                    let s = String::from_utf8_lossy(raw_bytes).trim().to_string();
                    if !s.is_empty() {
                        return format!("Logitech {}", s);
                    }
                }
            }
        }

        let hw_name = dev.get_product_string();
        lookup_logitech_name(receiver_pid, hw_name.as_deref())
    }
}

pub fn classify_logitech(pid: u16, name: &str) -> DeviceKind {
    let lower = name.to_lowercase();
    if lower.contains("keyboard") || lower.contains("k850") || lower.contains("g913") || lower.contains("g915") {
        DeviceKind::Keyboard
    } else if pid == 0x4088 || pid == 0x4089 || pid == 0x408E {
        DeviceKind::Keyboard
    } else {
        DeviceKind::Mouse
    }
}

pub fn lookup_logitech_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Receiver")
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return format!("Logitech {}", trimmed);
        }
    }

    match pid {
        0x408A => "Logitech G Pro Wireless (GPW 狗屁王)".to_string(),
        0x408C => "Logitech G Pro X Superlight (GPW 2代)".to_string(),
        0x4053 => "Logitech G304 Lightspeed 无线鼠标".to_string(),
        0x4074 => "Logitech G502 Lightspeed 无线鼠标".to_string(),
        0x407B => "Logitech G903 Lightspeed 无线鼠标".to_string(),
        0x4088 => "Logitech G913 / G915 Lightspeed 机械键盘".to_string(),
        0x4082 => "Logitech MX Master 3 双模无线鼠标".to_string(),
        0x4099 => "Logitech MX Master 3S 双模无线鼠标".to_string(),
        0xC53F | 0xC539 => "Logitech Lightspeed 游戏无线接收器".to_string(),
        0xC52B => "Logitech Unifying 优联无线接收器".to_string(),
        0xC547 => "Logitech Bolt 商用无线接收器".to_string(),
        _ => format!("Logitech 2.4G 设备 (0x{:04X})", pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_logitech_name() {
        let gpw = lookup_logitech_name(0x408A, None);
        assert!(gpw.contains("GPW"));

        let hw = lookup_logitech_name(0x408C, Some("G PRO X Superlight"));
        assert_eq!(hw, "Logitech G PRO X Superlight");

        let generic = lookup_logitech_name(0xC53F, Some("USB Receiver"));
        assert!(generic.contains("Lightspeed"));
    }

    #[test]
    fn test_classify_logitech() {
        assert_eq!(classify_logitech(0x4088, ""), DeviceKind::Keyboard);
        assert_eq!(classify_logitech(0x4099, "MX Master 3S"), DeviceKind::Mouse);
    }
}
