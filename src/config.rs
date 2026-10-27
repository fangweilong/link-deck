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

/// 停靠形态。
///
/// 除了原来的「贴边拉手 / 完整能量球」，再补三种常见形态：
/// 圆环进度、竖向胶囊、小徽章，覆盖不同桌面摆放习惯。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FloatingBallStyle {
    #[default]
    Tab,
    Orb,
    /// 圆环进度 + 中心百分比
    Ring,
    /// 竖向胶囊，顶部百分比 + 长条进度
    Pill,
    /// 小徽章：只有一个圆点与百分比，最不打扰
    Badge,
}

impl FloatingBallStyle {
    pub fn label(&self) -> &'static str {
        match self {
            FloatingBallStyle::Tab => "贴边拉手",
            FloatingBallStyle::Orb => "完整圆球",
            FloatingBallStyle::Ring => "圆环进度",
            FloatingBallStyle::Pill => "竖向胶囊",
            FloatingBallStyle::Badge => "小徽章",
        }
    }
    pub fn hint(&self) -> &'static str {
        match self {
            FloatingBallStyle::Tab => "只占 26px 的窄条",
            FloatingBallStyle::Orb => "大圆球，信息最多",
            FloatingBallStyle::Ring => "环形进度，读数直观",
            FloatingBallStyle::Pill => "竖向长条，贴边不挡内容",
            FloatingBallStyle::Badge => "最小尺寸，几乎无感",
        }
    }
}

/// 低电量提示音。`Voice` 是原有的内嵌语音（“饿死了”），其余为程序合成的短提示音。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AlertSound {
    #[default]
    Voice,
    /// 三音上行和弦，最“礼貌”
    Chime,
    /// 两声短促蜂鸣，最直接
    Beep,
    /// 低频脉冲，不吵但抓注意力
    Pulse,
}

impl AlertSound {
    pub fn label(&self) -> &'static str {
        match self {
            AlertSound::Voice => "语音提醒",
            AlertSound::Chime => "三音和弦",
            AlertSound::Beep => "短促蜂鸣",
            AlertSound::Pulse => "低频脉冲",
        }
    }
}

/// 界面风格（整套设计令牌的预设，不只是强调色）。
///
/// 旧版本只有「强调色」四选一，且只存不生效；这里升级为整套色板预设，
/// 旧配置值按最接近的风格迁移：橙→暖橙、蓝→冷岩、紫→极光、绿→竹青。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiStyle {
    #[default]
    #[serde(alias = "orange")]
    Ember,
    #[serde(alias = "blue")]
    Graphite,
    #[serde(alias = "purple")]
    Aurora,
    #[serde(alias = "green")]
    Jade,
    Snow,
    /// 用户自定义主色 + 明暗
    Custom,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Whether the edge-sticking floating ball is enabled
    pub enable_floating_ball: bool,
    /// Vertical position ratio (0.0 to 1.0 from top of screen)
    pub floating_ball_y_ratio: f32,
    /// Docked floating ball opacity in percent (40-100, lower = more see-through).
    /// The expanded drawer keeps its own readable floor regardless of this value.
    #[serde(default = "default_ball_opacity")]
    pub floating_ball_opacity: u8,
    /// Docked widget shape; hover never changes it (shape-swapping on hover used to flicker)
    #[serde(default)]
    pub floating_ball_style: FloatingBallStyle,
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
    /// 二次危险提醒阈值（默认 5，0 = 关闭）。
    /// 跌破前三次阈值后电量继续下降，会再弹一次 Critical，避免用户错过。
    #[serde(default = "default_second_threshold")]
    pub toast_threshold_second: u8,
    /// 低电量提示音（语音 / 合成音）
    #[serde(default)]
    pub alert_sound: AlertSound,
    /// Suppress topmost during fullscreen / gaming
    #[serde(default = "default_true")]
    pub toast_gaming_suppression: bool,
    /// Play an audible alarm together with the critical (<= 10%) toast
    #[serde(default = "default_true")]
    pub enable_low_battery_sound: bool,
    #[serde(default)]
    pub device_filter: DeviceFilter,
    #[serde(default)]
    pub device_sort: DeviceSort,
    /// 界面风格（整套色板预设）
    #[serde(default)]
    pub ui_style: UiStyle,
    /// 自定义配色的主色（0xRRGGBB），仅 `ui_style = custom` 时生效
    #[serde(default = "default_custom_accent")]
    pub custom_accent: u32,
    /// 自定义配色使用浅色底
    #[serde(default)]
    pub custom_light: bool,
    #[serde(default = "default_true")]
    pub show_onboarding: bool,
    /// 启动后自动检查 GitHub Releases 新版本
    #[serde(default = "default_true")]
    pub check_updates: bool,
    /// 开机（Windows 登录）时自动启动并常驻托盘
    #[serde(default)]
    pub auto_start: bool,
}

