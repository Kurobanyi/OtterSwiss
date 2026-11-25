//! Descargas verificadas: HTTPS a hosts permitidos, tope de tamaño, SHA-256 y Authenticode sobre el mismo handle.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use sha2::{Digest, Sha256};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetOpenUrlW, InternetOpenW, InternetQueryOptionW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
    INTERNET_OPTION_SEND_TIMEOUT, INTERNET_OPTION_URL,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_NONE, FILE_SHARE_READ,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::WinInetHandle;
use crate::core::platform::safe_fs::{inspect_handle, write_zone_identifier};
use crate::core::platform::wide::to_wide_null;
use crate::core::security::authenticode::verify_authenticode_on_handle;

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

    let agent_w = to_wide_null("OtterSwiss/3.0");
    let h_internet = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: "InternetOpenW falló".into(),
        source: None,
    })?;

    let conn_timeout: u32 = 15_000;
    let io_timeout: u32 = 30_000;
    unsafe {
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_CONNECT_TIMEOUT,
            Some(&conn_timeout as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_SEND_TIMEOUT,
            Some(&io_timeout as *const _ as *const _),
            4,
        );
        let _ = InternetSetOptionW(
            Some(h_internet.raw()),
            INTERNET_OPTION_RECEIVE_TIMEOUT,
            Some(&io_timeout as *const _ as *const _),
            4,
        );
    }

    let url_w = to_wide_null(policy.url);
    let h_url = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            h_internet.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            0,
        )
    })
    .ok_or_else(|| CoreError::Trust {
        message: format!("InternetOpenUrlW falló para '{}'", policy.url),
        source: None,
    })?;

    let mut status_code: u32 = 0;
    let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
    unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut status_code as *mut _ as *mut _),
            &mut status_len,
            None,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("No se pudo leer el código de estado HTTP: {}", e),
        source: None,
    })?;

    if status_code != 200 {
        return Err(CoreError::Trust {
            message: format!("Respuesta HTTP no permitida: {}", status_code),
            source: None,
        });
    }

    let mut final_url_len: u32 = 0;
    let _ = unsafe {
        InternetQueryOptionW(
            Some(h_url.raw()),
            INTERNET_OPTION_URL,
            None,
            &mut final_url_len,
        )
    };
    if final_url_len == 0 {
        return Err(CoreError::Trust {
            message: "No se pudo verificar la URL final tras redirecciones".into(),
            source: None,
        });
    }
    let mut url_buf = vec![0u16; (final_url_len as usize / 2) + 1];
    unsafe {
        InternetQueryOptionW(
            Some(h_url.raw()),
            INTERNET_OPTION_URL,
            Some(url_buf.as_mut_ptr() as *mut _),
            &mut final_url_len,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("InternetQueryOptionW(INTERNET_OPTION_URL) falló: {}", e),
        source: None,
    })?;
    let end = url_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(url_buf.len());
    let final_url = String::from_utf16_lossy(&url_buf[..end]);
    validate_https_host(&final_url, policy.allowed_final_hosts)?;

    let mut content_len: u32 = 0;
    let mut cl_size: u32 = std::mem::size_of::<u32>() as u32;
    let has_content_length = unsafe {
        HttpQueryInfoW(
            h_url.raw(),
            HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER,
            Some(&mut content_len as *mut _ as *mut _),
            &mut cl_size,
            None,
        )
    }
    .is_ok();
    if has_content_length && (content_len as u64) > policy.max_bytes {
        return Err(CoreError::Trust {
            message: format!(
                "Content-Length ({} bytes) excede el límite del paquete ({} bytes)",
                content_len, policy.max_bytes
            ),
            source: None,
        });
    }

    let mut hasher = Sha256::new();
    let mut total_read: u64 = 0;
    let mut chunk = [0u8; 32 * 1024];

    let expected_total = if has_content_length && content_len > 0 {
        content_len as u64
    } else {
        policy.max_bytes
    };

    loop {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(CoreError::Cancelled);
        }
        let mut bytes_read: u32 = 0;
        unsafe {
            InternetReadFile(
                h_url.raw(),
                chunk.as_mut_ptr() as *mut _,
                chunk.len() as u32,
                &mut bytes_read,
            )
        }
        .map_err(|e| CoreError::Trust {
            message: format!("InternetReadFile falló: {}", e),
            source: None,
        })?;

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

        let slice = &chunk[..bytes_read as usize];
        hasher.update(slice);
        file.write_all(slice)?;
        if let Some(p) = progress {
            p.store(
                ((total_read * 100) / expected_total.max(1)).min(99) as u8,
                Ordering::Release,
            );
        }
    }

    if has_content_length && total_read != content_len as u64 {
        return Err(CoreError::Trust {
            message: format!(
                "Descarga truncada: recibidos {} bytes de {} esperados",
                total_read, content_len
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
