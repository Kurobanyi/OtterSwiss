use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use serde::{Deserialize, Serialize};
use sysinfo::System;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, FreeLibrary, HANDLE, HMODULE,
    INVALID_HANDLE_VALUE, LUID, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Services::{CloseServiceHandle, SC_HANDLE};
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, EWX_FORCEIFHUNG, EWX_REBOOT, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_APPLICATION,
};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, OpenProcessToken, ReleaseMutex, WaitForSingleObject,
};
use windows::Win32::System::IO::DeviceIoControl;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

use crate::core::error::{CoreError, CoreResult};

pub fn to_wide_null(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn wide_ptr_to_string_and_free(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let result = unsafe {
        let mut len = 0usize;
        while *ptr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr.0, len);
        String::from_utf16_lossy(slice)
    };
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(ptr.0 as *const _));
    }
    result
}

/// Carpeta conocida de Windows por `SHGetKnownFolderPath`. Se usa en lugar de variables de entorno
/// (`%ProgramFiles%`, `%SystemRoot%`…), que un proceso sin elevar puede redefinir en `HKCU\Environment`.
pub fn known_folder(id: &windows::core::GUID) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let s = wide_ptr_to_string_and_free(raw);
    if s.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(s))
    }
}

pub fn program_files_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_ProgramFiles)
}

pub fn program_files_x86_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_ProgramFilesX86)
}

pub fn windows_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_Windows)
}

pub fn local_app_data_dir() -> Option<std::path::PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_LocalAppData)
}

/// Buffer de bytes alineado a 8. Las APIs de Windows devuelven estructuras dentro de un buffer de bytes;
/// reinterpretar un `Vec<u8>` (alineado a 1) como esa estructura es comportamiento indefinido (SEC-28).
pub struct AlignedBuf {
    words: Vec<u64>,
    len: usize,
}

impl AlignedBuf {
    pub fn zeroed(len: usize) -> Self {
        Self {
            words: vec![0u64; len.div_ceil(8).max(1)],
            len,
        }
    }
}

impl std::ops::Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }
}

impl std::ops::DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }
}

pub struct SafeHandle(HANDLE);

impl SafeHandle {
    pub fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_invalid() || handle.0.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for SafeHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0 .0.is_null() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
            self.0 = INVALID_HANDLE_VALUE;
        }
    }
}

pub struct SafeLibrary(HMODULE);

impl SafeLibrary {
    pub fn load_system(dll_name: &str) -> CoreResult<Self> {
        let wide = to_wide_null(dll_name);
        let module =
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|e| CoreError::SystemTool {
                    message: format!("No se pudo cargar DLL del sistema '{}': {}", dll_name, e),
                    source: None,
                })?;
        Ok(Self(module))
    }

    pub fn load_from_verified_path(path: &Path) -> CoreResult<Self> {
        let wide = to_wide_null(path);
        // Las dependencias de la DLL se buscan solo junto a ella y en System32, nunca en el directorio actual.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(wide.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
            .map_err(|e| CoreError::SystemTool {
                message: format!("No se pudo cargar DLL '{}': {}", path.display(), e),
                source: None,
            })?;
        Ok(Self(module))
    }

    /// # Safety
    /// `symbol` debe terminar en NUL y el llamador debe convertir el puntero devuelto a la firma real
    /// de la función exportada antes de llamarla.
    pub unsafe fn get_proc(&self, symbol: &[u8]) -> Option<unsafe extern "system" fn() -> isize> {
        debug_assert!(symbol.ends_with(&[0]));
        GetProcAddress(self.0, windows::core::PCSTR(symbol.as_ptr()))
    }
}

impl Drop for SafeLibrary {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe {
                let _ = FreeLibrary(self.0);
            }
        }
    }
}

pub struct ScHandle(SC_HANDLE);

impl ScHandle {
    pub fn new(handle: SC_HANDLE) -> Option<Self> {
        if handle.0.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    pub fn raw(&self) -> SC_HANDLE {
        self.0
    }
}

impl Drop for ScHandle {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe {
                let _ = CloseServiceHandle(self.0);
            }
        }
    }
}

pub struct JobObject(SafeHandle);

impl JobObject {
    pub fn new_kill_on_close() -> CoreResult<Self> {
        let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("Error creando Job Object: {}", e),
                source: None,
            }
        })?;
        let handle = SafeHandle::new(raw)
            .ok_or_else(|| CoreError::from_win32("CreateJobObjectW devolvió handle inválido"))?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error configurando límites del Job Object: {}", e),
            source: None,
        })?;

        Ok(Self(handle))
    }

    pub fn assign(&self, process: HANDLE) -> CoreResult<()> {
        unsafe { AssignProcessToJobObject(self.0.raw(), process) }.map_err(|e| {
            CoreError::SystemTool {
                message: format!("Error asignando proceso al Job Object: {}", e),
                source: None,
            }
        })
    }

    pub fn raw(&self) -> HANDLE {
        self.0.raw()
    }

    pub fn terminate_all(&self, exit_code: u32) {
        unsafe {
            let _ = TerminateJobObject(self.0.raw(), exit_code);
        }
    }
}