fn default_ball_opacity() -> u8 {
    80
}

/// Maps the user-facing 40-100% range onto the layered-window alpha channel and
/// clamps hand-edited config values so the widget can never become invisible.
pub fn opacity_to_alpha(percent: u8) -> u8 {
    let pct = percent.clamp(40, 100) as u32;
    ((pct * 255) / 100) as u8
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
    40
}
fn default_warning_threshold() -> u8 {
    30
}
fn default_critical_threshold() -> u8 {
    10
}
fn default_second_threshold() -> u8 {
    5
}
/// 默认自定义主色：暖橙，和默认风格一致
fn default_custom_accent() -> u32 {
    0xE8802A
}

/// 四档阈值的统一修正规则（设置页步进与配置落盘共用一份实现）。
///
/// 不变量：`info > warn > crit > second`，`second == 0` 表示关闭二次提醒。
pub fn normalize_thresholds(info: &mut u8, warn: &mut u8, crit: &mut u8, second: &mut u8) {
    *info = (*info).clamp(10, 90);
    *warn = (*warn).clamp(5, (*info).saturating_sub(1).max(5));
    *crit = (*crit).clamp(1, (*warn).saturating_sub(1).max(1));
    let max_second = (*crit).saturating_sub(1);
    *second = if *second == 0 || max_second == 0 {
        0
    } else {
        (*second).min(max_second)
    };
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enable_floating_ball: true, // Default: enabled on screen edge
            floating_ball_y_ratio: 0.75, // Lower-right area above taskbar
            floating_ball_opacity: default_ball_opacity(),
            floating_ball_style: FloatingBallStyle::Tab,
            poll_interval_ms: 300_000,   // Default: 5 minutes (300,000 ms)
            font_scale: 1.15,            // Clear, comfortably readable font scale
            panel_width: 640,
            panel_height: 680,
            ui_density: UiDensity::Standard,
            device_aliases: std::collections::HashMap::new(),
            enable_battery_toast: true,
            toast_threshold_info: 40,
            toast_threshold_warning: 30,
            toast_threshold_critical: 10,
            toast_threshold_second: 5,
            alert_sound: AlertSound::Voice,
            toast_gaming_suppression: true,
            enable_low_battery_sound: true,
            device_filter: DeviceFilter::All,
            device_sort: DeviceSort::Priority,
            ui_style: UiStyle::Ember,
            custom_accent: default_custom_accent(),
            custom_light: false,
            show_onboarding: true,
            check_updates: true,
            auto_start: false,
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

    /// 四档阈值必须严格递减：信息 > 警告 > 危险 > 二次提醒（0 表示关闭二次提醒）。
    /// 手改配置或连续点 − 都可能造出交叉，落盘前统一修正。
    pub fn normalize_toast_thresholds(&mut self) {
        normalize_thresholds(
            &mut self.toast_threshold_info,
            &mut self.toast_threshold_warning,
            &mut self.toast_threshold_critical,
            &mut self.toast_threshold_second,
        );
    }

    /// Docked ball opacity as a 0-255 layered-window alpha.
    pub fn ball_alpha(&self) -> u8 {
        opacity_to_alpha(self.floating_ball_opacity)
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
    fn test_auto_start_round_trip() {
        let mut cfg = AppConfig::default();
        assert!(!cfg.auto_start);
        cfg.auto_start = true;
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.auto_start);
    }

    #[test]
    fn test_dashboard_preferences_round_trip() {
        let mut cfg = AppConfig::default();
        cfg.device_filter = DeviceFilter::LowBattery;
        cfg.device_sort = DeviceSort::Updated;
        cfg.ui_style = UiStyle::Aurora;
        cfg.show_onboarding = false;
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.device_filter, DeviceFilter::LowBattery);
        assert_eq!(restored.device_sort, DeviceSort::Updated);
        assert_eq!(restored.ui_style, UiStyle::Aurora);
        assert!(!restored.show_onboarding);
    }

    #[test]
    fn legacy_accent_theme_migrates_to_style() {
        // 旧配置里的强调色必须迁移到最接近的风格，不能回退成默认值（暖橙）
        for (old, expected) in [
            ("\"orange\"", UiStyle::Ember),
            ("\"blue\"", UiStyle::Graphite),
            ("\"purple\"", UiStyle::Aurora),
            ("\"green\"", UiStyle::Jade),
        ] {
            // 只反序列化枚举本身：整表反序列化会要求其余字段，测不到别名迁移
            let style: UiStyle = serde_json::from_str(old).expect(old);
            assert_eq!(style, expected, "迁移失败: {}", old);
        }
    }

    #[test]
    fn ball_opacity_clamps_and_survives_old_configs() {
        // Hand-edited or legacy configs must never produce an invisible widget.
        assert_eq!(opacity_to_alpha(100), 255);
        assert_eq!(opacity_to_alpha(80), 204);
        assert_eq!(opacity_to_alpha(0), opacity_to_alpha(40));
        assert_eq!(opacity_to_alpha(250), 255);

        // Configs written before this field existed must fall back to the default.
        let mut json: serde_json::Value =
            serde_json::to_value(AppConfig::default()).unwrap();
        json.as_object_mut().unwrap().remove("floating_ball_opacity");
        let legacy: AppConfig = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.floating_ball_opacity, 80);
    }

    #[test]
    fn thresholds_never_cross_each_other() {
        // 手改配置成倒挂：修正后必须回到 info > warn > crit > second
        let (mut i, mut w, mut c, mut s) = (20, 60, 40, 30);
        normalize_thresholds(&mut i, &mut w, &mut c, &mut s);
        assert!(i > w && w > c && c > s, "修正后仍然倒挂: {i}/{w}/{c}/{s}");

        // 默认档位原样保留
        let (mut i, mut w, mut c, mut s) = (40, 30, 10, 5);
        normalize_thresholds(&mut i, &mut w, &mut c, &mut s);
        assert_eq!((i, w, c, s), (40, 30, 10, 5));

        // 0 = 关闭二次提醒，修正不能把它变成 1
        let (mut i, mut w, mut c, mut s) = (40, 30, 10, 0);
        normalize_thresholds(&mut i, &mut w, &mut c, &mut s);
        assert_eq!(s, 0);

        // 危险阈值被压到 1 时，二次提醒没有容身空间，只能关闭
        let (mut i, mut w, mut c, mut s) = (30, 10, 1, 5);
        normalize_thresholds(&mut i, &mut w, &mut c, &mut s);
        assert_eq!(s, 0);
    }

    #[test]
    fn config_normalize_matches_the_free_function() {
        let mut cfg = AppConfig::default();
        cfg.toast_threshold_info = 15;
        cfg.toast_threshold_warning = 40;
        cfg.toast_threshold_critical = 35;
        cfg.toast_threshold_second = 20;
        cfg.normalize_toast_thresholds();
        assert!(cfg.toast_threshold_info > cfg.toast_threshold_warning);
        assert!(cfg.toast_threshold_warning > cfg.toast_threshold_critical);
        assert!(cfg.toast_threshold_critical > cfg.toast_threshold_second);
    }

    #[test]
    fn new_alert_fields_survive_a_round_trip() {
        let mut cfg = AppConfig::default();
        cfg.alert_sound = AlertSound::Beep;
        cfg.toast_threshold_second = 3;
        cfg.custom_accent = 0x4C8DFF;
        cfg.custom_light = true;
        cfg.ui_style = UiStyle::Custom;
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.alert_sound, AlertSound::Beep);
        assert_eq!(back.toast_threshold_second, 3);
        assert_eq!(back.custom_accent, 0x4C8DFF);
        assert!(back.custom_light);
        assert_eq!(back.ui_style, UiStyle::Custom);
    }

    #[test]
    fn old_configs_without_new_fields_fall_back_to_defaults() {
        let mut json = serde_json::to_value(AppConfig::default()).unwrap();
        for key in ["toast_threshold_second", "alert_sound", "custom_accent", "custom_light"] {
            json.as_object_mut().unwrap().remove(key);
        }
        let legacy: AppConfig = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.toast_threshold_second, 5);
        assert_eq!(legacy.alert_sound, AlertSound::Voice);
        assert_eq!(legacy.custom_accent, 0xE8802A);
        assert!(!legacy.custom_light);
    }
}
