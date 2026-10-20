//! ==============================================================================
//! 厂商模块: RK (Royal Kludge) / Compx 方案客制化三模机械键盘
//! 协议类型: Compx 方案无线接收器与 USB 有线硬件报文
//! 通信机制: UsagePage 0xFF02 Report 0x06 (硬件毫伏电压直读与平滑放电曲线百分比换算)
//! 代表设备: RK108、F75、F99、M75、R75、S98、RK100、RK87、RK84、RK68 等全系键盘
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::SystemTime;

pub const RK_COMPX_VID: u16 = 0x25A7;
pub const RK_COMPX_PID: u16 = 0xFA70;
pub const RK_COMPX_VID2: u16 = 0x3554;
pub const RK_VENDOR_VID: u16 = 0x258A;
pub const RK_SONIX_VID: u16 = 0x0C45;

pub fn is_rk_keyboard_vid(vid: u16) -> bool {
    matches!(
        vid,
        0x25A7 | 0x3554 | 0x258A | 0x0C45 | 0x2F24 | 0x320F | 0x341A | 0x0416
    )
}
pub struct RkKeyboardDriver {
    pub info: Option<HidDeviceInfo>,
}

impl RkKeyboardDriver {
    pub fn new() -> Self {
        Self { info: None }
    }

    pub fn detect(&mut self) -> Option<DeviceSnapshot> {
        let devices = enumerate_hid_devices();

        // 1. Look for Compx 2.4G Receiver collection (UsagePage 0xFF02, Usage 2, Feat 8)
        let compx_dev = devices.iter().find(|d| {
            is_rk_keyboard_vid(d.vendor_id)
                && d.usage_page == 0xFF02
                && d.usage == 0x0002
                && d.feature_len >= 8
        });

        if let Some(dev) = compx_dev {
            self.info = Some(dev.clone());
            let mut battery = self.query_battery_internal(dev);
            if battery.charging {
                battery.charging_source = ChargingSource::Charging;
            }
            let hw_name = HidDevice::open(dev.clone(), false).ok().and_then(|h| h.get_product_string());
            let model_name = lookup_rk_model(dev.product_id, hw_name.as_deref(), "Compx 方案三模机械键盘");
            return Some(DeviceSnapshot {
                id: format!("rk_keyboard_{:04x}", dev.product_id),
                name: model_name,
                kind: DeviceKind::Keyboard,
                vendor_id: dev.vendor_id,
                product_id: dev.product_id,
                connection: ConnectionMode::Wireless24G,
                battery,
                path: dev.path.clone(),
            });
        }

        // 2. Look for RK Wired / Other mode (VID 0x258A, 0x0C45, or Compx wired)
        let rk_wired = devices.iter().find(|d| {
            is_rk_keyboard_vid(d.vendor_id)
                && ((d.usage_page == 0xFF02 && d.usage == 0x0002)
                    || (d.usage_page == 0xFF00 && d.usage == 0x0001))
                && d.product_id != RK_COMPX_PID
                && d.product_id != 0xFA09
        });

        if let Some(dev) = rk_wired {
            self.info = Some(dev.clone());
            let hw_name = HidDevice::open(dev.clone(), false).ok().and_then(|h| h.get_product_string());
            let mut battery = self.query_battery_internal(dev);
            let model_name = lookup_rk_model(dev.product_id, hw_name.as_deref(), "Compx / Sonix 方案机械键盘");

            // When plugged in via USB cable to PC, it is PC USB powered and charging
            battery.charging = true;
            battery.charging_source = ChargingSource::Charging;
            battery.online = true;
            if battery.percentage.is_none() {
                battery.percentage = Some(100);
            }

            return Some(DeviceSnapshot {
                id: format!("rk_keyboard_{:04x}", dev.product_id),
                name: model_name,
                kind: DeviceKind::Keyboard,
                vendor_id: dev.vendor_id,
                product_id: dev.product_id,
                connection: ConnectionMode::Wired,
                battery,
                path: dev.path.clone(),
            });
        }

        self.info = None;
        None
    }
    pub fn poll_battery(&mut self) -> Option<BatteryState> {
        // First, check if a wired RK keyboard is plugged into PC USB directly
        let devices = enumerate_hid_devices();
        let rk_wired = devices.iter().find(|d| {
            is_rk_keyboard_vid(d.vendor_id)
                && ((d.usage_page == 0xFF02 && d.usage == 0x0002)
                    || (d.usage_page == 0xFF00 && d.usage == 0x0001))
                && d.product_id != RK_COMPX_PID
                && d.product_id != 0xFA09
        });

        if let Some(wired_dev) = rk_wired {
            let mut b = self.query_battery_internal(wired_dev);
            b.charging = true;
            b.charging_source = ChargingSource::Charging;
            if b.percentage.is_none() {
                b.percentage = Some(100);
            }
            return Some(b);
        }
        if let Some(dev_info) = &self.info {
            let mut b = self.query_battery_internal(dev_info);
            if dev_info.vendor_id == RK_VENDOR_VID || dev_info.vendor_id == RK_SONIX_VID {
                b.charging = true;
                b.charging_source = ChargingSource::Charging;
                b.online = true;
                if b.percentage.is_none() {
                    b.percentage = Some(100);
                }
            } else if b.charging {
                b.charging_source = ChargingSource::Charging;
            } else {
                b.charging_source = ChargingSource::None;
            }
            Some(b)
        } else if let Some(snap) = self.detect() {
            Some(snap.battery)
        } else {
            None
        }
    }

