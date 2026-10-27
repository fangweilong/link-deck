#![allow(unsafe_op_in_unsafe_fn, static_mut_refs, dead_code)]
#![windows_subsystem = "windows"]

mod autostart;
mod config;
mod devices;
mod host;
mod sound;
mod ui;
mod update;

use config::AppConfig;
use devices::DeviceManager;
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use ui::floating_ball::FloatingBallWindow;
use ui::panel::PanelWindow;
use ui::tray::TrayIcon;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, ASFW_ANY, CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW,
    GetMessageW, GetWindowThreadProcessId, IsIconic, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, SetTimer, ShowWindow, TranslateMessage, MSG,
    SW_RESTORE, SW_SHOW, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP,
    WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW,
};
pub const WM_LINKDECK_ACTIVATE: u32 = WM_APP + 102;
/// 更新检查完成（后台线程 → 宿主窗口）
pub const WM_UPDATE_STATUS: u32 = WM_APP + 103;

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
        // Enable Windows GDI High-DPI Scaling (Windows 10 1809+ / Windows 11)
        // Automatically renders GDI text and graphics at crisp native monitor DPI with ClearType,
        // without bitmap stretching blur, while maintaining perfect layout coordinate fidelity.
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED) == 0 {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    let args: Vec<String> = std::env::args().collect();
    // 自更新后的新进程：第一时间写下启动标记，供旧进程确认替换成功。
    // 必须早于备份清理（旧进程可能还占用 *.old.exe，清理会重试等待）。
    if args.iter().any(|a| a == update::AFTER_UPDATE_FLAG) {
        update::signal_started();
    }

    // 任何启动方式都先清理上次自更新遗留的备份
    update::cleanup_old_backup();

    if args.iter().any(|a| a == "--check-update") {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!("LinkDeck 更新检查");
        println!("当前版本: v{}", update::current_version());
        match update::check_now() {
            Ok(latest) => {
                let newer = update::is_newer(&latest, update::current_version());
                println!("远端最新: v{}", latest);
                println!("状态: {}", if newer { "发现新版本" } else { "已是最新" });
                if let Some(url) = update::snapshot().download_url {
                    println!("下载地址: {}", url);
                }
            }
            Err(e) => println!("检查失败: {}", e),
        }
        return;
    }
    if args.iter().any(|a| a == "--download-update") || args.iter().any(|a| a == "--apply-update") {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        if args.iter().any(|a| a == "--download-update") {
            println!("当前版本: v{}", update::current_version());
            match update::check_now() {
                Ok(latest) => {
                    println!("远端最新: v{}", latest);
                    let force = args.iter().any(|a| a == "--force");
                    if force || update::is_newer(&latest, update::current_version()) {
                        match update::download_latest() {
                            Ok(path) => println!("已下载: {}", path.display()),
                            Err(e) => println!("下载失败: {}", e),
                        }
                    } else {
                        println!("已是最新版本，无需下载");
                    }
                }
                Err(e) => println!("检查失败: {}", e),
            }
        }
        if let Some(pos) = args.iter().position(|a| a == "--apply-update") {
            match args.get(pos + 1) {
                Some(path) => match update::apply_and_restart(std::path::Path::new(path)) {
                    Ok(()) => {}
                    Err(e) => println!("升级失败: {}", e),
                },
                None => println!("用法: --apply-update <新版本 exe 路径>"),
            }
        }
        return;
    }
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
    if args.iter().any(|a| a == "--protocols" || a == "--supported") {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!("====================================================");
        println!("     LinkDeck 内置私有协议与硬件支持库（含实机验证状态）     ");
        println!("====================================================");
        println!("1. 罗技 Logitech Lightspeed / Unifying 全系 (2.4G 无线 / 有线)  [已实测: MX Master 3S]");
        println!("   - 协议原理: 官方 HID++ 2.0 / 4.0 (Unified Battery 0x1004 / BatteryStatus 0x1000)");
        println!("   - 典型设备: GPW 狗屁王 1/2/3 代、G502 Lightspeed、G304、G903、G913/G915 键盘、MX Master 系列等");
        println!("2. RK / Compx 方案三模机械键盘 (2.4G 无线 / USB 有线)  [已实测: RK108]");
        println!("   - 协议原理: Compx 接收器 UsagePage 0xFF02 Report 0x06 (硬件毫伏直读)");
        println!("   - 典型设备: RK108、F75、F99、M75、R75、S98、RK100、RK87、RK84、RK68 等全系 Compx 方案键盘");
        println!("3. 迈从 MCHOSE 全系电竞无线鼠标 (2.4G 无线 / 有线)  [未实测]");
        println!("   - 协议原理: M-HUB 专有 Feature Report 0x11 指令 0x06 (XOR 0xFF 掩码通信)");
        println!("   - 典型设备: 迈从 A5 / A5 Pro / A5 Pro Max、AX5 / AX5 Pro、G3、A7 Ultra、L7、M7 等");
        println!("4. VXE / VGN / ATK 全系电竞无线鼠标 (2.4G 无线 / 有线)  [已实测: VGN 蜻蜓 F1 Pro Max]");
        println!("   - 协议原理 (双代支持):");
        println!("     * 第1代 Nordic 52840 (17字节 0x08/0x04 协议): VXE 蜻蜓 R1 / R1 Pro / SE+、VGN 蜻蜓 F1 / F2 系列、VXE MAD R、ATK F1 / A9 等");
        println!("     * 第2代 Nordic 54L15 (64字节 0x72/0x07 8K协议): ATK Zero 等 8K 旗舰无线鼠标");
        println!("     * 方案扩展支持: Zaopin 造品 Z2、Scyrox V8、达尔优 A950 系列、G-Wolves 等");
        println!("5. 赛技 Saiggi / Xenta / Areson 双模无线鼠标 (2.4G 无线 / 有线)  [已实测: TM015]");
        println!("   - 协议原理: Xenta COL03 Report 0x03 心跳探针与电量异步捕获");
        println!("   - 典型设备: 赛技 TM015、Areson 方案、凌阳方案等无线鼠标");
        println!("6. 雷蛇 Razer Hyperspeed 全系无线电竞鼠标 (2.4G 无线 / 有线)  [未实测]");
        println!("   - 协议原理: OpenRazer 90字节 Class 0x07 标准报文双向校验 (CMD 0x80 电量 / CMD 0x84 充电)");
        println!("   - 典型设备: 毒蝰 Viper V2/V3 Pro、炼狱蝰蛇 DeathAdder V2/V3 Pro、巴塞利斯蛇 Basilisk 系列、眼镜蛇 Cobra Pro、八岐大蛇 Orochi 等");
        println!("7. 赛睿 SteelSeries 2.4G 无线鼠标 / 耳机 / 机械键盘 (2.4G 无线 / 有线)  [未实测]");
        println!("   - 协议原理: SteelSeries 专有报文指令 (0xD2/0x92 鼠标电量, 0xAA Rival 650, 0x0618 Arctis耳机)");
        println!("   - 典型设备: Aerox 3/5/9 Wireless、Prime Wireless、Rival 650、Apex Pro 无线键盘、Arctis 7 / 9 / Pro / Nova 系列耳机");
        println!("8. 华硕 ASUS ROG / TUF 全系无线电竞外设 (2.4G 无线 / 有线)  [未实测]");
        println!("   - 协议原理: ROG 专有 64字节 Feature Report (0x1207 鼠标电量, 0x1201 键盘电量, 支持 Omni 接收器)");
        println!("   - 典型设备: ROG 龙鳞 Ace (Harpe Ace)、月刃 (Keris)、战刃 (Gladius III)、夜魔 (Azoth)、魔导士 (Falchion)、Claymore II 等");
        println!("9. 美商海盗船 Corsair Slipstream / Bragi 全系无线外设 (2.4G 无线 / 有线)  [未实测]");
        println!("   - 协议原理: Corsair Bragi 统一协议 (Magic 0x08, Prop 0x0F 电量千分比 / 0x10 充电状态) 及 NXP 兼容通道");
        println!("   - 典型设备: 暗影 Dark Core RGB Pro/SE、卡特 Katar Pro/Elite、铁爪 Ironclaw、圣剑 Sabre、K70 PRO Mini、K100 AIR、HS70/80 耳机等");
        println!("10. Windows 标准通用 HID 电量外设规范 (Standard HID Power)  [未实测]");
        println!("   - 协议原理: UsagePage 0x0084 (Power Device) / 0x0085 (Battery System)");
        println!("   - 典型设备: 实现了 Windows 官方标准 HID 电池规范的第三方外设");
        println!("11. Windows 原生蓝牙 GATT 规范 (全品牌通用)  [已实测: P6 耳机]");
        println!("   - 协议原理: Bluetooth SIG 统一 Battery Service 0x180F 及 HFP 音频协议");
        println!("   - 典型设备: 任何品牌走蓝牙连接电脑的鼠标、键盘、无线耳机 (AirPods/Sony等)、手柄");
        println!("{:-<70}", "");
        println!("状态说明:");
        println!("  [已实测]  已在本机真实硬件上读取并核对电量；同方案的其他品牌/型号未逐一验证。");
        println!("  [未实测]  仅按公开规范或开源实现编写，尚未用对应硬件验证，可能因固件/接收器批次失效。");
        println!("  [待复测]  实测失败并已修复，但尚未收到复测回执。");
        println!("  未实测协议不构成可用性承诺；实机结果请附 --scan-all 原始描述符反馈。");
        println!("{:-<70}", "");
        return;
    }

    if args.iter().any(|a| a == "--scan-all" || a == "--list-hid") {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        println!("====================================================");
        println!("     LinkDeck 全系统已插入 HID 设备诊断探测器     ");
        println!("====================================================");
        let raw_devs = devices::hid_win::enumerate_hid_devices();
        println!("检测到底层 HID 设备接口共 {} 个:", raw_devs.len());
        println!("{:-<70}", "");
        for (i, d) in raw_devs.iter().enumerate() {
            let hw_name = devices::hid_win::HidDevice::open(d.clone(), false)
                .ok()
                .and_then(|h| h.get_product_string())
                .unwrap_or_else(|| "N/A".to_string());
            println!(
                "[{:02}] VID: 0x{:04X} | PID: 0x{:04X} | UsagePage: 0x{:04X} | Usage: 0x{:04X}",
                i + 1, d.vendor_id, d.product_id, d.usage_page, d.usage
            );
            println!("     设备名: {} | FeatureLen: {} | OutputLen: {}", hw_name, d.feature_len, d.output_len);
            println!("     系统路径: {}", d.path);
        }
        println!("{:-<70}", "");
        println!("* 若你的设备尚未被自动识别，请将上述输出反馈给我们以快速适配！");
        return;
    }

    // Single-instance check: ensure only one LinkDeck process runs at a time
    let mutex_name: Vec<u16> = "Local\\LinkDeck_SingleInstance_Mutex\0".encode_utf16().collect();
    let mut single_instance_mutex = unsafe { CreateMutexW(ptr::null(), 1, mutex_name.as_ptr()) };

    // 自更新重启：旧进程可能还没退出，先等它释放互斥体，避免新进程自杀
    if args.iter().any(|a| a == update::AFTER_UPDATE_FLAG)
        && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS
    {
        for _ in 0..40 {
            unsafe { CloseHandle(single_instance_mutex) };
            std::thread::sleep(std::time::Duration::from_millis(250));
            single_instance_mutex = unsafe { CreateMutexW(ptr::null(), 1, mutex_name.as_ptr()) };
            if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
                break;
            }
        }
        // 旧进程此时已退出，锁定解除，可以删掉它的备份文件
        update::cleanup_old_backup();
    }
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
    // 令牌层按配置载入风格：所有窗口（面板 / 悬浮球 / Toast / 托盘气泡）统一取色
    unsafe {
        if matches!(config.ui_style, crate::config::UiStyle::Custom) {
            crate::ui::theme::apply_custom(config.custom_accent, config.custom_light);
        } else {
            crate::ui::theme::apply(config.ui_style);
        }
    }
    // 让注册表 Run 项与配置保持一致（程序移动/自更新后也刷新为新路径）
    autostart::sync(config.auto_start);
    let initial_ball_enabled = config.enable_floating_ball;
    let initial_font_scale = config.font_scale;
    let initial_poll_ms = config.poll_interval_ms;
    let initial_ball_alpha = config.ball_alpha();
    let initial_ball_style = config.floating_ball_style;
    unsafe {
        let instance = GetModuleHandleW(ptr::null());
        let class_name: Vec<u16> = "LinkDeck_HiddenHost\0".encode_utf16().collect();

        let mut wcx: WNDCLASSEXW = mem::zeroed();
        wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
        wcx.lpfnWndProc = Some(host::host_wnd_proc);
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
            FloatingBallWindow::new(
                hwnd,
                initial_ball_enabled,
                initial_font_scale,
                initial_ball_alpha,
                initial_ball_style,
            )
                .expect("Failed to create floating ball");

        // Create Management Panel Window
        let panel = PanelWindow::new(hwnd, &config)
            .expect("Failed to create panel");
        let app_core = Arc::new(Mutex::new(AppCore {
            config,
            manager,
            tray: Some(tray),
            floating_ball: Some(floating_ball),
            panel: Some(panel),
        }));

        APP_INSTANCE = Some(app_core);


        // 后台自动检查新版本（默认开启，可在偏好设置里关闭）
        {
            let auto = APP_INSTANCE
                .as_ref()
                .map(|app| app.lock().unwrap().config.check_updates)
                .unwrap_or(true);
            let host = hwnd as isize;
            update::spawn_auto_checker(auto, move || {
                PostMessageW(host as HWND, WM_UPDATE_STATUS, 0, 0);
            });
        }

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


// 宿主窗口过程与托盘命令在 host 模块里，这里只保留进程启动与消息循环。

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
