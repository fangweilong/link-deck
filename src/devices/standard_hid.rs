//! ==============================================================================
//! 通用模块: USB-IF HID 标准电源管理规范 (Standard HID Power)
//! 协议类型: USB HID 官方标准电源外设规范
//! 通信机制: UsagePage 0x0084 (Power Device) 与 UsagePage 0x0085 (Battery System)
//! 代表设备: 遵循 USB 官方标准 HID 电池描述符规范的第三方有线/无线外设
//! 实机验证: 未实测 — 按 USB-IF 规范实现，待对应硬件验证
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice};
use crate::devices::types::{BatteryState, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::SystemTime;

pub struct StandardHidBatteryDriver;

impl StandardHidBatteryDriver {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut results = Vec::new();

        for dev_info in devices {
            // Check for standard HID Power Device / Battery System
            // UsagePage 0x0084 = Power Device
            // UsagePage 0x0085 = Battery System
            if dev_info.usage_page == 0x0084 || dev_info.usage_page == 0x0085 || dev_info.usage == 0x0066 {
                if let Ok(dev) = HidDevice::open(dev_info.clone(), false) {
                    let mut buf = [0u8; 16];
                    if dev.get_feature(&mut buf).is_ok() {
                        let pct = buf[1].min(100);
                        let id = format!("hid_std_{:04x}_{:04x}", dev_info.vendor_id, dev_info.product_id);
                        let hw_name = dev.get_product_string();
                        let name = lookup_standard_hid_name(dev_info.product_id, hw_name.as_deref());
                        results.push(DeviceSnapshot {
                            id,
                            name,
                            kind: DeviceKind::Other,
                            vendor_id: dev_info.vendor_id,
                            product_id: dev_info.product_id,
                            connection: ConnectionMode::Unknown,
                            battery: BatteryState {
                                percentage: Some(pct),
                                voltage_mv: None,
                                charging: false,
                                charging_source: crate::devices::types::ChargingSource::None,
                                online: true,
                                last_updated: SystemTime::now(),
                            },
                            path: dev_info.path.clone(),
                        });
                    }
                }
            }
        }

        results
    }
}

pub fn lookup_standard_hid_name(product_id: u16, hw_name: Option<&str>) -> String {
    if let Some(hw) = hw_name {
        let trimmed = hw.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("USB Input Device")
            && !trimmed.eq_ignore_ascii_case("HID-compliant device")
            && !trimmed.eq_ignore_ascii_case("USB-HID")
        {
            return trimmed.to_string();
        }
    }
    format!("HID 标准外设 (0x{:04X})", product_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_standard_hid_name() {
        let real = lookup_standard_hid_name(0x1234, Some("Logitech G304"));
        assert_eq!(real, "Logitech G304");

        let generic = lookup_standard_hid_name(0x1234, Some("USB Input Device"));
        assert_eq!(generic, "HID 标准外设 (0x1234)");

        let none = lookup_standard_hid_name(0x5678, None);
        assert_eq!(none, "HID 标准外设 (0x5678)");
    }
}
