use crate::devices::hid_win::{enumerate_hid_devices, HidDevice, HidDeviceInfo};
use crate::devices::types::{BatteryState, ChargingSource, ConnectionMode, DeviceKind, DeviceSnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

pub const XENTA_SAIGGI_VID: u16 = 0x1D57;
pub const ARESON_VID: u16 = 0x24AE;
pub const PIXART_VID: u16 = 0x093A;

pub const SAIGGI_PID_WIRELESS: u16 = 0xFA60;
pub const SAIGGI_PID_WIRED: u16 = 0xFA61;

pub struct SaiggiMouseDriver {
    pub col03_info: Option<HidDeviceInfo>,
    pub col04_info: Option<HidDeviceInfo>,
    pub latest_battery: Arc<Mutex<BatteryState>>,
    running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
}

impl SaiggiMouseDriver {
    pub fn new() -> Self {
        Self {
            col03_info: None,
            col04_info: None,
            latest_battery: Arc::new(Mutex::new(BatteryState::default())),
            running: Arc::new(AtomicBool::new(false)),
            worker_handle: None,
        }
    }

    pub fn detect(&mut self) -> Option<DeviceSnapshot> {
        let devices = enumerate_hid_devices();

        // 1. Prefer wired mouse if plugged in (PID 0xFA61)
        let col03_wired = devices.iter().find(|d| {
            (d.vendor_id == XENTA_SAIGGI_VID || d.vendor_id == ARESON_VID || d.vendor_id == PIXART_VID)
                && d.product_id == SAIGGI_PID_WIRED
                && d.usage_page == 0x000A
                && d.usage == 0x0000
        });

        // 2. Otherwise fall back to wireless receiver (PID 0xFA60) or any other compatible VID/PID
        let col03 = col03_wired.or_else(|| {
            devices.iter().find(|d| {
                (d.vendor_id == XENTA_SAIGGI_VID || d.vendor_id == ARESON_VID || d.vendor_id == PIXART_VID)
                    && d.usage_page == 0x000A
                    && d.usage == 0x0000
            })
        });

        if let Some(c03) = col03 {
            // Check if PID or path changed compared to current worker
            let changed = if let Some(prev) = &self.col03_info {
                prev.product_id != c03.product_id || prev.path != c03.path
            } else {
                false
            };

            if changed {
                self.running.store(false, Ordering::SeqCst);
                if let Some(h) = self.worker_handle.take() {
                    let _ = h.join();
                }
            }

            self.col03_info = Some(c03.clone());

            // Find COL04 feature endpoint matching the EXACT product_id!
            let col04 = devices.iter().find(|d| {
                d.vendor_id == c03.vendor_id
                    && d.product_id == c03.product_id
                    && d.usage_page == 0x000B
                    && d.usage == 0x0000
            });
            self.col04_info = col04.cloned();

            let conn = if c03.product_id == SAIGGI_PID_WIRELESS {
                ConnectionMode::Wireless24G
            } else {
                ConnectionMode::Wired
            };
            let hw_name = HidDevice::open(c03.clone(), false).ok().and_then(|h| h.get_product_string());
            let mouse_name = if let Some(hw) = hw_name {
                if !hw.trim().is_empty() && !hw.eq_ignore_ascii_case("USB Input Device") {
                    hw.trim().to_string()
                } else if c03.product_id == SAIGGI_PID_WIRELESS || c03.product_id == SAIGGI_PID_WIRED {
                    if conn == ConnectionMode::Wired {
                        "赛技 TM015 双模鼠标 (有线充电模式)".to_string()
                    } else {
                        "赛技 TM015 双模无线鼠标".to_string()
                    }
                } else {
                    format!("无线双模鼠标 (VID 0x{:04X} : PID 0x{:04X})", c03.vendor_id, c03.product_id)
                }
            } else if c03.product_id == SAIGGI_PID_WIRELESS || c03.product_id == SAIGGI_PID_WIRED {
                if conn == ConnectionMode::Wired {
                    "赛技 TM015 双模鼠标 (有线充电模式)".to_string()
                } else {
                    "赛技 TM015 双模无线鼠标".to_string()
                }
            } else {
                format!("无线双模鼠标 (VID 0x{:04X} : PID 0x{:04X})", c03.vendor_id, c03.product_id)
            };
            // Start background listener
            self.ensure_worker_running();

            // Send ping to query
            self.ping_device();

            let mut current_battery = {
                let guard = self.latest_battery.lock().unwrap();
                guard.clone()
            };
            if conn == ConnectionMode::Wired {
                current_battery.charging = true;
                current_battery.charging_source = ChargingSource::Charging;
                current_battery.online = true;
                if current_battery.percentage.is_none() {
                    current_battery.percentage = Some(100);
                }
            } else if current_battery.charging {
                current_battery.charging_source = ChargingSource::Charging;
            } else {
                current_battery.charging_source = ChargingSource::None;
            }
            return Some(DeviceSnapshot {
                id: format!("mouse_{:04x}_{:04x}", c03.vendor_id, c03.product_id),
                name: mouse_name,
                kind: DeviceKind::Mouse,
                vendor_id: c03.vendor_id,
                product_id: c03.product_id,
                connection: conn,
                battery: current_battery,
                path: c03.path.clone(),
            });
        }

        self.col03_info = None;
        self.col04_info = None;
        None
    }

    pub fn poll_battery(&mut self) -> Option<BatteryState> {
        // Detect if mouse plugged in wired or unplugged
        let should_redetect = if let Some(c03) = &self.col03_info {
            let devices = enumerate_hid_devices();
            if c03.product_id == SAIGGI_PID_WIRELESS {
                // Check if wired mouse appeared
                devices.iter().any(|d| d.vendor_id == c03.vendor_id && d.product_id == SAIGGI_PID_WIRED)
            } else {
                // Check if wired mouse unplugged
                !devices.iter().any(|d| d.path == c03.path)
            }
        } else {
            true
        };

        if should_redetect {
            if let Some(snap) = self.detect() {
                return Some(snap.battery);
            }
        }

        self.ping_device();

        let mut bat = {
            let guard = self.latest_battery.lock().unwrap();
            guard.clone()
        };
        if let Some(c03) = &self.col03_info {
            if c03.product_id != SAIGGI_PID_WIRELESS {
                bat.charging = true;
                bat.charging_source = ChargingSource::Charging;
                bat.online = true;
                if bat.percentage.is_none() {
                    bat.percentage = Some(100);
                }
            } else if bat.charging {
                bat.charging_source = ChargingSource::Charging;
            } else {
                bat.charging_source = ChargingSource::None;
            }
        }
        Some(bat)
    }

    pub fn ping_device(&self) {
        if let Some(c04) = &self.col04_info {
            if let Ok(dev) = HidDevice::open(c04.clone(), false) {
                let payload = [0x0Cu8, 0x0A, 0x01, 0xFE, 0x01, 0xFE, 0, 0, 0, 0];
                let buf_size = (c04.feature_len as usize).max(payload.len());
                let mut buf = vec![0u8; buf_size];
                buf[..payload.len()].copy_from_slice(&payload);
                let _ = dev.set_feature(&buf);
            }
        }
    }

    fn ensure_worker_running(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        let Some(c03_info) = self.col03_info.clone() else {
            return;
        };

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let battery_arc = Arc::clone(&self.latest_battery);

        let handle = thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match HidDevice::open(c03_info.clone(), true) {
                    Ok(dev) => {
                        let mut buf = [0u8; 64];
                        while running.load(Ordering::SeqCst) {
                            match dev.read_overlapped(&mut buf, Duration::from_millis(1500)) {
                                Ok(n) if n >= 5 => {
                                    let is_wired = c03_info.product_id != SAIGGI_PID_WIRELESS;
                                    if let Some(parsed) = parse_mouse_battery_report(&buf[..n], is_wired) {
                                        let mut state = battery_arc.lock().unwrap();
                                        *state = parsed;
                                    }
                                }
                                Ok(_) => {}
                                Err(_) => {}
                            }
                        }
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(2000));
                    }
                }
            }
        });

        self.worker_handle = Some(handle);
    }
}

