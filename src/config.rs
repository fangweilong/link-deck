use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiDensity {
    Compact,
    #[default]
    Standard,
    Spacious,
}

impl UiDensity {
    pub fn as_str(&self) -> &'static str {
        match self {
            UiDensity::Compact => "紧凑",
            UiDensity::Standard => "标准",
            UiDensity::Spacious => "宽松",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceFilter {
    #[default]
    All,
    LowBattery,
    Charging,
    Bluetooth,
    Wireless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceSort {
    #[default]
    Priority,
    Name,
    Battery,
    Updated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentTheme {
    #[default]
    Orange,
    Blue,
    Purple,
    Green,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Whether the edge-sticking floating ball is enabled
    pub enable_floating_ball: bool,
    /// Vertical position ratio (0.0 to 1.0 from top of screen)
    pub floating_ball_y_ratio: f32,
    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,
    /// UI Font size scale factor (1.0 = standard, 1.15 = medium, 1.3 = large)
    pub font_scale: f32,
    /// Management panel window width (default: 640)
    #[serde(default = "default_panel_width")]
    pub panel_width: i32,
    /// Management panel window height (default: 680)
    #[serde(default = "default_panel_height")]
    pub panel_height: i32,
    /// UI layout density (Compact, Standard, Spacious)
    #[serde(default)]
    pub ui_density: UiDensity,
    /// Custom device aliases by device id, e.g. {"rk_keyboard_fa70": "我的客制化 RK108"}
    #[serde(default)]
    pub device_aliases: std::collections::HashMap<String, String>,
    /// Whether low-battery toast notifications are enabled
    #[serde(default = "default_true")]
    pub enable_battery_toast: bool,
    /// Threshold for battery info notification (default: 50)
    #[serde(default = "default_info_threshold")]
    pub toast_threshold_info: u8,
    /// Threshold for battery warning notification (default: 30)
    #[serde(default = "default_warning_threshold")]
    pub toast_threshold_warning: u8,
    /// Threshold for battery critical notification (default: 10)
    #[serde(default = "default_critical_threshold")]
    pub toast_threshold_critical: u8,
    /// Suppress topmost during fullscreen / gaming
    #[serde(default = "default_true")]
    pub toast_gaming_suppression: bool,
    #[serde(default)]
    pub device_filter: DeviceFilter,
    #[serde(default)]
    pub device_sort: DeviceSort,
    #[serde(default)]
    pub accent_theme: AccentTheme,
    #[serde(default = "default_true")]
    pub show_onboarding: bool,
    /// 启动后自动检查 GitHub Releases 新版本
    #[serde(default = "default_true")]
    pub check_updates: bool,
}

fn default_panel_width() -> i32 {
    640
}
fn default_panel_height() -> i32 {
    680
}

fn default_true() -> bool {
    true
}
fn default_info_threshold() -> u8 {
    50
}
fn default_warning_threshold() -> u8 {
    30
}
fn default_critical_threshold() -> u8 {
    10
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enable_floating_ball: true, // Default: enabled on screen edge
            floating_ball_y_ratio: 0.75, // Lower-right area above taskbar
            poll_interval_ms: 300_000,   // Default: 5 minutes (300,000 ms)
            font_scale: 1.15,            // Clear, comfortably readable font scale
            panel_width: 640,
            panel_height: 680,
            ui_density: UiDensity::Standard,
            device_aliases: std::collections::HashMap::new(),
            enable_battery_toast: true,
            toast_threshold_info: 50,
            toast_threshold_warning: 30,
            toast_threshold_critical: 10,
            toast_gaming_suppression: true,
            device_filter: DeviceFilter::All,
            device_sort: DeviceSort::Priority,
            accent_theme: AccentTheme::Orange,
            show_onboarding: true,
            check_updates: true,
        }
    }
}
impl AppConfig {
    pub fn set_panel_size(&mut self, width: i32, height: i32) {
        self.panel_width = width.max(480);
        self.panel_height = height.max(400);
    }

    pub fn set_ui_density(&mut self, density: UiDensity) {
        self.ui_density = density;
    }

    /// Standard user configuration directory: `%APPDATA%\LinkDeck` on Windows,
    /// or `~/.config/linkdeck` as cross-platform fallback.
    pub fn config_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata).join("LinkDeck");
            }
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            if !profile.trim().is_empty() {
                return PathBuf::from(profile).join(".config").join("linkdeck");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.trim().is_empty() {
                return PathBuf::from(home).join(".config").join("linkdeck");
            }
        }
        PathBuf::from(".")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let target_path = Self::config_path();

        // 1. Primary path in user directory (%APPDATA%\LinkDeck\config.json)
        if target_path.exists() {
            if let Ok(content) = fs::read_to_string(&target_path) {
                if let Ok(cfg) = serde_json::from_str(&content) {
                    return cfg;
                }
            }
        }

        // 2. Alternate name in user directory (%APPDATA%\LinkDeck\linkdeck_config.json)
        let alt_target = Self::config_dir().join("linkdeck_config.json");
        if alt_target.exists() {
            if let Ok(content) = fs::read_to_string(&alt_target) {
                if let Ok(cfg) = serde_json::from_str::<Self>(&content) {
                    let _ = cfg.save();
                    return cfg;
                }
            }
        }