    fn query_battery_internal(&self, dev_info: &HidDeviceInfo) -> BatteryState {
        // Method A: Compx 8-byte Feature Report (Report ID 6)
        if dev_info.usage_page == 0xFF02 && dev_info.usage == 0x0002 {
            if let Ok(dev) = HidDevice::open(dev_info.clone(), false) {
                let mut buf = [6u8, 0, 0, 0, 0, 0, 0, 0];
                let _ = dev.set_feature(&buf);
                if dev.get_feature(&mut buf).is_ok() {
                    if let Some(state) = parse_rk_feature_report_6(&buf) {
                        return state;
                    }
                }
            }
        }

        // Method B: G5KB 520-byte Feature Report (Command 0x87)
        if dev_info.feature_len >= 520 {
            if let Ok(dev) = HidDevice::open(dev_info.clone(), false) {
                let mut buf = vec![0u8; 520];
                buf[1] = 0x87;
                buf[4] = 1;
                buf[6] = 2;
                if dev.set_feature(&buf).is_ok() && dev.get_feature(&mut buf).is_ok() {
                    if buf[1] == 0x87 {
                        let charging = buf[8] != 0;
                        let raw_lvl = buf[9];
                        let pct = if raw_lvl <= 10 { raw_lvl * 10 } else { raw_lvl };
                        return BatteryState {
                            percentage: Some(pct.min(100)),
                            voltage_mv: None,
                            charging,
                            charging_source: if charging { ChargingSource::Charging } else { ChargingSource::None },
                            online: true,
                            last_updated: SystemTime::now(),
                        };
                    }
                }
            }
        }

        BatteryState {
            percentage: None,
            voltage_mv: None,
            charging: false,
            charging_source: ChargingSource::None,
            online: false,
            last_updated: SystemTime::now(),
        }
    }
}

pub fn lookup_rk_model(pid: u16, hw_name: Option<&str>, default_name: &str) -> String {
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
    match pid {
        0xFA70 | 0xFA09 => "Compx 方案三模机械键盘".to_string(),
        0x01C6 | 0x01D0 | 0x01D2 | 0x01DE => "RK F75 三模机械键盘".to_string(),
        0x0218 => "RK F99 三模机械键盘".to_string(),
        0x0147 | 0x0163 | 0x0175 => "RK M75 三模机械键盘".to_string(),
        0x0151 | 0x015B | 0x015E | 0x01FC => "RK R75 三模机械键盘".to_string(),
        0x0174 | 0x01AF | 0x01BF => "RK S98 三模机械键盘".to_string(),
        0x0056 | 0x004B | 0x00E0 | 0x023E => "RK100 三模机械键盘".to_string(),
        0x0059 | 0x00C8 | 0x00F3 | 0x00F4 | 0x010D | 0x02A6 => "RK84 三模机械键盘".to_string(),
        0x004C | 0x007C | 0x00C4 | 0x00F2 | 0x010F => "RK87 三模机械键盘".to_string(),
        0x0055 | 0x004A | 0x0060 | 0x00E1 | 0x00F8 | 0x0183 | 0x02D4 => "RK61 三模机械键盘".to_string(),
        0x005E | 0x0064 | 0x006F | 0x0079 | 0x008E | 0x00DA | 0x00FF | 0x010A | 0x02AC => {
            "RK68 三模机械键盘".to_string()
        }
        _ => default_name.to_string(),
    }
}

