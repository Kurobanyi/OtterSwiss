//! Descargas verificadas: HTTPS a hosts permitidos, tope de tamaño, SHA-256 y Authenticode sobre el mismo handle.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_NONE, FILE_SHARE_READ,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{inspect_handle, write_zone_identifier};
use crate::core::security::authenticode::verify_authenticode_on_handle;
use crate::core::security::http::{self, Timeouts};

#[derive(Debug, Clone)]
pub struct DownloadPolicy<'a> {
    pub url: &'a str,
    pub allowed_final_hosts: &'a [&'a str],
    pub expected_sha256_hex: Option<&'a str>,
    pub expected_cert_thumbprint_sha1: Option<&'a str>,
    pub expected_signer_cn: Option<&'a str>,
    pub max_bytes: u64,
}

pub fn secure_download_and_verify(
    dest_path: &Path,
    policy: &DownloadPolicy<'_>,
) -> CoreResult<File> {
    secure_download_and_verify_with_progress(dest_path, policy, None, None)
}

/// Igual que `secure_download_and_verify`, informando el porcentaje en `progress` y abortando si `cancel`
/// se activa.
pub fn secure_download_and_verify_with_progress(
    dest_path: &Path,
    policy: &DownloadPolicy<'_>,
    progress: Option<&std::sync::atomic::AtomicU8>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> CoreResult<File> {
    use std::sync::atomic::Ordering;
    if !policy.url.starts_with("https://") {
        return Err(CoreError::Trust {
            message: "Solo se permiten descargas HTTPS".into(),
            source: None,
        });
    }

    if policy.expected_sha256_hex.is_none() && policy.expected_signer_cn.is_none() {
        return Err(CoreError::Trust {
            message: "Política de descarga inválida: requiere SHA-256 o firmante Authenticode"
                .into(),
            source: None,
        });
    }

    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .share_mode(FILE_SHARE_NONE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(dest_path)?;

    let raw_handle = HANDLE(file.as_raw_handle() as _);
    let h_info = inspect_handle(raw_handle)?;
    if h_info.is_reparse_point || h_info.is_directory || h_info.number_of_links > 1 {
        return Err(CoreError::Trust {
            message: "Archivo de destino comprometido por reparse point o hardlink".into(),
            source: None,
        });
    }

    let response = http::get(
        policy.url,
        Timeouts {
            connect_ms: 15_000,
            io_ms: 30_000,
        },
    )?;
    response.require_ok()?;
    let final_url = response.final_url()?;
    validate_https_host(&final_url, policy.allowed_final_hosts)?;

    let content_len = response.content_length();
    if let Some(len) = content_len.filter(|&len| len > policy.max_bytes) {
        return Err(CoreError::Trust {
            message: format!(
                "Content-Length ({} bytes) excede el límite del paquete ({} bytes)",
                len, policy.max_bytes
            ),
            source: None,
        });
    }

    let mut hasher = Sha256::new();
    let mut total_read: u64 = 0;
    let mut chunk = [0u8; 32 * 1024];

    let expected_total = content_len
        .filter(|&len| len > 0)
        .unwrap_or(policy.max_bytes);

    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(CoreError::Cancelled);
        }
        let bytes_read = response.read_chunk(&mut chunk)?;
        if bytes_read == 0 {
            break;
        }

        total_read += bytes_read as u64;
        if total_read > policy.max_bytes {
            return Err(CoreError::Trust {
                message: format!("Descarga abortada: excede {} bytes", policy.max_bytes),
                source: None,
            });
        }

        let slice = &chunk[..bytes_read];
        hasher.update(slice);
        file.write_all(slice)?;
        if let Some(p) = progress {
            p.store(
                ((total_read * 100) / expected_total.max(1)).min(99) as u8,
                Ordering::Release,
            );
        }
    }

    if let Some(len) = content_len.filter(|&len| len != total_read) {
        return Err(CoreError::Trust {
            message: format!(
                "Descarga truncada: recibidos {} bytes de {} esperados",
                total_read, len
            ),
            source: None,
        });
    }

    file.sync_all()?;

    if let Some(expected_sha) = policy.expected_sha256_hex {
        let actual_sha = format!("{:x}", hasher.finalize());
        if !actual_sha.eq_ignore_ascii_case(expected_sha.trim()) {
            return Err(CoreError::Trust {
                message: format!(
                    "SHA-256 inválido: esperado {}, obtenido {}",
                    expected_sha, actual_sha
                ),
                source: None,
            });
        }
    }

    write_zone_identifier(dest_path, &final_url)?;

    if let Some(expected_cn) = policy.expected_signer_cn {
        verify_authenticode_on_handle(
            dest_path,
            raw_handle,
            expected_cn,
            policy.expected_cert_thumbprint_sha1,
        )?;
    }

    Ok(file)
}

/// Reabre un instalador ya descargado para ejecutarlo. El handle comparte solo lectura, así que mientras
/// viva nadie puede escribir, renombrar ni borrar el archivo; SHA-256 y Authenticode se repiten sobre este
/// mismo handle. El llamador mantiene el `File` vivo hasta que el proceso haya terminado.
pub fn reopen_verified_for_exec(path: &Path, policy: &DownloadPolicy<'_>) -> CoreResult<File> {
    use std::io::{Seek, SeekFrom};

    if policy.expected_sha256_hex.is_none() && policy.expected_signer_cn.is_none() {
        return Err(CoreError::Trust {
            message: "Política inválida: requiere SHA-256 o firmante Authenticode".into(),
            source: None,
        });
    }

    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let handle = HANDLE(file.as_raw_handle() as _);
    let info = inspect_handle(handle)?;
    if info.is_reparse_point || info.is_directory || info.number_of_links > 1 {
        return Err(CoreError::Trust {
            message: format!(
                "El instalador '{}' fue reemplazado por un enlace o hardlink",
                path.display()
            ),
            source: None,
        });
    }

    if let Some(expected_sha) = policy.expected_sha256_hex {
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_sha.trim()) {
            return Err(CoreError::Trust {
                message: format!(
                    "El instalador cambió después de verificarse (SHA-256 {})",
                    actual
                ),
                source: None,
            });
        }
        file.seek(SeekFrom::Start(0))?;
    }

    if let Some(expected_cn) = policy.expected_signer_cn {
        verify_authenticode_on_handle(
            path,
            handle,
            expected_cn,
            policy.expected_cert_thumbprint_sha1,
        )?;
    }

    Ok(file)
}

pub fn validate_https_host(url: &str, allowed_hosts: &[&str]) -> CoreResult<()> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| CoreError::Trust {
            message: format!("Redirección insegura fuera de HTTPS: '{}'", url),
            source: None,
        })?;
    let host_port = rest.split('/').next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("");
    let host_low = host.to_ascii_lowercase();

    let is_allowed = !host_low.is_empty()
        && allowed_hosts.iter().any(|&allowed| {
            let pat = allowed.to_ascii_lowercase();
            if let Some(suffix) = pat.strip_prefix("*.") {
                host_low == suffix || host_low.ends_with(&format!(".{}", suffix))
            } else {
                host_low == pat
            }
        });

    if !is_allowed {
        return Err(CoreError::Trust {
            message: format!("Host final '{}' no está en la lista blanca", host),
            source: None,
        });
    }
    Ok(())
}
