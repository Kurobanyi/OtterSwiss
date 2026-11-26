//! Driver de GeForce desde el servicio de NVIDIA, elegido por el Hardware ID de la GPU.

use serde::Deserialize;
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

use crate::core::drivers::hwid::is_version_newer;
use crate::core::drivers::{InstalledDriverDevice, VendorDriverUpdate, fetch_bytes, url_encode};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::power::detect_battery_presence;

const NVIDIA_API_HOSTS: &[&str] = &["gfwsl.geforce.com"];
/// Hosts de descarga de NVIDIA (el comodín cubre los espejos regionales).
pub(super) const NVIDIA_DOWNLOAD_HOSTS: &[&str] = &[
    "*.download.nvidia.com",
    "us.download.nvidia.com",
    "uk.download.nvidia.com",
    "international.download.nvidia.com",
];
pub(super) const NVIDIA_SIGNER: &str = "NVIDIA Corporation";

#[derive(Debug, Deserialize)]
struct NvidiaLookupResp {
    #[serde(rename = "DriverAttributes")]
    driver_attributes: Option<NvidiaDriverAttrs>,
}

#[derive(Debug, Deserialize)]
struct NvidiaDriverAttrs {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "ReleaseDateTime")]
    release_date: Option<String>,
    #[serde(rename = "DownloadURLAdmin")]
    download_url_admin: Option<String>,
    #[serde(rename = "DownloadURL")]
    download_url: Option<String>,
    #[serde(rename = "DownloadURLFileSize")]
    file_size: Option<String>,
}

pub(in crate::core::drivers) fn check_nvidia_updates(
    devices: &[InstalledDriverDevice],
) -> CoreResult<Vec<VendorDriverUpdate>> {
    let mut results = Vec::new();
    let is_laptop = detect_battery_presence();
    let Some(os_build) = read_windows_build() else {
        return Ok(results);
    };

    let Some(nv_gpu) = devices.iter().find(|d| {
        let is_display = d
            .class_name
            .as_deref()
            .map(|c| c.eq_ignore_ascii_case("Display"))
            .unwrap_or(false);
        let is_nv = d
            .extract_ven_dev()
            .map(|(v, _, _)| v == "10DE")
            .unwrap_or(false);
        is_display && is_nv
    }) else {
        return Ok(results);
    };

    let Some((_, dev, subsys)) = nv_gpu.extract_ven_dev() else {
        return Ok(results);
    };

    let did_entry = match (is_laptop, subsys) {
        (false, Some(sub)) if sub.len() == 8 => {
            format!("{}_10DE_{}_{}", dev, &sub[..4], &sub[4..8])
        }
        _ => format!("{}_10DE", dev),
    };

    let json_req = format!(
        r#"{{"dIDa":["{}"],"osC":"10.0","osB":"{}","is6":"1","lg":"1033","iLp":"{}","prvMd":"0","upCRD":"0"}}"#,
        did_entry,
        os_build,
        if is_laptop { "1" } else { "0" }
    );
    let url = format!(
        "https://gfwsl.geforce.com/nvidia_web_services/controller.gfeclientcontent.NG.php/com.nvidia.services.GFEClientContent_NG.getDispDrvrByDevid/{}",
        url_encode(&json_req)
    );

    // El servicio de NVIDIA responde a su cliente (NvBackend), no a un agente genérico.
    let bytes = fetch_bytes(
        &url,
        NVIDIA_API_HOSTS,
        Some("NvBackend/36.0.0.0"),
        2 * 1024 * 1024,
    )?;
    let parsed =
        serde_json::from_slice::<NvidiaLookupResp>(&bytes).map_err(|e| CoreError::SystemTool {
            message: "Respuesta inesperada del servicio de drivers".into(),
            source: Some(Box::new(e)),
        })?;
    let Some(attrs) = parsed.driver_attributes else {
        return Ok(results);
    };

    let Some(latest_ver) = attrs.version else {
        return Ok(results);
    };
    let Some(dl_url) = attrs.download_url_admin.or(attrs.download_url) else {
        return Ok(results);
    };
    if !dl_url.starts_with("https://") {
        return Ok(results);
    }

    // ERR-23: la versión interna (`32.0.15.x`) no se compara con la pública (`566.xx`).
    let current_ver = nv_gpu
        .friendly_gpu_version()
        .unwrap_or_else(|| "Desconocida".into());

    if nv_gpu.has_problem() || is_version_newer(&latest_ver, &current_ver) {
        results.push(VendorDriverUpdate {
            id: format!("nvidia_{}", latest_ver),
            vendor: "NVIDIA".into(),
            title: attrs
                .name
                .unwrap_or_else(|| "NVIDIA GeForce Game Ready Driver".into())
                .replace("%20", " "),
            current_version: current_ver,
            latest_version: latest_ver,
            release_date: attrs.release_date.unwrap_or_default(),
            download_url: dl_url,
            size_label: attrs.file_size.unwrap_or_else(|| "—".into()),
            expected_signer_cn: Some(NVIDIA_SIGNER.into()),
            allowed_hosts: NVIDIA_DOWNLOAD_HOSTS.iter().map(|h| (*h).into()).collect(),
            matched_hw_id: nv_gpu.short_hw_id().unwrap_or_default(),
        });
    }

    Ok(results)
}

/// Número de build de Windows, o `None` si no se puede leer (antes se inventaba `22631`, ERR-23).
fn read_windows_build() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ)
        .ok()?;
    let build: String = key.get_value("CurrentBuildNumber").ok()?;
    let build = build.trim().to_string();
    build.chars().all(|c| c.is_ascii_digit()).then_some(build)
}