/// Convert 1S Li-Po / Li-ion battery voltage in millivolts to percentage (0 - 100%)
pub fn millivolts_to_percent(mv: u16) -> u8 {
    if mv >= 4200 {
        100
    } else if mv >= 4050 {
        90 + ((mv - 4050) as f32 / 150.0 * 10.0) as u8
    } else if mv >= 3920 {
        75 + ((mv - 3920) as f32 / 130.0 * 15.0) as u8
    } else if mv >= 3800 {
        50 + ((mv - 3800) as f32 / 120.0 * 25.0) as u8
    } else if mv >= 3700 {
        25 + ((mv - 3700) as f32 / 100.0 * 25.0) as u8
    } else if mv >= 3550 {
        10 + ((mv - 3550) as f32 / 150.0 * 15.0) as u8
    } else if mv >= 3400 {
        ((mv - 3400) as f32 / 150.0 * 10.0) as u8
    } else {
        0
    }
}

/// Parses the 8-byte Compx feature report 6.
pub fn parse_rk_feature_report_6(buf: &[u8]) -> Option<BatteryState> {
    if buf.len() < 8 || buf[0] != 6 {
        return None;
    }
    let mv = (buf[6] as u16) | ((buf[7] as u16) << 8);
    if !(3000..=4500).contains(&mv) {
        return None;
    }
    let pct = millivolts_to_percent(mv);
    // In Compx 8-byte report, buf[2] is the device collection index (0x02 = keyboard),
    // not a charging flag. Battery voltage >= 4200mV is standard resting full voltage.
    // External charging over 2.4G is indicated by status bytes buf[3] / buf[5].
    let charging = buf[3] != 0 || buf[5] != 0;
    Some(BatteryState {
        percentage: Some(pct),
        voltage_mv: Some(mv),
        charging,
        charging_source: if charging { ChargingSource::Charging } else { ChargingSource::None },
        online: true,
        last_updated: SystemTime::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rk_feature_report_6_discharging_full() {
        // Raw buffer captured from live RK108 2.4G keyboard when fully charged (4241 mV) and unplugged:
        let buf = [0x06, 0x00, 0x02, 0x00, 0x00, 0x00, 0x91, 0x10];
        let state = parse_rk_feature_report_6(&buf).expect("should parse");
        assert_eq!(state.voltage_mv, Some(4241));
        assert_eq!(state.percentage, Some(100));
        assert!(!state.charging, "Keyboard must not be marked charging when unplugged");
        assert_eq!(state.charging_source, ChargingSource::None);
    }

    #[test]
    fn test_parse_rk_feature_report_6_charging() {
        // When charging byte 3 or 5 is non-zero:
        let buf = [0x06, 0x00, 0x02, 0x01, 0x00, 0x00, 0x91, 0x10];
        let state = parse_rk_feature_report_6(&buf).expect("should parse");
        assert!(state.charging, "Keyboard should be marked charging when status flag set");
        assert_eq!(state.charging_source, ChargingSource::Charging);
    }

    #[test]
    fn test_lookup_rk_model_hw_name_priority() {
        let name = lookup_rk_model(0xFA70, Some("My Custom RK F75"), "默认键盘");
        assert_eq!(name, "My Custom RK F75");
    }

    #[test]
    fn test_lookup_rk_model_ignores_generic_hw_names() {
        let name = lookup_rk_model(0x01C6, Some("USB Input Device"), "默认键盘");
        assert_eq!(name, "RK F75 三模机械键盘");

        let name_hid = lookup_rk_model(0x0218, Some("HID-compliant device"), "默认键盘");
        assert_eq!(name_hid, "RK F99 三模机械键盘");
    }

    #[test]
    fn test_lookup_rk_model_dongle_and_fallbacks() {
        let dongle = lookup_rk_model(0xFA70, None, "默认键盘");
        assert_eq!(dongle, "Compx 方案三模机械键盘");

        let unknown = lookup_rk_model(0x9999, None, "默认键盘");
        assert_eq!(unknown, "默认键盘");
    }
}
