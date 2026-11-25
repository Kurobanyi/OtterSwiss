//! Firma Authenticode de un ejecutable, verificada sobre un handle abierto.

use std::path::Path;
use std::ptr;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{
    CERT_CHAIN_CONTEXT, CERT_CHAIN_PARA, CERT_CHAIN_POLICY_BASE,
    CERT_CHAIN_POLICY_IGNORE_ALL_NOT_TIME_VALID_FLAGS, CERT_CHAIN_POLICY_PARA,
    CERT_CHAIN_POLICY_STATUS, CERT_CONTEXT, CERT_NAME_ATTR_TYPE, CERT_SHA1_HASH_PROP_ID,
    CertFreeCertificateChain, CertGetCertificateChain, CertGetCertificateContextProperty,
    CertGetNameStringW, CertVerifyCertificateChainPolicy, HCERTCHAINENGINE,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::core::PCWSTR;

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::wide::to_wide_null;

/// `HCCE_LOCAL_MACHINE` de wincrypt.h: motor de cadenas que solo usa los almacenes del equipo.
const HCCE_LOCAL_MACHINE: HCERTCHAINENGINE = HCERTCHAINENGINE(1 as _);

/// `szOID_COMMON_NAME` de wincrypt.h.
const SZ_OID_COMMON_NAME_BYTES: &[u8] = b"2.5.4.3\0";

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
            // SAFETY: la estructura WINTRUST_DATA y el archivo apuntan a datos vivos durante la llamada.
            unsafe {
                let _ = WinVerifyTrust(
                    HWND::default(),
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

    // SAFETY: la estructura WINTRUST_DATA y el archivo apuntan a datos vivos durante la llamada.
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
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

    // SAFETY: el estado viene de un WinVerifyTrust exitoso y sigue abierto.
    let prov_data = unsafe { WTHelperProvDataFromStateData(guard.trust_data.hWVTStateData) };
    if prov_data.is_null() {
        return Err(CoreError::Trust {
            message: "No se pudo obtener CRYPT_PROVIDER_DATA tras WinVerifyTrust".into(),
            source: None,
        });
    }

    // SAFETY: los datos del proveedor son válidos mientras el estado siga abierto.
    let signer = unsafe { WTHelperGetProvSignerFromChain(prov_data, 0, false, 0) };
    if signer.is_null() {
        return Err(CoreError::Trust {
            message: "El ejecutable no contiene firmante Authenticode válido".into(),
            source: None,
        });
    }

    // SAFETY: el firmante es válido mientras el estado siga abierto.
    let leaf_cert = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
    if leaf_cert.is_null() {
        return Err(CoreError::Trust {
            message: "No se pudo leer el certificado hoja del firmante".into(),
            source: None,
        });
    }

    // SAFETY: el certificado hoja no es nulo (comprobado arriba) y vive mientras el estado siga abierto.
    let cert_ctx = unsafe { (*leaf_cert).pCert };
    if cert_ctx.is_null() {
        return Err(CoreError::Trust {
            message: "Contexto de certificado nulo en la cadena Authenticode".into(),
            source: None,
        });
    }

    let mut cn_buf = [0u16; 256];
    // SAFETY: el contexto del certificado es válido y el búfer tiene la longitud que se pasa.
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

    verify_chain_with_machine_roots(cert_ctx)?;

    if let Some(expected_tp) = expected_thumbprint_sha1 {
        let mut hash = [0u8; 20];
        let mut hash_len = hash.len() as u32;
        // SAFETY: el contexto del certificado es válido y el búfer tiene la longitud que se informa.
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

/// SEC-32: WinVerifyTrust acepta raíces del almacén del usuario, y el proceso elevado comparte HKCU con él: un programa
/// sin privilegios podría agregar su propia raíz y firmar un binario con el CN esperado. Se vuelve a construir la cadena
/// del firmante con el motor de la máquina, que solo confía en las raíces de LocalMachine (que solo un administrador
/// cambia). La vigencia y la revocación ya las comprobó WinVerifyTrust (con sello de tiempo), así que aquí se ignora
/// el tiempo: un instalador firmado con un certificado ya vencido pero sellado sigue siendo válido.
fn verify_chain_with_machine_roots(cert_ctx: *const CERT_CONTEXT) -> CoreResult<()> {
    let chain_para = CERT_CHAIN_PARA {
        cbSize: std::mem::size_of::<CERT_CHAIN_PARA>() as u32,
        ..Default::default()
    };
    let mut chain: *mut CERT_CHAIN_CONTEXT = ptr::null_mut();
    // SAFETY: `cert_ctx` es el certificado hoja válido mientras siga abierto el estado de WinVerifyTrust (lo garantiza
    // quien llama); su almacén aporta los intermedios de la firma; la cadena devuelta se libera abajo.
    unsafe {
        CertGetCertificateChain(
            Some(HCCE_LOCAL_MACHINE),
            cert_ctx,
            None,
            Some((*cert_ctx).hCertStore),
            &chain_para,
            0,
            None,
            &mut chain,
        )
    }
    .map_err(CoreError::trust(
        "No se pudo construir la cadena del firmante: ",
    ))?;

    struct ChainGuard(*mut CERT_CHAIN_CONTEXT);
    impl Drop for ChainGuard {
        fn drop(&mut self) {
            // SAFETY: la cadena la creó CertGetCertificateChain y solo se libera aquí.
            unsafe { CertFreeCertificateChain(self.0) };
        }
    }
    let _guard = ChainGuard(chain);

    let policy_para = CERT_CHAIN_POLICY_PARA {
        cbSize: std::mem::size_of::<CERT_CHAIN_POLICY_PARA>() as u32,
        dwFlags: CERT_CHAIN_POLICY_IGNORE_ALL_NOT_TIME_VALID_FLAGS,
        ..Default::default()
    };
    let mut status = CERT_CHAIN_POLICY_STATUS {
        cbSize: std::mem::size_of::<CERT_CHAIN_POLICY_STATUS>() as u32,
        ..Default::default()
    };
    // SAFETY: la cadena es válida mientras viva `_guard`; las estructuras tienen su tamaño en `cbSize`.
    let ok = unsafe {
        CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_BASE, chain, &policy_para, &mut status)
    }
    .as_bool();
    if !ok || status.dwError != 0 {
        return Err(CoreError::Trust {
            message: format!(
                "La firma no encadena a una raíz de confianza del equipo (código 0x{:08X})",
                status.dwError
            ),
            source: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsRawHandle;

    use super::*;

    /// Un binario de Windows con firma incrustada pasa la verificación completa (WinVerifyTrust y cadena con las
    /// raíces del equipo, SEC-32) y no pasa con otro CN. Se salta si el equipo no tiene Edge.
    #[test]
    fn firma_de_edge_encadena_a_raiz_del_equipo() {
        let path = Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
        let Ok(file) = std::fs::File::open(path) else {
            return;
        };
        let handle = HANDLE(file.as_raw_handle());
        verify_authenticode_on_handle(path, handle, "Microsoft Corporation", None)
            .expect("firma de Edge válida");
        assert!(verify_authenticode_on_handle(path, handle, "Otra Empresa", None).is_err());
    }
}
