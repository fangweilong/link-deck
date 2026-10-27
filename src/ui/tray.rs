#![allow(static_mut_refs, unsafe_op_in_unsafe_fn)]

use std::mem;
use std::ptr;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetDC, InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    DT_CALCRECT, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetSystemMetrics,
    LoadCursorW, PostMessageW, RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow,
    IDC_ARROW, SM_CXSCREEN, SM_CYSCREEN, WM_CAPTURECHANGED, WM_COMMAND, WM_DESTROY,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_PAINT, WM_RBUTTONUP, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

use crate::ui::floating_ball::{create_clear_font, draw_text_clean};
use crate::ui::theme::THEME;

pub const WM_TRAY_CALLBACK: u32 = 0x8000 + 100; // WM_APP + 100

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
pub const IDM_TOGGLE_AUTOSTART: usize = 1032;

// --------------------------------------------------------------------------
// 右键菜单：整扇窗口自己画
// --------------------------------------------------------------------------
//
// 之前用 `MF_OWNERDRAW` 自绘菜单项，但菜单**窗口本身**还是系统的：
// 圆角外面那一圈浅色边框和留白由系统画，改不掉，于是深色菜单外面永远镶着一圈白边。
// 这里改成真正的自绘弹出窗口（无边框 `WS_POPUP` + 圆角 region），
// 白边、圆角、分隔线、hover 高亮全部我们自己控制，配色也跟着界面风格走。

pub enum MenuKind {
    /// 主操作（打开控制中心）：字号更大、加粗
    Primary,
    Action,
    /// 开关：右侧显示状态
    Toggle,
    /// 破坏性操作
    Danger,
    Separator,
}

pub struct MenuSpec {
    pub id: usize,
    pub label: String,
    /// 右侧状态文字
    pub hint: Option<String>,
    pub kind: MenuKind,
    /// 弱化显示（例如正在检查更新）
    pub dim: bool,
    /// 开关是否处于开启态，决定状态文字的颜色
    pub on: bool,
}

/// 菜单左右内边距
const MENU_PAD_X: i32 = 14;
/// 菜单上下内边距
const MENU_PAD_Y: i32 = 6;
const MENU_HINT_GAP: i32 = 24;
const MENU_ROW_H: i32 = 30;
const MENU_PRIMARY_H: i32 = 34;
const MENU_SEP_H: i32 = 9;
const MENU_FONT: i32 = 13;
/// hover 项圆角块相对菜单边缘的缩进
const MENU_ROW_INSET: i32 = 5;
/// 窗口圆角半径
const MENU_RADIUS: i32 = 10;
/// 与屏幕边缘至少留出的距离
const MENU_SCREEN_MARGIN: i32 = 4;
const MENU_SEP_BASE: usize = 9000;

/// 一行的几何。纯数据，便于脱离 Win32 做单测。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowGeom {
    pub y: i32,
    pub h: i32,
    /// 指向 `MenuSpec` 的下标
    pub spec: usize,
    pub sep: bool,
}

/// 行高只由语义决定：主操作最高，分隔线最矮。
pub fn row_height(kind: &MenuKind) -> i32 {
    match kind {
        MenuKind::Separator => MENU_SEP_H,
        MenuKind::Primary => MENU_PRIMARY_H,
        _ => MENU_ROW_H,
    }
}

/// 自上而下排出行几何。
pub fn layout_rows(items: &[MenuSpec]) -> Vec<RowGeom> {
    let mut rows = Vec::with_capacity(items.len());
    let mut y = MENU_PAD_Y;
    for (idx, spec) in items.iter().enumerate() {
        let h = row_height(&spec.kind);
        rows.push(RowGeom {
            y,
            h,
            spec: idx,
            sep: matches!(spec.kind, MenuKind::Separator),
        });
        y += h;
    }
    rows
}

