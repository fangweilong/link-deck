#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code, unused_variables)]
#![windows_subsystem = "windows"]

mod config;
mod devices;
mod ui;

use config::AppConfig;
use devices::DeviceManager;
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use ui::floating_ball::FloatingBallWindow;
use ui::panel::PanelWindow;
use ui::tray::{
    TrayIcon, IDM_EXIT, IDM_FONT_SCALE_100, IDM_FONT_SCALE_115, IDM_FONT_SCALE_130,
    IDM_POLL_1500, IDM_POLL_3000, IDM_POLL_10000, IDM_POLL_30000, IDM_REFRESH, IDM_SHOW_PANEL,
    IDM_TOGGLE_BALL, WM_TRAY_CALLBACK,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, ASFW_ANY, CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW,
    GetMessageW, GetWindowThreadProcessId, IsIconic, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, SetTimer, ShowWindow, TranslateMessage, MSG,
    SW_RESTORE, SW_SHOW, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER,
    WNDCLASSEXW,
};
pub const WM_LINKDECK_ACTIVATE: u32 = WM_APP + 102;

struct SingleInstanceGuard(HANDLE);

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != -1isize as HANDLE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

static mut APP_INSTANCE: Option<Arc<Mutex<AppCore>>> = None;

struct AppCore {
    config: AppConfig,
    manager: DeviceManager,
    tray: Option<TrayIcon>,
    floating_ball: Option<FloatingBallWindow>,
    panel: Option<PanelWindow>,
}

fn main() {
    unsafe {
        // Enable Windows Per-Monitor V2 High-DPI Awareness
        // Prevents blurriness, ensures pixel-perfect native ClearType font rendering
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--status" || a == "--scan") {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!("====================================================");
        println!("     LinkDeck 外设管理中心 & 状态栏常驻电量工具     ");
        println!("====================================================");
        println!("* 硬件直连引擎已就绪 (2.4G 无线 / 蓝牙 / USB 有线)");
        let config = AppConfig::load();
        let mut manager = DeviceManager::new();
        manager.set_aliases(config.device_aliases);
        manager.start();
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let snaps = manager.get_snapshots();
        println!("已扫描并连接的外设设备列表 (共 {} 台):", snaps.len());
        println!("{:-<70}", "");
        for dev in &snaps {
            let icon = match dev.kind {
                devices::DeviceKind::Keyboard => "⌨️",
                devices::DeviceKind::Mouse => "🖱️",
                devices::DeviceKind::Headset => "🎧",
                _ => "📱",
            };
            let pct_text = dev
                .battery
                .percentage
                .map(|p| format!("{}%", p))
                .unwrap_or_else(|| "休眠/待机".to_string());
            let mv_text = dev
                .battery
                .voltage_mv
                .map(|mv| format!("{} mV ({:.3}V)", mv, mv as f32 / 1000.0))
                .unwrap_or_else(|| "N/A".to_string());
            let chg_mark = dev.battery.charging_source.status_mark(dev.battery.charging);
            let online_mark = if dev.battery.online { "在线" } else { "离线" };

            println!(
                "{} [{}] {} (ID: {})",
                icon,
                dev.connection.as_str(),
                dev.name,
                dev.id
            );
            println!(
                "   电量: {}{} | 电压: {} | 状态: {}",
                pct_text, chg_mark, mv_text, online_mark
            );
            println!("   物理路径: {}", dev.path);
        }
        println!("{:-<70}", "");
        return;
    }

    // Single-instance check: ensure only one LinkDeck process runs at a time
    let mutex_name: Vec<u16> = "Local\\LinkDeck_SingleInstance_Mutex\0".encode_utf16().collect();
    let single_instance_mutex = unsafe { CreateMutexW(ptr::null(), 1, mutex_name.as_ptr()) };
    if !single_instance_mutex.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Another instance is already running!
        // Notify the existing instance to display its control panel and bring to foreground.
        let host_class: Vec<u16> = "LinkDeck_HiddenHost\0".encode_utf16().collect();
        let mut existing_host = unsafe { FindWindowW(host_class.as_ptr(), ptr::null()) };

        // If the first instance is still initializing its window, retry briefly
        for _ in 0..10 {
            if !existing_host.is_null() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            existing_host = unsafe { FindWindowW(host_class.as_ptr(), ptr::null()) };
        }

        if !existing_host.is_null() {
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(existing_host, &mut pid);
                if pid != 0 {
                    AllowSetForegroundWindow(pid);
                } else {
                    AllowSetForegroundWindow(ASFW_ANY);
                }
                PostMessageW(existing_host, WM_LINKDECK_ACTIVATE, 0, 0);
            }
        }

        let panel_class: Vec<u16> = "LinkDeck_ManagementPanel\0".encode_utf16().collect();
        let existing_panel = unsafe { FindWindowW(panel_class.as_ptr(), ptr::null()) };
        if !existing_panel.is_null() {
            unsafe {
                if IsIconic(existing_panel) != 0 {
                    ShowWindow(existing_panel, SW_RESTORE);
                } else {
                    ShowWindow(existing_panel, SW_SHOW);
                }
                SetForegroundWindow(existing_panel);
            }
        }

        unsafe {
            CloseHandle(single_instance_mutex);
        }
        return;
    }

    let _instance_guard = SingleInstanceGuard(single_instance_mutex);
    let config = AppConfig::load();
    let initial_ball_enabled = config.enable_floating_ball;
    let initial_font_scale = config.font_scale;
    let initial_poll_ms = config.poll_interval_ms;
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class_name: Vec<u16> = "LinkDeck_HiddenHost\0".encode_utf16().collect();

        let mut wcx: WNDCLASSEXW = mem::zeroed();
        wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
        wcx.lpfnWndProc = Some(host_wnd_proc);
        wcx.hInstance = instance;
        wcx.lpszClassName = class_name.as_ptr();

        RegisterClassExW(&wcx);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            ptr::null(),
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null(),
        );

        if hwnd.is_null() {
            eprintln!("Failed to create host message window");
            return;
        }

        // Initialize DeviceManager
        let mut manager = DeviceManager::with_interval(initial_poll_ms);
        manager.set_aliases(config.device_aliases.clone());
        manager.start();
        let tray = TrayIcon::new(hwnd, "LinkDeck 外设管理 - 正在检测外设...");

        // Create Edge-sticking Floating Ball Window
        let floating_ball =
            FloatingBallWindow::new(hwnd, initial_ball_enabled, initial_font_scale)
                .expect("Failed to create floating ball");

        // Create Management Panel Window
        let panel = PanelWindow::new(hwnd, initial_ball_enabled, initial_font_scale, initial_poll_ms)
            .expect("Failed to create panel");
        let app_core = Arc::new(Mutex::new(AppCore {
            config,
            manager,
            tray: Some(tray),
            floating_ball: Some(floating_ball),
            panel: Some(panel),
        }));

        APP_INSTANCE = Some(app_core);

        // UI update timer (every 1000ms)
        SetTimer(hwnd, 1, 1000, None);

        // Run Win32 message pump
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn toggle_floating_ball() {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.enable_floating_ball = !app.config.enable_floating_ball;
            let enabled = app.config.enable_floating_ball;
            let font_scale = app.config.font_scale;
            let poll_ms = app.config.poll_interval_ms;
            let _ = app.config.save();

            if let Some(fb) = &app.floating_ball {
                fb.set_enabled(enabled);
            }

            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, enabled, font_scale, poll_ms);
            }
        }
    }
}

