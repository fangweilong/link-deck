use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, SystemTime};

pub const VXE_ATK_VID: u16 = 0x373B;
pub const VXE_COMPX_VID: u16 = 0x3554;
pub const VXE_ALT_VID: u16 = 0x24AE;
pub const VXE_DAREU_VID: u16 = 0x260D;
pub const VXE_GWOLVES_VID: u16 = 0x33E4;

pub struct VxeDriver;

impl VxeDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        let vxe_devs: Vec<&HidDeviceInfo> = devices
            .iter()
            .filter(|d| {
                (d.vendor_id == VXE_ATK_VID
                    || d.vendor_id == VXE_COMPX_VID
                    || d.vendor_id == VXE_ALT_VID
                    || d.vendor_id == VXE_DAREU_VID
                    || d.vendor_id == VXE_GWOLVES_VID)
                    && (d.usage_page == 0xFF02 || d.usage_page == 0xFF05 || d.output_len >= 17 || d.feature_len >= 17)
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
        let dev = HidDevice::open(dev_info.clone(), true).ok()?;
        let timeout = Duration::from_millis(60);

        // Protocol 1: Nordic 52840 / Compx 17-byte Report 0x08, Subcommand 0x04
        // Used by: VXE R1, VXE MAD R, VGN F1, VGN F2, ATK F1, ATK A9, Zaopin, Scyrox, Dareu
        let req_17: [u8; 17] = [
            0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x49,
        ];

        let _ = dev.write_report(&req_17);

        let mut resp_17 = [0u8; 64];
        resp_17[0] = 0x08;

        let got_17 = if dev.read_report(&mut resp_17, timeout).is_ok() {
            true
        } else if dev.get_feature(&mut resp_17[..17]).is_ok() {
            true
        } else {
            false
        };

        if got_17 && resp_17[0] == 0x08 && resp_17[1] == 0x04 {
            let pct = resp_17[6];
            let wired_charging = resp_17[7] != 0;
            let voltage_mv = ((resp_17[8] as u16) << 8) | (resp_17[9] as u16);

            let hw_name = dev.get_product_string();
            let name = lookup_vxe_name(dev_info.vendor_id, dev_info.product_id, hw_name.as_deref());
            let is_wired = is_vxe_wired(dev_info.product_id);

            return Some(DeviceSnapshot {
                id: format!("vxe_{:04x}_{:04x}", dev_info.vendor_id, dev_info.product_id),
                name,
                kind: DeviceKind::Mouse,
                vendor_id: dev_info.vendor_id,
                product_id: dev_info.product_id,
                connection: if is_wired { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
                battery: BatteryState {
                    percentage: if pct > 0 && pct <= 100 { Some(pct) } else { None },
                    voltage_mv: if voltage_mv >= 3000 && voltage_mv <= 4500 { Some(voltage_mv) } else { None },
                    charging: wired_charging,
                    charging_source: if wired_charging { ChargingSource::Charging } else { ChargingSource::None },
                    online: true,
                    last_updated: SystemTime::now(),
                },
                path: dev_info.path.clone(),
            });
        }

        // Protocol 2: Nordic 54L15 64-byte Report 0x08, Command 0x72, Sub-index 0x07 (ATK Zero / 8K Dongles)
        let is_wired = is_vxe_wired(dev_info.product_id);
        let mut req_64 = [0u8; 64];
        req_64[0] = 0x08;
        req_64[1] = 0x7D;
        req_64[2] = 0x72;
        req_64[3] = 0x02;
        req_64[5] = if is_wired { 0x00 } else { 0x01 }; // link index: 0 = cable, 1 = dongle
        req_64[6] = 0x07; // battery query sub-index
        req_64[7] = 0x01;

        let _ = dev.write_report(&req_64);

        let mut resp_64 = [0u8; 64];
        resp_64[0] = 0x08;

        let got_64 = if dev.read_report(&mut resp_64, timeout).is_ok() {
            true
        } else if dev.get_feature(&mut resp_64).is_ok() {
            true
        } else {
            false
        };

        if got_64 && resp_64[0] == 0x08 && resp_64[1] == 0x72 && resp_64[5] == 0x07 && resp_64[2] == 0x00 {
            let pct = resp_64[7];
            let voltage_mv = (resp_64[8] as u16) | ((resp_64[9] as u16) << 8);

            let hw_name = dev.get_product_string();
            let name = lookup_vxe_name(dev_info.vendor_id, dev_info.product_id, hw_name.as_deref());

            return Some(DeviceSnapshot {
                id: format!("vxe_{:04x}_{:04x}", dev_info.vendor_id, dev_info.product_id),
                name,
                kind: DeviceKind::Mouse,
                vendor_id: dev_info.vendor_id,
                product_id: dev_info.product_id,
                connection: if is_wired { ConnectionMode::Wired } else { ConnectionMode::Wireless24G },
                battery: BatteryState {
                    percentage: if pct > 0 && pct <= 100 { Some(pct) } else { None },
                    voltage_mv: if voltage_mv >= 3000 && voltage_mv <= 4500 { Some(voltage_mv) } else { None },
                    charging: is_wired && pct < 100,
                    charging_source: if is_wired { ChargingSource::Charging } else { ChargingSource::None },
                    online: true,
                    last_updated: SystemTime::now(),
                },
                path: dev_info.path.clone(),
            });
        }

        None
    }
}

pub fn is_vxe_wired(pid: u16) -> bool {
    matches!(
        pid,
        0x102E | 0x11B6 | 0x103F | 0x104C | 0xF58C | 0xF58F | 0xF502 | 0xFB3D | 0xFB57 | 0xF526 | 0xF5F6 | 0x1193 | 0x1196 | 0x1154
    )
}

pub fn lookup_vxe_name(vid: u16, pid: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
            && !trimmed.eq_ignore_ascii_case("2.4G Wireless Receiver")
        {
            return trimmed.to_string();
        }
    }

    match (vid, pid) {
        // ATK Series
        (0x373B, 0x1031 | 0x102E) => "ATK F1 Ultimate 无线鼠标".to_string(),
        (0x373B, 0x11D9 | 0x11B6) => "ATK A9 Ultimate 无线鼠标".to_string(),
        (0x373B, 0x1155 | 0x124F | 0x1154) => "ATK Zero 8K 旗舰无线鼠标".to_string(),

        // VXE Series
        (0x373B, 0x104D | 0x103F) => "VXE MAD R 无线鼠标".to_string(),
        (0x373B, 0x1040 | 0x104C) => "VXE MAD R Major Plus 无线鼠标".to_string(),
        (0x3554, 0xF58A | 0xF58C) => "VXE 蜻蜓 R1 Pro Max 无线鼠标".to_string(),
        (0x3554, 0xF58E | 0xF58F) => "VXE 蜻蜓 R1 SE+ 无线鼠标".to_string(),

        // VGN Series
        (0x3554, 0xF503 | 0xF502) => "VGN 蜻蜓 F1 Pro 无线鼠标".to_string(),
        (0x3554, 0xFB3E | 0xFB3D) => "VGN 蜻蜓 F2 Pro Max 无线鼠标".to_string(),
        (0x3554, 0xFB56 | 0xFB57) => "VGN 蜻蜓 F2 Master+ 无线鼠标".to_string(),

        // OEM Partners using same Compx silicon
        (0x3554, 0xF524 | 0xF526) => "Zaopin 造品 Z2 Mini 无线鼠标".to_string(),
        (0x3554, 0xF5F7 | 0xF5F6) => "Scyrox V8 8K 无线鼠标".to_string(),
        (0x260D, 0x1175 | 0x1193) => "达尔优 DAREU A950 Air 无线鼠标".to_string(),
        (0x260D, 0x1154 | 0x1196) => "达尔优 DAREU A950 Wing 无线鼠标".to_string(),
        (0x33E4, 0x3854 | 0x4719) => "G-Wolves Lycan 无线鼠标".to_string(),
        (0x3554, 0xF582 | 0xF583) => "Kysona M600 无线鼠标".to_string(),

        _ => format!("VXE / VGN 方案无线鼠标 (0x{:04X}:0x{:04X})", vid, pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_vxe_name() {
        let r1 = lookup_vxe_name(VXE_COMPX_VID, 0xF58A, None);
        assert!(r1.contains("蜻蜓 R1"));

        let f1 = lookup_vxe_name(VXE_COMPX_VID, 0xF503, None);
        assert!(f1.contains("蜻蜓 F1"));

        let madr = lookup_vxe_name(VXE_ATK_VID, 0x104D, None);
        assert!(madr.contains("MAD R"));

        let atk_zero = lookup_vxe_name(VXE_ATK_VID, 0x1155, None);
        assert!(atk_zero.contains("ATK Zero"));

        let hw = lookup_vxe_name(VXE_COMPX_VID, 0xF58A, Some("Custom Mouse"));
        assert_eq!(hw, "Custom Mouse");
    }

    #[test]
    fn test_is_vxe_wired() {
        assert!(is_vxe_wired(0xF58C));
        assert!(!is_vxe_wired(0xF58A));
        assert!(is_vxe_wired(0x103F));
    }
}
