//! Interfaz en Dioxus. Solo presenta: toda la lógica y el acceso a Windows están en `core`.

// CL-01: toda la FFI vive en core; la UI no puede usar unsafe.
#![forbid(unsafe_code)]

mod app;
pub mod components;
pub mod i18n;
pub mod installer;
pub mod prefs;
pub mod theme;
pub mod views;

pub use app::{sync_document_attrs, App};
