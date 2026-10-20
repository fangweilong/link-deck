use crate::devices::bluetooth::BluetoothBatteryScanner;
use crate::devices::logitech::LogitechDriver;
use crate::devices::mchose::MchoseDriver;
use crate::devices::rk_keyboard::RkKeyboardDriver;
use crate::devices::saiggi_mouse::SaiggiMouseDriver;
use crate::devices::standard_hid::StandardHidBatteryDriver;
use crate::devices::vxe::VxeDriver;
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
                push_unique(&mut current, d);
            }
            if let Some(d) = mouse.detect() {
                push_unique(&mut current, d);
            }
            for bt_dev in BluetoothBatteryScanner::scan_devices() {
                push_unique(&mut current, bt_dev);
            }
            for hid_dev in StandardHidBatteryDriver::scan_devices() {
                push_unique(&mut current, hid_dev);
            }
            for logi_dev in LogitechDriver::scan_devices() {
                push_unique(&mut current, logi_dev);
            }
            for mchose_dev in MchoseDriver::scan_devices() {
                push_unique(&mut current, mchose_dev);
            }
            for vxe_dev in VxeDriver::scan_devices() {
                push_unique(&mut current, vxe_dev);
            }
            *devices_arc.write().unwrap() = current;

            while running.load(Ordering::SeqCst) {
                // Poll Keyboard
                let rk_bat = rk.poll_battery();

                // Poll Mouse
                let mouse_bat = mouse.poll_battery();

                // Poll Bluetooth devices
                let bt_devices = BluetoothBatteryScanner::scan_devices();
                let logi_devices = LogitechDriver::scan_devices();
                let mchose_devices = MchoseDriver::scan_devices();
                let vxe_devices = VxeDriver::scan_devices();
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
                                dev.id = format!("mouse_{:04x}_{:04x}", c03.vendor_id, c03.product_id);
                                dev.vendor_id = c03.vendor_id;
                                dev.product_id = c03.product_id;
                                dev.path = c03.path.clone();
                                if c03.product_id == crate::devices::saiggi_mouse::SAIGGI_PID_WIRED {
                                    dev.connection = crate::devices::types::ConnectionMode::Wired;
                                    dev.battery.charging = true;
                                    dev.battery.charging_source = crate::devices::types::ChargingSource::Charging;
                                    dev.battery.online = true;
                                    if dev.battery.percentage.is_none() {
                                        dev.battery.percentage = Some(100);
                                    }
                                } else {
                                    dev.connection = crate::devices::types::ConnectionMode::Wireless24G;
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

                    // Sync Bluetooth devices: remove disconnected, update existing, add newly connected
                    list.retain(|d| {
                        if d.connection == crate::devices::types::ConnectionMode::Bluetooth {
                            bt_devices.iter().any(|b| b.id == d.id)
                        } else {
                            true
                        }
                    });
                    for bt_dev in bt_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == bt_dev.id) {
                            existing.battery = bt_dev.battery;
                        } else {
                            list.push(bt_dev);
                        }
                    }

                    // Sync Logitech devices
                    for logi_dev in logi_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == logi_dev.id) {
                            existing.battery = logi_dev.battery;
                        } else {
                            list.push(logi_dev);
                        }
                    }

                    // Sync MCHOSE devices
                    for mchose_dev in mchose_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == mchose_dev.id) {
                            existing.battery = mchose_dev.battery;
                        } else {
                            list.push(mchose_dev);
                        }
                    }

                    // Sync VXE / VGN devices
                    for vxe_dev in vxe_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == vxe_dev.id) {
                            existing.battery = vxe_dev.battery;
                        } else {
                            list.push(vxe_dev);
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
            if let Some(alias) = resolve_alias(&aliases_guard, dev) {
                dev.name = alias.to_string();
            }
        }
        snaps
    }
    pub fn refresh(&self) {
        let mut rk = RkKeyboardDriver::new();
        let mut mouse = SaiggiMouseDriver::new();
        let mut updated = Vec::new();

        if let Some(d) = rk.detect() {
            push_unique(&mut updated, d);
        }
        if let Some(d) = mouse.detect() {
            push_unique(&mut updated, d);
        }
        for bt_dev in BluetoothBatteryScanner::scan_devices() {
            push_unique(&mut updated, bt_dev);
        }
        for hid_dev in StandardHidBatteryDriver::scan_devices() {
            push_unique(&mut updated, hid_dev);
        }
        for logi_dev in LogitechDriver::scan_devices() {
            push_unique(&mut updated, logi_dev);
        }
        for mchose_dev in MchoseDriver::scan_devices() {
            push_unique(&mut updated, mchose_dev);
        }
        for vxe_dev in VxeDriver::scan_devices() {
            push_unique(&mut updated, vxe_dev);
        }

        *self.devices.write().unwrap() = updated;
    }
}

