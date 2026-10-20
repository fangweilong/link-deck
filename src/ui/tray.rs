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
pub const IDM_TOGGLE_BALL: usize = 1002;
pub const IDM_REFRESH: usize = 1003;
pub const IDM_FONT_SCALE_100: usize = 1010;
pub const IDM_FONT_SCALE_115: usize = 1011;
pub const IDM_FONT_SCALE_130: usize = 1012;
pub const IDM_POLL_1500: usize = 1020;
pub const IDM_POLL_3000: usize = 1021;
pub const IDM_POLL_10000: usize = 1022;
pub const IDM_POLL_30000: usize = 1023;
pub const IDM_EXIT: usize = 1004;

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

    pub fn show_context_menu(hwnd: HWND, ball_enabled: bool, font_scale: f32, poll_interval_ms: u64) -> usize {
        unsafe {
            let mut pt = mem::zeroed();
            GetCursorPos(&mut pt);

            let hmenu = CreatePopupMenu();
            let add_item = |menu: HWND, id: usize, text: &str| {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                AppendMenuW(menu, MF_STRING, id, wide.as_ptr());
            };

            add_item(hmenu, IDM_SHOW_PANEL, "📊 外设控制中心 (管理面板)");
            let ball_label = if ball_enabled {
                "🔘 贴边悬浮球: [已开启]"
            } else {
                "⚪ 贴边悬浮球: [已关闭]"
            };
            add_item(hmenu, IDM_TOGGLE_BALL, ball_label);

            // Font size options
            let font_label_100 = if font_scale <= 1.05 { "  ✓ 标准字号 (100%)" } else { "    标准字号 (100%)" };
            let font_label_115 = if font_scale > 1.05 && font_scale <= 1.22 { "  ✓ 适中字号 (115%)" } else { "    适中字号 (115%)" };
            let font_label_130 = if font_scale > 1.22 { "  ✓ 放大字号 (130%)" } else { "    放大字号 (130%)" };
            add_item(hmenu, IDM_FONT_SCALE_100, font_label_100);
            add_item(hmenu, IDM_FONT_SCALE_115, font_label_115);
            add_item(hmenu, IDM_FONT_SCALE_130, font_label_130);
            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());

            // Polling interval options
            let p1500 = if poll_interval_ms <= 2000 { "  ✓ 极速轮询 (1.5秒 - 实时灵敏)" } else { "    极速轮询 (1.5秒 - 实时灵敏)" };
            let p3000 = if poll_interval_ms > 2000 && poll_interval_ms <= 6000 { "  ✓ 均衡轮询 (3秒 - 推荐平衡)" } else { "    均衡轮询 (3秒 - 推荐平衡)" };
            let p10000 = if poll_interval_ms > 6000 && poll_interval_ms <= 20000 { "  ✓ 省电轮询 (10秒 - 低RF功耗)" } else { "    省电轮询 (10秒 - 低RF功耗)" };
            let p30000 = if poll_interval_ms > 20000 { "  ✓ 长续航 (30秒 - 极少发包)" } else { "    长续航 (30秒 - 极少发包)" };
            add_item(hmenu, IDM_POLL_1500, p1500);
            add_item(hmenu, IDM_POLL_3000, p3000);
            add_item(hmenu, IDM_POLL_10000, p10000);
            add_item(hmenu, IDM_POLL_30000, p30000);

            AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            add_item(hmenu, IDM_REFRESH, "🔄 立即刷新设备状态");
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
