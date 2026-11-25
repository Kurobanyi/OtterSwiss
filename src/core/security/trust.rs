use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr;

use sha2::{Digest, Sha256};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::Foundation::{HANDLE, HLOCAL};
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetQueryOptionW,
    InternetReadFile, InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER,
    HTTP_QUERY_STATUS_CODE, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD,
    INTERNET_FLAG_SECURE, INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT,
    INTERNET_OPTION_RECEIVE_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT, INTERNET_OPTION_URL,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, SetEntriesInAclW,
    SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS, SDDL_REVISION_1, SE_FILE_OBJECT,
    TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows::Win32::Security::Cryptography::{
    CertGetCertificateContextProperty, CertGetNameStringW, CERT_NAME_ATTR_TYPE,
    CERT_SHA1_HASH_PROP_ID,
};
use windows::Win32::Security::WinTrust::{
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
    WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};
use windows::Win32::Security::{
    CreateWellKnownSid, IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid, ACL,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_ALL_ACCESS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_NONE,
    FILE_SHARE_READ,
};
use windows::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{inspect_handle, write_zone_identifier};
use crate::core::platform::win32::{to_wide_null, wide_ptr_to_string_and_free};

/// `szOID_COMMON_NAME` de wincrypt.h.
const SZ_OID_COMMON_NAME_BYTES: &[u8] = b"2.5.4.3\0";

struct WinInetHandle(*mut std::ffi::c_void);

impl WinInetHandle {
    fn new(ptr: *mut std::ffi::c_void) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }
    fn raw(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

impl Drop for WinInetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
            self.0 = ptr::null_mut();
        }
    }
}

pub fn get_program_data_dir() -> CoreResult<PathBuf> {
    let raw_ptr: PWSTR =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None) }.map_err(
            |e| CoreError::Trust {
                message: format!("SHGetKnownFolderPath(FOLDERID_ProgramData) falló: {}", e),
                source: None,
            },
        )?;
    let path_str = wide_ptr_to_string_and_free(raw_ptr);
    if path_str.is_empty() {
        return Err(CoreError::Trust {
            message: "FOLDERID_ProgramData devolvió ruta vacía".into(),
            source: None,
        });
    }
    Ok(PathBuf::from(path_str))
}

/// Único punto de acceso a `C:\ProgramData\OtterSwiss`.
///
/// `C:\ProgramData` deja a cualquier usuario crear carpetas y archivos (de los que queda como dueño), así que
/// la carpeta se crea ya con un descriptor solo-admin, se reinician dueño y DACL si existía, y se rechaza
/// cualquier reparse point o hijo cuyo dueño no sea Administradores o SYSTEM: esos archivos pudo plantarlos
/// un usuario sin privilegios para que OtterSwiss los lea o ejecute como administrador.
pub fn ensure_admin_only_data_dir() -> CoreResult<PathBuf> {
    let base = get_program_data_dir()?;
    let target = base.join("OtterSwiss");

    match std::fs::symlink_metadata(&target) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(CoreError::Trust {
                    message: format!(
                        "Ruta crítica '{}' es un symlink o archivo no válido (posible squatting)",
                        target.display()
                    ),
                    source: None,
                });
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_admin_only_dir(&target)?,
        Err(e) => return Err(CoreError::Io(e)),
    }

    apply_admin_only_acl(&target)?;
    verify_tree_owned_by_admins(&target, 8)?;
    Ok(target)
}

/// Crea `path` con un descriptor de seguridad solo-admin desde el primer instante.
pub fn create_admin_only_dir(path: &Path) -> CoreResult<()> {
    let sddl = to_wide_null("O:BAG:SYD:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)");
    let mut psd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut psd,
            None,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("No se pudo construir el descriptor de seguridad: {}", e),
        source: None,
    })?;

    let attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: psd.0,
        bInheritHandle: false.into(),
    };
    let wide = to_wide_null(path);
    let res = unsafe { CreateDirectoryW(PCWSTR(wide.as_ptr()), Some(&attrs)) };
    unsafe {
        let _ = windows::Win32::Foundation::LocalFree(HLOCAL(psd.0));
    }

    match res {
        Ok(()) => Ok(()),
        // Otro proceso la creó entre la comprobación y aquí: la validación posterior decide.
        Err(e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult() => Ok(()),
        Err(e) => Err(CoreError::Trust {
            message: format!("No se pudo crear '{}': {}", path.display(), e),
            source: None,
        }),
    }
}

