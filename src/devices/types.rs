use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Keypad,
    Headset,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMode {
    Wireless24G,
    Wired,
    Bluetooth,
    Unknown,
}

impl ConnectionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionMode::Wireless24G => "2.4G 无线",
            ConnectionMode::Wired => "有线连接",
            ConnectionMode::Bluetooth => "蓝牙连接",
            ConnectionMode::Unknown => "未知模式",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChargingSource {
    #[default]
    None,
    /// Charging / USB power input
    Charging,
}

impl ChargingSource {
    pub fn is_charging(&self) -> bool {
        matches!(self, ChargingSource::Charging)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ChargingSource::None => "未充电",
            ChargingSource::Charging => "充电中",
        }
    }

    /// Concise badge with flash icon, e.g. "⚡[充电中]"
    pub fn tag_label(&self, charging: bool) -> &'static str {
        if charging || self.is_charging() {
            "⚡[充电中]"
        } else {
            ""
        }
    }

    /// Detailed label with flash icon for status/tooltips
    pub fn display_badge(&self, charging: bool) -> &'static str {
        if charging || self.is_charging() {
            "⚡[充电中]"
        } else {
            ""
        }
    }

    /// Short badge with leading space for inline status printing
    pub fn status_mark(&self, charging: bool) -> &'static str {
        if charging || self.is_charging() {
            " ⚡[充电中]"
        } else {
            ""
        }
    }

    /// Short tray mark
    pub fn tray_mark(&self, charging: bool) -> &'static str {
        if charging || self.is_charging() {
            " ⚡[充电中]"
        } else {
            ""
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryState {
    /// Percentage 0..=100
    pub percentage: Option<u8>,
    /// Exact millivolts if supported (e.g. 4241 mV)
    pub voltage_mv: Option<u16>,
    /// Whether currently charging
    pub charging: bool,
    /// Source of charging power: PC USB or external adapter
    #[serde(default)]
    pub charging_source: ChargingSource,
    /// Whether device is awake and responding
    pub online: bool,
    /// Last successful poll time
    #[serde(skip, default = "SystemTime::now")]
    pub last_updated: SystemTime,
}

impl Default for BatteryState {
    fn default() -> Self {
        Self {
            percentage: None,
            voltage_mv: None,
            charging: false,
            charging_source: ChargingSource::None,
            online: false,
            last_updated: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub vendor_id: u16,
    pub product_id: u16,
    pub connection: ConnectionMode,
    pub battery: BatteryState,
    pub path: String,
}