pub struct NamedMutex {
    handle: SafeHandle,
    locked: bool,
}

impl NamedMutex {
    /// Crea (o abre) el mutex con una DACL solo para Administradores y SYSTEM: así un usuario sin privilegios
    /// no puede crearlo antes y retenerlo para bloquear respaldos y rollbacks (SEC-23).
    fn open(name: &str) -> CoreResult<SafeHandle> {
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

        let sddl = to_wide_null("D:P(A;;GA;;;BA)(A;;GA;;;SY)");
        let mut psd = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut psd,
                None,
            )
        }
        .map_err(|e| CoreError::SystemTool {
            message: format!("No se pudo construir el descriptor del mutex: {}", e),
            source: None,
        })?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: psd.0,
            bInheritHandle: false.into(),
        };
        let wide = to_wide_null(name);
        let res = unsafe { CreateMutexW(Some(&sa), false, PCWSTR(wide.as_ptr())) };
        unsafe {
            let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(psd.0));
        }
        let raw = res.map_err(|e| CoreError::SystemTool {
            message: format!("Error creando mutex '{}': {}", name, e),
            source: None,
        })?;
        SafeHandle::new(raw).ok_or_else(|| CoreError::from_win32("CreateMutexW devolvió handle inválido"))
    }

    /// `WAIT_ABANDONED` significa que el dueño anterior murió sin soltarlo: el mutex ya es nuestro (CON-01).
    fn wait(handle: &SafeHandle, timeout_ms: u32) -> windows::Win32::Foundation::WAIT_EVENT {
        unsafe { WaitForSingleObject(handle.raw(), timeout_ms) }
    }

    pub fn acquire(name: &str) -> CoreResult<Self> {
        let handle = Self::open(name)?;
        let w = Self::wait(&handle, 0);
        if w == WAIT_OBJECT_0 || w == WAIT_ABANDONED {
            return Ok(Self {
                handle,
                locked: true,
            });
        }
        Err(CoreError::AlreadyRunning)
    }

    /// Espera el mutex hasta 60 s; nunca indefinidamente.
    pub fn acquire_blocking(name: &str) -> CoreResult<Self> {
        let handle = Self::open(name)?;
        let w = Self::wait(&handle, 60_000);
        if w == WAIT_OBJECT_0 || w == WAIT_ABANDONED {
            return Ok(Self {
                handle,
                locked: true,
            });
        }
        if w == WAIT_TIMEOUT {
            return Err(CoreError::AlreadyRunning);
        }
        Err(CoreError::from_win32("WaitForSingleObject falló en mutex"))
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        if self.locked {
            unsafe {
                let _ = ReleaseMutex(self.handle.raw());
            }
            self.locked = false;
        }
    }
}

pub struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    pub fn init_mt() -> Self {
        let hr = unsafe { CoInitializeEx(Some(ptr::null()), COINIT_MULTITHREADED) };
        Self {
            initialized: hr.is_ok(),
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTier {
    LowEnd,
    MidRange,
    HighEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub logical_cores: usize,
    pub total_ram_mb: u64,
    pub is_laptop_or_battery: bool,
    pub tier: HardwareTier,
    #[serde(default)]
    pub cpu_name: String,
    #[serde(default)]
    pub gpu_names: Vec<String>,
    #[serde(default)]
    pub motherboard: String,
    #[serde(default)]
    pub storage_drives: Vec<String>,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let logical_cores = sys.cpus().len().max(1);
        let total_ram_mb = sys.total_memory() / (1024 * 1024);
        let is_laptop_or_battery = detect_battery_presence();

        let tier = if logical_cores >= 8 && total_ram_mb >= 14_336 {
            HardwareTier::HighEnd
        } else if logical_cores >= 4 && total_ram_mb >= 7_168 {
            HardwareTier::MidRange
        } else {
            HardwareTier::LowEnd
        };

        let cpu_name = detect_cpu_name(&sys);
        let gpu_names = detect_gpu_names();
        let motherboard = detect_motherboard_name();
        let storage_drives = detect_storage_drives();

        Self {
            logical_cores,
            total_ram_mb,
            is_laptop_or_battery,
            tier,
            cpu_name,
            gpu_names,
            motherboard,
            storage_drives,
        }
    }
}

fn detect_cpu_name(sys: &System) -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey_with_flags(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        if let Ok(name) = k.get_value::<String, _>("ProcessorNameString") {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    sys.cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Procesador x86_64".into())
}

fn detect_motherboard_name() -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(k) = hklm.open_subkey_with_flags(
        r"HARDWARE\DESCRIPTION\System\BIOS",
        KEY_READ | KEY_WOW64_64KEY,
    ) {
        let mfg = k
            .get_value::<String, _>("BaseBoardManufacturer")
            .unwrap_or_default()
            .trim()
            .to_string();
        let prod = k
            .get_value::<String, _>("BaseBoardProduct")
            .unwrap_or_default()
            .trim()
            .to_string();
        let sys_prod = k
            .get_value::<String, _>("SystemProductName")
            .unwrap_or_default()
            .trim()
            .to_string();

        let board = match (mfg.is_empty(), prod.is_empty()) {
            (false, false) => format!("{} {}", mfg, prod),
            (false, true) => mfg,
            (true, false) => prod,
            (true, true) => sys_prod,
        };
        if !board.is_empty() {
            return board;
        }
    }
    "Placa Base Estándar".into()
}

fn detect_gpu_names() -> Vec<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut gpus = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(class_key) = hklm.open_subkey_with_flags(base, KEY_READ | KEY_WOW64_64KEY) {
        for idx in 0..16 {
            let sub = format!("{:04}", idx);
            if let Ok(dev_key) = class_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WOW64_64KEY) {
                if let Ok(desc) = dev_key.get_value::<String, _>("DriverDesc") {
                    let clean = desc.trim().to_string();
                    if !clean.is_empty() && seen.insert(clean.clone()) {
                        gpus.push(clean);
                    }
                }
            }
        }
    }

    if gpus.len() > 1 {
        gpus.retain(|g| !g.eq_ignore_ascii_case("Microsoft Basic Display Adapter"));
    }
    if gpus.is_empty() {
        gpus.push("Adaptador de pantalla estándar".into());
    }
    gpus
}

