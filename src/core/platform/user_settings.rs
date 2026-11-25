//! Preferencias del usuario en Windows que la app respeta (idioma y modo claro u oscuro).

use winreg::RegKey;
use winreg::enums::KEY_READ;

/// Si el idioma de la interfaz de Windows del usuario es español (cualquier variante).
pub fn user_ui_language_is_spanish() -> bool {
    const LANG_SPANISH: u16 = 0x0a;
    // SAFETY: función sin parámetros que solo lee la configuración del usuario.
    let lang_id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    lang_id & 0x3ff == LANG_SPANISH
}

/// Si el usuario eligió el modo claro para las aplicaciones en Configuración > Personalización > Colores.
/// Sin el valor (Windows antiguo o política) se asume oscuro, el tema por defecto de la app.
pub fn apps_use_light_theme() -> bool {
    RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            KEY_READ,
        )
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .is_ok_and(|v| v != 0)
}