/// Quita la DACL protegida de `path` y deja que herede los permisos de su carpeta padre (y los propague a
/// todo el árbol). Se usa al terminar una extracción hecha en un staging solo-admin.
pub fn reset_to_inherited_acl(path: &Path) -> CoreResult<()> {
    let mut empty_acl = ACL::default();
    unsafe {
        windows::Win32::Security::InitializeAcl(
            &mut empty_acl,
            std::mem::size_of::<ACL>() as u32,
            windows::Win32::Security::ACL_REVISION,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("InitializeAcl falló: {}", e),
        source: None,
    })?;
    let wide = to_wide_null(path);
    let status = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION,
            PSID::default(),
            PSID::default(),
            Some(&empty_acl),
            None,
        )
    };
    if status.0 != 0 {
        return Err(CoreError::Trust {
            message: format!(
                "No se pudieron restablecer los permisos de '{}' (código {})",
                path.display(),
                status.0
            ),
            source: None,
        });
    }
    Ok(())
}

/// `true` si el dueño de `path` es Administradores o SYSTEM.
fn is_owned_by_admins(path: &Path) -> CoreResult<bool> {
    let wide = to_wide_null(path);
    let mut owner = PSID::default();
    let mut psd = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            &mut psd,
        )
    };
    if status.0 != 0 {
        return Err(CoreError::Trust {
            message: format!(
                "No se pudo leer el dueño de '{}' (código {})",
                path.display(),
                status.0
            ),
            source: None,
        });
    }
    let ok = unsafe {
        IsWellKnownSid(owner, WinBuiltinAdministratorsSid).as_bool()
            || IsWellKnownSid(owner, WinLocalSystemSid).as_bool()
    };
    unsafe {
        let _ = windows::Win32::Foundation::LocalFree(HLOCAL(psd.0));
    }
    Ok(ok)
}

/// Recorre `root` y falla si encuentra un reparse point o un objeto que no sea de Administradores o SYSTEM.
fn verify_tree_owned_by_admins(root: &Path, max_depth: usize) -> CoreResult<()> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                return Err(CoreError::Trust {
                    message: format!(
                        "Se encontró un enlace o junction en la carpeta de datos: '{}'. Bórralo y vuelve a intentar.",
                        path.display()
                    ),
                    source: None,
                });
            }
            if !is_owned_by_admins(&path)? {
                return Err(CoreError::Trust {
                    message: format!(
                        "'{}' no pertenece a Administradores ni a SYSTEM (posible manipulación). Bórralo y vuelve a intentar.",
                        path.display()
                    ),
                    source: None,
                });
            }
            if ft.is_dir() {
                if depth + 1 > max_depth {
                    return Err(CoreError::Trust {
                        message: format!(
                            "Carpeta de datos demasiado profunda en '{}'",
                            path.display()
                        ),
                        source: None,
                    });
                }
                stack.push((path, depth + 1));
            }
        }
    }
    Ok(())
}

pub fn apply_admin_only_acl(path: &Path) -> CoreResult<()> {
    let mut admin_sid_buf = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut admin_sid_len = admin_sid_buf.len() as u32;
    let admin_psid = PSID(admin_sid_buf.as_mut_ptr() as *mut _);
    unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            None,
            admin_psid,
            &mut admin_sid_len,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("CreateWellKnownSid(Administrators) falló: {}", e),
        source: None,
    })?;

    let mut system_sid_buf = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut system_sid_len = system_sid_buf.len() as u32;
    let system_psid = PSID(system_sid_buf.as_mut_ptr() as *mut _);
    unsafe { CreateWellKnownSid(WinLocalSystemSid, None, system_psid, &mut system_sid_len) }
        .map_err(|e| CoreError::Trust {
            message: format!("CreateWellKnownSid(LocalSystem) falló: {}", e),
            source: None,
        })?;

    let entries = [
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS.0,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: PWSTR(admin_psid.0 as *mut _),
                ..Default::default()
            },
        },
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS.0,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: PWSTR(system_psid.0 as *mut _),
                ..Default::default()
            },
        },
    ];

    let mut new_dacl: *mut ACL = ptr::null_mut();
    let acl_status = unsafe { SetEntriesInAclW(Some(&entries), None, &mut new_dacl) };
    if acl_status.0 != 0 || new_dacl.is_null() {
        return Err(CoreError::Trust {
            message: format!("SetEntriesInAclW falló con código {}", acl_status.0),
            source: None,
        });
    }

    let wide_path = to_wide_null(path);
    let sec_status = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(wide_path.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            admin_psid,
            PSID::default(),
            Some(new_dacl),
            None,
        )
    };

    unsafe {
        let _ = windows::Win32::Foundation::LocalFree(HLOCAL(new_dacl as _));
    }

    if sec_status.0 != 0 {
        return Err(CoreError::Trust {
            message: format!(
                "SetNamedSecurityInfoW falló en '{}' con código {}",
                path.display(),
                sec_status.0
            ),
            source: None,
        });
    }

    Ok(())
}

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

