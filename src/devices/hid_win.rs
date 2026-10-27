use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::time::Duration;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    HDEVINFO, SP_DEVICE_INTERFACE_DATA,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetFeature, HidD_GetHidGuid,
    HidD_GetInputReport, HidD_GetManufacturerString, HidD_GetPreparsedData, HidD_GetProductString,
    HidD_SetFeature, HidD_SetOutputReport, HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, ResetEvent, WaitForSingleObject,
};
use windows_sys::Win32::System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED};

#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub version_number: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
}

pub struct HidDevice {
    pub handle: HANDLE,
    pub info: HidDeviceInfo,
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE as _ {
            unsafe { CloseHandle(self.handle) };
        }
    }
}

unsafe impl Send for HidDevice {}

pub fn enumerate_hid_devices() -> Vec<HidDeviceInfo> {
    let mut result = Vec::new();

    unsafe {
        let mut hid_guid = mem::zeroed();
        HidD_GetHidGuid(&mut hid_guid);

        let dev_info: HDEVINFO = SetupDiGetClassDevsW(
            &hid_guid,
            ptr::null(),
            ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if dev_info == -1isize {
            return result;
        }

        let mut interface_data: SP_DEVICE_INTERFACE_DATA = mem::zeroed();
        interface_data.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

        let mut index = 0;
        while SetupDiEnumDeviceInterfaces(
            dev_info,
            ptr::null(),
            &hid_guid,
            index,
            &mut interface_data,
        ) != 0
        {
            index += 1;

            let mut required_size = 0u32;
            SetupDiGetDeviceInterfaceDetailW(
                dev_info,
                &interface_data,
                ptr::null_mut(),
                0,
                &mut required_size,
                ptr::null_mut(),
            );

            if required_size == 0 {
                continue;
            }

            let mut buffer: Vec<u8> = vec![0; required_size as usize];
            let detail_ptr = buffer.as_mut_ptr() as *mut u8;

            #[cfg(target_pointer_width = "64")]
            let cb_size = 8u32;
            #[cfg(target_pointer_width = "32")]
            let cb_size = 5u32;

            *(detail_ptr as *mut u32) = cb_size;

            if SetupDiGetDeviceInterfaceDetailW(
                dev_info,
                &interface_data,
                detail_ptr as _,
                required_size,
                ptr::null_mut(),
                ptr::null_mut(),
            ) != 0
            {
                let path_start = detail_ptr.add(4) as *const u16;
                let mut path_len = 0;
                while *path_start.add(path_len) != 0 {
                    path_len += 1;
                }
                let slice = std::slice::from_raw_parts(path_start, path_len);
                let path = String::from_utf16_lossy(slice);

                if let Some(info) = inspect_hid_path(&path) {
                    result.push(info);
                }
            }
        }

        SetupDiDestroyDeviceInfoList(dev_info);
    }

    result
}

pub fn inspect_hid_path(path: &str) -> Option<HidDeviceInfo> {
    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let handle = CreateFileW(
            wide_path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );

        if handle.is_null() || handle == INVALID_HANDLE_VALUE as _ {
            return None;
        }

        let mut attrib: HIDD_ATTRIBUTES = mem::zeroed();
        attrib.Size = mem::size_of::<HIDD_ATTRIBUTES>() as u32;
        let mut caps: HIDP_CAPS = mem::zeroed();

        let has_attrib = HidD_GetAttributes(handle, &mut attrib) != 0;

        let mut preparsed = 0 as isize;
        let mut has_caps = false;
        if HidD_GetPreparsedData(handle, &mut preparsed) != 0 {
            let status = HidP_GetCaps(preparsed, &mut caps);
            if status == 0x00110000 || status == 0 {
                has_caps = true;
            }
            HidD_FreePreparsedData(preparsed);
        }

        CloseHandle(handle);

        if has_attrib && has_caps {
            Some(HidDeviceInfo {
                path: path.to_string(),
                vendor_id: attrib.VendorID,
                product_id: attrib.ProductID,
                version_number: attrib.VersionNumber,
                usage_page: caps.UsagePage,
                usage: caps.Usage,
                input_len: caps.InputReportByteLength,
                output_len: caps.OutputReportByteLength,
                feature_len: caps.FeatureReportByteLength,
            })
        } else {
            None
        }
    }
}

