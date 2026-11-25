use crate::core::hardware::profile::HardwareTier;
use serde::{Deserialize, Serialize};

pub use crate::core::platform::registry::RegHive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegValueKind {
    Dword,
    Qword,
    Sz,
    ExpandSz,
    Binary,
}

impl RegValueKind {
    pub fn win32_type(self) -> u32 {
        match self {
            Self::Sz => 1,       // REG_SZ
            Self::ExpandSz => 2, // REG_EXPAND_SZ
            Self::Binary => 3,   // REG_BINARY
            Self::Dword => 4,    // REG_DWORD
            Self::Qword => 11,   // REG_QWORD
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegTargetValue {
    Dword(u32),
    Sz(&'static str),
    Binary(&'static [u8]),
}

impl RegTargetValue {
    pub fn kind(&self) -> RegValueKind {
        match self {
            Self::Dword(_) => RegValueKind::Dword,
            Self::Sz(_) => RegValueKind::Sz,
            Self::Binary(_) => RegValueKind::Binary,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Dword(v) => v.to_le_bytes().to_vec(),
            Self::Sz(s) => s
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|u| u.to_le_bytes())
                .collect(),
            Self::Binary(b) => b.to_vec(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegistryTweakSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub hive: RegHive,
    pub subkey: &'static str,
    pub value_name: &'static str,
    pub value: RegTargetValue,
    pub min_tier: HardwareTier,
    pub applies_to_low_end: bool,
    pub requires_relogon: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTargetAction {
    DisableAndStop,
    ManualStart,
}

#[derive(Debug, Clone)]
pub struct ServiceTweakSpec {
    pub id: &'static str,
    pub service_name: &'static str,
    pub label: &'static str,
    pub action: ServiceTargetAction,
    pub min_tier: HardwareTier,
    pub applies_to_low_end: bool,
}

/// Tabla única de verdad (patrón 4 de docs/BUGS.md): usada tanto por el motor para aplicar
/// como por `backup.rs` y `registry.rs` para validar cada clave, valor y tipo.
pub const REGISTRY_TWEAKS: &[RegistryTweakSpec] = &[
    RegistryTweakSpec {
        id: "game_dvr_enabled",
        label: "Desactivar grabación en segundo plano (GameDVR)",
        description: "Deshabilita AppCaptureEnabled en GameDVR para evitar sobrecarga de captura.",
        hive: RegHive::Hkcu,
        subkey: r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
        value_name: "AppCaptureEnabled",
        value: RegTargetValue::Dword(0),
        min_tier: HardwareTier::LowEnd,
        applies_to_low_end: true,
        requires_relogon: false,
    },
    RegistryTweakSpec {
        id: "game_config_store_fse",
        label: "Optimizar modo pantalla completa (GameConfigStore)",
        description: "Configura GameDVR_FSEBehaviorMode=2 para reducir latencia en juegos.",
        hive: RegHive::Hkcu,
        subkey: r"System\GameConfigStore",
        value_name: "GameDVR_FSEBehaviorMode",
        value: RegTargetValue::Dword(2),
        min_tier: HardwareTier::LowEnd,
        applies_to_low_end: true,
        requires_relogon: false,
    },
    RegistryTweakSpec {
        id: "hw_gpu_scheduling",
        label: "Programación de GPU acelerada por hardware (HAGS)",
        description: "Activa HwSchMode=2 en el subsistema gráfico del kernel.",
        hive: RegHive::Hklm,
        subkey: r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
        value_name: "HwSchMode",
        value: RegTargetValue::Dword(2),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "mmcss_network_throttling",
        label: "Eliminar limitación de red multimedia (NetworkThrottlingIndex)",
        description: "Fija NetworkThrottlingIndex=0xFFFFFFFF en Multimedia\\SystemProfile.",
        hive: RegHive::Hklm,
        subkey: r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
        value_name: "NetworkThrottlingIndex",
        value: RegTargetValue::Dword(0xFFFF_FFFF),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "mmcss_system_responsiveness",
        label: "Priorizar capacidad de respuesta interactiva (SystemResponsiveness)",
        description: "Reduce SystemResponsiveness a 10% para procesos en primer plano.",
        hive: RegHive::Hklm,
        subkey: r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
        value_name: "SystemResponsiveness",
        value: RegTargetValue::Dword(10),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "mouse_no_acceleration",
        label: "Desactivar aceleración del puntero (curva lineal)",
        description: "Fija MouseSpeed=0, MouseThreshold1=0 y MouseThreshold2=0 (requiere reiniciar sesión).",
        hive: RegHive::Hkcu,
        subkey: r"Control Panel\Mouse",
        value_name: "MouseSpeed",
        value: RegTargetValue::Sz("0"),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "mouse_threshold_1",
        label: "Umbral 1 de aceleración de ratón a cero",
        description: "Parte del perfil lineal de ratón en Control Panel\\Mouse.",
        hive: RegHive::Hkcu,
        subkey: r"Control Panel\Mouse",
        value_name: "MouseThreshold1",
        value: RegTargetValue::Sz("0"),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "mouse_threshold_2",
        label: "Umbral 2 de aceleración de ratón a cero",
        description: "Parte del perfil lineal de ratón en Control Panel\\Mouse.",
        hive: RegHive::Hkcu,
        subkey: r"Control Panel\Mouse",
        value_name: "MouseThreshold2",
        value: RegTargetValue::Sz("0"),
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
        requires_relogon: true,
    },
    RegistryTweakSpec {
        id: "telemetry_minimal",
        label: "Reducir nivel de telemetría diagnóstica",
        description: "Establece AllowTelemetry=1 (mínimo efectivo en ediciones Home/Pro; 0 solo aplica en Enterprise/Education).",
        hive: RegHive::Hklm,
        subkey: r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        value_name: "AllowTelemetry",
        value: RegTargetValue::Dword(1),
        min_tier: HardwareTier::LowEnd,
        applies_to_low_end: true,
        requires_relogon: false,
    },
];

pub const SERVICE_TWEAKS: &[ServiceTweakSpec] = &[
    ServiceTweakSpec {
        id: "svc_diagtrack",
        service_name: "DiagTrack",
        label: "Experiencias del usuario y telemetría conectadas",
        action: ServiceTargetAction::DisableAndStop,
        min_tier: HardwareTier::LowEnd,
        applies_to_low_end: true,
    },
    ServiceTweakSpec {
        id: "svc_dmwappush",
        service_name: "dmwappushservice",
        label: "Servicio de enrutamiento de mensajes push WAP",
        action: ServiceTargetAction::DisableAndStop,
        min_tier: HardwareTier::LowEnd,
        applies_to_low_end: true,
    },
    ServiceTweakSpec {
        id: "svc_sysmain",
        service_name: "SysMain",
        label: "Superfetch / SysMain (precarga en memoria)",
        action: ServiceTargetAction::DisableAndStop,
        min_tier: HardwareTier::MidRange,
        applies_to_low_end: false,
    },
];

/// Comprueba si una entrada de registro del respaldo coincide exactamente con la tabla única.
pub fn is_allowed_registry_entry(hive: RegHive, subkey: &str, value_name: &str) -> bool {
    REGISTRY_TWEAKS.iter().any(|spec| {
        spec.hive == hive
            && spec.subkey.eq_ignore_ascii_case(subkey)
            && spec.value_name.eq_ignore_ascii_case(value_name)
    })
}

/// Comprueba si un nombre de servicio del respaldo pertenece a la tabla única.
pub fn is_allowed_service_name(service_name: &str) -> bool {
    SERVICE_TWEAKS
        .iter()
        .any(|spec| spec.service_name.eq_ignore_ascii_case(service_name))
}

pub use crate::core::platform::services::is_valid_service_start_type;