        // 3. Seamless migration from legacy path next to exe (./linkdeck_config.json)
        let legacy_path = PathBuf::from("linkdeck_config.json");
        if legacy_path.exists() {
            if let Ok(content) = fs::read_to_string(&legacy_path) {
                if let Ok(cfg) = serde_json::from_str::<Self>(&content) {
                    if cfg.save().is_ok() {
                        // Successfully migrated to user directory, clean up legacy file next to exe
                        let _ = fs::remove_file(&legacy_path);
                    }
                    return cfg;
                }
            }
        }

        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;
        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self).unwrap_or_default();
        fs::write(path, content)
    }
    pub fn get_alias<'a>(&'a self, device_id: &str) -> Option<&'a str> {
        self.device_aliases.get(device_id).map(|s| s.as_str())
    }

    pub fn set_alias(&mut self, device_id: &str, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            self.device_aliases.remove(device_id);
            if device_id == "mouse_1d57_fa60" {
                self.device_aliases.remove("mouse_1d57_fa61");
            } else if device_id == "mouse_1d57_fa61" {
                self.device_aliases.remove("mouse_1d57_fa60");
            }
        } else {
            self.device_aliases.insert(device_id.to_string(), trimmed.to_string());
            if device_id == "mouse_1d57_fa60" {
                self.device_aliases.insert("mouse_1d57_fa61".to_string(), trimmed.to_string());
            } else if device_id == "mouse_1d57_fa61" {
                self.device_aliases.insert("mouse_1d57_fa60".to_string(), trimmed.to_string());
            }
        }
    }

    pub fn resolve_alias<'a>(&'a self, device_id: &str) -> Option<&'a str> {
        if let Some(alias) = self.device_aliases.get(device_id) {
            if !alias.trim().is_empty() {
                return Some(alias.trim());
            }
        }
        if device_id == "mouse_1d57_fa60" {
            if let Some(alias) = self.device_aliases.get("mouse_1d57_fa61") {
                if !alias.trim().is_empty() {
                    return Some(alias.trim());
                }
            }
        } else if device_id == "mouse_1d57_fa61" {
            if let Some(alias) = self.device_aliases.get("mouse_1d57_fa60") {
                if !alias.trim().is_empty() {
                    return Some(alias.trim());
                }
            }
        }
        if device_id.starts_with("rk_keyboard_") {
            for (k, v) in &self.device_aliases {
                if k.starts_with("rk_keyboard_") && !v.trim().is_empty() {
                    return Some(v.trim());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        let dir = AppConfig::config_dir();
        let path = AppConfig::config_path();
        assert!(dir.to_string_lossy().contains("LinkDeck") || dir.to_string_lossy().contains("linkdeck"));
        assert_eq!(path, dir.join("config.json"));
    }

    #[test]
    fn test_config_serialization() {
        let mut cfg = AppConfig::default();
        cfg.device_aliases.insert("test_dev".to_string(), "测试键盘".to_string());
        let json = serde_json::to_string(&cfg).expect("serialize");
        let deserialized: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.get_alias("test_dev"), Some("测试键盘"));
    }

    #[test]
    fn test_config_set_alias_syncs_dual_mode_mouse() {
        let mut cfg = AppConfig::default();
        cfg.set_alias("mouse_1d57_fa60", "我的双模鼠标");
        assert_eq!(cfg.resolve_alias("mouse_1d57_fa60"), Some("我的双模鼠标"));
        assert_eq!(cfg.resolve_alias("mouse_1d57_fa61"), Some("我的双模鼠标"));

        cfg.set_alias("mouse_1d57_fa60", "");
        assert_eq!(cfg.resolve_alias("mouse_1d57_fa60"), None);
        assert_eq!(cfg.resolve_alias("mouse_1d57_fa61"), None);
    }
    #[test]
    fn test_config_ui_density_and_window_size() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.panel_width, 640);
        assert_eq!(cfg.panel_height, 680);
        assert_eq!(cfg.ui_density, UiDensity::Standard);
        cfg.set_panel_size(820, 760);
        cfg.set_ui_density(UiDensity::Compact);
        let json = serde_json::to_string(&cfg).expect("serialize");
        let deserialized: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.panel_width, 820);
        assert_eq!(deserialized.panel_height, 760);
        assert_eq!(deserialized.ui_density, UiDensity::Compact);
    }

    #[test]
    fn test_dashboard_preferences_round_trip() {
        let mut cfg = AppConfig::default();
        cfg.device_filter = DeviceFilter::LowBattery;
        cfg.device_sort = DeviceSort::Updated;
        cfg.accent_theme = AccentTheme::Blue;
        cfg.show_onboarding = false;
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.device_filter, DeviceFilter::LowBattery);
        assert_eq!(restored.device_sort, DeviceSort::Updated);
        assert_eq!(restored.accent_theme, AccentTheme::Blue);
        assert!(!restored.show_onboarding);
    }
}