/// Parses the Beken/Xenta report ID 3 battery packet: `[0x03, 0x10, 0x40, sub_type, level, ...]`
pub fn parse_mouse_battery_report(buf: &[u8], is_wired: bool) -> Option<BatteryState> {
    if buf.len() < 5 || buf[0] != 3 || buf[1] != 0x10 || buf[2] != 0x40 {
        return None;
    }
    // In Beken/Xenta 2.4G mouse protocol:
    // buf[3] subtype: 0x01 = normal discharging battery report; 0x02, 0x03, 0x80 = charging.
    let is_dock_charging = matches!(buf[3], 0x02 | 0x03 | 0x80)
        || (buf.len() >= 6 && buf[5] != 0)
        || (buf.len() >= 7 && buf[6] != 0)
        || (buf.len() >= 8 && buf[7] != 0);
    let charging = is_wired || is_dock_charging;
    let raw_lvl = buf[4];
    let pct = if (1..=10).contains(&raw_lvl) {
        raw_lvl * 10
    } else if raw_lvl == 0 && is_wired {
        100
    } else {
        raw_lvl.min(100)
    };
    Some(BatteryState {
        percentage: Some(pct),
        voltage_mv: None,
        charging,
        charging_source: if is_wired || charging {
            ChargingSource::Charging
        } else {
            ChargingSource::None
        },
        online: true,
        last_updated: SystemTime::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mouse_battery_report_discharging() {
        // Raw buffer captured from live TM015 mouse when discharging at 90%:
        let buf = [0x03, 0x10, 0x40, 0x01, 0x09];
        let state = parse_mouse_battery_report(&buf, false).expect("should parse");
        assert_eq!(state.percentage, Some(90));
        assert!(!state.charging, "Mouse must not be marked charging when subtype is 0x01 (discharge)");
        assert_eq!(state.charging_source, ChargingSource::None);
    }

    #[test]
    fn test_parse_mouse_battery_report_charging_dock() {
        let buf = [0x03, 0x10, 0x40, 0x02, 0x09];
        let state = parse_mouse_battery_report(&buf, false).expect("should parse");
        assert_eq!(state.percentage, Some(90));
        assert!(state.charging);
        assert_eq!(state.charging_source, ChargingSource::Charging);
    }

    #[test]
    fn test_parse_mouse_battery_report_wired() {
        let buf = [0x03, 0x10, 0x40, 0x01, 0x0A];
        let state = parse_mouse_battery_report(&buf, true).expect("should parse");
        assert!(state.charging);
        assert_eq!(state.charging_source, ChargingSource::Charging);
    }
}

impl Drop for SaiggiMouseDriver {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
