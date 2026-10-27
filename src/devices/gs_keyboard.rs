//! ==============================================================================
//! 厂商模块: GANSS (迦斯) GS3108T-PRO 三模机械键盘
//! 协议类型: GANSS 私有 33 字节 HID Output/Input 报文 (UsagePage 0xFF60, Usage 0x61)
//! 通信机制: Output Report 命令 0x20 查询电量，Input Report 回读 0x20 应答
//! 代表设备: GS3108T-PRO（2.4G 接收器 05AC:024F MI_03）
//! 实机验证: 已实测 — GS3108T-PRO 2.4G；键盘休眠时接收器不转发命令
//! ==============================================================================

use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::time::{Duration, Instant, SystemTime};

pub const GANSS_VID: u16 = 0x05AC;
pub const GANSS_GS3108T_PRO_PID: u16 = 0x024F;

/// 电量查询命令，位于报文第 2 字节（第 1 字节是 Report ID 0x00）。
pub const CMD_BATTERY: u8 = 0x20;
/// Output/Input 报文固定长度，含 Report ID。
pub const REPORT_LEN: usize = 33;

/// 应答帧中电量字段的下标。
const BATTERY_INDEX: usize = 4;

pub fn is_gs_keyboard(info: &HidDeviceInfo) -> bool {
    info.vendor_id == GANSS_VID
        && info.product_id == GANSS_GS3108T_PRO_PID
        && info.usage_page == 0xFF60
        && info.usage == 0x0061
        && info.input_len as usize >= REPORT_LEN
        && info.output_len as usize >= REPORT_LEN
}

/// 解析 `0x20` 应答：`00 20 [状态] 00 [电量] ... [校验和]`。
///
/// 校验和为前 32 字节之和的低 8 位。字节 2 的含义尚未确认（官方驱动读到时
/// 为 0x01，独立查询时为 0x00），因此不参与充电状态判断。
pub fn parse_battery_response(buf: &[u8]) -> Option<u8> {
    if buf.len() < REPORT_LEN || buf[0] != 0x00 || buf[1] != CMD_BATTERY {
        return None;
    }
    let expected = buf[..buf.len() - 1]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    if expected != buf[buf.len() - 1] {
        return None;
    }
    let percentage = buf[BATTERY_INDEX];
    (percentage <= 100).then_some(percentage)
}

pub struct GsKeyboardDriver {
    pub info: Option<HidDeviceInfo>,
    /// 最后一次成功读数，休眠时用于保留旧百分比。
    last: BatteryState,
}

impl GsKeyboardDriver {
    pub fn new() -> Self {
        Self {
            info: None,
            last: BatteryState::default(),
        }
    }

    pub fn detect_all(&mut self) -> Vec<DeviceSnapshot> {
        let devices = enumerate_hid_devices();
        let Some(target) = devices.into_iter().find(is_gs_keyboard) else {
            self.info = None;
            return Vec::new();
        };

        self.info = Some(target.clone());
        let battery = self.query_battery_internal(&target);
        let name = HidDevice::open(target.clone(), false)
            .ok()
            .and_then(|h| h.get_product_string())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "GANSS GS3108T-PRO".to_string());

        vec![DeviceSnapshot {
            id: format!("gs_keyboard_{:04x}", target.product_id),
            name,
            kind: DeviceKind::Keyboard,
            vendor_id: target.vendor_id,
            product_id: target.product_id,
            connection: ConnectionMode::Wireless24G,
            battery,
            path: target.path.clone(),
        }]
    }

    /// 发一次 0x20 查询并等待应答。键盘休眠时接收器不转发命令，此时
    /// 返回 `online = false` 并保留上一次百分比。
    fn query_battery_internal(&mut self, info: &HidDeviceInfo) -> BatteryState {
        let mut state = self.last.clone();
        state.online = false;
        state.charging = false;
        state.charging_source = ChargingSource::None;

        let Ok(device) = HidDevice::open(info.clone(), true) else {
            return state;
        };

        let mut cmd = [0u8; REPORT_LEN];
        cmd[1] = CMD_BATTERY;
        if device.write_overlapped(&cmd, Duration::from_millis(300)).is_err() {
            return state;
        }

        let start = Instant::now();
        let budget = Duration::from_millis(600);
        while start.elapsed() < budget {
            let mut buf = [0u8; 64];
            let remaining = budget - start.elapsed();
            match device.read_overlapped(&mut buf, remaining) {
                Ok(n) => {
                    // 键盘空闲时会推送全零报告，跳过并继续等应答。
                    if n >= REPORT_LEN {
                        if let Some(percentage) = parse_battery_response(&buf[..REPORT_LEN]) {
                            state.percentage = Some(percentage);
                            state.online = true;
                            state.last_updated = SystemTime::now();
                            self.last = state.clone();
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }

        state
    }
}

impl Default for GsKeyboardDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(state: u8, percentage: u8) -> Vec<u8> {
        let mut frame = vec![0u8; REPORT_LEN];
        frame[1] = CMD_BATTERY;
        frame[2] = state;
        frame[BATTERY_INDEX] = percentage;
        let checksum = frame[..REPORT_LEN - 1]
            .iter()
            .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        frame[REPORT_LEN - 1] = checksum;
        frame
    }

    #[test]
    fn test_parse_battery_response_reads_percentage() {
        // 与官方驱动实测帧一致：00 20 01 00 60 ... 81
        let frame = response(0x01, 0x60);
        assert_eq!(frame[REPORT_LEN - 1], 0x81);
        assert_eq!(parse_battery_response(&frame), Some(96));
    }

    #[test]
    fn test_parse_battery_response_rejects_bad_checksum() {
        let mut frame = response(0x00, 90);
        frame[REPORT_LEN - 1] ^= 0xFF;
        assert_eq!(parse_battery_response(&frame), None);
    }

    #[test]
    fn test_parse_battery_response_rejects_other_commands() {
        let mut frame = response(0x00, 90);
        frame[1] = 0x02;
        assert_eq!(parse_battery_response(&frame), None);
    }

    #[test]
    fn test_parse_battery_response_rejects_idle_frame() {
        assert_eq!(parse_battery_response(&[0u8; REPORT_LEN]), None);
    }

    #[test]
    fn test_parse_battery_response_rejects_out_of_range() {
        let frame = response(0x00, 101);
        assert_eq!(parse_battery_response(&frame), None);
    }
}
