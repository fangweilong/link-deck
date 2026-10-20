use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    /// Custom device aliases by device id, e.g. {"rk_keyboard_fa70": "我的客制化 RK108"}
    #[serde(default)]
    pub device_aliases: std::collections::HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enable_floating_ball: true, // Default: enabled on screen edge
            floating_ball_y_ratio: 0.75, // Lower-right area above taskbar
            poll_interval_ms: 2500,
            font_scale: 1.15, // Clear, comfortably readable font scale
            device_aliases: std::collections::HashMap::new(),
        }
    }
}

impl AppConfig {
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
}
