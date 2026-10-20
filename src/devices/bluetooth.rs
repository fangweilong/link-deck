use crate::devices::types::{BatteryState, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::mem;
use std::ptr;
use std::time::SystemTime;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW, SetupDiGetDeviceRegistryPropertyW,
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME,
    SP_DEVINFO_DATA,
};
use windows_sys::Win32::Devices::Properties::DEVPROPKEY;

// DEVPKEY_Device_BatteryLevel: {104EA319-6EE2-4701-BD47-8DDBF425BBE5}, 2
const DEVPKEY_DEVICE_BATTERY_LEVEL: DEVPROPKEY = DEVPROPKEY {
    fmtid: windows_sys::core::GUID {
        data1: 0x104EA319,
        data2: 0x6EE2,
        data3: 0x4701,
        data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5],
    },
    pid: 2,
};

pub struct BluetoothBatteryScanner;

impl BluetoothBatteryScanner {
    pub fn scan_devices() -> Vec<DeviceSnapshot> {
        let mut results = Vec::new();

        unsafe {
            let dev_info: HDEVINFO = SetupDiGetClassDevsW(
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                DIGCF_ALLCLASSES | DIGCF_PRESENT,
            );
            if dev_info == -1isize {
                return results;
            }

            let mut dev_data: SP_DEVINFO_DATA = mem::zeroed();
            dev_data.cbSize = mem::size_of::<SP_DEVINFO_DATA>() as u32;

            let mut index = 0;
            let mut prop_type = 0u32;
            let mut prop_buf = [0u8; 256];
            let mut prop_size;

            while SetupDiEnumDeviceInfo(dev_info, index, &mut dev_data) != 0 {
                index += 1;

                prop_size = prop_buf.len() as u32;
                let ok = SetupDiGetDevicePropertyW(
                    dev_info,
                    &dev_data,
                    &DEVPKEY_DEVICE_BATTERY_LEVEL,
                    &mut prop_type,
                    prop_buf.as_mut_ptr(),
                    prop_buf.len() as u32,
                    &mut prop_size,
                    0,
                );

                if ok != 0 && prop_size > 0 {
                    let battery_pct = prop_buf[0].min(100);

                    // Get instance ID
                    let mut inst_buf = [0u16; 512];
                    SetupDiGetDeviceInstanceIdW(
                        dev_info,
                        &dev_data,
                        inst_buf.as_mut_ptr(),
                        inst_buf.len() as u32,
                        ptr::null_mut(),
                    );
                    let inst_id = from_wide(&inst_buf);

                    // Get Friendly Name
                    let mut name_buf = [0u16; 512];
                    let mut name_size = 0u32;
                    let got_name = SetupDiGetDeviceRegistryPropertyW(
                        dev_info,
                        &dev_data,
                        SPDRP_FRIENDLYNAME,
                        ptr::null_mut(),
                        name_buf.as_mut_ptr() as _,
                        (name_buf.len() * 2) as u32,
                        &mut name_size,
                    ) != 0;

                    let friendly_name = if got_name {
                        from_wide(&name_buf)
                    } else {
                        // Fallback to DeviceDesc
                        let got_desc = SetupDiGetDeviceRegistryPropertyW(
                            dev_info,
                            &dev_data,
                            SPDRP_DEVICEDESC,
                            ptr::null_mut(),
                            name_buf.as_mut_ptr() as _,
                            (name_buf.len() * 2) as u32,
                            &mut name_size,
                        ) != 0;
                        if got_desc {
                            from_wide(&name_buf)
                        } else {
                            "蓝牙外设".to_string()
                        }
                    };

                    // Format cleaner name (strip Hands-Free AG / Avrcp suffixes)
                    let clean_name = friendly_name
                        .replace(" Hands-Free AG", "")
                        .replace(" Avrcp Transport", "")
                        .replace(" Bluetooth Device", "");

                    let kind = classify_device(&clean_name, &inst_id);

                    // Clean id from inst_id or MAC
                    let id = format!(
                        "bt_{}",
                        clean_name.to_lowercase().replace(' ', "_").replace('-', "_")
                    );

                    // Check if already in results (e.g. Handsfree AG vs Avrcp sibling)
                    if results.iter().any(|d: &DeviceSnapshot| d.id == id) {
                        continue;
                    }

                    results.push(DeviceSnapshot {
                        id,
                        name: format!("{} (蓝牙)", clean_name),
                        kind,
                        vendor_id: 0,
                        product_id: 0,
                        connection: ConnectionMode::Bluetooth,
                        battery: BatteryState {
                            percentage: Some(battery_pct),
                            voltage_mv: None,
                            charging: false,
                            charging_source: crate::devices::types::ChargingSource::None,
                            online: true,
                            last_updated: SystemTime::now(),
                        },
                        path: inst_id,
                    });
                }
            }

            SetupDiDestroyDeviceInfoList(dev_info);
        }

        results
    }
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn classify_device(name: &str, inst_id: &str) -> DeviceKind {
    let lower = format!("{} {}", name, inst_id).to_lowercase();
    if lower.contains("headset")
        || lower.contains("hands-free")
        || lower.contains("audio")
        || lower.contains("earphone")
        || lower.contains("earbud")
        || lower.contains("airpod")
        || lower.contains("freebud")
        || lower.contains("p6")
        || lower.contains("wh-1000")
        || lower.contains("buds")
    {
        DeviceKind::Headset
    } else if lower.contains("mouse") {
        DeviceKind::Mouse
    } else if lower.contains("keyboard") || lower.contains("kbd") {
        DeviceKind::Keyboard
    } else {
        DeviceKind::Other
    }
}
