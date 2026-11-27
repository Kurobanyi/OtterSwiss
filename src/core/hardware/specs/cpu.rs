//! CPU: nombre y familia por CPUID, núcleos e hilos, y cachés por `GetLogicalProcessorInformationEx`.

use std::arch::x86_64::{__cpuid, __cpuid_count, CpuidResult};

use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationCache, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

use super::CpuSpecs;
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::memory::AlignedBuf;

/// CPUID de una hoja. En Rust 1.93 (la versión mínima del proyecto) la intrínseca todavía es `unsafe`; en las versiones
/// nuevas ya no, y el bloque sobra: por eso el `allow`.
#[allow(
    unused_unsafe,
    reason = "en Rust 1.93 la intrínseca es unsafe; en las versiones nuevas el bloque sobra"
)]
fn cpuid(leaf: u32) -> CpuidResult {
    // SAFETY: CPUID existe en todo procesador x86_64 y solo lee registros; no toca memoria.
    unsafe { __cpuid(leaf) }
}

/// CPUID de una hoja y subhoja (ver `cpuid`).
#[allow(
    unused_unsafe,
    reason = "en Rust 1.93 la intrínseca es unsafe; en las versiones nuevas el bloque sobra"
)]
fn cpuid_count(leaf: u32, sub_leaf: u32) -> CpuidResult {
    // SAFETY: igual que en `cpuid`: instrucción siempre disponible en x86_64 y sin acceso a memoria.
    unsafe { __cpuid_count(leaf, sub_leaf) }
}

pub(super) fn read_cpu_specs() -> CoreResult<CpuSpecs> {
    let leaf0 = cpuid(0);
    let max_basic_leaf = leaf0.eax;

    let mut vendor_bytes = Vec::with_capacity(12);
    vendor_bytes.extend_from_slice(&leaf0.ebx.to_le_bytes());
    vendor_bytes.extend_from_slice(&leaf0.edx.to_le_bytes());
    vendor_bytes.extend_from_slice(&leaf0.ecx.to_le_bytes());
    let vendor = String::from_utf8_lossy(&vendor_bytes).trim().to_string();

    let (family, model, stepping, mut features) = if max_basic_leaf >= 1 {
        let leaf1 = cpuid(1);
        let step = leaf1.eax & 0xF;
        let base_model = (leaf1.eax >> 4) & 0xF;
        let base_family = (leaf1.eax >> 8) & 0xF;
        let ext_model = (leaf1.eax >> 16) & 0xF;
        let ext_family = (leaf1.eax >> 20) & 0xFF;

        let eff_family = if base_family == 0xF {
            base_family + ext_family
        } else {
            base_family
        };
        let eff_model = if base_family == 0x6 || base_family == 0xF {
            (ext_model << 4) + base_model
        } else {
            base_model
        };

        let mut isa = Vec::new();
        if (leaf1.edx & (1 << 25)) != 0 {
            isa.push("SSE".into());
        }
        if (leaf1.edx & (1 << 26)) != 0 {
            isa.push("SSE2".into());
        }
        if (leaf1.ecx & (1 << 0)) != 0 {
            isa.push("SSE3".into());
        }
        if (leaf1.ecx & (1 << 9)) != 0 {
            isa.push("SSSE3".into());
        }
        if (leaf1.ecx & (1 << 19)) != 0 {
            isa.push("SSE4.1".into());
        }
        if (leaf1.ecx & (1 << 20)) != 0 {
            isa.push("SSE4.2".into());
        }
        if (leaf1.ecx & (1 << 25)) != 0 {
            isa.push("AES-NI".into());
        }
        if (leaf1.ecx & (1 << 28)) != 0 {
            isa.push("AVX".into());
        }

        if max_basic_leaf >= 7 {
            let leaf7 = cpuid_count(7, 0);
            if (leaf7.ebx & (1 << 5)) != 0 {
                isa.push("AVX2".into());
            }
            if (leaf7.ebx & (1 << 16)) != 0 {
                isa.push("AVX-512F".into());
            }
            if (leaf7.ebx & (1 << 29)) != 0 {
                isa.push("SHA".into());
            }
        }

        (eff_family, eff_model, step, isa)
    } else {
        (0, 0, 0, Vec::new())
    };

    let max_ext = cpuid(0x8000_0000).eax;
    let brand = if max_ext >= 0x8000_0004 {
        let mut bytes = Vec::with_capacity(48);
        for leaf in 0x8000_0002..=0x8000_0004 {
            let res = cpuid(leaf);
            bytes.extend_from_slice(&res.eax.to_le_bytes());
            bytes.extend_from_slice(&res.ebx.to_le_bytes());
            bytes.extend_from_slice(&res.ecx.to_le_bytes());
            bytes.extend_from_slice(&res.edx.to_le_bytes());
        }
        String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    } else {
        "Procesador x86_64".to_string()
    };

    if max_ext >= 0x8000_0001 {
        let ext1 = cpuid(0x8000_0001);
        if (ext1.edx & (1 << 29)) != 0 {
            features.push("x86-64".into());
        }
    }

    let (physical_cores, p_cores, e_cores, logical_threads) = read_core_topology()?;
    let (l1_bytes, l2_bytes, l3_bytes) = read_cache_sizes()?;

    Ok(CpuSpecs {
        brand,
        vendor,
        family,
        model,
        stepping,
        physical_cores,
        p_cores,
        e_cores,
        logical_threads,
        l1_cache_kb: l1_bytes / 1024,
        l2_cache_kb: l2_bytes / 1024,
        l3_cache_kb: l3_bytes / 1024,
        instruction_sets: features,
    })
}

