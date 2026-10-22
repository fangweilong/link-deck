//! 开机自启：以当前用户身份写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`。
//!
//! 使用 HKCU 而非 HKLM，无需管理员权限；配置项 `auto_start` 为唯一事实来源，
//! 每次启动时同步一次，程序被移动或自更新替换后路径自动刷新。

use std::ptr;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "LinkDeck";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Run 键中保存的命令行；路径加引号避免含空格时被截断。
fn command_line(exe: &std::path::Path) -> String {
    format!("\"{}\"", exe.display())
}

/// 读取指定值；不存在或非字符串时返回 None。
fn read_command(name: &str) -> Option<String> {
    unsafe {
        let subkey = wide(RUN_KEY);
        let value = wide(name);
        let mut hkey: HKEY = ptr::null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        ) != 0
        {
            return None;
        }

        let mut kind: REG_VALUE_TYPE = 0;
        let mut size: u32 = 0;
        let mut found = RegQueryValueExW(
            hkey,
            value.as_ptr(),
            ptr::null(),
            &mut kind,
            ptr::null_mut(),
            &mut size,
        ) == 0
            && kind == REG_SZ
            && size >= 2;

        let mut text = String::new();
        if found {
            let mut buf = vec![0u16; size as usize / 2 + 1];
            let mut cb = (buf.len() * 2) as u32;
            found = RegQueryValueExW(
                hkey,
                value.as_ptr(),
                ptr::null(),
                &mut kind,
                buf.as_mut_ptr() as *mut u8,
                &mut cb,
            ) == 0;
            if found {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                text = String::from_utf16_lossy(&buf[..len]);
            }
        }

        RegCloseKey(hkey);
        if found && !text.trim().is_empty() {
            Some(text)
        } else {
            None
        }
    }
}

/// 写入（Some）或删除（None）自启项。
fn write_command(name: &str, command: Option<&str>) -> Result<(), String> {
    unsafe {
        let subkey = wide(RUN_KEY);
        let value = wide(name);
        let mut hkey: HKEY = ptr::null_mut();

        if let Some(cmd) = command {
            let rc = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                ptr::null(),
                &mut hkey,
                ptr::null_mut(),
            );
            if rc != 0 {
                return Err(format!("打开 Run 注册表键失败 (错误码 {rc})"));
            }
            let data: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let rc = RegSetValueExW(
                hkey,
                value.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            RegCloseKey(hkey);
            if rc != 0 {
                return Err(format!("写入 Run 注册表值失败 (错误码 {rc})"));
            }
        } else {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            ) != 0
            {
                // 键不存在即代表未设置自启。
                return Ok(());
            }
            let rc = RegDeleteValueW(hkey, value.as_ptr());
            RegCloseKey(hkey);
            if rc != 0 && rc != ERROR_FILE_NOT_FOUND {
                return Err(format!("删除 Run 注册表值失败 (错误码 {rc})"));
            }
        }
    }
    Ok(())
}

pub fn is_enabled() -> bool {
    read_command(VALUE_NAME).is_some()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if !enabled {
        return write_command(VALUE_NAME, None);
    }
    let exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {e}"))?;
    write_command(VALUE_NAME, Some(&command_line(&exe)))
}

/// 启动时把配置落到注册表；失败只记录，不阻塞启动。
pub fn sync(enabled: bool) {
    if let Err(err) = set_enabled(enabled) {
        eprintln!("开机自启同步失败: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_value_round_trip() {
        let name = format!("LinkDeck_Test_{}", std::process::id());
        assert!(write_command(&name, Some("\"C:\\Temp\\link-deck.exe\"")).is_ok());
        assert_eq!(
            read_command(&name).as_deref(),
            Some("\"C:\\Temp\\link-deck.exe\"")
        );
        assert!(write_command(&name, None).is_ok());
        assert_eq!(read_command(&name), None);
        // 重复删除应幂等。
        assert!(write_command(&name, None).is_ok());
    }
}
