//! ==============================================================================
//! 模块: 自动更新 (单一 exe 自替换)
//! 说明:
//!   1. 通过 WinHTTP 访问 GitHub Releases API，取最新 tag 与 exe 资产地址；
//!   2. 与编译期版本号做语义化比较，只有远端更高才提示；
//!   3. 下载到 %APPDATA%\LinkDeck\LinkDeck-update.exe，校验 MZ 头与体积；
//!   4. 自替换：把正在运行的 exe 改名成 *.old.exe（Windows 允许重命名运行中文件），
//!      再把新文件复制回原路径并拉起新进程，等到新进程写下启动标记后旧进程才退出；
//!      新进程被拦截、立即崩溃或超时则自动回滚，并记录 %APPDATA%\LinkDeck\update.log；
//!   5. 下次启动时清理残留的 *.old.exe。
//! 目标: 不引入额外依赖、不弹控制台、失败时保证原 exe 完好。
//! ==============================================================================

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use std::ffi::c_void;

// ---------------------------------------------------------------------------
// WinHTTP 直连声明（windows-sys 0.59 未提供该模块的绑定）
// ---------------------------------------------------------------------------
type HInternet = *mut c_void;
type Lpcwstr = *const u16;

const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_OPTION_REDIRECT_POLICY: u32 = 110;
const WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS: u32 = 2;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const INTERNET_DEFAULT_HTTPS_PORT: u16 = 443;

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        agent: Lpcwstr,
        access_type: u32,
        proxy: Lpcwstr,
        proxy_bypass: Lpcwstr,
        flags: u32,
    ) -> HInternet;
    fn WinHttpConnect(
        session: HInternet,
        host: Lpcwstr,
        port: u16,
        reserved: u32,
    ) -> HInternet;
    fn WinHttpOpenRequest(
        connect: HInternet,
        verb: Lpcwstr,
        object: Lpcwstr,
        version: Lpcwstr,
        referrer: Lpcwstr,
        accept_types: *const Lpcwstr,
        flags: u32,
    ) -> HInternet;
    fn WinHttpSetOption(
        handle: HInternet,
        option: u32,
        buffer: *const c_void,
        len: u32,
    ) -> i32;
    fn WinHttpSendRequest(
        request: HInternet,
        headers: Lpcwstr,
        headers_len: u32,
        optional: *const c_void,
        optional_len: u32,
        total_len: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: HInternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: HInternet,
        info_level: u32,
        name: Lpcwstr,
        buffer: *mut c_void,
        len: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(
        request: HInternet,
        buffer: *mut c_void,
        to_read: u32,
        read: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(handle: HInternet) -> i32;
}

/// 自更新重启时传给新进程的参数：让新进程等待旧进程释放单实例互斥体
pub const AFTER_UPDATE_FLAG: &str = "--after-update";

/// GitHub 仓库（发布源）
const REPO: &str = "fangweilong/link-deck";
/// 更新检查间隔
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// 启动后首次检查的延迟，避开启动高峰
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);

/// 当前编译期版本
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 更新模块对 UI 暴露的状态
#[derive(Debug, Default, Clone)]
pub struct UpdateState {
    pub checking: bool,
    pub applying: bool,
    /// 远端最新版本（去掉前缀 v）
    pub latest: Option<String>,
    /// 新版本 exe 的下载地址
    pub download_url: Option<String>,
    /// 已下载到本地的临时文件
    pub downloaded: Option<PathBuf>,
    pub error: Option<String>,
    /// 是否已针对该版本弹过提示
    pub notified: bool,
    pub last_check_epoch: u64,
}

static STATE: Mutex<Option<UpdateState>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut UpdateState) -> R) -> R {
    let mut guard = STATE.lock().unwrap();
    let state = guard.get_or_insert_with(UpdateState::default);
    f(state)
}

pub fn snapshot() -> UpdateState {
    with_state(|s| s.clone())
}

/// 是否有可安装的新版本
pub fn has_update() -> bool {
    with_state(|s| {
        s.latest
            .as_deref()
            .map(|latest| is_newer(latest, current_version()))
            .unwrap_or(false)
    })
}

/// 语义化版本比较：`is_newer("0.3.0", "0.2.1") == true`
pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

