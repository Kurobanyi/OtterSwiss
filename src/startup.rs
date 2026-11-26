//! Lo que pasa antes de abrir una ventana Dioxus: diálogos nativos, comprobación de WebView2 y errores inesperados.
//! No depende de WebView2, porque se usa justo cuando puede faltar.

use otter_swiss::ui::i18n::strings;

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
    use otter_swiss::core::store::dependencies;

    if dependencies::is_webview2_installed() {
        return true;
    }
    let txt = strings(en).shell;
    let (title, ask) = (txt.webview2_title, txt.webview2_ask);
    if !ask_yes_no(title, ask) {
        return false;
    }
    match dependencies::install_runtime("webview2") {
        Ok(()) if dependencies::is_webview2_installed() => true,
        Ok(()) => {
            show_warning(title, txt.webview2_not_detected);
            false
        }
        Err(e) => {
            show_error(title, &(txt.webview2_failed)(&e));
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
            .unwrap_or_else(|| strings(en).shell.panic_unknown.into());
        let place = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        // IN-05: el diálogo sigue el idioma de la app.
        let txt = strings(en).shell;
        let (title, intro) = (txt.panic_title, txt.panic_intro);
        show_error(title, &format!("{intro}\n\n{msg}{place}"));
        default_hook(info);
    }));
}
