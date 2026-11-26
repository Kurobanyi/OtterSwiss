//! IN-02: textos de la vista debloater. Un campo por texto; el compilador exige los dos idiomas.

pub struct DebloaterStrings {
    pub apply_the_debloat: &'static str,
    pub apps_are_removed_for_every: &'static str,
    pub debloat_progress: &'static str,
    pub not_installed: &'static str,
    pub privacy_ad_block_registry_tweaks: &'static str,
    pub recommended: &'static str,
    pub remove_preinstalled_apps_and_ads: &'static str,
    pub rescan: &'static str,
    pub revert_registry: &'static str,
    pub summary: &'static str,
    pub windows_debloater: &'static str,
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
    apply_the_debloat: "¿Aplicar el Debloater?",
    apps_are_removed_for_every: "Las apps se quitan para todos los usuarios de este equipo y de la imagen de Windows. Se pueden reinstalar desde Microsoft Store. Los ajustes de registro se pueden revertir.",
    debloat_progress: "Progreso",
    not_installed: "No instalada",
    privacy_ad_block_registry_tweaks: "Ajustes Anti-Publicidad (Reversibles)",
    recommended: "Recomendados",
    remove_preinstalled_apps_and_ads: "Quita apps preinstaladas y anuncios (la Store y Winget quedan protegidas)",
    rescan: "Volver a escanear",
    revert_registry: "Revertir registro",
    summary: "Resumen",
    windows_debloater: "Debloater de Windows",
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
    apply_the_debloat: "Apply the debloat?",
    apps_are_removed_for_every: "Apps are removed for every user of this PC and from the Windows image. You can reinstall them from Microsoft Store. Registry tweaks can be reverted.",
    debloat_progress: "Debloat Progress",
    not_installed: "Not installed",
    privacy_ad_block_registry_tweaks: "Privacy & Ad-Block Registry Tweaks",
    recommended: "Recommended",
    remove_preinstalled_apps_and_ads: "Remove preinstalled apps and ads (Store and Winget stay protected)",
    rescan: "Rescan",
    revert_registry: "Revert Registry",
    summary: "Summary",
    windows_debloater: "Windows Debloater",
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
                s.apply_the_debloat,
                s.apps_are_removed_for_every,
                s.debloat_progress,
                s.not_installed,
                s.privacy_ad_block_registry_tweaks,
                s.recommended,
                s.remove_preinstalled_apps_and_ads,
                s.rescan,
                s.revert_registry,
                s.summary,
                s.windows_debloater,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
