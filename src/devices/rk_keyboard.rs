//! ==============================================================================
//! 厂商模块: RK (Royal Kludge) / Compx 方案客制化三模机械键盘
//! 协议类型: Compx 方案无线接收器与 USB 有线硬件报文
//! 通信机制: UsagePage 0xFF02 Report 0x06（静态回显缓冲，2.4G 模式不提供实时电量）
//! 代表设备: RK108、F75、F99、M75、R75、S98、RK100、RK87、RK84、RK68 等全系键盘
//! 实机验证: 已实测 — RK108（2.4G 0x25A7:0xFA70 / 有线 0x258A:0x00A1）
//!         2.4G 电量不可读：0x06 的 mV 字段恒为 4241（固件常量），键盘 LED 指示走本地固件
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

    /// 枚举 RK/Compx 键盘。
    ///
    /// 同一把三模键盘在用 USB 线连接时会同时出现「有线集合」和「2.4G 接收器」，
    /// 但它们是同一台设备：这里合并成一条，并**优先采用有线状态**（充电、供电）。
    /// 设备 ID 以接收器 PID 为稳定基准（无接收器时用有线 PID），
    /// 这样插拔 USB 线不会导致列表里多一条或 ID 变化。
    pub fn detect_all(&mut self) -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let mut out = Vec::new();

        // 有线键盘集合（厂商页 0xFF00/0xFF02，排除接收器 PID）
        let wired = devices.iter().find(|d| {
            is_rk_keyboard_vid(d.vendor_id)
                && (d.usage_page == 0xFF00 || d.usage_page == 0xFF02)
                && d.product_id != RK_COMPX_PID
                && d.product_id != 0xFA09
        });
        // 2.4G 接收器状态集合
        let dongle = devices.iter().find(|d| {
            is_rk_keyboard_vid(d.vendor_id)
                && d.usage_page == 0xFF02
                && d.usage == 0x0002
                && d.feature_len >= 8
                && d.feature_len < 520
        });

        let dongle_battery = dongle.map(|d| self.query_battery_internal(d));

        let anchor = wired.cloned().or_else(|| dongle.cloned());
        let Some(anchor) = anchor else {
            self.info = None;
            return out;
        };

        // 稳定 ID：优先接收器 PID（常驻），其次有线 PID
        let id_pid = dongle.map(|d| d.product_id).unwrap_or(anchor.product_id);
        let wired_present = wired.is_some();

        let mut battery = dongle_battery.unwrap_or_default();
        if wired_present {
            // 有线优先：插着线就是在供电，电量取接收器回读（若有），否则视为满电
            battery.charging = true;
            battery.charging_source = ChargingSource::Charging;
            battery.online = true;
            if battery.percentage.is_none() {
                battery.percentage = Some(100);
            }
        } else if battery.online {
            battery.charging_source = ChargingSource::None;
        }

        let (vendor_id, product_id, path, connection) = if let Some(w) = wired {
            (w.vendor_id, w.product_id, w.path.clone(), ConnectionMode::Wired)
        } else {
            let d = dongle.unwrap();
            (d.vendor_id, d.product_id, d.path.clone(), ConnectionMode::Wireless24G)
        };

        let hw_name = HidDevice::open(anchor, false)
            .ok()
            .and_then(|h| h.get_product_string());
        let model_name = lookup_rk_model(product_id, hw_name.as_deref(), "Compx 方案三模机械键盘");

        out.push(DeviceSnapshot {
            id: format!("rk_keyboard_{:04x}", id_pid),
            name: model_name,
            kind: DeviceKind::Keyboard,
            vendor_id,
            product_id,
            connection,
            battery,
            path,
        });

        self.info = wired.cloned().or_else(|| dongle.cloned());
        out
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
        // Compx 接收器的 0x06 feature 报告是静态回显缓冲区：字节 6-7 恒为固件
        // 常量 0x1091（4241 mV，满电值），按键、唤醒、重插接收器都不会刷新。
        // 键盘自身的 LED 电量指示不经过该通道，因此这里不产出百分比或电压，
        // 只标记在线，避免设备被当作休眠而移出轮播。
        if dev_info.usage_page == 0xFF02 && dev_info.usage == 0x0002 && dev_info.feature_len < 520 {
            return BatteryState {
                percentage: None,
                voltage_mv: None,
                charging: false,
                charging_source: ChargingSource::None,
                online: true,
                last_updated: SystemTime::now(),
            };
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
        0x00A1 => "RK932RGB 机械键盘".to_string(),
        _ => default_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
