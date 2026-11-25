//! Actualizaciones directas de fabricantes (NVIDIA e Intel) y la política fija de cada uno: hosts de descarga y
//! firmante Authenticode (SEC-30).

mod intel;
mod nvidia;

use crate::core::error::{CoreError, CoreResult};

use intel::{INTEL_DOWNLOAD_HOSTS, INTEL_SIGNER};
use nvidia::{NVIDIA_DOWNLOAD_HOSTS, NVIDIA_SIGNER};

/// SEC-30: hosts permitidos y firmante Authenticode de cada fabricante.
pub(in crate::core::drivers) fn vendor_policy(
    vendor: &str,
) -> CoreResult<(&'static [&'static str], &'static str)> {
    match vendor {
        "NVIDIA" => Ok((NVIDIA_DOWNLOAD_HOSTS, NVIDIA_SIGNER)),
        "Intel" => Ok((INTEL_DOWNLOAD_HOSTS, INTEL_SIGNER)),
        other => Err(CoreError::Validation(format!(
            "Fabricante de drivers no soportado: '{other}'"
        ))),
    }
}
