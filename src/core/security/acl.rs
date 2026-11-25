//! Permisos de carpetas: crear solo para administradores, comprobar dueños y devolver la herencia.

#![warn(clippy::undocumented_unsafe_blocks)]

use std::path::Path;
use std::ptr;

use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, SE_FILE_OBJECT, SetEntriesInAclW,
    SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows::Win32::Security::{
    ACL, CreateWellKnownSid, DACL_SECURITY_INFORMATION, IsWellKnownSid, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_MAX_SID_SIZE,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT, UNPROTECTED_DACL_SECURITY_INFORMATION,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows::Win32::Storage::FileSystem::{CreateDirectoryW, FILE_ALL_ACCESS};
use windows::core::{PCWSTR, PWSTR};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::memory::{LocalMem, SecurityDescriptor};
use crate::core::platform::wide::to_wide_null;

/// Crea `path` con un descriptor de seguridad solo-admin desde el primer instante.
pub fn create_admin_only_dir(path: &Path) -> CoreResult<()> {
    let sd = SecurityDescriptor::from_sddl("O:BAG:SYD:PAI(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)")?;
    let attrs = sd.attributes();
    let wide = to_wide_null(path);
    // SAFETY: `attrs` apunta a `sd`, que vive hasta el final; `wide` termina en NUL.
    let res = unsafe { CreateDirectoryW(PCWSTR(wide.as_ptr()), Some(&attrs)) };

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
    // SAFETY: el búfer de la ACL es nuestro y tiene el tamaño que se informa.
    unsafe {
        windows::Win32::Security::InitializeAcl(
            &mut empty_acl,
            std::mem::size_of::<ACL>() as u32,
            windows::Win32::Security::ACL_REVISION,
        )
    }
    .map_err(CoreError::trust("InitializeAcl falló: "))?;
    let wide = to_wide_null(path);
    // SAFETY: la ruta termina en NUL; SID y ACL son válidos durante la llamada.
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
pub(crate) fn is_owned_by_admins(path: &Path) -> CoreResult<bool> {
    let wide = to_wide_null(path);
    let mut owner = PSID::default();
    let mut psd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: la ruta termina en NUL; el descriptor devuelto pasa a LocalMem.
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
    // El descriptor (y `owner`, que apunta dentro de él) se libera al salir, también en el error.
    let _psd_guard = LocalMem::new(psd.0);
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
    // SAFETY: `owner` apunta dentro del descriptor, que sigue vivo en `_psd_guard`.
    let ok = unsafe {
        IsWellKnownSid(owner, WinBuiltinAdministratorsSid).as_bool()
            || IsWellKnownSid(owner, WinLocalSystemSid).as_bool()
    };
    Ok(ok)
}

/// Recorre `root` y falla si encuentra un reparse point o un objeto que no sea de Administradores o SYSTEM.
pub(crate) fn verify_tree_owned_by_admins(root: &Path, max_depth: usize) -> CoreResult<()> {
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
    // SAFETY: el búfer tiene SECURITY_MAX_SID_SIZE bytes, el máximo que puede escribir.
    unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            None,
            admin_psid,
            &mut admin_sid_len,
        )
    }
    .map_err(CoreError::trust(
        "CreateWellKnownSid(Administrators) falló: ",
    ))?;

    let mut system_sid_buf = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut system_sid_len = system_sid_buf.len() as u32;
    let system_psid = PSID(system_sid_buf.as_mut_ptr() as *mut _);
    // SAFETY: el búfer tiene SECURITY_MAX_SID_SIZE bytes, el máximo que puede escribir.
    unsafe { CreateWellKnownSid(WinLocalSystemSid, None, system_psid, &mut system_sid_len) }
        .map_err(CoreError::trust("CreateWellKnownSid(LocalSystem) falló: "))?;

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
    // SAFETY: las entradas viven durante la llamada; la ACL devuelta pasa a LocalMem.
    let acl_status = unsafe { SetEntriesInAclW(Some(&entries), None, &mut new_dacl) };
    if acl_status.0 != 0 || new_dacl.is_null() {
        return Err(CoreError::Trust {
            message: format!("SetEntriesInAclW falló con código {}", acl_status.0),
            source: None,
        });
    }

    let _dacl_guard = LocalMem::new(new_dacl as _);
    let wide_path = to_wide_null(path);
    // SAFETY: la ruta termina en NUL; SID y ACL son válidos durante la llamada.
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
