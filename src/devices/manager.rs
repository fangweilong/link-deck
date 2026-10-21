use crate::devices::bluetooth::BluetoothBatteryScanner;
use crate::devices::logitech::LogitechDriver;
use crate::devices::mchose::MchoseDriver;
use crate::devices::rk_keyboard::RkKeyboardDriver;
use crate::devices::saiggi_mouse::SaiggiMouseDriver;
use crate::devices::standard_hid::StandardHidBatteryDriver;
use crate::devices::vxe::VxeDriver;
use crate::devices::razer::RazerDriver;
use crate::devices::steelseries::SteelSeriesDriver;
use crate::devices::asus_rog::AsusRogDriver;
use crate::devices::corsair::CorsairDriver;
use crate::devices::types::DeviceSnapshot;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefreshResult {
    pub total: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub failed: usize,
}


pub struct DeviceManager {
    devices: Arc<RwLock<Vec<DeviceSnapshot>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,
    running: Arc<AtomicBool>,
    poll_interval_ms: Arc<AtomicU64>,
    refresh_requested: Arc<AtomicBool>,
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
            refresh_requested: Arc::new(AtomicBool::new(false)),
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
        let refresh_requested = Arc::clone(&self.refresh_requested);
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
            for razer_dev in RazerDriver::scan_devices() {
                push_unique(&mut current, razer_dev);
            }
            for ss_dev in SteelSeriesDriver::scan_devices() {
                push_unique(&mut current, ss_dev);
            }
            for asus_dev in AsusRogDriver::scan_devices() {
                push_unique(&mut current, asus_dev);
            }
            for corsair_dev in CorsairDriver::scan_devices() {
                push_unique(&mut current, corsair_dev);
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
                let razer_devices = RazerDriver::scan_devices();
                let ss_devices = SteelSeriesDriver::scan_devices();
                let asus_devices = AsusRogDriver::scan_devices();
                let corsair_devices = CorsairDriver::scan_devices();
                {
                    let mut list = devices_arc.write().unwrap();
                    let mut found_rk = false;
                    let mut found_mouse = false;

                    for dev in list.iter_mut() {
                        if dev.id.contains("rk_keyboard") {
                            found_rk = true;
                            if let Some(b) = &rk_bat {
                                let old_pct = dev.battery.percentage;
                                dev.battery = b.clone();
                                if dev.battery.percentage.is_none() && old_pct.is_some() {
                                    dev.battery.percentage = old_pct;
                                }
                            }
                        } else if dev.id.contains("mouse_") {
                            found_mouse = true;
                            if let Some(b) = &mouse_bat {
                                let old_pct = dev.battery.percentage;
                                dev.battery = b.clone();
                                if dev.battery.percentage.is_none() && old_pct.is_some() {
                                    dev.battery.percentage = old_pct;
                                }
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

                    // Sync Razer devices
                    for razer_dev in razer_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == razer_dev.id) {
                            existing.battery = razer_dev.battery;
                        } else {
                            list.push(razer_dev);
                        }
                    }

                    // Sync SteelSeries devices
                    for ss_dev in ss_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == ss_dev.id) {
                            existing.battery = ss_dev.battery;
                        } else {
                            list.push(ss_dev);
                        }
                    }

                    // Sync ASUS ROG devices
                    for asus_dev in asus_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == asus_dev.id) {
                            existing.battery = asus_dev.battery;
                        } else {
                            list.push(asus_dev);
                        }
                    }

                    // Sync Corsair devices
                    for corsair_dev in corsair_devices {
                        if let Some(existing) = list.iter_mut().find(|d| d.id == corsair_dev.id) {
                            existing.battery = corsair_dev.battery;
                        } else {
                            list.push(corsair_dev);
                        }
                    }
                }
                let target_ms = poll_interval.load(Ordering::Relaxed).max(500);
                let slice = 100u64;
                let mut elapsed = 0u64;
                while elapsed < target_ms && running.load(Ordering::SeqCst) {
                    if refresh_requested.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(slice));
                    elapsed += slice;

                    // Real-time synchronization of background mouse reports:
                    let mouse_latest = mouse.get_latest_battery();
                    let was_recent = mouse_latest.online && mouse_latest.last_updated.elapsed().map(|d| d < Duration::from_secs(15)).unwrap_or(false);
                    if was_recent {
                        let mut list = devices_arc.write().unwrap();
                        if let Some(dev) = list.iter_mut().find(|d| d.id.starts_with("mouse_")) {
                            if mouse_latest.percentage.is_some() && dev.battery.percentage != mouse_latest.percentage {
                                dev.battery.percentage = mouse_latest.percentage;
                                dev.battery.online = true;
                                dev.battery.charging = mouse_latest.charging;
                                dev.battery.charging_source = mouse_latest.charging_source;
                                dev.battery.last_updated = mouse_latest.last_updated;
                            } else if !dev.battery.online {
                                dev.battery.online = true;
                                dev.battery.last_updated = mouse_latest.last_updated;
                            }
                        }
                    } else {
                        let mut list = devices_arc.write().unwrap();
                        if let Some(dev) = list.iter_mut().find(|d| d.id.starts_with("mouse_")) {
                            if dev.connection == crate::devices::types::ConnectionMode::Wireless24G && dev.battery.online {
                                dev.battery.online = false;
                            }
                        }
                    }

                    let current_target = poll_interval.load(Ordering::Relaxed).max(500);
                    if elapsed >= current_target {
                        break;
                    }
                }
                refresh_requested.store(false, Ordering::SeqCst);
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
        if self.running.load(Ordering::SeqCst) {
            // Signal background worker thread with live driver instances to poll immediately
            self.refresh_requested.store(true, Ordering::SeqCst);
            for _ in 0..30 {
                thread::sleep(Duration::from_millis(10));
                if !self.refresh_requested.load(Ordering::SeqCst) {
                    break;
                }
            }
        } else {
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
            for razer_dev in RazerDriver::scan_devices() {
                push_unique(&mut updated, razer_dev);
            }
            for ss_dev in SteelSeriesDriver::scan_devices() {
                push_unique(&mut updated, ss_dev);
            }
            for asus_dev in AsusRogDriver::scan_devices() {
                push_unique(&mut updated, asus_dev);
            }
            for corsair_dev in CorsairDriver::scan_devices() {
                push_unique(&mut updated, corsair_dev);
            }

            let mut list = self.devices.write().unwrap();
            for dev in updated {
                if let Some(existing) = list.iter_mut().find(|d| d.id == dev.id) {
                    let old_pct = existing.battery.percentage;
                    existing.battery = dev.battery;
                    if existing.battery.percentage.is_none() && old_pct.is_some() && existing.battery.online {
                        existing.battery.percentage = old_pct;
                    }
                } else {
                    list.push(dev);
                }
            }
        }
    }

    pub fn refresh_with_result(&self) -> RefreshResult {
        let before = self.get_snapshots();
        self.refresh();
        let after = self.get_snapshots();
        let added = after.iter().filter(|dev| !before.iter().any(|old| old.id == dev.id)).count();
        let removed = before.iter().filter(|old| !after.iter().any(|dev| dev.id == old.id)).count();
        let changed = after.iter().filter(|dev| {
            before.iter().find(|old| old.id == dev.id).map(|old| {
                old.battery.percentage != dev.battery.percentage
                    || old.battery.online != dev.battery.online
                    || old.battery.charging != dev.battery.charging
                    || old.battery.voltage_mv != dev.battery.voltage_mv
            }).unwrap_or(false)
        }).count();
        RefreshResult { total: after.len(), added, removed, changed, failed: 0 }
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