fn set_poll_interval(ms: u64) {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.poll_interval_ms = ms;
            let _ = app.config.save();
            app.manager.set_poll_interval_ms(ms);
            let ball_on = app.config.enable_floating_ball;
            let scale = app.config.font_scale;
            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, ball_on, scale, ms);
            }
        }
    }
}

fn set_font_scale(scale: f32) {
    unsafe {
        if let Some(app_arc) = &APP_INSTANCE {
            let mut app = app_arc.lock().unwrap();
            app.config.font_scale = scale;
            let ball_on = app.config.enable_floating_ball;
            let poll_ms = app.config.poll_interval_ms;

            if let Some(fb) = &app.floating_ball {
                fb.set_font_scale(scale);
            }

            let snaps = app.manager.get_snapshots();
            if let Some(panel) = &app.panel {
                panel.update_devices(&snaps, ball_on, scale, poll_ms);
            }
        }
    }
}

unsafe extern "system" fn host_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            if wparam == 1 {
                if let Some(app_arc) = &APP_INSTANCE {
                    let mut app = app_arc.lock().unwrap();
                    let snaps = app.manager.get_snapshots();
                    let ball_on = app.config.enable_floating_ball;
                    let font_scale = app.config.font_scale;
                    let poll_ms = app.config.poll_interval_ms;

                    // Update floating ball if enabled
                    if let Some(fb) = &app.floating_ball {
                        if ball_on {
                            fb.update_devices(&snaps);
                        }
                    }

                    // Update management panel
                    if let Some(panel) = &app.panel {
                        panel.update_devices(&snaps, ball_on, font_scale, poll_ms);
                    }
                    // Format tray tooltip
                    let mut tip_lines = Vec::new();
                    tip_lines.push("LinkDeck 外设电量管理".to_string());
                    tip_lines.push("---------------------".to_string());

                    for dev in &snaps {
                        let pct_text = dev
                            .battery
                            .percentage
                            .map(|p| format!("{}%", p))
                            .unwrap_or_else(|| "休眠".to_string());
                        let chg_mark = dev.battery.charging_source.tray_mark(dev.battery.charging);
                        let icon = match dev.kind {
                            devices::DeviceKind::Keyboard => "⌨️",
                            devices::DeviceKind::Mouse => "🖱️",
                            devices::DeviceKind::Headset => "🎧",
                            _ => "📱",
                        };
                        tip_lines.push(format!("{} {}: {}{}", icon, dev.name, pct_text, chg_mark));
                    }

                    if snaps.is_empty() {
                        tip_lines.push("未检测到受支持设备".to_string());
                    }

                    let full_tip = tip_lines.join("\n");
                    if let Some(tray) = &mut app.tray {
                        tray.update_tooltip(&full_tip);
                    }
                }
            }
            0
        }
        WM_TRAY_CALLBACK => {
            let event = (lparam & 0xFFFF) as u32;
            if event == WM_RBUTTONUP {
                let (ball_on, font_scale, poll_ms) = if let Some(app_arc) = &APP_INSTANCE {
                    let cfg = &app_arc.lock().unwrap().config;
                    (cfg.enable_floating_ball, cfg.font_scale, cfg.poll_interval_ms)
                } else {
                    (false, 1.15, 3000)
                };

                let cmd = TrayIcon::show_context_menu(hwnd, ball_on, font_scale, poll_ms);
                if cmd > 0 {
                    PostMessageW(hwnd, WM_COMMAND, cmd, 0);
                }
            } else if event == WM_LBUTTONUP {
                PostMessageW(hwnd, WM_COMMAND, IDM_SHOW_PANEL as _, 0);
            }
            0
        }
        WM_COMMAND => {
            let cmd_id = (wparam & 0xFFFF) as usize;
            match cmd_id {
                IDM_SHOW_PANEL => {
                    if let Some(app_arc) = &APP_INSTANCE {
                        let app = app_arc.lock().unwrap();
                        if let Some(panel) = &app.panel {
                            panel.toggle();
                        }
                    }
                }
                IDM_TOGGLE_BALL => {
                    toggle_floating_ball();
                }
                IDM_FONT_SCALE_100 => {
                    set_font_scale(1.00);
                }
                IDM_FONT_SCALE_115 => {
                    set_font_scale(1.15);
                }
                IDM_FONT_SCALE_130 => {
                    set_font_scale(1.30);
                }
                IDM_POLL_1500 => {
                    set_poll_interval(1500);
                }
                IDM_POLL_3000 => {
                    set_poll_interval(3000);
                }
                IDM_POLL_10000 => {
                    set_poll_interval(10000);
                }
                IDM_POLL_30000 => {
                    set_poll_interval(30000);
                }
                IDM_REFRESH => {
                    if let Some(app_arc) = &APP_INSTANCE {
                        let app = app_arc.lock().unwrap();
                        app.manager.refresh();
                    }
                }
                IDM_EXIT => {
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        crate::ui::panel::WM_DEVICE_RENAME => {
            let ptr = wparam as *mut (String, String);
            if !ptr.is_null() {
                let (dev_id, new_name) = *Box::from_raw(ptr);
                if let Some(app_arc) = &APP_INSTANCE {
                    let mut app = app_arc.lock().unwrap();
                    app.config.set_alias(&dev_id, &new_name);
                    let _ = app.config.save();
                    app.manager.set_aliases(app.config.device_aliases.clone());

                    // Refresh snapshots and UI immediately
                    let snaps = app.manager.get_snapshots();
                    let ball_on = app.config.enable_floating_ball;
                    let font_scale = app.config.font_scale;
                    let poll_ms = app.config.poll_interval_ms;

                    if let Some(fb) = &app.floating_ball {
                        if ball_on {
                            fb.update_devices(&snaps);
                        }
                    }
                    if let Some(panel) = &app.panel {
                        panel.update_devices(&snaps, ball_on, font_scale, poll_ms);
                    }
                }
            }
            0
        }
        WM_LINKDECK_ACTIVATE => {
            if let Some(app_arc) = &APP_INSTANCE {
                let app = app_arc.lock().unwrap();
                let snaps = app.manager.get_snapshots();
                let ball_on = app.config.enable_floating_ball;
                let font_scale = app.config.font_scale;
                let poll_ms = app.config.poll_interval_ms;
                if let Some(panel) = &app.panel {
                    panel.update_devices(&snaps, ball_on, font_scale, poll_ms);
                    panel.show();
                }
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_mutex_lifecycle() {
        let test_name: Vec<u16> = "Local\\LinkDeck_Test_SingleInstance_Lifecycle\0"
            .encode_utf16()
            .collect();
        let m1 = unsafe { CreateMutexW(ptr::null(), 1, test_name.as_ptr()) };
        assert!(!m1.is_null());
        let err1 = unsafe { GetLastError() };
        assert_ne!(err1, ERROR_ALREADY_EXISTS);

        // Second call with same name should detect already exists
        let m2 = unsafe { CreateMutexW(ptr::null(), 1, test_name.as_ptr()) };
        assert!(!m2.is_null());
        let err2 = unsafe { GetLastError() };
        assert_eq!(err2, ERROR_ALREADY_EXISTS);

        // Close second handle
        unsafe { CloseHandle(m2) };

        // Close first handle
        unsafe { CloseHandle(m1) };

        // Third call should succeed anew
        let m3 = unsafe { CreateMutexW(ptr::null(), 1, test_name.as_ptr()) };
        assert!(!m3.is_null());
        let err3 = unsafe { GetLastError() };
        assert_ne!(err3, ERROR_ALREADY_EXISTS);
        unsafe { CloseHandle(m3) };
    }
}
