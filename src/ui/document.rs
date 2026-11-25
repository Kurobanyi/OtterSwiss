//! El documento HTML de cada ventana: estilos, script de foco de los diálogos y atributos de `<html>`.

use dioxus::prelude::*;

/// TE-22: los estilos van una sola vez en el `<head>` de cada ventana, con el script de foco de los diálogos.
pub fn document_head() -> String {
    format!(
        "<style>{}</style>{}",
        include_str!("../../assets/main.css"),
        FOCUS_TRAP_SCRIPT
    )
}

/// TE-04 e IN-07: copia el tema y el idioma a `<html>` (color-scheme, scrollbars, lectores de pantalla); la raíz de
/// la app ya los lleva desde el primer render.
pub fn sync_document_attrs(css_theme: &'static str, html_lang: &'static str) {
    document::eval(&format!(
        "document.documentElement.dataset.theme = '{css_theme}'; document.documentElement.lang = '{html_lang}';"
    ));
}

/// UI-09: con un `[role=dialog]` abierto, Tab y Mayús+Tab recorren solo sus controles.
const FOCUS_TRAP_SCRIPT: &str = r#"<script>
document.addEventListener('keydown', function (e) {
  if (e.key !== 'Tab') return;
  var dialogs = document.querySelectorAll('[role=dialog]');
  if (!dialogs.length) return;
  var dialog = dialogs[dialogs.length - 1];
  var items = Array.prototype.filter.call(
    dialog.querySelectorAll('button, input, select, textarea, a[href], [tabindex]:not([tabindex="-1"])'),
    function (el) { return !el.disabled && el.offsetParent !== null; });
  if (!items.length) { e.preventDefault(); dialog.focus(); return; }
  var first = items[0], last = items[items.length - 1];
  if (!dialog.contains(document.activeElement)) { e.preventDefault(); first.focus(); }
  else if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
});
</script>"#;
