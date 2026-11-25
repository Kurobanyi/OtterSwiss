//! Línea de comandos de OtterSwiss. Los flags se escriben solo aquí: los usan `main.rs`, el relanzamiento elevado
//! (`ElevationManager`), el menú contextual y la UI. Un flag permitido sin manejador ya no puede quedar suelto
//! (así nació SEC-29).

use std::path::PathBuf;

pub const ELEVATED: &str = "--elevated";
pub const ROLLBACK: &str = "--rollback";
pub const INSTALL_CONTEXT_MENU: &str = "--install-context-menu";
pub const UNINSTALL_CONTEXT_MENU: &str = "--uninstall-context-menu";
pub const EXTRACT: &str = "--extract";
pub const COMPRESS_ZIP: &str = "--compress-zip";

/// Flags que se pueden pedir al relanzar la app como administrador. Cada uno tiene su rama en `CliCommand::parse`.
pub const RELAUNCH_FLAGS: &[&str] = &[
    ELEVATED,
    ROLLBACK,
    INSTALL_CONTEXT_MENU,
    UNINSTALL_CONTEXT_MENU,
    EXTRACT,
    COMPRESS_ZIP,
];

/// Lo que pide la línea de comandos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Revertir todos los cambios y salir (lo llama el desinstalador).
    Rollback,
    InstallContextMenu,
    UninstallContextMenu,
    /// Extraer el archivo indicado en la mini ventana.
    Extract(PathBuf),
    /// Comprimir la ruta indicada a .zip en la mini ventana.
    CompressZip(PathBuf),
    /// Abrir la ventana principal (también con `--elevated`, que solo marca un relanzamiento).
    Gui,
}

impl CliCommand {
    /// `args` sin el nombre del ejecutable. Las rutas llegan del menú contextual entre comillas.
    pub fn parse(args: &[String]) -> Self {
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let path_after = |flag: &str| {
            let pos = args.iter().position(|a| a == flag)?;
            args.get(pos + 1)
                .map(|p| PathBuf::from(p.trim().trim_matches('"')))
        };

        if has(ROLLBACK) {
            Self::Rollback
        } else if has(INSTALL_CONTEXT_MENU) {
            Self::InstallContextMenu
        } else if has(UNINSTALL_CONTEXT_MENU) {
            Self::UninstallContextMenu
        } else if let Some(src) = path_after(EXTRACT) {
            Self::Extract(src)
        } else if let Some(src) = path_after(COMPRESS_ZIP) {
            Self::CompressZip(src)
        } else {
            Self::Gui
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cada_flag_de_relanzamiento_tiene_su_rama() {
        for flag in RELAUNCH_FLAGS {
            let cmd = CliCommand::parse(&args(&[flag, r#""C:\datos\a.zip""#]));
            let esperado = match *flag {
                ELEVATED => CliCommand::Gui,
                ROLLBACK => CliCommand::Rollback,
                INSTALL_CONTEXT_MENU => CliCommand::InstallContextMenu,
                UNINSTALL_CONTEXT_MENU => CliCommand::UninstallContextMenu,
                EXTRACT => CliCommand::Extract(PathBuf::from(r"C:\datos\a.zip")),
                COMPRESS_ZIP => CliCommand::CompressZip(PathBuf::from(r"C:\datos\a.zip")),
                other => panic!("flag sin rama: {other}"),
            };
            assert_eq!(cmd, esperado, "{flag}");
        }
    }

    #[test]
    fn sin_ruta_o_sin_flags_abre_la_ventana() {
        assert_eq!(CliCommand::parse(&args(&[])), CliCommand::Gui);
        assert_eq!(CliCommand::parse(&args(&[EXTRACT])), CliCommand::Gui);
        assert_eq!(
            CliCommand::parse(&args(&["--apply-profile"])),
            CliCommand::Gui
        );
    }
}
