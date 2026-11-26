//! Dispositivos presentes (SetupAPI y Configuration Manager) y respaldo del Driver Store.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, CR_SUCCESS, DN_HAS_PROBLEM,
    DN_NEED_RESTART, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_CLASS,
    SPDRP_DEVICEDESC, SPDRP_DRIVER, SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID, SPDRP_MFG,
    SetupDiEnumDeviceInfo, SetupDiGetDeviceRegistryPropertyW,
};
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use super::hwid::{short_hw_id_of, ven_dev_of};
use super::{DriverManager, PNPUTIL_TIMEOUT};
use crate::core::error::{CoreError, CoreResult};
use crate::core::hardware::pci::vendor_name_hex;
use crate::core::platform::handles::DevInfoList;
use crate::core::platform::memory::AlignedBuf;
use crate::core::platform::process::{run_captured, system32_dir};
use crate::core::security::acl::create_admin_only_dir;
use crate::core::security::data_dir::ensure_admin_only_data_dir;

#[derive(Debug, Clone)]
pub struct InstalledDriverDevice {
    pub device_desc: Option<String>,
    pub friendly_name: Option<String>,
    pub manufacturer: Option<String>,
    pub provider_name: Option<String>,
    pub class_name: Option<String>,
    pub hardware_id: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    pub inf_path: Option<String>,
    pub dev_inst: u32,
    pub has_problem: bool,
    pub problem_code: u32,
    pub needs_reboot: bool,
}

impl InstalledDriverDevice {
    pub fn display_title(&self) -> &str {
        self.friendly_name
            .as_deref()
            .or(self.device_desc.as_deref())
            .unwrap_or("Dispositivo desconocido")
    }

    pub fn is_third_party_oem(&self) -> bool {
        self.inf_path
            .as_deref()
            .map(|p| p.to_ascii_lowercase().starts_with("oem"))
            .unwrap_or(false)
    }

    /// ID corto para buscar en el Catálogo: `PCI\VEN_x&DEV_y`, `USB\VID_x&PID_y` o los dos primeros campos.
    pub fn short_hw_id(&self) -> Option<String> {
        short_hw_id_of(self.hardware_id.as_deref()?)
    }

    /// `(fabricante, dispositivo, subsistema)` del Hardware ID (VEN/DEV o VID/PID).
    pub(crate) fn extract_ven_dev(&self) -> Option<(String, String, Option<String>)> {
        ven_dev_of(self.hardware_id.as_deref()?)
    }

    pub fn decoded_vendor_name(&self) -> Option<&'static str> {
        let (ven, _, _) = self.extract_ven_dev()?;
        vendor_name_hex(&ven)
    }

    pub fn hardware_hint(&self) -> Option<&'static str> {
        let (ven, dev, _) = self.extract_ven_dev()?;
        if ven == "8086" {
            match dev.as_str() {
                "7D1D" | "AD1D" | "643E" | "B03E" => return Some("Intel® NPU (AI Boost)"),
                "7E4C" | "A74F" | "464F" | "9A11" | "7D4C" => {
                    return Some("Intel® GNA Scoring Accelerator");
                }
                "A77D" | "7D60" | "4660" | "51EE" | "7A68" | "7E70" | "5468" => {
                    return Some("Intel® Management Engine / Serial IO");
                }
                "461D" | "A71D" | "7D03" | "9A03" | "641D" => {
                    return Some("Intel® Dynamic Tuning (DTT)");
                }
                "09AB" | "467F" | "7D0B" | "9A0B" => return Some("Intel® VMD / RST Storage"),
                _ => {}
            }
        }
        None
    }

    pub fn friendly_gpu_version(&self) -> Option<String> {
        let ver = self.driver_version.as_deref()?;
        let is_nvidia = self
            .provider_name
            .as_deref()
            .or(self.manufacturer.as_deref())
            .map(|s| s.to_ascii_lowercase().contains("nvidia"))
            .unwrap_or(false);

        if !is_nvidia {
            return None;
        }

        let digits: String = ver.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 5 {
            let tail = &digits[digits.len() - 5..];
            return Some(format!("{}.{}", &tail[..3], &tail[3..]));
        }
        None
    }
}

