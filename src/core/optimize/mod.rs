//! Perfil de optimización: catálogo de ajustes, motor que los aplica y respaldo para revertirlos.

pub mod backup;
pub mod engine;
pub mod power_plan;
pub(crate) mod registry_tweaks;
pub(crate) mod service_tweaks;
pub mod tweaks_catalog;