fn read_core_topology() -> CoreResult<(usize, Option<usize>, Option<usize>, usize)> {
    let mut len = 0u32;
    // SAFETY: el búfer tiene la longitud que se informa en `len`; la API escribe como mucho eso.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    if len == 0 {
        return Err(CoreError::system_tool_msg(
            "No se pudo determinar el tamaño de topología CPU",
        ));
    }

    let mut buf = AlignedBuf::zeroed(len as usize);
    // SAFETY: el búfer tiene la longitud que se informa en `len`; la API escribe como mucho eso.
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(
                buf.as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            ),
            &mut len,
        )
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error leyendo núcleos físicos: {e}"),
            source: Some(Box::new(e)),
        })?;
    }

    let mut cores_classes = Vec::new();
    let mut logical_threads = 0usize;
    let mut offset = 0usize;

    while offset + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= len as usize {
        // SAFETY: el búfer es un AlignedBuf (alineado a 8) y se comprobó que la API escribió al menos el tamaño de la estructura.
        let info = unsafe {
            &*(buf
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>())
        };
        if info.Size == 0 {
            break;
        }
        if info.Relationship == RelationProcessorCore {
            // SAFETY: el búfer es un AlignedBuf (alineado a 8) y se comprobó que la API escribió al menos el tamaño de la estructura.
            let proc_rel = unsafe { &info.Anonymous.Processor };
            cores_classes.push(proc_rel.EfficiencyClass);
            for idx in 0..(proc_rel.GroupCount as usize) {
                if let Some(group) = proc_rel.GroupMask.get(idx) {
                    logical_threads += group.Mask.count_ones() as usize;
                }
            }
        }
        offset += info.Size as usize;
    }

    let physical_cores = cores_classes.len();
    let max_class = cores_classes.iter().copied().max().unwrap_or(0);
    let (p_cores, e_cores) = if max_class > 0 {
        let p = cores_classes.iter().filter(|&&c| c > 0).count();
        let e = physical_cores.saturating_sub(p);
        (Some(p), Some(e))
    } else {
        (None, None)
    };

    Ok((physical_cores, p_cores, e_cores, logical_threads))
}

fn read_cache_sizes() -> CoreResult<(u32, u32, u32)> {
    let mut len = 0u32;
    // SAFETY: el búfer tiene la longitud que se informa en `len`; la API escribe como mucho eso.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationCache, None, &mut len) };
    if len == 0 {
        return Ok((0, 0, 0));
    }

    let mut buf = AlignedBuf::zeroed(len as usize);
    // SAFETY: el búfer tiene la longitud que se informa en `len`; la API escribe como mucho eso.
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationCache,
            Some(
                buf.as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            ),
            &mut len,
        )
        .map_err(|e| CoreError::SystemTool {
            message: format!("Error leyendo cachés CPU: {e}"),
            source: Some(Box::new(e)),
        })?;
    }

    let (mut l1, mut l2, mut l3) = (0u32, 0u32, 0u32);
    let mut offset = 0usize;

    while offset + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= len as usize {
        // SAFETY: el búfer es un AlignedBuf (alineado a 8) y se comprobó que la API escribió al menos el tamaño de la estructura.
        let info = unsafe {
            &*(buf
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>())
        };
        if info.Size == 0 {
            break;
        }
        if info.Relationship == RelationCache {
            // SAFETY: el búfer es un AlignedBuf (alineado a 8) y se comprobó que la API escribió al menos el tamaño de la estructura.
            let cache = unsafe { info.Anonymous.Cache };
            match cache.Level {
                1 => l1 = l1.saturating_add(cache.CacheSize),
                2 => l2 = l2.saturating_add(cache.CacheSize),
                3 => l3 = l3.saturating_add(cache.CacheSize),
                _ => {}
            }
        }
        offset += info.Size as usize;
    }

    Ok((l1, l2, l3))
}
