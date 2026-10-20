use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const MCHOSE_VID_1: u16 = 0x3837;
pub const MCHOSE_VID_2: u16 = 0x5253;
pub const MCHOSE_VID_3: u16 = 0x2023;

pub struct MchoseDriver;

impl MchoseDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let mchose_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                (d.vendor_id == MCHOSE_VID_1 || d.vendor_id == MCHOSE_VID_2 || d.vendor_id == MCHOSE_VID_3)
                    && (d.feature_len >= 20 || d.feature_len == 65)
            })
            .collect();

        for dev_info in mchose_devs {
            if let Some(snapshot) = Self::query_device(dev_info) {
                if !results.iter().any(|d: &DeviceSnapshot| d.id == snapshot.id) {
                    results.push(snapshot);
                }
            }
        }

        results
    }

    fn query_device(dev_info: &HidDeviceInfo) -> Option<DeviceSnapshot> {
        let dev = HidDevice::open(dev_info.clone(), false).ok()?;

        // Send Feature Report 0x11, Command 0x06 (Basic Info: Battery & Charging)
        // Protocol specification: bytes are XOR'd with 0xFF
        let mut req = [0xFFu8; 20];
        req[0] = 0x11;
        req[1] = 0x06 ^ 0xFF; // 0xF9

        let _ = dev.set_feature(&req);
        std::thread::sleep(Duration::from_millis(30));

        let mut resp = [0xFFu8; 20];
        resp[0] = 0x11;

        if dev.get_feature(&mut resp).is_ok() {
            // Check if echoed command matches 0x06 (decoded)
            let echoed_cmd = resp[1] ^ 0xFF;
            if echoed_cmd == 0x06 {
                let conn_mode_raw = resp[7] ^ 0xFF;
                let pct = (resp[8] ^ 0xFF).min(100);
                let charging_raw = resp[9] ^ 0xFF;

                let charging = charging_raw == 1;
                let conn_mode = match conn_mode_raw & 0x07 {
                    0 => ConnectionMode::Wired,
                    2 => ConnectionMode::Bluetooth,
                    _ => ConnectionMode::Wireless24G,
                };

                let hw_name = dev.get_product_string();
                let name = lookup_mchose_name(dev_info.product_id, hw_name.as_deref());

                return Some(DeviceSnapshot {
                    id: format!("mchose_{:04x}", dev_info.product_id),
                    name,
                    kind: DeviceKind::Mouse,
                    vendor_id: dev_info.vendor_id,
                    product_id: dev_info.product_id,
                    connection: conn_mode,
                    battery: BatteryState {
                        percentage: if pct > 0 { Some(pct) } else { None },
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

        None
    }
}

pub fn lookup_mchose_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return format!("迈从 {}", trimmed);
        }
    }

    match pid {
        0x0011 | 0x1020 | 0x1021 => "迈从 MCHOSE A7 Ultra 无线鼠标".to_string(),
        0xF013 | 0xF015 | 0xF017 => "迈从 MCHOSE A5 Pro 无线鼠标".to_string(),
        0xF018 | 0xF019 => "迈从 MCHOSE A5 Pro Max 无线鼠标".to_string(),
        0xF023 | 0xF024 | 0xF025 | 0xF026 => "迈从 MCHOSE AX5 Pro 无线鼠标".to_string(),
        0xF021 | 0xF022 => "迈从 MCHOSE AX5 Pro Max 无线鼠标".to_string(),
        _ => format!("迈从 MCHOSE 无线鼠标 (0x{:04X})", pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_mchose_name() {
        let a5 = lookup_mchose_name(0xF013, None);
        assert!(a5.contains("A5 Pro"));

        let a7 = lookup_mchose_name(0x0011, None);
        assert!(a7.contains("A7 Ultra"));

        let hw = lookup_mchose_name(0xF013, Some("AX5"));
        assert_eq!(hw, "迈从 AX5");
    }
}
