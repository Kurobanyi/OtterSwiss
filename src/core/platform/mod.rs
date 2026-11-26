//! Primitivas de Windows: cadenas anchas, handles RAII, carpetas conocidas, archivos verificados, procesos,
//! elevación, registro, servicios y energía. No conocen ninguna función de la app.

pub(crate) mod elevation;
pub mod folders;
pub(crate) mod guid;
pub(crate) mod handles;
pub(crate) mod json_file;
pub(crate) mod memory;
pub(crate) mod mutex;
pub(crate) mod power;
pub(crate) mod process;
pub mod registry;
pub(crate) mod safe_fs;
pub(crate) mod services;
pub(crate) mod user_settings;
pub(crate) mod version;
pub mod wide;
