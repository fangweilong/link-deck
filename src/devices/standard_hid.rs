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
                        results.push(DeviceSnapshot {
                            id,
                            name: format!("HID 标准外设 (0x{:04X})", dev_info.product_id),
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
