use std::ptr;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

pub unsafe fn copy_text_to_clipboard(hwnd: HWND, text: &str) -> bool {
    if OpenClipboard(hwnd) == 0 {
        return false;
    }
    EmptyClipboard();
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if !h_mem.is_null() {
        let p = GlobalLock(h_mem) as *mut u16;
        if !p.is_null() {
            ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            GlobalUnlock(h_mem);
            SetClipboardData(13 /* CF_UNICODETEXT */, h_mem as _);
        }
    }
    CloseClipboard();
    true
}