pub fn parse_version(raw: &str) -> (u32, u32, u32) {
    let trimmed = raw.trim().trim_start_matches('v');
    let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed);
    let mut parts = core.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// GitHub Releases API 返回体里抽出最新版本与 exe 下载地址。
pub fn parse_release(body: &str) -> Result<(String, Option<String>), String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("解析发布信息失败: {}", e))?;
    if let Some(msg) = value.get("message").and_then(|m| m.as_str()) {
        return Err(format!("GitHub API: {}", msg));
    }
    let tag = value
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or_else(|| "发布信息缺少 tag_name".to_string())?;
    let version = tag.trim_start_matches('v').to_string();

    let mut fallback: Option<String> = None;
    if let Some(assets) = value.get("assets").and_then(|a| a.as_array()) {
        for asset in assets {
            let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let url = asset
                .get("browser_download_url")
                .and_then(|u| u.as_str())
                .unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".exe") && !lower.contains("setup") {
                return Ok((version, Some(url.to_string())));
            }
            if lower.ends_with(".exe") && fallback.is_none() {
                fallback = Some(url.to_string());
            }
        }
    }
    Ok((version, fallback))
}

/// 检查更新；成功时写入全局状态。
pub fn check_now() -> Result<String, String> {
    with_state(|s| {
        s.checking = true;
        s.error = None;
    });
    let result = (|| -> Result<String, String> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
        let body = http_get_text(&url)?;
        let (version, url) = parse_release(&body)?;
        with_state(|s| {
            s.latest = Some(version.clone());
            s.download_url = url;
            s.last_check_epoch = now_epoch();
        });
        Ok(version)
    })();
    with_state(|s| {
        s.checking = false;
        if let Err(e) = &result {
            s.error = Some(e.clone());
        }
    });
    result
}

/// 下载更新包到本地临时目录并做基本校验。
pub fn download_latest() -> Result<PathBuf, String> {
    let (url, version) = with_state(|s| (s.download_url.clone(), s.latest.clone()));
    let url = url.ok_or_else(|| "尚未获取到下载地址，请先检查更新".to_string())?;
    let version = version.ok_or_else(|| "尚未获取到版本号，请先检查更新".to_string())?;
    let dir = crate::config::AppConfig::config_dir().join("updates");
    fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败: {}", e))?;
    let dest = dir.join(format!("LinkDeck-{}.exe", version));

    with_state(|s| {
        s.applying = true;
        s.error = None;
    });
    let result = download_to(&url, &dest);
    with_state(|s| {
        s.applying = false;
        match &result {
            Ok(_) => {
                s.downloaded = Some(dest.clone());
            }
            Err(e) => s.error = Some(e.clone()),
        }
    });
    result.map(|_| dest)
}

/// 新版本进程启动标记（自更新握手文件）
pub fn update_signal_path() -> PathBuf {
    crate::config::AppConfig::config_dir().join("update.started")
}

/// 新版本进程进入 main 后立即调用：让旧进程确认新 exe 真的跑起来了。
pub fn signal_started() {
    let _ = fs::create_dir_all(crate::config::AppConfig::config_dir());
    let _ = fs::write(
        update_signal_path(),
        format!("pid={} version={}\n", std::process::id(), current_version()),
    );
}

fn clear_signal() {
    let _ = fs::remove_file(update_signal_path());
}

fn log_update(message: &str) {
    use std::io::Write;
    let path = crate::config::AppConfig::config_dir().join("update.log");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {}", now_epoch(), message);
    }
}

/// 用已下载的新 exe 替换当前进程并重启。
///
/// 关键点：Windows 允许重命名正在运行的可执行文件，
/// 因此顺序为「改名旧文件 → 复制新文件 → 拉起新进程并等待启动标记 → 退出」，
/// 新进程只要没成功跑起来（被拦截、立即崩溃、超时）就回滚，保证始终有一份可用 exe。
pub fn apply_and_restart(new_exe: &Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("获取当前程序路径失败: {}", e))?;
    clear_signal();
    let old = swap_executable(&current, new_exe)?;
    log_update(&format!("已写入新版本: {}", current.display()));

    match spawn_new_version(&current) {
        Ok(pid) => {
            clear_signal();
            // 下载缓存与旧文件仅在新进程确认启动后清理；旧文件可能仍被本进程占用，
            // 由新进程启动时的 cleanup_old_backup 兜底删除。
            let _ = fs::remove_file(new_exe);
            let _ = fs::remove_file(&old);
            log_update(&format!("新版本进程 {} 已启动，旧进程退出", pid));
            std::process::exit(0);
        }
        Err(err) => {
            log_update(&format!("新版本启动失败: {}；回滚", err));
            clear_signal();
            rollback(&current, &old);
            Err(format!("{}（已回滚到原版本）", err))
        }
    }
}

