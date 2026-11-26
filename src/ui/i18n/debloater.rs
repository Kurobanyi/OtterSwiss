//! IN-02: textos de la vista debloater. Un campo por texto; el compilador exige los dos idiomas.

pub struct DebloaterStrings {
    pub result_failed: &'static str,
    pub result_ok: &'static str,
    pub apply_the_debloat: &'static str,
    pub apps_are_removed_for_every: &'static str,
    pub debloat_progress: &'static str,
    pub not_installed: &'static str,
    pub privacy_ad_block_registry_tweaks: &'static str,
    pub recommended: &'static str,
    pub rescan: &'static str,
    pub revert_registry: &'static str,
    pub summary: &'static str,
    pub reversible: &'static str,
    pub reinstallable: &'static str,
    pub nothing_selected: &'static str,
    pub apps_group_note: &'static str,
    pub tweaks_group_note: &'static str,
    pub selected_of: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub selected_count: fn(&dyn std::fmt::Display) -> String,
    pub apps_removed_tweaks_applied_errors:
        fn(&dyn std::fmt::Display, &dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub cancelled_done_before_cancelling: fn(&dyn std::fmt::Display) -> String,
    pub finished_with_errors_see_the: fn(&dyn std::fmt::Display) -> String,
    pub done: fn(&dyn std::fmt::Display) -> String,
    pub reverted_registry_values: fn(&dyn std::fmt::Display) -> String,
    pub apply: fn(&dyn std::fmt::Display) -> String,
    pub found: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: DebloaterStrings = DebloaterStrings {
    result_failed: "Falló",
    result_ok: "Hecho",
    apply_the_debloat: "¿Aplicar el Debloater?",
    apps_are_removed_for_every: "Las apps se quitan para todos los usuarios de este equipo y de la imagen de Windows. Se pueden reinstalar desde Microsoft Store. Los ajustes de registro se pueden revertir.",
    debloat_progress: "Progreso",
    not_installed: "No instalada",
    privacy_ad_block_registry_tweaks: "Ajustes anti-publicidad",
    recommended: "Recomendados",
    rescan: "Volver a escanear",
    revert_registry: "Revertir registro",
    summary: "Resumen",
    reversible: "Reversible",
    reinstallable: "Se reinstalan desde Store",
    nothing_selected: "Elige al menos un ajuste o una app para aplicar.",
    apps_group_note: "Las apps se quitan para todos los usuarios. La Store y Winget quedan protegidas.",
    tweaks_group_note: "Cambian ajustes de Windows. Revertir registro los devuelve a su valor anterior.",
    selected_of: |a, b| format!("{a} de {b} elegidas"),
    selected_count: |n| format!("{n} elegidas"),
    apps_removed_tweaks_applied_errors: |a0, a1, a2| {
        format!("{a0} apps quitadas, {a1} ajustes aplicados, {a2} errores")
    },
    cancelled_done_before_cancelling: |counts| {
        format!("Cancelado. Hecho antes de cancelar: {counts}.")
    },
    finished_with_errors_see_the: |counts| {
        format!("Terminado con errores: {counts}. Mira el resumen.")
    },
    done: |counts| format!("Listo: {counts}."),
    reverted_registry_values: |count| format!("Revertidos {count} valores de registro."),
    apply: |total_selected| format!("Aplicar ({total_selected})"),
    found: |installed_count| format!("{installed_count} detectadas"),
};

pub static EN: DebloaterStrings = DebloaterStrings {
    result_failed: "Failed",
    result_ok: "Done",
    apply_the_debloat: "Apply the debloat?",
    apps_are_removed_for_every: "Apps are removed for every user of this PC and from the Windows image. You can reinstall them from Microsoft Store. Registry tweaks can be reverted.",
    debloat_progress: "Progress",
    not_installed: "Not installed",
    privacy_ad_block_registry_tweaks: "Privacy and ad-block tweaks",
    recommended: "Recommended",
    rescan: "Rescan",
    revert_registry: "Revert registry",
    summary: "Summary",
    reversible: "Reversible",
    reinstallable: "Reinstall from Store",
    nothing_selected: "Choose at least one tweak or app to apply.",
    apps_group_note: "Apps are removed for all users. Store and Winget stay protected.",
    tweaks_group_note: "These change Windows settings. Revert registry restores their previous values.",
    selected_of: |a, b| format!("{a} of {b} selected"),
    selected_count: |n| format!("{n} selected"),
    apps_removed_tweaks_applied_errors: |a0, a1, a2| {
        format!("{a0} apps removed, {a1} tweaks applied, {a2} errors")
    },
    cancelled_done_before_cancelling: |counts| {
        format!("Cancelled. Done before cancelling: {counts}.")
    },
    finished_with_errors_see_the: |counts| {
        format!("Finished with errors: {counts}. See the summary.")
    },
    done: |counts| format!("Done: {counts}."),
    reverted_registry_values: |count| format!("Reverted {count} registry values."),
    apply: |total_selected| format!("Apply ({total_selected})"),
    found: |installed_count| format!("{installed_count} found"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.result_failed,
                s.result_ok,
                s.apply_the_debloat,
                s.apps_are_removed_for_every,
                s.debloat_progress,
                s.not_installed,
                s.privacy_ad_block_registry_tweaks,
                s.recommended,
                s.rescan,
                s.revert_registry,
                s.summary,
                s.reversible,
                s.reinstallable,
                s.nothing_selected,
                s.apps_group_note,
                s.tweaks_group_note,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