impl HidDevice {
    pub fn open(info: HidDeviceInfo, overlapped: bool) -> Result<Self, String> {
        let wide_path: Vec<u16> = info.path.encode_utf16().chain(std::iter::once(0)).collect();
        let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { 0 };

        unsafe {
            let mut handle = CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                flags,
                ptr::null_mut(),
            );

            // If access denied (e.g. system keyboard/mouse collection), fallback to query access (0)
            if handle.is_null() || handle == INVALID_HANDLE_VALUE as _ {
                let err = GetLastError();
                if err == 5 { // ERROR_ACCESS_DENIED
                    handle = CreateFileW(
                        wide_path.as_ptr(),
                        0,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                        ptr::null(),
                        OPEN_EXISTING,
                        flags,
                        ptr::null_mut(),
                    );
                }
            }

            if handle.is_null() || handle == INVALID_HANDLE_VALUE as _ {
                return Err(format!("Failed to open HID device: {}", GetLastError()));
            }

            Ok(Self { handle, info })
        }
    }

    pub fn get_feature(&self, buf: &mut [u8]) -> Result<(), String> {
        unsafe {
            if HidD_GetFeature(self.handle, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) != 0
            {
                Ok(())
            } else {
                Err(format!("HidD_GetFeature failed: {}", GetLastError()))
            }
        }
    }

    pub fn set_feature(&self, buf: &[u8]) -> Result<(), String> {
        unsafe {
            if HidD_SetFeature(self.handle, buf.as_ptr() as *mut c_void, buf.len() as u32) != 0 {
                Ok(())
            } else {
                Err(format!("HidD_SetFeature failed: {}", GetLastError()))
            }
        }
    }
    pub fn set_output_report(&self, buf: &[u8]) -> Result<(), String> {
        unsafe {
            if HidD_SetOutputReport(self.handle, buf.as_ptr() as *mut c_void, buf.len() as u32) != 0 {
                Ok(())
            } else {
                Err(format!("HidD_SetOutputReport failed: {}", GetLastError()))
            }
        }
    }

    pub fn get_input_report(&self, buf: &mut [u8]) -> Result<(), String> {
        unsafe {
            if HidD_GetInputReport(self.handle, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) != 0 {
                Ok(())
            } else {
                Err(format!("HidD_GetInputReport failed: {}", GetLastError()))
            }
        }
    }

    pub fn write_report(&self, buf: &[u8]) -> Result<(), String> {
        unsafe {
            let mut bytes_written = 0u32;
            if WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut bytes_written,
                ptr::null_mut(),
            ) != 0 && bytes_written > 0 {
                return Ok(());
            }

            if HidD_SetOutputReport(self.handle, buf.as_ptr() as *mut c_void, buf.len() as u32) != 0 {
                return Ok(());
            }

            if HidD_SetFeature(self.handle, buf.as_ptr() as *mut c_void, buf.len() as u32) != 0 {
                return Ok(());
            }

            Err(format!("write_report failed: {}", GetLastError()))
        }
    }

    pub fn read_report(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, String> {
        if let Ok(n) = self.read_overlapped(buf, timeout) {
            if n > 0 {
                return Ok(n);
            }
        }

        unsafe {
            if HidD_GetInputReport(self.handle, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) != 0 {
                return Ok(buf.len());
            }

            if HidD_GetFeature(self.handle, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) != 0 {
                return Ok(buf.len());
            }

            Err(format!("read_report failed: {}", GetLastError()))
        }
    }

    /// 在 overlapped 句柄上写入报文并限时等待完成。
    ///
    /// 部分接收器（如 GANSS 2.4G）的同步 WriteFile 会永久阻塞，必须走
    /// FILE_FLAG_OVERLAPPED + 事件等待，超时后用 CancelIo 收尾。
    pub fn write_overlapped(&self, buf: &[u8], timeout: Duration) -> Result<usize, String> {
        unsafe {
            let event = CreateEventW(ptr::null(), 1, 0, ptr::null());
            if event.is_null() {
                return Err("Failed to create event".into());
            }

            let mut ov: OVERLAPPED = mem::zeroed();
            ov.hEvent = event;

            let mut bytes_written = 0u32;
            let ok = WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut bytes_written,
                &mut ov,
            );

            if ok == 0 {
                let err = GetLastError();
                if err != ERROR_IO_PENDING {
                    CloseHandle(event);
                    return Err(format!("WriteFile failed: {}", err));
                }

                let wait_res = WaitForSingleObject(event, timeout.as_millis() as u32);
                if wait_res == WAIT_OBJECT_0 {
                    let mut transferred = 0u32;
                    let got = GetOverlappedResult(self.handle, &ov, &mut transferred, 0);
                    CloseHandle(event);
                    if got != 0 {
                        Ok(transferred as usize)
                    } else {
                        Err(format!("GetOverlappedResult failed: {}", GetLastError()))
                    }
                } else {
                    CancelIo(self.handle);
                    ResetEvent(event);
                    CloseHandle(event);
                    Err("Timed out writing HID report".into())
                }
            } else {
                CloseHandle(event);
                Ok(bytes_written as usize)
            }
        }
    }

    pub fn read_overlapped(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, String> {
        unsafe {
            let event = CreateEventW(ptr::null(), 1, 0, ptr::null());
            if event.is_null() {
                return Err("Failed to create event".into());
            }

            let mut ov: OVERLAPPED = mem::zeroed();
            ov.hEvent = event;

            let mut bytes_read = 0u32;
            let ok = ReadFile(
                self.handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut bytes_read,
                &mut ov,
            );

            if ok == 0 {
                let err = GetLastError();
                if err != ERROR_IO_PENDING {
                    CloseHandle(event);
                    return Err(format!("ReadFile failed: {}", err));
                }

                let wait_ms = timeout.as_millis() as u32;
                let wait_res = WaitForSingleObject(event, wait_ms);

                if wait_res == WAIT_OBJECT_0 {
                    let mut transferred = 0u32;
                    let got = GetOverlappedResult(self.handle, &ov, &mut transferred, 0);
                    CloseHandle(event);
                    if got != 0 {
                        Ok(transferred as usize)
                    } else {
                        Err(format!("GetOverlappedResult failed: {}", GetLastError()))
                    }
                } else {
                    CancelIo(self.handle);
                    ResetEvent(event);
                    CloseHandle(event);
                    Err("Timed out waiting for HID report".into())
                }
            } else {
                CloseHandle(event);
                Ok(bytes_read as usize)
            }
        }
    }

    pub fn get_product_string(&self) -> Option<String> {
        unsafe {
            let mut buf = [0u16; 256];
            if HidD_GetProductString(
                self.handle,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 2) as u32,
            ) != 0
            {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
            None
        }
    }

    pub fn get_manufacturer_string(&self) -> Option<String> {
        unsafe {
            let mut buf = [0u16; 256];
            if HidD_GetManufacturerString(
                self.handle,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 2) as u32,
            ) != 0
            {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
            None
        }
    }
}