/// 拉起新进程并等待它的启动标记；失败时确保不留残进程。
fn spawn_new_version(current: &Path) -> Result<u32, String> {
    let signal = update_signal_path();
    let mut last_err = String::new();
    for attempt in 0..3 {
        match std::process::Command::new(current)
            .arg(AFTER_UPDATE_FLAG)
            .spawn()
        {
            Ok(mut child) => {
                let pid = child.id();
                log_update(&format!("启动新版本进程 {} (第 {} 次)", pid, attempt + 1));
                match wait_for_started(&mut child, &signal, Duration::from_secs(5)) {
                    Ok(()) => return Ok(pid),
                    Err(err) => {
                        // 已经启动过但立即退出/超时，重试同一文件没有意义
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(err);
                    }
                }
            }
            Err(e) => {
                last_err = format!("启动新版本失败: {}", e);
                log_update(&last_err);
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(400));
                }
            }
        }
    }
    Err(last_err)
}

/// 等待新进程写下启动标记；进程提前退出或超时都视为失败。
fn wait_for_started(
    child: &mut std::process::Child,
    signal: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if signal.exists() {
            return Ok(());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!("新版本启动后立即退出（{}）", status));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("无法确认新版本运行状态: {}", e)),
        }
        if std::time::Instant::now() >= deadline {
            return Err("等待新版本启动超时".to_string());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 回滚：删除新文件并恢复旧文件；对杀毒软件/索引服务短暂占用做重试。
fn rollback(current: &Path, old: &Path) {
    for _ in 0..10 {
        match fs::remove_file(current) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => {
                log_update(&format!("回滚删除新文件失败: {}", e));
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    for _ in 0..10 {
        match fs::rename(old, current) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => {
                log_update(&format!("回滚恢复旧文件失败: {}", e));
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// 把 `current` 换成 `new_exe`，返回旧文件路径。失败时保证 `current` 仍是可用文件。
pub fn swap_executable(current: &Path, new_exe: &Path) -> Result<PathBuf, String> {
    if !new_exe.exists() {
        return Err("更新包不存在，请重新下载".to_string());
    }
    let size = fs::metadata(new_exe).map(|m| m.len()).unwrap_or(0);
    if size < 200 * 1024 {
        return Err(format!("更新包体积异常（{} 字节），已中止", size));
    }

    let old = old_backup_path(current);
    let _ = fs::remove_file(&old);
    fs::rename(current, &old).map_err(|e| {
        format!(
            "无法暂存当前版本（可能被其他程序占用）：{}。可稍后重试或手动替换。",
            e
        )
    })?;

    if let Err(e) = fs::copy(new_exe, current) {
        let _ = fs::rename(&old, current);
        return Err(format!("写入新版本失败: {}（已回滚）", e));
    }
    Ok(old)
}

/// 清理上次升级遗留的 `*.old.exe`（旧进程可能还没完全退出，重试几次）
pub fn cleanup_old_backup() {
    if let Ok(current) = std::env::current_exe() {
        let old = old_backup_path(&current);
        for _ in 0..30 {
            match fs::remove_file(&old) {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }
}

pub fn old_backup_path(current: &Path) -> PathBuf {
    let stem = current
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "link-deck".to_string());
    current.with_file_name(format!("{}.old.exe", stem))
}

/// 后台自动检查线程：启动后延迟一次，之后按间隔轮询；`notify` 用于通知宿主窗口。
pub fn spawn_auto_checker(enabled: bool, notify: impl Fn() + Send + 'static) {
    if !enabled {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            let _ = check_now();
            notify();
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// 手动检查（不阻塞 UI）
pub fn spawn_check(notify: impl Fn() + Send + 'static) {
    if with_state(|s| s.checking) {
        return;
    }
    std::thread::spawn(move || {
        let _ = check_now();
        notify();
    });
}

/// 手动下载并升级（不阻塞 UI）
pub fn spawn_apply(notify: impl Fn() + Send + 'static) {
    if with_state(|s| s.applying) {
        return;
    }
    std::thread::spawn(move || {
        let result = download_latest().and_then(|p| apply_and_restart(&p));
        if let Err(e) = result {
            with_state(|s| s.error = Some(e));
        }
        notify();
    });
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// WinHTTP 传输层
// ---------------------------------------------------------------------------

struct Url {
    host: String,
    path: String,
}

fn split_url(url: &str) -> Result<Url, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "仅支持 https 地址".to_string())?;
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    Ok(Url {
        host: authority.to_string(),
        path: path.to_string(),
    })
}

unsafe fn get(
    url: &str,
    headers: &str,
    mut on_data: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<u32, String> {
    let parsed = split_url(url)?;
    let agent = wide("LinkDeck-Updater/1.0");
    let host = wide(&parsed.host);
    let path = wide(&parsed.path);
    let hdrs = wide(headers);
    let verb = wide("GET");

    let session = WinHttpOpen(
        agent.as_ptr(),
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        std::ptr::null(),
        std::ptr::null(),
        0,
    );
    if session.is_null() {
        return Err("初始化 WinHTTP 失败".to_string());
    }
    let result = (|| -> Result<u32, String> {
        let connect = WinHttpConnect(session, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            return Err("连接更新服务器失败".to_string());
        }
        let guard_connect = HandleGuard(connect);
        let request = WinHttpOpenRequest(
            guard_connect.0,
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return Err("创建更新请求失败".to_string());
        }
        let guard_req = HandleGuard(request);

        let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
        WinHttpSetOption(
            guard_req.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            &policy as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        let sent: i32 = WinHttpSendRequest(
            guard_req.0,
            hdrs.as_ptr(),
            u32::MAX,
            std::ptr::null(),
            0,
            0,
            0,
        );
        if sent == 0 {
            return Err(last_error("发送请求失败"));
        }
        if WinHttpReceiveResponse(guard_req.0, std::ptr::null_mut()) == 0 {
            return Err(last_error("等待响应失败"));
        }

        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            guard_req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
        );

        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut read: u32 = 0;
            if WinHttpReadData(
                guard_req.0,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
                &mut read,
            ) == 0
            {
                return Err(last_error("读取响应失败"));
            }
            if read == 0 {
                break;
            }
            on_data(&buf[..read as usize])?;
        }
        Ok(status)
    })();
    unsafe { WinHttpCloseHandle(session) };
    result
}

struct HandleGuard(HInternet);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn http_get_text(url: &str) -> Result<String, String> {
    let headers = format!(
        "User-Agent: LinkDeck/{} (Windows)\r\nAccept: application/vnd.github+json\r\n",
        current_version()
    );
    let mut body: Vec<u8> = Vec::new();
    let status = unsafe { get(url, &headers, |chunk| Ok(body.extend_from_slice(chunk))) }?;
    if status != 200 {
        return Err(format!("更新服务器返回 HTTP {}", status));
    }
    String::from_utf8(body).map_err(|_| "响应不是有效 UTF-8".to_string())
}

fn download_to(url: &str, dest: &Path) -> Result<u64, String> {
    let headers = format!("User-Agent: LinkDeck/{}\r\n", current_version());
    let mut file = fs::File::create(dest).map_err(|e| format!("创建更新文件失败: {}", e))?;
    let mut total: u64 = 0;
    let mut head: Vec<u8> = Vec::with_capacity(2);
    let status = unsafe {
        get(url, &headers, |chunk| {
            if head.len() < 2 {
                head.extend_from_slice(&chunk[..chunk.len().min(2 - head.len())]);
            }
            std::io::Write::write_all(&mut file, chunk).map_err(|e| e.to_string())?;
            total += chunk.len() as u64;
            Ok(())
        })
    }?;
    drop(file);
    if status != 200 {
        let _ = fs::remove_file(dest);
        return Err(format!("下载返回 HTTP {}", status));
    }
    if head != b"MZ" {
        let _ = fs::remove_file(dest);
        return Err("下载内容不是可执行文件，已丢弃".to_string());
    }
    Ok(total)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(prefix: &str) -> String {
    format!("{}（错误码 {}）", prefix, std::io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("v0.2.10", "0.2.9"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(is_newer("1.0", "0.9.9"));
    }

    #[test]
    fn version_parsing_tolerates_prefix_and_suffix() {
        assert_eq!(parse_version("v1.2.3"), (1, 2, 3));
        assert_eq!(parse_version("1.2.3-beta.1"), (1, 2, 3));
        assert_eq!(parse_version("1.2"), (1, 2, 0));
    }

    #[test]
    fn release_picks_exe_asset() {
        let body = r#"{
            "tag_name": "v0.3.0",
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://x/notes.txt"},
                {"name": "link-deck-windows.zip", "browser_download_url": "https://x/a.zip"},
                {"name": "link-deck.exe", "browser_download_url": "https://x/link-deck.exe"}
            ]
        }"#;
        let (version, url) = parse_release(body).unwrap();
        assert_eq!(version, "0.3.0");
        assert_eq!(url.as_deref(), Some("https://x/link-deck.exe"));
    }

    #[test]
    fn release_reports_api_error() {
        let body = r#"{"message": "Not Found"}"#;
        assert!(parse_release(body).unwrap_err().contains("Not Found"));
    }

    #[test]
    fn old_backup_name_keeps_exe_suffix() {
        let p = old_backup_path(Path::new(r"C:\apps\link-deck.exe"));
        assert_eq!(p, PathBuf::from(r"C:\apps\link-deck.old.exe"));
    }

    #[test]
    fn swap_replaces_file_and_keeps_backup() {
        let dir = std::env::temp_dir().join(format!("linkdeck-swap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let current = dir.join("link-deck.exe");
        let new_exe = dir.join("download.exe");
        fs::write(&current, vec![b'A'; 300 * 1024]).unwrap();
        fs::write(&new_exe, vec![b'B'; 300 * 1024]).unwrap();

        let old = swap_executable(&current, &new_exe).unwrap();
        assert_eq!(fs::read(&current).unwrap()[0], b'B');
        assert_eq!(fs::read(&old).unwrap()[0], b'A');
        assert_eq!(old.file_name().unwrap().to_string_lossy(), "link-deck.old.exe");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_rejects_tiny_download() {
        let dir = std::env::temp_dir().join(format!("linkdeck-swap-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let current = dir.join("link-deck.exe");
        let new_exe = dir.join("download.exe");
        fs::write(&current, b"original").unwrap();
        fs::write(&new_exe, b"MZ").unwrap();
        assert!(swap_executable(&current, &new_exe).is_err());
        assert_eq!(fs::read(&current).unwrap(), b"original");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_for_started_detects_immediate_exit() {
        let dir = std::env::temp_dir().join(format!("linkdeck-wait-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let signal = dir.join("update.started");
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "exit", "7"])
            .spawn()
            .expect("spawn cmd");
        let err = wait_for_started(&mut child, &signal, Duration::from_secs(2)).unwrap_err();
        assert!(err.contains("立即退出"), "{err}");
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_for_started_accepts_signal_and_times_out() {
        let dir = std::env::temp_dir().join(format!("linkdeck-wait-ok-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let signal = dir.join("update.started");

        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "6", "127.0.0.1"])
            .spawn()
            .expect("spawn cmd");
        // 没有标记文件时必须超时失败
        assert!(wait_for_started(&mut child, &signal, Duration::from_millis(300)).is_err());
        // 标记出现后立即成功
        fs::write(&signal, "pid=1").unwrap();
        assert!(wait_for_started(&mut child, &signal, Duration::from_secs(2)).is_ok());
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn url_split() {
        let u = split_url("https://api.github.com/repos/a/b/releases/latest").unwrap();
        assert_eq!(u.host, "api.github.com");
        assert_eq!(u.path, "/repos/a/b/releases/latest");
        assert!(split_url("http://x").is_err());
    }
}