/// 菜单宽度 = 最宽的一行（标签 + 状态文字）+ 两侧内边距。
/// `text_w` 抽象掉真实字体度量，测试里可以换成确定性实现。
pub fn menu_width(items: &[MenuSpec], text_w: impl Fn(&str, bool) -> i32) -> i32 {
    let mut w = 0;
    for spec in items {
        let bold = matches!(spec.kind, MenuKind::Primary);
        let mut row = text_w(&spec.label, bold);
        if let Some(hint) = &spec.hint {
            row += MENU_HINT_GAP + text_w(hint, false);
        }
        w = w.max(row);
    }
    (w + MENU_PAD_X * 2).clamp(160, 420)
}

/// 命中测试：返回鼠标落在哪一行（分隔线不可选）。
pub fn hit_row(rows: &[RowGeom], y: i32) -> Option<usize> {
    rows.iter()
        .enumerate()
        .find(|(_, r)| !r.sep && y >= r.y && y < r.y + r.h)
        .map(|(i, _)| i)
}

/// 菜单左上角：默认右上角贴着光标，放不下就翻到另一侧，始终留在屏幕内。
pub fn menu_origin(cursor: POINT, w: i32, h: i32, screen_w: i32, screen_h: i32) -> (i32, i32) {
    let mut x = cursor.x;
    let mut y = cursor.y - h;
    if x + w > screen_w - MENU_SCREEN_MARGIN {
        x = (screen_w - w - MENU_SCREEN_MARGIN).max(MENU_SCREEN_MARGIN);
    }
    if x < MENU_SCREEN_MARGIN {
        x = MENU_SCREEN_MARGIN;
    }
    if y < MENU_SCREEN_MARGIN {
        y = (cursor.y + h > screen_h - MENU_SCREEN_MARGIN)
            .then(|| (screen_h - h - MENU_SCREEN_MARGIN).max(MENU_SCREEN_MARGIN))
            .unwrap_or(cursor.y);
    }
    (x, y)
}

fn separator(n: usize) -> MenuSpec {
    MenuSpec {
        id: MENU_SEP_BASE + n,
        label: String::new(),
        hint: None,
        kind: MenuKind::Separator,
        dim: false,
        on: false,
    }
}

fn item(id: usize, label: impl Into<String>, kind: MenuKind) -> MenuSpec {
    MenuSpec { id, label: label.into(), hint: None, kind, dim: false, on: false }
}

fn toggle(id: usize, label: &str, on: bool) -> MenuSpec {
    MenuSpec {
        id,
        label: label.into(),
        hint: Some(if on { "已开启".into() } else { "已关闭".into() }),
        kind: MenuKind::Toggle,
        dim: false,
        on,
    }
}

fn build_specs(ball_enabled: bool, auto_start: bool) -> Vec<MenuSpec> {
    let mut v = Vec::new();

    v.push(item(IDM_SHOW_PANEL, "外设控制中心", MenuKind::Primary));
    v.push(item(IDM_SHOW_SETTINGS, "偏好设置", MenuKind::Action));
    v.push(item(IDM_REFRESH, "立即刷新设备状态", MenuKind::Action));

    v.push(separator(0));
    v.push(toggle(IDM_TOGGLE_BALL, "桌面悬浮球", ball_enabled));
    v.push(toggle(IDM_TOGGLE_AUTOSTART, "开机自动启动", auto_start));

    v.push(separator(1));
    let update = crate::update::snapshot();
    if crate::update::has_update() {
        let latest = update.latest.clone().unwrap_or_default();
        v.push(item(IDM_APPLY_UPDATE, format!("立即升级到 v{}", latest), MenuKind::Action));
    } else if update.checking {
        let mut s = item(IDM_CHECK_UPDATE, "正在检查新版本…", MenuKind::Action);
        s.dim = true;
        v.push(s);
    } else {
        v.push(item(
            IDM_CHECK_UPDATE,
            format!("检查更新 (当前 v{})", crate::update::current_version()),
            MenuKind::Action,
        ));
    }

    v.push(separator(2));
    v.push(item(IDM_EXIT, "退出 LinkDeck", MenuKind::Danger));

    v
}