pub fn verify_authenticode_on_handle(
    path: &Path,
    handle: HANDLE,
    expected_cn: &str,
    expected_thumbprint_sha1: Option<&str>,
) -> CoreResult<()> {
    struct StateDataGuard {
        action: windows::core::GUID,
        trust_data: WINTRUST_DATA,
    }
    impl Drop for StateDataGuard {
        fn drop(&mut self) {
            self.trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
            unsafe {
                let _ = WinVerifyTrust(
                    None,
                    &mut self.action,
                    &mut self.trust_data as *mut _ as *mut _,
                );
            }
        }
    }

    let wide_path = to_wide_null(path);
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide_path.as_ptr()),
        hFile: handle,
        pgKnownSubject: ptr::null_mut(),
    };

    let trust_data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        // Comprobar revocación de toda la cadena (excepto la raíz): un certificado revocado por robo de la
        // clave privada no debe seguir siendo aceptado (SEC-12).
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };

    let mut guard = StateDataGuard {
        action: WINTRUST_ACTION_GENERIC_VERIFY_V2,
        trust_data,
    };

    let status = unsafe {
        WinVerifyTrust(
            None,
            &mut guard.action,
            &mut guard.trust_data as *mut _ as *mut _,
        )
    };

    if status != 0 {
        return Err(CoreError::Trust {
            message: format!(
                "WinVerifyTrust rechazó la firma (código 0x{:08X})",
                status as u32
            ),
            source: None,
        });
    }

    let prov_data = unsafe { WTHelperProvDataFromStateData(guard.trust_data.hWVTStateData) };
    if prov_data.is_null() {
        return Err(CoreError::Trust {
            message: "No se pudo obtener CRYPT_PROVIDER_DATA tras WinVerifyTrust".into(),
            source: None,
        });
    }

    let signer = unsafe { WTHelperGetProvSignerFromChain(prov_data, 0, false, 0) };
    if signer.is_null() {
        return Err(CoreError::Trust {
            message: "El ejecutable no contiene firmante Authenticode válido".into(),
            source: None,
        });
    }

    let leaf_cert = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if leaf_cert.is_null() {
        return Err(CoreError::Trust {
            message: "No se pudo leer el certificado hoja del firmante".into(),
            source: None,
        });
    }

    let cert_ctx = unsafe { (*leaf_cert).pCert };
    if cert_ctx.is_null() {
        return Err(CoreError::Trust {
            message: "Contexto de certificado nulo en la cadena Authenticode".into(),
            source: None,
        });
    }

    let mut cn_buf = [0u16; 256];
    let cn_len = unsafe {
        CertGetNameStringW(
            cert_ctx,
            CERT_NAME_ATTR_TYPE,
            0,
            Some(SZ_OID_COMMON_NAME_BYTES.as_ptr() as *const _),
            Some(&mut cn_buf),
        )
    };

    let actual_cn = if cn_len > 1 {
        String::from_utf16_lossy(&cn_buf[..(cn_len as usize - 1)])
    } else {
        String::new()
    };

    let cn_allowed = expected_cn
        .split('|')
        .map(str::trim)
        .any(|allowed| !allowed.is_empty() && actual_cn == allowed);

    if !cn_allowed {
        return Err(CoreError::Trust {
            message: format!(
                "El certificado firmante CN='{}' no coincide con el esperado '{}'",
                actual_cn, expected_cn
            ),
            source: None,
        });
    }

    if let Some(expected_tp) = expected_thumbprint_sha1 {
        let mut hash = [0u8; 20];
        let mut hash_len = hash.len() as u32;
        let ok = unsafe {
            CertGetCertificateContextProperty(
                cert_ctx,
                CERT_SHA1_HASH_PROP_ID,
                Some(hash.as_mut_ptr() as *mut _),
                &mut hash_len,
            )
        }
        .is_ok();
        if !ok || hash_len != 20 {
            return Err(CoreError::Trust {
                message: "No se pudo leer el thumbprint SHA-1 del certificado".into(),
                source: None,
            });
        }
        let actual_tp: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        if !actual_tp.eq_ignore_ascii_case(expected_tp.trim()) {
            return Err(CoreError::Trust {
                message: format!(
                    "Thumbprint SHA-1 '{}' no coincide con '{}'",
                    actual_tp, expected_tp
                ),
                source: None,
            });
        }
    }

    Ok(())
}
