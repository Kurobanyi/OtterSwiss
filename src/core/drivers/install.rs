//! Descarga e instalación verificada de paquetes de fabricante (SEC-30): ejecutable firmado dentro de un Job. Los
//! temporales viven en carpetas solo-admin.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::vendor::vendor_policy;
use super::{
    DriverInstallReport, DriverManager, VendorDriverUpdate, ensure_driver_updates_allowed,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::process::{Launch, run_in_job};
use crate::core::platform::safe_fs::{create_verified_new_file, delete_tree_verified};
use crate::core::security::acl::create_admin_only_dir;
use crate::core::security::data_dir::ensure_admin_only_data_dir;
use crate::core::security::download::{
    DownloadPolicy, StagedDownload, reopen_verified_for_exec, validate_https_host,
};
use crate::core::security::http;

/// Un instalador de drivers puede pedir interacción (NVIDIA no tiene modo silencioso aquí): margen amplio antes de
/// cortarlo, porque matar un instalador de drivers a mitad es peor que esperar.
const VENDOR_INSTALLER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

impl DriverManager {
    /// SEC-30: descarga e instala una actualización de fabricante. Hosts y firmante salen de [`vendor_policy`] (fijos en el
    /// binario), nunca de `upd`, que viene del escaneo (de la red) y la vista devuelve tal cual. Todo ejecutable pasa por
    /// Authenticode con el firmante del fabricante y corre dentro de un Job; los temporales se borran al terminar.
    pub(super) fn install_vendor_update_inner(
        upd: &VendorDriverUpdate,
    ) -> CoreResult<DriverInstallReport> {
        ensure_driver_updates_allowed()?;
        let (hosts, signer) = vendor_policy(&upd.vendor)?;
        validate_https_host(&upd.download_url, hosts)?;
        let safe_id: String = upd
            .id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let policy = DownloadPolicy {
            url: &upd.download_url,
            allowed_final_hosts: hosts,
            expected_sha256_hex: None,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: Some(signer),
            max_bytes: 1600 * 1024 * 1024,
        };
        let installer = StagedDownload::download(&safe_id, "exe", &policy)?;
        run_verified_vendor_installer(installer.path(), &policy, &upd.title)
    }
}

/// Reabre `path` comprobando de nuevo la firma sobre el handle que se ejecuta y lo corre en un Job hasta que termina.
fn run_verified_vendor_installer(
    path: &Path,
    policy: &DownloadPolicy<'_>,
    title: &str,
) -> CoreResult<DriverInstallReport> {
    let _exec_lock = reopen_verified_for_exec(path, policy)?;
    let working_dir = path.parent().unwrap_or(path);
    let exit = run_in_job(
        &Launch {
            application: path,
            command_line: format!("\"{}\"", path.display()),
            working_dir,
            user_token: None,
        },
        VENDOR_INSTALLER_TIMEOUT,
        None,
    )?;
    if exit.is_success() {
        Ok(DriverInstallReport::single(
            exit.exit_code == 3010 || exit.exit_code == 1641,
        ))
    } else {
        Err(CoreError::SystemTool {
            message: format!(
                "El instalador de '{title}' terminó con código {}",
                exit.code_display()
            ),
            source: None,
        })
    }
}

/// Carpeta temporal solo-admin de un paquete de drivers (`DriverCache\<kind>\<stem>_<nonce>`): se borra al soltarse
/// (SEC-34).
pub(super) struct ScratchDir {
    root: PathBuf,
    pub(super) dir: PathBuf,
}

impl ScratchDir {
    pub(super) fn new(kind: &str, stem: &str) -> CoreResult<Self> {
        let root = ensure_admin_only_data_dir()?.join("DriverCache").join(kind);
        std::fs::create_dir_all(&root)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = root.join(format!("{stem}_{nonce}"));
        create_admin_only_dir(&dir)?;
        Ok(Self { root, dir })
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = delete_tree_verified(&self.dir, &self.root, 64);
    }
}

/// Baja `url` a `dest` (archivo nuevo) por el cliente HTTPS común, exigiendo estado 200, que el host final esté en
/// `hosts` y que no pase de `max_bytes`. Una lectura cortada falla en vez de dejar un archivo truncado (ERR-19).
pub(super) fn download_to_file(
    url: &str,
    hosts: &[&str],
    dest: &Path,
    max_bytes: u64,
) -> CoreResult<()> {
    let resp = http::get(
        url,
        http::Timeouts {
            connect_ms: 30_000,
            io_ms: 120_000,
        },
    )?;
    resp.require_ok()?;
    validate_https_host(&resp.final_url()?, hosts)?;
    let (mut f, _) = create_verified_new_file(dest, true)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = resp.read_chunk(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_bytes {
            return Err(CoreError::Trust {
                message: format!("La descarga de '{url}' excede el tamaño máximo"),
                source: None,
            });
        }
        f.write_all(&buf[..n])?;
    }
    if resp.content_length().is_some_and(|len| len != total) {
        return Err(CoreError::Trust {
            message: format!("Descarga incompleta de '{url}'"),
            source: None,
        });
    }
    f.sync_all()?;
    Ok(())
}
