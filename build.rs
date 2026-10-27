use std::env;
use std::path::{Path, PathBuf};

/// 把目录前置到 PATH。
///
/// `windres` 自己还要用 shell 调 `gcc -E` 做预处理；PATH 里若先命中别的
/// MSYS 环境（Git for Windows 的 `usr\bin` 就是典型），预处理会以
/// `preprocessing failed` 收场。实测把 binutils 目录前置即可。
fn prepend_path(dir: &Path) {
    let mut dirs = vec![dir.to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(dirs) {
        // build script 是独立进程，改 PATH 不会影响 cargo 本身。
        unsafe { env::set_var("PATH", joined) };
    }
}

/// 在 PATH 里找可执行文件。
fn which(exe: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

/// 找 GNU 资源工具链（`windres` + `ar`）。
///
/// rustup 的 `x86_64-pc-windows-gnu` 只发 rustc，不带 binutils；这两个工具
/// 通常在 MSYS2 的 `mingw64\bin` 里，但默认不进 PATH。找不到时 `winres` 只会
/// 返回一个错误，而 build script 的 stderr 在普通构建下不显示——结果就是
/// exe 悄悄少了图标和版本号。所以这里先查 PATH，再探常见安装位置。
fn find_binutils() -> Option<(PathBuf, PathBuf)> {
    let mut dirs: Vec<PathBuf> = ["MSYS2_ROOT", "MINGW64_ROOT"]
        .iter()
        .filter_map(|key| env::var_os(key))
        .map(PathBuf::from)
        .flat_map(|root| [root.join(r"mingw64\bin"), root.join("bin")])
        .collect();
    dirs.extend(
        [
            r"C:\msys64\mingw64\bin",
            r"C:\msys2\mingw64\bin",
            r"C:\msys64\ucrt64\bin",
            r"C:\ProgramData\msys64\mingw64\bin",
            r"C:\mingw64\bin",
        ]
        .iter()
        .map(PathBuf::from),
    );

    let find = |exe: &str| {
        which(exe).or_else(|| dirs.iter().map(|dir| dir.join(exe)).find(|p| p.is_file()))
    };

    Some((find("windres.exe")?, find("ar.exe")?))
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=linkdeck.ico");

    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon("linkdeck.ico");
    res.set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <asmv3:application>
    <asmv3:windowsSettings>
      <gdiScaling xmlns="http://schemas.microsoft.com/SMI/2017/WindowsSettings">true</gdiScaling>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
"#);

    // 版本号（`FILEVERSION` / `PRODUCTVERSION` 及字符串字段）由 winres 从
    // `CARGO_PKG_VERSION*` 自动填，唯一真源仍是 Cargo.toml；这里只把默认
    // 的包名换成用户能看懂的显示名。
    res.set("ProductName", "LinkDeck");
    res.set("FileDescription", "LinkDeck - Peripheral Battery Monitor");
    res.set("OriginalFilename", "link-deck.exe");

    match find_binutils() {
        Some((windres, ar)) => {
            if let Some(bin_dir) = windres.parent() {
                prepend_path(bin_dir);
            }
            res.set_windres_path(windres.to_string_lossy().as_ref());
            res.set_ar_path(ar.to_string_lossy().as_ref());
        }
        None => {
            println!(
                "cargo:warning=未找到 windres / ar，本次构建的 exe 不含图标与版本资源。\
                 装 MSYS2 或把 mingw64\\bin 加进 PATH 后重新构建。"
            );
            return;
        }
    }

    if let Err(e) = res.compile() {
        println!("cargo:warning=资源编译失败，exe 将缺少图标与版本号：{e}");
    }
}
