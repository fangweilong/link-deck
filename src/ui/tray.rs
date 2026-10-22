use std::mem;
use std::ptr;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RETURNCMD, WM_APP,
};

pub const WM_TRAY_CALLBACK: u32 = WM_APP + 100;

pub const IDM_SHOW_PANEL: usize = 1001;
pub const IDM_SHOW_SETTINGS: usize = 1005;
pub const IDM_TOGGLE_BALL: usize = 1002;
pub const IDM_REFRESH: usize = 1003;
pub const IDM_FONT_SCALE_100: usize = 1010;
pub const IDM_FONT_SCALE_115: usize = 1011;
pub const IDM_FONT_SCALE_130: usize = 1012;
pub const IDM_POLL_30S: usize = 1020;
pub const IDM_POLL_1M: usize = 1021;
pub const IDM_POLL_5M: usize = 1022;
pub const IDM_POLL_15M: usize = 1023;
pub const IDM_POLL_30M: usize = 1024;
pub const IDM_POLL_60M: usize = 1025;
pub const IDM_EXIT: usize = 1004;
pub const IDM_CHECK_UPDATE: usize = 1030;
pub const IDM_APPLY_UPDATE: usize = 1031;

pub struct TrayIcon {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
}

impl TrayIcon {
    pub fn new(hwnd: HWND, tooltip: &str) -> Self {
        unsafe {
            let mut nid: NOTIFYICONDATAW = mem::zeroed();
            nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY_CALLBACK;
            let custom_icon = crate::ui::icon::create_linkdeck_icon(32);
            nid.hIcon = if !custom_icon.is_null() {
                custom_icon
            } else {
                LoadIconW(ptr::null_mut(), IDI_APPLICATION)
            };
            Self::copy_tooltip(&mut nid.szTip, tooltip);

            Shell_NotifyIconW(NIM_ADD, &nid);

            Self { hwnd, nid }
        }
    }

    pub fn update_tooltip(&mut self, tooltip: &str) {
        unsafe {
            Self::copy_tooltip(&mut self.nid.szTip, tooltip);
            self.nid.uFlags = NIF_TIP;
            Shell_NotifyIconW(NIM_MODIFY, &self.nid);
        }
    }

    fn copy_tooltip(dst: &mut [u16; 128], text: &str) {
        let mut chars = text.encode_utf16().take(127);
        for item in dst.iter_mut() {
            *item = chars.next().unwrap_or(0);
        }
    }

    pub fn show_context_menu(hwnd: HWND, ball_enabled: bool, _font_scale: f32, _poll_interval_ms: u64) -> usize {
        unsafe {
            let mut pt = mem::zeroed();
            GetCursorPos(&mut pt);

            let hmenu = CreatePopupMenu();
            let add_item = |menu: HWND, id: usize, text: &str| {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                AppendMenuW(menu, MF_STRING, id, wide.as_ptr());
            };

            add_item(hmenu, IDM_SHOW_PANEL, "📊 外设控制中心 (主界面)");
            add_item(hmenu, IDM_SHOW_SETTINGS, "⚙️ 偏好设置 (定时间隔/字号/Toast)");
            add_item(hmenu, IDM_REFRESH, "🔄 立即刷新设备状态");

            let update = crate::update::snapshot();
            let has_update = crate::update::has_update();
            let update_label = if has_update {
                format!(
                    "⬆️ 立即升级到 v{}",
                    update.latest.clone().unwrap_or_default()
                )
            } else if update.checking {
                "⏳ 正在检查新版本…".to_string()
            } else {
                format!("⬆️ 检查更新 (当前 v{})", crate::update::current_version())
            };
            add_item(
                hmenu,
                if has_update { IDM_APPLY_UPDATE } else { IDM_CHECK_UPDATE },
                &update_label,
            );
            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());

            let ball_label = if ball_enabled {
                "🔘 桌面贴边悬浮球: [已开启]"
            } else {
                "⚪ 桌面贴边悬浮球: [已关闭]"
            };
            add_item(hmenu, IDM_TOGGLE_BALL, ball_label);
            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            add_item(hmenu, IDM_EXIT, "❌ 退出 LinkDeck");

            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                hmenu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
                pt.x,
                pt.y,
                0,
                hwnd,
                ptr::null(),
            );
            DestroyMenu(hmenu);
            cmd as usize
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &self.nid);
        }
    }
}