fn read_cstr_at_offset(buf: &[u8], offset: u32) -> String {
    let off = offset as usize;
    if offset == 0 || off >= buf.len() {
        return String::new();
    }
    let slice = &buf[off..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

fn detect_storage_drives() -> Vec<String> {
    let mut drives = Vec::new();
    for drive_num in 0..8 {
        let path_str = format!(r"\\.\PhysicalDrive{}", drive_num);
        let wide = to_wide_null(&path_str);
        let raw = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        };
        let Some(handle) = SafeHandle::new(raw.unwrap_or(HANDLE(ptr::null_mut()))) else {
            continue;
        };

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0; 1],
        };

        let mut buf = AlignedBuf::zeroed(1024);
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle.raw(),
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(&query as *const _ as *const _),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        };

        if ok.is_err() || (returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            continue;
        }

        let valid = &buf[..(returned as usize).min(buf.len())];
        let desc = unsafe { &*(valid.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
        let vendor = read_cstr_at_offset(valid, desc.VendorIdOffset);
        let product = read_cstr_at_offset(valid, desc.ProductIdOffset);

        let bus_label = match desc.BusType.0 {
            17 => "NVMe",
            11 => "SATA",
            7 => "USB",
            8 => "RAID",
            1 | 3 | 10 => "SCSI/ATA",
            _ => "Disco",
        };

        let model = match (vendor.is_empty(), product.is_empty()) {
            (false, false) if !product.to_ascii_lowercase().starts_with(&vendor.to_ascii_lowercase()) => {
                format!("{} {}", vendor, product)
            }
            (_, false) => product,
            (false, true) => vendor,
            (true, true) => format!("Unidad Física {}", drive_num),
        };

        drives.push(format!("[{}] {}", bus_label, model));
    }
    drives
}

pub fn detect_battery_presence() -> bool {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_ok() {
        status.BatteryFlag != 128 && status.BatteryFlag != 255
    } else {
        false
    }
}

/// Reinicia Windows de forma nativa habilitando `SE_SHUTDOWN_NAME` en el token actual.
pub fn reboot_system_now() -> CoreResult<()> {
    let mut raw_token = HANDLE::default();
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut raw_token,
        )
    }
    .map_err(|e| CoreError::SystemTool {
        message: format!("OpenProcessToken falló para reiniciar: {}", e),
        source: None,
    })?;

    let token = SafeHandle::new(raw_token)
        .ok_or_else(|| CoreError::from_win32("Token de proceso inválido"))?;

    let mut luid = LUID::default();
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid) }.map_err(
        |e| CoreError::SystemTool {
            message: format!("LookupPrivilegeValueW(SE_SHUTDOWN_NAME) falló: {}", e),
            source: None,
        },
    )?;

    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };

    unsafe { AdjustTokenPrivileges(token.raw(), false, Some(&tp), 0, None, None) }.map_err(
        |e| CoreError::SystemTool {
            message: format!("AdjustTokenPrivileges(SE_SHUTDOWN_NAME) falló: {}", e),
            source: None,
        },
    )?;

    unsafe {
        ExitWindowsEx(
            EWX_REBOOT | EWX_FORCEIFHUNG,
            SHTDN_REASON_MAJOR_APPLICATION | SHTDN_REASON_FLAG_PLANNED,
        )
    }
    .map_err(|e| CoreError::SystemTool {
        message: format!("ExitWindowsEx(EWX_REBOOT) falló: {}", e),
        source: None,
    })?;

    Ok(())
}