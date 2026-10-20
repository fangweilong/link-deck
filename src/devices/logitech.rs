use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::SystemTime;

pub const LOGITECH_VID: u16 = 0x046D;

pub struct LogitechDriver {
    pub info: Option<HidDeviceInfo>,
    pub paired_device_index: u8,
}

impl LogitechDriver {
    pub fn new() -> Self {
        Self {
            info: None,
            paired_device_index: 0x01, // default to paired device 1 (gaming mouse / keyboard)
        }
    }

    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        // Search for Logitech HID++ communication endpoints (Vendor collection usage_page >= 0xFF00)
        let logi_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                d.vendor_id == LOGITECH_VID
                    && (d.usage_page >= 0xFF00 || d.usage_page == 0x0001)
                    && (d.feature_len >= 7 || d.output_len >= 7 || d.input_len >= 7)
            })
            .collect();

        for dev_info in logi_devs {
            // Check device 1..=2 on this receiver
            for dev_idx in 1..=2u8 {
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
        let dev = HidDevice::open(dev_info.clone(), false).ok()?;

        // Query HID++ 2.0 Feature 0x1004 (Unified Battery)
        let mut req_1004 = [0u8; 20];
        req_1004[0] = 0x11; // Long report
        req_1004[1] = dev_idx; // Device index
        req_1004[2] = 0x00; // Feature 0x0000 (Root)
        req_1004[3] = 0x00; // Function 0 (getFeature)
        req_1004[4] = 0x10; // Feature ID high
        req_1004[5] = 0x04; // Feature ID low (0x1004 = Unified Battery)

        let mut resp = [0u8; 20];
        resp[0] = 0x11;

        let query_unified_ok = if dev.set_feature(&req_1004).is_ok() && dev.get_feature(&mut resp).is_ok() {
            true
        } else if dev.set_output_report(&req_1004).is_ok() && dev.get_input_report(&mut resp).is_ok() {
            true
        } else {
            false
        };

        if query_unified_ok && resp[0] == 0x11 && resp[1] == dev_idx && resp[2] == 0x00 {
            let feat_idx = resp[4];
            if feat_idx != 0 {
                // Call Function 1: getBatteryInfo
                let mut req_info = [0u8; 20];
                req_info[0] = 0x11;
                req_info[1] = dev_idx;
                req_info[2] = feat_idx;
                req_info[3] = 0x10; // Function 1 << 4

                let mut info_resp = [0u8; 20];
                info_resp[0] = 0x11;

                let got_info = if dev.set_feature(&req_info).is_ok() && dev.get_feature(&mut info_resp).is_ok() {
                    true
                } else if dev.set_output_report(&req_info).is_ok() && dev.get_input_report(&mut info_resp).is_ok() {
                    true
                } else {
                    false
                };

                if got_info && info_resp[0] == 0x11 && info_resp[1] == dev_idx && info_resp[2] == feat_idx {
                    let pct = info_resp[4];
                    let status = info_resp[6];
                    let charging = status == 1 || status == 2 || status == 3;
                    let hw_name = dev.get_product_string();
                    let name = lookup_logitech_name(dev_info.product_id, hw_name.as_deref());

                    return Some(DeviceSnapshot {
                        id: format!("logitech_{:04x}_{}", dev_info.product_id, dev_idx),
                        name,
                        kind: classify_logitech(dev_info.product_id),
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

        // Fallback: Query HID++ 2.0 Feature 0x1000 (Battery Status)
        let mut req_1000 = [0u8; 20];
        req_1000[0] = 0x11;
        req_1000[1] = dev_idx;
        req_1000[2] = 0x00;
        req_1000[3] = 0x00;
        req_1000[4] = 0x10;
        req_1000[5] = 0x00; // 0x1000 = Battery Status

        let mut resp_1000 = [0u8; 20];
        resp_1000[0] = 0x11;

        let query_status_ok = if dev.set_feature(&req_1000).is_ok() && dev.get_feature(&mut resp_1000).is_ok() {
            true
        } else if dev.set_output_report(&req_1000).is_ok() && dev.get_input_report(&mut resp_1000).is_ok() {
            true
        } else {
            false
        };

        if query_status_ok && resp_1000[0] == 0x11 && resp_1000[1] == dev_idx && resp_1000[2] == 0x00 {
            let feat_idx = resp_1000[4];
            if feat_idx != 0 {
                // Call Function 0: getBatteryLevelStatus
                let mut req_info = [0u8; 20];
                req_info[0] = 0x11;
                req_info[1] = dev_idx;
                req_info[2] = feat_idx;
                req_info[3] = 0x00;

                let mut info_resp = [0u8; 20];
                info_resp[0] = 0x11;

                let got_info = if dev.set_feature(&req_info).is_ok() && dev.get_feature(&mut info_resp).is_ok() {
                    true
                } else if dev.set_output_report(&req_info).is_ok() && dev.get_input_report(&mut info_resp).is_ok() {
                    true
                } else {
                    false
                };

                if got_info && info_resp[0] == 0x11 && info_resp[1] == dev_idx && info_resp[2] == feat_idx {
                    let pct = info_resp[4];
                    let charging_status = info_resp[6];
                    let charging = charging_status == 1 || charging_status == 2;
                    let hw_name = dev.get_product_string();
                    let name = lookup_logitech_name(dev_info.product_id, hw_name.as_deref());

                    return Some(DeviceSnapshot {
                        id: format!("logitech_{:04x}_{}", dev_info.product_id, dev_idx),
                        name,
                        kind: classify_logitech(dev_info.product_id),
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

pub fn classify_logitech(pid: u16) -> DeviceKind {
    match pid {
        0x4088 | 0x4089 | 0x408E => DeviceKind::Keyboard,
        _ => DeviceKind::Mouse,
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
        assert_eq!(classify_logitech(0x4088), DeviceKind::Keyboard);
        assert_eq!(classify_logitech(0x408A), DeviceKind::Mouse);
    }
}
