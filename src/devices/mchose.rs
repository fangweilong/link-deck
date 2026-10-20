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
                    && (d.feature_len >= 20 || d.output_len >= 20 || d.input_len >= 20)
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
        let dev = HidDevice::open(dev_info.clone(), true).ok()?;
        let timeout = Duration::from_millis(60);

        // Protocol 1: Legacy Protocol (Feature Report 0x11, Command 0x06, XOR 0xFF)
        // Used by: MCHOSE A5 Pro, AX5 Pro, G3, A7, L7, M7
        let mut req_legacy = [0xFFu8; 20];
        req_legacy[0] = 0x11;
        req_legacy[1] = 0x06 ^ 0xFF; // 0xF9

        let _ = dev.set_feature(&req_legacy);
        std::thread::sleep(Duration::from_millis(25));

        let mut resp_legacy = [0xFFu8; 20];
        resp_legacy[0] = 0x11;

        if dev.get_feature(&mut resp_legacy).is_ok() {
            let echoed_cmd = resp_legacy[1] ^ 0xFF;
            if echoed_cmd == 0x06 {
                let conn_mode_raw = resp_legacy[7] ^ 0xFF;
                let pct = (resp_legacy[8] ^ 0xFF).min(100);
                let charging_raw = resp_legacy[9] ^ 0xFF;

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

        // Protocol 2: Modern Protocol (Report 0x4D, Command 0x0900: Basic Info & Battery)
        // Used by: MCHOSE A5 V3, A7 V3, G3 V3, K7 V2, R7, V7
        let mut req_modern = [0u8; 64];
        req_modern[0] = 0x4D; // Report ID
        req_modern[1] = 0x01;
        req_modern[2] = 0x01; // Flags
        req_modern[3] = 0x00; // Payload length
        req_modern[4] = 0x00; // Command ID lo (0x0900)
        req_modern[5] = 0x09; // Command ID hi
        req_modern[6] = 0x00; // BizCode
        req_modern[7] = 0x00; // Sequence
        // Checksum: XOR over b[2..8]
        req_modern[8] = req_modern[2] ^ req_modern[3] ^ req_modern[4] ^ req_modern[5] ^ req_modern[6] ^ req_modern[7];

        let _ = dev.write_report(&req_modern);

        let mut resp_modern = [0u8; 64];
        resp_modern[0] = 0x4D;

        if dev.read_report(&mut resp_modern, timeout).is_ok()
            && resp_modern[0] == 0x4D
            && resp_modern[4] == 0x00
            && resp_modern[5] == 0x09
        {
            let pct = resp_modern[12].min(100);
            let charging = resp_modern[13] == 1;
            let conn_mode_raw = resp_modern[14];
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
}

pub fn lookup_mchose_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
            && !trimmed.eq_ignore_ascii_case("2.4G Wireless Receiver")
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
        _ => format!("迈从 MCHOSE 鼠标 (0x{:04X})", pid),
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
