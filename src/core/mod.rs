//! Lógica de la app, sin Dioxus ni nada de presentación. Capas, de abajo hacia arriba:
//! `error` → `platform` → `security` → funciones (`optimize`, `debloat`, `uninstall`, `archive`, `store`,
//! `cleaner`, `dns`…). Una capa no importa de las que están encima.

// HE-14: ninguna función de core pasa del techo de `clippy.toml` (hoy la mayor tiene 176 líneas; CL-05 las va partiendo).
#![warn(clippy::too_many_lines)]

pub mod error;
pub mod platform;
pub mod security;

pub mod archive;
pub mod cleaner;
pub mod cli;
pub mod context_menu;
pub mod debloat;
pub mod dns;
pub mod drivers;
pub mod hardware;
pub mod optimize;
pub(crate) mod progress;
pub mod rollback;
pub mod store;
pub(crate) mod sys_tools;
#[cfg(test)]
pub(crate) mod test_util;
pub mod uninstall;
pub mod updater;
