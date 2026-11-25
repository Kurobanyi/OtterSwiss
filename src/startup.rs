//! Lo que pasa antes de abrir una ventana Dioxus: diálogos nativos, comprobación de WebView2 y errores inesperados.
//! No depende de WebView2, porque se usa justo cuando puede faltar.

/// Pregunta Sí/No con un cuadro nativo. `true` si se eligió «Sí».
fn ask_yes_no(title: &str, text: &str) -> bool {
    native_message(title, text, true, false)
}

fn show_warning(title: &str, text: &str) {
    native_message(title, text, false, false);
}

fn show_error(title: &str, text: &str) {
    native_message(title, text, false, true);
}

/// Cuadro de mensaje nativo (no depende de WebView2). Devuelve `true` si se eligió «Sí». CL-16: los llamadores usan
/// `ask_yes_no`, `show_warning` o `show_error` en vez de pasar los dos booleanos.
fn native_message(title: &str, text: &str, ask_yes_no: bool, is_error: bool) -> bool {
    use otter_swiss::core::platform::wide::to_wide_null;
    use windows::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO, MessageBoxW,
    };
    use windows::core::PCWSTR;
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);
    let mut style = if ask_yes_no { MB_YESNO } else { MB_OK };
    style |= if is_error {
        MB_ICONERROR
    } else {
        MB_ICONWARNING
    };
    // SAFETY: las dos cadenas terminan en NUL y viven hasta que MessageBoxW vuelve.
    let res = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            style,
        )
    };
    res == IDYES
}

/// ERR-13: la interfaz se dibuja con Microsoft Edge WebView2. Sin él, dioxus-desktop termina con un `panic`
/// antes de abrir la ventana, así que se comprueba aquí y se ofrece instalarlo con el instalador verificado.
/// Devuelve `false` si no se puede continuar.
pub fn ensure_webview2(en: bool) -> bool {
    use otter_swiss::core::store::dependencies::DependencyManager;

    if DependencyManager::is_webview2_installed() {
        return true;
    }
    let (title, ask) = if en {
        (
            "OtterSwiss — WebView2 required",
            "OtterSwiss needs the Microsoft Edge WebView2 Runtime to show its window, and it is not installed.\n\n\
             Download and install it now from Microsoft? (about 2 MB, verified before running)",
        )
    } else {
        (
            "OtterSwiss — Falta WebView2",
            "OtterSwiss necesita Microsoft Edge WebView2 Runtime para mostrar su ventana y no está instalado.\n\n\
             ¿Descargarlo e instalarlo ahora desde Microsoft? (unos 2 MB, verificado antes de ejecutarse)",
        )
    };
    if !ask_yes_no(title, ask) {
        return false;
    }
    match DependencyManager::install_runtime("webview2") {
        Ok(()) if DependencyManager::is_webview2_installed() => true,
        Ok(()) => {
            show_warning(
                title,
                if en {
                    "WebView2 was installed but is not detected yet. Restart OtterSwiss."
                } else {
                    "WebView2 se instaló pero todavía no se detecta. Vuelve a abrir OtterSwiss."
                },
            );
            false
        }
        Err(e) => {
            show_error(
                title,
                &if en {
                    format!(
                        "WebView2 could not be installed: {e}\n\nInstall it from \
                         https://developer.microsoft.com/microsoft-edge/webview2/ and open OtterSwiss again."
                    )
                } else {
                    format!(
                        "No se pudo instalar WebView2: {e}\n\nInstálalo desde \
                         https://developer.microsoft.com/microsoft-edge/webview2/ y vuelve a abrir OtterSwiss."
                    )
                },
            );
            false
        }
    }
}

/// Un `panic` (p. ej. dentro de dioxus-desktop al crear la ventana) se muestra en un cuadro nativo en vez de
/// cerrar la app sin explicación: el binario es `windows_subsystem = "windows"` y no tiene consola.
pub fn install_panic_dialog(en: bool) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| {
                if en {
                    "unknown error"
                } else {
                    "error desconocido"
                }
                .into()
            });
        let place = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        // IN-05: el diálogo sigue el idioma de la app.
        let (title, intro) = if en {
            (
                "OtterSwiss — Unexpected error",
                "OtterSwiss closed because of an unexpected error:",
            )
        } else {
            (
                "OtterSwiss — Error inesperado",
                "OtterSwiss se cerró por un error inesperado:",
            )
        };
        show_error(title, &format!("{intro}\n\n{msg}{place}"));
        default_hook(info);
    }));
}
