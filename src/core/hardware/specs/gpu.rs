//! GPU por DXGI, sin el render de software ni adaptadores repetidos.

use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};

use super::GpuSpecs;
use crate::core::error::{CoreError, CoreResult};

/// (proveedor, dispositivo, subsistema, revisión, VRAM dedicada) de un adaptador DXGI.
pub(super) type GpuKey = (u32, u32, u32, u32, usize);

/// UI-16: descarta «Microsoft Basic Render Driver» (1414:008C) y la segunda vista del mismo hardware que
/// crea un controlador de pantalla virtual (mismo hardware, otro LUID). Dos GPU idénticas reales se ven como una.
pub(super) fn keep_gpu_adapter(seen: &mut std::collections::HashSet<GpuKey>, key: GpuKey) -> bool {
    const MICROSOFT: u32 = 0x1414;
    const BASIC_RENDER: u32 = 0x8C;
    if key.0 == MICROSOFT && key.1 == BASIC_RENDER {
        return false;
    }
    seen.insert(key)
}

pub(super) fn read_dxgi_gpus() -> CoreResult<Vec<GpuSpecs>> {
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| CoreError::SystemTool {
            message: format!("Error inicializando DXGI: {e}"),
            source: None,
        })?;

    let mut gpus = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut index = 0u32;

    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };

        if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32)) != 0 {
            continue;
        }
        let key = (
            desc.VendorId,
            desc.DeviceId,
            desc.SubSysId,
            desc.Revision,
            desc.DedicatedVideoMemory,
        );
        if !keep_gpu_adapter(&mut seen, key) {
            continue;
        }

        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..end])
            .trim()
            .to_string();

        let vendor_name = match desc.VendorId {
            0x1002 => "AMD",
            0x10DE => "NVIDIA",
            0x8086 => "Intel",
            _ => "Otro",
        }
        .to_string();

        gpus.push(GpuSpecs {
            name,
            vendor_name,
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            dedicated_vram_mb: (desc.DedicatedVideoMemory as u64) / (1024 * 1024),
            shared_system_mb: (desc.SharedSystemMemory as u64) / (1024 * 1024),
        });
    }

    Ok(gpus)
}
