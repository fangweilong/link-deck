use crate::devices::bluetooth::BluetoothBatteryScanner;
use crate::devices::rk_keyboard::RkKeyboardDriver;
use crate::devices::saiggi_mouse::SaiggiMouseDriver;
use crate::devices::standard_hid::StandardHidBatteryDriver;
use crate::devices::types::DeviceSnapshot;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::collections::HashMap;

pub struct DeviceManager {
    devices: Arc<RwLock<Vec<DeviceSnapshot>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,
    running: Arc<AtomicBool>,
    poll_interval_ms: Arc<AtomicU64>,
    poller_handle: Option<JoinHandle<()>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self::with_interval(3000)
    }

    pub fn with_interval(initial_interval_ms: u64) -> Self {
        Self {
            devices: Arc::new(RwLock::new(Vec::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            poll_interval_ms: Arc::new(AtomicU64::new(initial_interval_ms.max(500))),
            poller_handle: None,
        }
    }

    pub fn set_poll_interval_ms(&self, ms: u64) {
        self.poll_interval_ms.store(ms.max(500), Ordering::SeqCst);
    }

    pub fn get_poll_interval_ms(&self) -> u64 {
        self.poll_interval_ms.load(Ordering::Relaxed)
    }

    pub fn set_aliases(&self, aliases: HashMap<String, String>) {
        let mut guard = self.aliases.write().unwrap();
        *guard = aliases;
    }

    pub fn start(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let devices_arc = Arc::clone(&self.devices);
        let poll_interval = Arc::clone(&self.poll_interval_ms);

        let handle = thread::spawn(move || {
            let mut rk = RkKeyboardDriver::new();
            let mut mouse = SaiggiMouseDriver::new();

            // Initial detection
            let mut current = Vec::new();
            if let Some(d) = rk.detect() {
                current.push(d);
            }
            if let Some(d) = mouse.detect() {
                current.push(d);
            }
            for bt_dev in BluetoothBatteryScanner::scan_devices() {
                current.push(bt_dev);
            }
            for hid_dev in StandardHidBatteryDriver::scan_devices() {
                current.push(hid_dev);
            }
            *devices_arc.write().unwrap() = current;

            while running.load(Ordering::SeqCst) {
                // Poll Keyboard
                let rk_bat = rk.poll_battery();

                // Poll Mouse
                let mouse_bat = mouse.poll_battery();

                // Poll Bluetooth devices
                let bt_devices = BluetoothBatteryScanner::scan_devices();

                {
                    let mut list = devices_arc.write().unwrap();
                    let mut found_rk = false;
                    let mut found_mouse = false;

                    for dev in list.iter_mut() {
                        if dev.id.contains("rk_keyboard") {
                            found_rk = true;
                            if let Some(b) = &rk_bat {
                                dev.battery = b.clone();
                            }
                        } else if dev.id.contains("mouse_") {
                            found_mouse = true;
                            if let Some(b) = &mouse_bat {
                                dev.battery = b.clone();
                            }
                            if let Some(c03) = &mouse.col03_info {
                                if c03.product_id == crate::devices::saiggi_mouse::SAIGGI_PID_WIRED {
                                    dev.connection = crate::devices::types::ConnectionMode::Wired;
                                    dev.name = "赛技 TM015 双模鼠标 (有线充电模式)".to_string();
                                    dev.battery.charging = true;
                                    dev.battery.charging_source = crate::devices::types::ChargingSource::Charging;
                                    dev.battery.online = true;
                                    if dev.battery.percentage.is_none() {
                                        dev.battery.percentage = Some(100);
                                    }
                                } else {
                                    dev.connection = crate::devices::types::ConnectionMode::Wireless24G;
                                    dev.name = "赛技 TM015 双模无线鼠标".to_string();
                                    if dev.battery.charging {
                                        dev.battery.charging_source = crate::devices::types::ChargingSource::Charging;
                                    } else {
                                        dev.battery.charging_source = crate::devices::types::ChargingSource::None;
                                    }
                                }
                            }
                        }
                        if dev.connection == crate::devices::types::ConnectionMode::Wired {
                            dev.battery.charging = true;
                            dev.battery.charging_source = crate::devices::types::ChargingSource::Charging;
                            dev.battery.online = true;
                            if dev.battery.percentage.is_none() {
                                dev.battery.percentage = Some(100);
                            }
                        } else if dev.battery.charging {
                            dev.battery.charging_source = crate::devices::types::ChargingSource::Charging;
                        } else {
                            dev.battery.charging_source = crate::devices::types::ChargingSource::None;
                        }
                    }

                    // Redetect if missing
                    if !found_rk {
                        if let Some(d) = rk.detect() {
                            list.push(d);
                        }
                    }
                    if !found_mouse {
                        if let Some(d) = mouse.detect() {
                            list.push(d);
                        }
                    }

                    // Sync Bluetooth devices
                    for bt_dev in bt_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == bt_dev.id) {
                            existing.battery = bt_dev.battery;
                        } else {
                            list.push(bt_dev);
                        }
                    }
                }
                let target_ms = poll_interval.load(Ordering::Relaxed).max(500);
                let slice = 100u64;
                let mut elapsed = 0u64;
                while elapsed < target_ms && running.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(slice));
                    elapsed += slice;
                    let current_target = poll_interval.load(Ordering::Relaxed).max(500);
                    if elapsed >= current_target {
                        break;
                    }
                }
            }
        });

        self.poller_handle = Some(handle);
    }

    pub fn get_snapshots(&self) -> Vec<DeviceSnapshot> {
        let guard = self.devices.read().unwrap();
        let aliases_guard = self.aliases.read().unwrap();
        let mut snaps = guard.clone();
        for dev in snaps.iter_mut() {
            if let Some(alias) = aliases_guard.get(&dev.id) {
                if !alias.trim().is_empty() {
                    dev.name = alias.trim().to_string();
                }
            }
        }
        snaps
    }
    pub fn refresh(&self) {
        let mut rk = RkKeyboardDriver::new();
        let mut mouse = SaiggiMouseDriver::new();
        let mut updated = Vec::new();

        if let Some(d) = rk.detect() {
            updated.push(d);
        }
        if let Some(d) = mouse.detect() {
            updated.push(d);
        }
        for bt_dev in BluetoothBatteryScanner::scan_devices() {
            updated.push(bt_dev);
        }
        for hid_dev in StandardHidBatteryDriver::scan_devices() {
            updated.push(hid_dev);
        }

        *self.devices.write().unwrap() = updated;
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
