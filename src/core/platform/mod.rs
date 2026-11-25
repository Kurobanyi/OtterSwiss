//! Primitivas de Windows: cadenas anchas, handles RAII, carpetas conocidas, archivos verificados, procesos,
//! elevación, registro, servicios y energía. No conocen ninguna función de la app.

pub mod elevation;
pub mod folders;
pub mod guid;
pub mod handles;
pub mod json_file;
pub mod memory;
pub mod mutex;
pub mod power;
pub mod process;
pub mod registry;
pub mod safe_fs;
pub mod services;
pub mod user_settings;
pub mod wide;