fn push_unique(list: &mut Vec<DeviceSnapshot>, dev: DeviceSnapshot) {
    if !list.iter().any(|d| d.id == dev.id || (!dev.path.is_empty() && d.path == dev.path)) {
        list.push(dev);
    }
}

impl Drop for DeviceManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Resolves alias with exact match, dual-mode paired ID fallback, or device family fallback.
pub fn resolve_alias<'a>(aliases: &'a HashMap<String, String>, dev: &DeviceSnapshot) -> Option<&'a str> {
    // 1. Exact id match
    if let Some(alias) = aliases.get(&dev.id) {
        if !alias.trim().is_empty() {
            return Some(alias.trim());
        }
    }

    // 2. Dual-mode mouse paired ID fallback (wireless dongle 0xFA60 <-> wired USB 0xFA61)
    if dev.id == "mouse_1d57_fa60" {
        if let Some(alias) = aliases.get("mouse_1d57_fa61") {
            if !alias.trim().is_empty() {
                return Some(alias.trim());
            }
        }
    } else if dev.id == "mouse_1d57_fa61" {
        if let Some(alias) = aliases.get("mouse_1d57_fa60") {
            if !alias.trim().is_empty() {
                return Some(alias.trim());
            }
        }
    }

    // 3. RK keyboard wireless dongle <-> wired fallback
    if dev.id.starts_with("rk_keyboard_") {
        for (k, v) in aliases {
            if k.starts_with("rk_keyboard_") && !v.trim().is_empty() {
                return Some(v.trim());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::types::{BatteryState, ConnectionMode, DeviceKind};

    fn dummy_dev(id: &str, name: &str) -> DeviceSnapshot {
        DeviceSnapshot {
            id: id.to_string(),
            name: name.to_string(),
            kind: DeviceKind::Mouse,
            vendor_id: 0x1D57,
            product_id: 0xFA60,
            connection: ConnectionMode::Wireless24G,
            battery: BatteryState::default(),
            path: String::new(),
        }
    }

    #[test]
    fn test_resolve_alias_exact() {
        let mut aliases = HashMap::new();
        aliases.insert("mouse_1d57_fa60".to_string(), "我的无线鼠标".to_string());
        let dev = dummy_dev("mouse_1d57_fa60", "原名称");
        assert_eq!(resolve_alias(&aliases, &dev), Some("我的无线鼠标"));
    }

    #[test]
    fn test_resolve_alias_paired_mouse() {
        let mut aliases = HashMap::new();
        aliases.insert("mouse_1d57_fa60".to_string(), "我的双模鼠标".to_string());
        // Mouse is plugged in wired mode with PID 0xFA61
        let dev_wired = dummy_dev("mouse_1d57_fa61", "原名称");
        assert_eq!(resolve_alias(&aliases, &dev_wired), Some("我的双模鼠标"));
    }

    #[test]
    fn test_resolve_alias_keyboard_fallback() {
        let mut aliases = HashMap::new();
        aliases.insert("rk_keyboard_fa70".to_string(), "我的客制化键盘".to_string());
        let mut dev_wired = dummy_dev("rk_keyboard_01c6", "原键盘");
        dev_wired.kind = DeviceKind::Keyboard;
        assert_eq!(resolve_alias(&aliases, &dev_wired), Some("我的客制化键盘"));
    }
}
