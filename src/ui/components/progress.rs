//! Barra de progreso accesible (`role="progressbar"`).

use dioxus::prelude::*;

/// CL-11: una sola barra de progreso; antes había seis copias con estilos inline distintos.
#[component]
pub fn ProgressBar(percent: u8) -> Element {
    let pct = percent.min(100);
    rsx! {
        div {
            class: "progress",
            role: "progressbar",
            "aria-valuemin": "0",
            "aria-valuemax": "100",
            "aria-valuenow": "{pct}",
            div { class: "progress-fill", style: "width: {pct}%;" }
        }
    }
}