impl DriverManager {
    pub fn list_present_devices() -> CoreResult<Vec<InstalledDriverDevice>> {
        let list = DevInfoList::present_devices()?;
        let h_dev_info = list.raw();
        let mut devices = Vec::new();
        let mut dev_data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        let mut member_idx = 0u32;
        loop {
            // SAFETY: `h_dev_info` sigue vivo (lo guarda `list`) y `dev_data` lleva su `cbSize`.
            let ok = unsafe { SetupDiEnumDeviceInfo(h_dev_info, member_idx, &mut dev_data) };
            if ok.is_err() {
                break;
            }
            member_idx += 1;

            let desc = get_device_property(h_dev_info, &mut dev_data, SPDRP_DEVICEDESC);
            let friendly = get_device_property(h_dev_info, &mut dev_data, SPDRP_FRIENDLYNAME);
            let mfg = get_device_property(h_dev_info, &mut dev_data, SPDRP_MFG);
            let hw_id = get_device_property(h_dev_info, &mut dev_data, SPDRP_HARDWAREID);
            let class_name = get_device_property(h_dev_info, &mut dev_data, SPDRP_CLASS);
            let driver_key = get_device_property(h_dev_info, &mut dev_data, SPDRP_DRIVER);

            if desc.is_none() && friendly.is_none() && hw_id.is_none() {
                continue;
            }

            let (driver_version, driver_date, provider_name, inf_path) = match driver_key {
                Some(ref key) => read_driver_class_metadata(key),
                None => (None, None, None, None),
            };

            let mut status = CM_DEVNODE_STATUS_FLAGS::default();
            let mut problem = CM_PROB::default();
            // SAFETY: salidas locales y un `DevInst` que acaba de devolver SetupAPI.
            let cr =
                unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, dev_data.DevInst, 0) };

            let (has_problem, problem_code, needs_reboot) = if cr == CR_SUCCESS {
                let prob_flag = (status.0 & DN_HAS_PROBLEM.0) != 0;
                let reboot_flag = (status.0 & DN_NEED_RESTART.0) != 0;
                (
                    prob_flag,
                    if prob_flag { problem.0 } else { 0 },
                    reboot_flag,
                )
            } else {
                (false, 0, false)
            };

            devices.push(InstalledDriverDevice {
                device_desc: desc,
                friendly_name: friendly,
                manufacturer: mfg,
                provider_name,
                class_name,
                hardware_id: hw_id,
                driver_version,
                driver_date,
                inf_path,
                dev_inst: dev_data.DevInst,
                has_problem,
                problem_code,
                needs_reboot,
            });
        }

        devices.sort_by(|a, b| {
            b.has_problem
                .cmp(&a.has_problem)
                .then_with(|| b.is_third_party_oem().cmp(&a.is_third_party_oem()))
                .then_with(|| {
                    a.display_title()
                        .to_ascii_lowercase()
                        .cmp(&b.display_title().to_ascii_lowercase())
                })
        });

        Ok(devices)
    }

    pub fn export_driver_store_backup() -> CoreResult<PathBuf> {
        let base_dir = ensure_admin_only_data_dir()?;
        let backup_root = base_dir.join("DriverBackup");
        create_admin_only_dir(&backup_root)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup_dir = backup_root.join(format!("backup_{}", stamp));
        create_admin_only_dir(&backup_dir)?;

        let out = run_captured(
            &system32_dir()?.join("pnputil.exe"),
            &[
                std::ffi::OsStr::new("/export-driver"),
                std::ffi::OsStr::new("*"),
                backup_dir.as_os_str(),
            ],
            PNPUTIL_TIMEOUT,
        )?;
        if !out.success {
            return Err(CoreError::SystemTool {
                message: format!(
                    "pnputil falló al exportar drivers (código {}): {}",
                    out.code.unwrap_or(-1),
                    out.stdout.trim()
                ),
                source: None,
            });
        }

        Ok(backup_dir)
    }
}

fn get_device_property(
    h_dev_info: HDEVINFO,
    dev_data: &mut SP_DEVINFO_DATA,
    property: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<String> {
    let mut required_size = 0u32;
    // SAFETY: consulta de tamaño sin búfer; `dev_data` es una entrada válida de `h_dev_info`.
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

    if required_size < 2 {
        return None;
    }

    let mut buf = AlignedBuf::zeroed(required_size as usize);
    // SAFETY: el búfer es nuestro y mide `required_size`, el tamaño que pidió la API.
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
        let len_u16 = (required_size as usize).min(buf.len()) / 2;
        // SAFETY: `AlignedBuf` está alineado a 8 bytes y tiene al menos `len_u16 * 2` bytes inicializados.
        let u16_slice = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, len_u16) };
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

fn read_driver_class_metadata(
    driver_key: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let path = format!(r"SYSTEM\CurrentControlSet\Control\Class\{}", driver_key);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(subkey) = hklm.open_subkey_with_flags(&path, KEY_READ) else {
        return (None, None, None, None);
    };

    let read_opt = |name: &str| -> Option<String> {
        subkey
            .get_value::<String, _>(name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    (
        read_opt("DriverVersion"),
        read_opt("DriverDate"),
        read_opt("ProviderName"),
        read_opt("InfPath"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_amigable_de_nvidia() {
        let dev = InstalledDriverDevice {
            device_desc: None,
            friendly_name: None,
            manufacturer: Some("NVIDIA".into()),
            provider_name: Some("NVIDIA".into()),
            class_name: Some("Display".into()),
            hardware_id: None,
            driver_version: Some("32.0.15.6636".into()),
            driver_date: None,
            inf_path: None,
            dev_inst: 0,
            has_problem: false,
            problem_code: 0,
            needs_reboot: false,
        };
        assert_eq!(dev.friendly_gpu_version().as_deref(), Some("566.36"));
    }
}
