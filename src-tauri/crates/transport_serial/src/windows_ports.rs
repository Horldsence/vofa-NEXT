//! Windows 串口 Description 枚举
//!
//! `serialport` crate 的 UsbPortInfo 只暴露 product/manufacturer/serial_number，
//! 不读取设备管理器中的 Description (SPDRP_DEVICEDESC)。本模块使用 SetupAPI
//! 自行枚举 Ports / Modem 两类设备，建立 `COM端口名 -> Description` 映射。

use std::collections::HashMap;
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDevRegKey, DICS_FLAG_GLOBAL, DIGCF_PRESENT,
    DIREG_DEV, SPDRP_DEVICEDESC, SP_DEVINFO_DATA,
};
use windows_sys::Win32::Foundation::{BOOL, ERROR_SUCCESS, HWND, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Registry::{RegCloseKey, RegQueryValueExW, HKEY, KEY_READ};

/// 设备类 GUID: Ports (COM & LPT ports)
const GUID_DEVCLASS_PORTS: GUID = GUID {
    data1: 0x4D36E978,
    data2: 0xE325,
    data3: 0x11CE,
    data4: [0xBF, 0xC1, 0x08, 0x00, 0x2B, 0xE1, 0x03, 0x18],
};

/// 设备类 GUID: Modem
const GUID_DEVCLASS_MODEM: GUID = GUID {
    data1: 0x4D36E96D,
    data2: 0xE325,
    data3: 0x11CE,
    data4: [0xBF, 0xC1, 0x08, 0x00, 0x2B, 0xE1, 0x03, 0x18],
};

/// 读取设备管理器中的 Description，建立端口名映射。
///
/// 非 Windows 平台由 `#[cfg(windows)]` 控制，不会编译本文件；
/// 为保持接口一致，这里仍提供一个空映射的桩函数占位说明。
pub fn port_descriptions() -> HashMap<String, String> {
    let mut map = HashMap::new();

    for guid in [&GUID_DEVCLASS_PORTS, &GUID_DEVCLASS_MODEM] {
        unsafe {
            let info_set = SetupDiGetClassDevsW(guid, std::ptr::null(), 0 as HWND, DIGCF_PRESENT);
            if info_set == INVALID_HANDLE_VALUE {
                continue;
            }

            let mut dev_info = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ClassGuid: GUID {
                    data1: 0,
                    data2: 0,
                    data3: 0,
                    data4: [0; 8],
                },
                DevInst: 0,
                Reserved: 0,
            };

            let mut index = 0u32;
            while SetupDiEnumDeviceInfo(info_set, index, &mut dev_info) != 0 {
                index += 1;

                // 打开设备注册表键，读取 PortName
                let hkey = SetupDiOpenDevRegKey(
                    info_set,
                    &mut dev_info,
                    DICS_FLAG_GLOBAL,
                    0,
                    DIREG_DEV,
                    KEY_READ,
                );
                if hkey == 0 {
                    continue;
                }

                let port_name = match read_reg_string(hkey, "PortName") {
                    Some(name) if !name.is_empty() => name,
                    _ => {
                        let _ = RegCloseKey(hkey);
                        continue;
                    }
                };
                let _ = RegCloseKey(hkey);

                // 过滤并口（LPT）
                if port_name.to_ascii_uppercase().starts_with("LPT") {
                    continue;
                }

                // 读取 SPDRP_DEVICEDESC
                if let Some(description) = read_device_description(info_set, &mut dev_info) {
                    if !description.is_empty() {
                        map.insert(port_name, description);
                    }
                }
            }

            let _ = SetupDiDestroyDeviceInfoList(info_set);
        }
    }

    map
}

/// 从注册表键读取 UTF-16 字符串值
unsafe fn read_reg_string(hkey: HKEY, value_name: &str) -> Option<String> {
    let name_wide: Vec<u16> = value_name.encode_utf16().chain(Some(0)).collect();
    let mut buf_len: u32 = 0;
    let mut value_type: u32 = 0;

    let ret = RegQueryValueExW(
        hkey,
        name_wide.as_ptr(),
        std::ptr::null_mut(),
        &mut value_type,
        std::ptr::null_mut(),
        &mut buf_len,
    );
    if ret != ERROR_SUCCESS || buf_len == 0 {
        return None;
    }

    let mut buf: Vec<u8> = vec![0; buf_len as usize];
    let ret = RegQueryValueExW(
        hkey,
        name_wide.as_ptr(),
        std::ptr::null_mut(),
        &mut value_type,
        buf.as_mut_ptr(),
        &mut buf_len,
    );
    if ret != ERROR_SUCCESS {
        return None;
    }

    if value_type != 1 {
        // REG_SZ
        return None;
    }

    let wide: &[u16] = std::slice::from_raw_parts(
        buf.as_ptr() as *const u16,
        (buf_len as usize) / std::mem::size_of::<u16>(),
    );
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16(&wide[..end]).ok()
}

/// 读取 SPDRP_DEVICEDESC 属性
unsafe fn read_device_description(
    info_set: isize,
    dev_info: &mut SP_DEVINFO_DATA,
) -> Option<String> {
    let mut data_type: u32 = 0;
    let mut buf_len: u32 = 0;

    let ok: BOOL = SetupDiGetDeviceRegistryPropertyW(
        info_set,
        dev_info,
        SPDRP_DEVICEDESC,
        &mut data_type,
        std::ptr::null_mut(),
        0,
        &mut buf_len,
    );
    if ok == 0 && buf_len == 0 {
        return None;
    }

    let mut buf: Vec<u8> = vec![0; buf_len as usize];
    let ok = SetupDiGetDeviceRegistryPropertyW(
        info_set,
        dev_info,
        SPDRP_DEVICEDESC,
        &mut data_type,
        buf.as_mut_ptr(),
        buf_len,
        &mut buf_len,
    );
    if ok == 0 {
        return None;
    }

    if data_type != 1 {
        // REG_SZ
        return None;
    }

    let wide: &[u16] = std::slice::from_raw_parts(
        buf.as_ptr() as *const u16,
        (buf_len as usize) / std::mem::size_of::<u16>(),
    );
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16(&wide[..end]).ok()
}
