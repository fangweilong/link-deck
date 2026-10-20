use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::SystemTime;

pub const VXE_COMPX_VID: u16 = 0x3554;
pub const VXE_ALT_VID: u16 = 0x24AE;

pub struct VxeDriver;

impl VxeDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let vxe_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                (d.vendor_id == VXE_COMPX_VID || d.vendor_id == VXE_ALT_VID)
                    && (d.output_len >= 17 || d.feature_len >= 17)
            })
            .collect();

        for dev_info in vxe_devs {
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

        // Query Battery: Report ID 0x08, Command 0x04 (Length 17 bytes)
        let req: [u8; 17] = [
            0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x49,
        ];

        let mut resp = [0u8; 17];
        resp[0] = 0x08;

        let ok = if dev.set_output_report(&req).is_ok() && dev.get_input_report(&mut resp).is_ok() {
            true
        } else if dev.set_feature(&req).is_ok() && dev.get_feature(&mut resp).is_ok() {
            true
        } else {
            false
        };

        if ok && resp[0] == 0x08 && resp[1] == 0x04 {
            let pct = resp[6].min(100);
            let charging = resp[7] != 0;
            let voltage_mv = ((resp[8] as u16) << 8) | (resp[9] as u16);

            let hw_name = dev.get_product_string();
            let name = lookup_vxe_name(dev_info.product_id, hw_name.as_deref());

            let is_wired = dev_info.product_id == 0xF58C || dev_info.product_id == 0xF589 || dev_info.product_id == 0xF583;

            return Some(DeviceSnapshot {
                id: format!("vxe_{:04x}", dev_info.product_id),
                name,
                kind: DeviceKind::Mouse,
                vendor_id: dev_info.vendor_id,
                product_id: dev_info.product_id,
                connection: if is_wired { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
                battery: BatteryState {
                    percentage: if pct > 0 { Some(pct) } else { None },
                    voltage_mv: if voltage_mv >= 3000 && voltage_mv <= 4500 { Some(voltage_mv) } else { None },
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

pub fn lookup_vxe_name(pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
        {
            return trimmed.to_string();
        }
    }

    match pid {
        0xF58A | 0xF58C => "VXE 蜻蜓 R1 Pro 无线鼠标".to_string(),
        0xF588 | 0xF589 => "VGN 蜻蜓 F1 无线鼠标".to_string(),
        0xF582 | 0xF583 => "Kysona M600 无线鼠标".to_string(),
        _ => format!("VXE / VGN 方案无线鼠标 (0x{:04X})", pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_vxe_name() {
        let r1 = lookup_vxe_name(0xF58A, None);
        assert!(r1.contains("蜻蜓 R1"));

        let f1 = lookup_vxe_name(0xF588, None);
        assert!(f1.contains("蜻蜓 F1"));

        let hw = lookup_vxe_name(0xF58A, Some("Custom VXE Mouse"));
        assert_eq!(hw, "Custom VXE Mouse");
    }
}