/// 文本宽度（屏幕 DC，不需要窗口句柄）
unsafe fn measure_text(text: &str, bold: bool) -> i32 {
    let hdc = GetDC(ptr::null_mut());
    let f = create_clear_font(MENU_FONT + if bold { 1 } else { 0 }, bold);
    let old = SelectObject(hdc, f);
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    DrawTextW(hdc, wide.as_ptr(), -1, &mut rc, DT_CALCRECT | DT_SINGLELINE);
    SelectObject(hdc, old);
    DeleteObject(f);
    ReleaseDC(ptr::null_mut(), hdc);
    rc.right
}

// --------------------------------------------------------------------------
// 菜单窗口
// --------------------------------------------------------------------------

struct MenuWindow {
    hwnd: HWND,
    /// 命令要回投给谁
    host: HWND,
    items: Vec<MenuSpec>,
    rows: Vec<RowGeom>,
    hover: Option<usize>,
    /// 鼠标按下的行：抬起必须还在同一行才算选中
    pressed: Option<usize>,
}

static mut MENU: Option<MenuWindow> = None;

const MENU_CLASS: &str = "LinkDeck_TrayMenu\0";

/// 点击判定：按下与抬起必须落在同一行（标准菜单行为）。
/// 按下后滑到别处再松开不算选中，避免手一抖就执行了「退出」。
pub fn resolve_click(pressed: Option<usize>, released: Option<usize>) -> Option<usize> {
    match (pressed, released) {
        (Some(a), Some(b)) if a == b => Some(a),
        _ => None,
    }
}

/// 关掉菜单。`cmd` 非 0 时把命令回投给宿主窗口。
///
/// 先 `take` 再销毁窗口：销毁过程中会重入 `WM_CAPTURECHANGED` / `WM_DESTROY`，
/// 此时 `MENU` 已经是 `None`，这些消息就不会再触发一次关闭。
unsafe fn dismiss_menu(cmd: usize) {
    let Some(m) = MENU.take() else {
        return;
    };
    ReleaseCapture();
    if !m.hwnd.is_null() {
        DestroyWindow(m.hwnd);
    }
    if cmd > 0 && !m.host.is_null() {
        PostMessageW(m.host, WM_COMMAND, cmd as WPARAM, 0);
    }
}

