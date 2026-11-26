//! Interfaz en Dioxus. Solo presenta: toda la lógica y el acceso a Windows están en `core`.

// CL-01: toda la FFI vive en core; la UI no puede usar unsafe.
#![forbid(unsafe_code)]

mod app;
pub(crate) mod components;
pub mod document;
pub(crate) mod format;
pub mod i18n;
pub(crate) mod installer;
pub(crate) mod layout;
pub(crate) mod prefs;
pub(crate) mod task;
pub mod theme;
pub mod views;

pub use app::App;
