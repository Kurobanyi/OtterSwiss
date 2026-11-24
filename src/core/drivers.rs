use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW, DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO,
    SETUP_DI_REGISTRY_PROPERTY, SPDRP_DEVICEDESC, SPDRP_HARDWAREID, SPDRP_MFG, SP_DEVINFO_DATA,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::win_common::AlignedBuf;

#[derive(Debug, Clone)]
pub struct InstalledDriverDevice {
    pub device_desc: Option<String>,
    pub manufacturer: Option<String>,
    pub hardware_id: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
}

pub struct DriverManager;

impl DriverManager {
    pub fn list_present_devices() -> CoreResult<Vec<InstalledDriverDevice>> {
        let h_dev_info: HDEVINFO = unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("SetupDiGetClassDevsW falló: {}", e),
            source: None,
        })?;

        let mut devices = Vec::new();
        let mut dev_data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        let mut member_idx = 0u32;
        loop {
            let ok = unsafe { SetupDiEnumDeviceInfo(h_dev_info, member_idx, &mut dev_data) };

            if ok.is_err() {
                break;
            }

            let desc = get_device_property(h_dev_info, &mut dev_data, SPDRP_DEVICEDESC);
            let mfg = get_device_property(h_dev_info, &mut dev_data, SPDRP_MFG);
            let hw_id = get_device_property(h_dev_info, &mut dev_data, SPDRP_HARDWAREID);

            if desc.is_some() || hw_id.is_some() {
                devices.push(InstalledDriverDevice {
                    device_desc: desc,
                    manufacturer: mfg,
                    hardware_id: hw_id,
                    driver_version: None,
                    driver_date: None,
                });
            }

            member_idx += 1;
        }

        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(h_dev_info);
        }

        Ok(devices)
    }
}

fn get_device_property(
    h_dev_info: HDEVINFO,
    dev_data: &mut SP_DEVINFO_DATA,
    property: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<String> {
    let mut required_size = 0u32;
    let _ = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            h_dev_info,
            dev_data,
            property,
            None,
            None,
            Some(&mut required_size),
        )
    };

    if required_size == 0 {
        return None;
    }

    let mut buf = AlignedBuf::zeroed(required_size as usize);
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            h_dev_info,
            dev_data,
            property,
            None,
            Some(&mut buf[..]),
            Some(&mut required_size),
        )
    };

    if ok.is_ok() && required_size > 1 {
        let u16_slice = unsafe {
            std::slice::from_raw_parts(buf.as_ptr() as *const u16, required_size as usize / 2)
        };
        let end = u16_slice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(u16_slice.len());
        let val = String::from_utf16_lossy(&u16_slice[..end])
            .trim()
            .to_string();
        if !val.is_empty() {
            return Some(val);
        }
    }

    None
}