unsafe extern "system" fn menu_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_menu(hwnd);
            0
        }
        // 点击不抢焦点：抢焦点会触发 WM_KILLFOCUS，菜单还没响应点击就先自己关了
        WM_MOUSEACTIVATE => 3, // MA_NOACTIVATE
        WM_MOUSEMOVE => {
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let new_hover = MENU.as_ref().and_then(|m| hit_row(&m.rows, y));
            let changed = MENU.as_ref().map(|m| m.hover) != Some(new_hover);
            if let Some(m) = MENU.as_mut() {
                m.hover = new_hover;
            }
            if changed {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            if let Some(m) = MENU.as_mut() {
                m.pressed = hit_row(&m.rows, y);
                m.hover = m.pressed;
            }
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_LBUTTONUP => {
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let (pressed, released, id) = match MENU.as_ref() {
                Some(m) => {
                    let released = hit_row(&m.rows, y);
                    let picked = resolve_click(m.pressed, released);
                    let id = picked.map(|i| m.items[m.rows[i].spec].id).unwrap_or(0);
                    (m.pressed, released, id)
                }
                None => (None, None, 0),
            };
            if pressed.is_some() && released.is_some() && id > 0 {
                dismiss_menu(id);
            } else {
                // 抬起时不在任何一行上（含点在菜单外）→ 直接关掉
                dismiss_menu(0);
            }
            0
        }
        WM_RBUTTONUP => {
            dismiss_menu(0);
            0
        }
        WM_KEYDOWN => match wparam as u32 {
            0x1B => {
                dismiss_menu(0);
                0
            }
            0x0D => {
                let id = MENU
                    .as_ref()
                    .and_then(|m| m.hover.map(|i| m.items[m.rows[i].spec].id))
                    .unwrap_or(0);
                dismiss_menu(id);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        },
        // 鼠标捕获被别人抢走（切窗口、弹出其他对话框）：菜单跟着消失
        WM_CAPTURECHANGED => {
            if MENU.is_some() {
                dismiss_menu(0);
            }
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_menu(hwnd: HWND) {
    let Some(m) = MENU.as_ref() else {
        return;
    };
    let mut ps: PAINTSTRUCT = mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut rc);

    let bg = CreateSolidBrush(THEME.card);
    FillRect(hdc, &rc, bg);
    DeleteObject(bg);

    SetBkMode(hdc, TRANSPARENT as i32);
    for (idx, row) in m.rows.iter().enumerate() {
        let spec = &m.items[row.spec];
        if row.sep {
            let line = CreateSolidBrush(THEME.border);
            let mid = row.y + row.h / 2;
            let r = RECT { left: 10, top: mid, right: rc.right - 10, bottom: mid + 1 };
            FillRect(hdc, &r, line);
            DeleteObject(line);
            continue;
        }
        if m.hover == Some(idx) {
            let brush = CreateSolidBrush(THEME.btn_hover);
            let r = RECT {
                left: MENU_ROW_INSET,
                top: row.y,
                right: rc.right - MENU_ROW_INSET,
                bottom: row.y + row.h,
            };
            FillRect(hdc, &r, brush);
            DeleteObject(brush);
            let bar = CreateSolidBrush(THEME.accent);
            let b = RECT {
                left: MENU_ROW_INSET,
                top: row.y + 4,
                right: MENU_ROW_INSET + 3,
                bottom: row.y + row.h - 4,
            };
            FillRect(hdc, &b, bar);
            DeleteObject(bar);
        }

        let bold = matches!(spec.kind, MenuKind::Primary);
        let f = create_clear_font(MENU_FONT + if bold { 1 } else { 0 }, bold);
        let old = SelectObject(hdc, f);
        let clr = match spec.kind {
            MenuKind::Danger => THEME.danger,
            _ if spec.dim => THEME.muted,
            _ => THEME.text,
        };
        SetTextColor(hdc, clr);
        let mut r = RECT {
            left: MENU_PAD_X,
            top: row.y,
            right: rc.right - MENU_PAD_X,
            bottom: row.y + row.h,
        };
        draw_text_clean(hdc, &spec.label, &mut r, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
        if let Some(hint) = &spec.hint {
            SetTextColor(hdc, if spec.on { THEME.accent } else { THEME.muted });
            draw_text_clean(hdc, hint, &mut r, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);
        }
        SelectObject(hdc, old);
        DeleteObject(f);
    }

    // 圆角描边：region 已经把四角裁圆，这里补一圈发丝线勾出边界
    let pen_brush = CreateSolidBrush(THEME.border);
    let edge = RECT { left: 0, top: 0, right: rc.right, bottom: 1 };
    FillRect(hdc, &edge, pen_brush);
    let edge = RECT { left: 0, top: rc.bottom - 1, right: rc.right, bottom: rc.bottom };
    FillRect(hdc, &edge, pen_brush);
    let edge = RECT { left: 0, top: 0, right: 1, bottom: rc.bottom };
    FillRect(hdc, &edge, pen_brush);
    let edge = RECT { left: rc.right - 1, top: 0, right: rc.right, bottom: rc.bottom };
    FillRect(hdc, &edge, pen_brush);
    DeleteObject(pen_brush);

    EndPaint(hwnd, &ps);
}

/// 注册窗口类（重复调用无害）
unsafe fn ensure_menu_class() {
    let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(ptr::null());
    let class_name: Vec<u16> = MENU_CLASS.encode_utf16().collect();
    let mut wcx: WNDCLASSEXW = mem::zeroed();
    wcx.cbSize = mem::size_of::<WNDCLASSEXW>() as u32;
    wcx.style = 0;
    wcx.lpfnWndProc = Some(menu_wnd_proc);
    wcx.hInstance = instance;
    wcx.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
    wcx.hbrBackground = ptr::null_mut();
    wcx.lpszClassName = class_name.as_ptr();
    RegisterClassExW(&wcx);
}

// --------------------------------------------------------------------------
// 托盘图标
// --------------------------------------------------------------------------

pub struct TrayIcon {
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
                windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(
                    ptr::null_mut(),
                    windows_sys::Win32::UI::WindowsAndMessaging::IDI_APPLICATION,
                )
            };
            Self::copy_tooltip(&mut nid.szTip, tooltip);

            Shell_NotifyIconW(NIM_ADD, &nid);

            Self { nid }
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

    /// 弹出自绘菜单，返回被选中的命令 id（0 = 取消）。
    /// 弹出托盘菜单（非模态）。
    ///
    /// 之前的实现会在这个函数里跑一个局部消息循环，把菜单当模态窗口用；
    /// 那种写法一旦碰上「窗口没拿到前台/焦点」，点击就落不到菜单项上，
    /// 表现为点了没反应。现在改成非模态：菜单自己处理点击，
    /// 选中后把 `WM_COMMAND` 回投给宿主窗口，主消息循环完全不受影响。
    pub fn show_context_menu(
        host: HWND,
        ball_enabled: bool,
        auto_start: bool,
        _font_scale: f32,
        _poll_interval_ms: u64,
    ) {
        unsafe {
            if MENU.is_some() {
                dismiss_menu(0);
            }

            let items = build_specs(ball_enabled, auto_start);
            let rows = layout_rows(&items);
            let width = menu_width(&items, |t, bold| measure_text(t, bold));
            let height = rows.last().map(|r| r.y + r.h + MENU_PAD_Y).unwrap_or(MENU_PAD_Y * 2);

            let mut cursor: POINT = mem::zeroed();
            GetCursorPos(&mut cursor);
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let (x, y) = menu_origin(cursor, width, height, screen_w, screen_h);

            // 托盘菜单的标准做法：先把宿主窗口提到前台，
            // 否则后台进程弹的窗口收不到输入，点击会直接落到别的窗口上。
            SetForegroundWindow(host);

            ensure_menu_class();
            let class_name: Vec<u16> = MENU_CLASS.encode_utf16().collect();
            let hwnd_menu = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name.as_ptr(),
                ptr::null(),
                WS_POPUP,
                x,
                y,
                width,
                height,
                host,
                ptr::null_mut(),
                windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(ptr::null()),
                ptr::null(),
            );
            if hwnd_menu.is_null() {
                return;
            }

            // 圆角：窗口自身的形状裁掉直角，系统那圈浅色边框也就不存在了
            let rgn = CreateRoundRectRgn(0, 0, width + 1, height + 1, MENU_RADIUS, MENU_RADIUS);
            SetWindowRgn(hwnd_menu, rgn, 1);

            MENU = Some(MenuWindow {
                hwnd: hwnd_menu,
                host,
                items,
                rows,
                hover: None,
                pressed: None,
            });

            ShowWindow(hwnd_menu, 4); // SW_SHOWNOACTIVATE
            SetWindowPos(hwnd_menu, hwnd_menu, x, y, width, height, 0x0004 | 0x0010 | 0x0040);
            // 捕获鼠标：点菜单外也能收到一条按键消息，据此关闭
            SetCapture(hwnd_menu);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> Vec<MenuSpec> {
        vec![
            item(IDM_SHOW_PANEL, "外设控制中心", MenuKind::Primary),
            item(IDM_SHOW_SETTINGS, "偏好设置", MenuKind::Action),
            separator(0),
            toggle(IDM_TOGGLE_BALL, "桌面悬浮球", true),
            item(IDM_EXIT, "退出 LinkDeck", MenuKind::Danger),
        ]
    }

    #[test]
    fn row_heights_follow_the_kind() {
        assert!(row_height(&MenuKind::Primary) > row_height(&MenuKind::Action));
        assert!(row_height(&MenuKind::Separator) < row_height(&MenuKind::Action));
    }

    #[test]
    fn rows_stack_without_gaps_or_overlap() {
        let rows = layout_rows(&specs());
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].y, MENU_PAD_Y);
        for w in rows.windows(2) {
            assert_eq!(w[0].y + w[0].h, w[1].y, "行之间不能留缝也不能重叠");
        }
        assert!(rows.iter().any(|r| r.sep));
    }

    #[test]
    fn width_covers_the_widest_label_plus_hint() {
        // 宽度取最宽的一行 + 两侧内边距，并且不小于最小宽度
        let w = menu_width(&specs(), |t, _| t.chars().count() as i32 * 10);
        assert_eq!(w, (60 + MENU_PAD_X * 2).max(160));
        // 把某一行拉得很宽时，菜单跟着变宽（内容不被截断）
        let long = "一条特别特别特别特别长的菜单项";
        let wide = vec![item(1, long, MenuKind::Action)];
        let w2 = menu_width(&wide, |t, _| t.chars().count() as i32 * 10);
        assert_eq!(w2, long.chars().count() as i32 * 10 + MENU_PAD_X * 2);
        // 状态文字也计入宽度
        let only_toggle = vec![toggle(1, "悬浮球", true)];
        let w3 = menu_width(&only_toggle, |t, _| t.chars().count() as i32 * 10);
        assert_eq!(w3, (30 + MENU_HINT_GAP + 30 + MENU_PAD_X * 2).max(160));
    }

    #[test]
    fn separators_are_not_clickable() {
        let rows = layout_rows(&specs());
        let sep = rows.iter().position(|r| r.sep).unwrap();
        let y = rows[sep].y + 2;
        assert_eq!(hit_row(&rows, y), None, "分隔线不该被选中");
        // 分隔线上下相邻的两项仍可点
        assert!(hit_row(&rows, rows[sep - 1].y + 2).is_some());
        assert!(hit_row(&rows, rows[sep + 1].y + 2).is_some());
    }

    /// 回归：按下与抬起不在同一行时不能执行命令。
    /// 「点退出没反应」修好后再加这道锁——不能让按下即执行的旧行为回来。
    #[test]
    fn click_requires_press_and_release_on_the_same_row() {
        assert_eq!(resolve_click(Some(2), Some(2)), Some(2));
        assert_eq!(resolve_click(Some(2), Some(3)), None, "滑到别的行松开不算选中");
        assert_eq!(resolve_click(Some(2), None), None, "抬起到菜单外不算选中");
        assert_eq!(resolve_click(None, Some(2)), None);
        assert_eq!(resolve_click(None, None), None);
    }

    #[test]
    fn menu_stays_inside_the_screen() {
        let (w, h) = (240, 260);
        // 光标在右下角：菜单必须翻到光标左上，不能溢出屏幕
        let (x, y) = menu_origin(POINT { x: 1900, y: 1070 }, w, h, 1920, 1080);
        assert!(x >= MENU_SCREEN_MARGIN && x + w <= 1920 - MENU_SCREEN_MARGIN);
        assert!(y >= MENU_SCREEN_MARGIN && y + h <= 1080 - MENU_SCREEN_MARGIN);
        // 光标在左上角：菜单向下展开
        let (x2, y2) = menu_origin(POINT { x: 10, y: 10 }, w, h, 1920, 1080);
        assert!(x2 >= MENU_SCREEN_MARGIN);
        assert!(y2 >= MENU_SCREEN_MARGIN && y2 + h <= 1080);
    }
}
