use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{
    FOLDERID_InternetCache, FOLDERID_LocalAppData, FOLDERID_Windows, SHEmptyRecycleBinW,
    SHGetKnownFolderPath, SHQueryRecycleBinW, KF_FLAG_DEFAULT, SHERB_NOCONFIRMATION,
    SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::safe_fs::{
    canonical_dos_path, delete_verified_empty_dir_under, delete_verified_file_under,
    is_safe_non_root_dir,
};
use crate::core::platform::services::is_service_stopped;
use crate::core::platform::wide::wide_ptr_to_string_and_free;

#[derive(Debug, Clone, Default)]
pub struct CategoryCleanResult {
    pub category: String,
    pub bytes_freed: u64,
    pub files_removed: usize,
    pub skipped_recent_or_locked: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CleanProgress {
    pub current_step: usize,
    pub total_steps: usize,
    pub current_category: String,
    pub percent: u8,
    pub bytes_freed_so_far: u64,
    pub files_removed_so_far: usize,
}

pub struct SystemCleaner;

impl SystemCleaner {
    pub fn get_known_folder(id: &windows::core::GUID) -> CoreResult<PathBuf> {
        unsafe {
            let ptr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| {
                CoreError::SystemTool {
                    message: format!("Error obteniendo carpeta conocida de Windows: {}", e),
                    source: None,
                }
            })?;
            let s = wide_ptr_to_string_and_free(ptr);
            if s.is_empty() {
                return Err(CoreError::SystemTool {
                    message: "Ruta conocida vacía".into(),
                    source: None,
                });
            }
            Ok(PathBuf::from(s))
        }
    }

    pub fn clean_temp_directory(
        category_name: &str,
        dir: &Path,
        min_age: Duration,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<CleanProgress>>>,
    ) -> CategoryCleanResult {
        Self::clean_temp_directory_with_depth(category_name, dir, min_age, 3, cancel, progress)
    }

    fn clean_temp_directory_with_depth(
        category_name: &str,
        dir: &Path,
        min_age: Duration,
        min_components: usize,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<CleanProgress>>>,
    ) -> CategoryCleanResult {
        let mut result = CategoryCleanResult {
            category: category_name.to_string(),
            bytes_freed: 0,
            files_removed: 0,
            skipped_recent_or_locked: 0,
            error: None,
        };

        if !dir.exists() {
            return result;
        }

        if !is_safe_non_root_dir(dir, min_components) {
            result.error = Some(format!(
                "Directorio '{}' rechazado: no cumple comprobación no-raíz segura",
                dir.display()
            ));
            return result;
        }

        let root_canon = match canonical_dos_path(dir) {
            Ok(c) => c,
            Err(e) => {
                result.error = Some(format!("No se pudo canonizar '{}': {}", dir.display(), e));
                return result;
            }
        };

        let now = SystemTime::now();
        let mut stack: Vec<(PathBuf, usize)> = vec![(dir.to_path_buf(), 0)];

        while let Some((current_dir, depth)) = stack.pop() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if depth > 16 {
                continue;
            }

            let entries = match std::fs::read_dir(&current_dir) {
                Ok(e) => e,
                Err(_) => {
                    result.skipped_recent_or_locked += 1;
                    continue;
                }
            };

            let mut subdirs = Vec::new();

            for entry in entries.flatten() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let entry_path = entry.path();
                let file_name = entry.file_name();
                let file_name_lossy = file_name.to_string_lossy();

                if file_name_lossy.eq_ignore_ascii_case("Content.Outlook") {
                    result.skipped_recent_or_locked += 1;
                    continue;
                }

                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => {
                        result.skipped_recent_or_locked += 1;
                        continue;
                    }
                };

                if meta.is_dir() {
                    subdirs.push(entry_path);
                } else {
                    let file_len = meta.len();
                    if !min_age.is_zero() {
                        let modified = meta.modified().unwrap_or(now);
                        let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
                        if age < min_age {
                            result.skipped_recent_or_locked += 1;
                            continue;
                        }
                    }

                    match delete_verified_file_under(&entry_path, Some(&root_canon)) {
                        Ok(()) => {
                            result.bytes_freed += file_len;
                            result.files_removed += 1;
                            if let Some(p_arc) = progress {
                                if let Ok(mut p) = p_arc.lock() {
                                    p.bytes_freed_so_far += file_len;
                                    p.files_removed_so_far += 1;
                                }
                            }
                        }
                        Err(_) => {
                            result.skipped_recent_or_locked += 1;
                        }
                    }
                }
            }

            for subdir in subdirs {
                if depth < 16 {
                    stack.push((subdir.clone(), depth + 1));
                }
                let _ = delete_verified_empty_dir_under(&subdir, Some(&root_canon));
            }
        }

        result
    }

    pub fn clean_recycle_bin() -> CategoryCleanResult {
        let mut result = CategoryCleanResult {
            category: "Papelera de reciclaje".into(),
            bytes_freed: 0,
            files_removed: 0,
            skipped_recent_or_locked: 0,
            error: None,
        };

        let mut rb_info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };

        let query = |info: &mut SHQUERYRBINFO| -> Option<(u64, usize)> {
            unsafe { SHQueryRecycleBinW(PCWSTR::null(), info) }
                .ok()
                .map(|()| (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as usize))
        };

        let Some((bytes_before, items_before)) = query(&mut rb_info) else {
            result.error = Some("No se pudo consultar la papelera".into());
            return result;
        };
        // Si la papelera ya está vacía, SHEmptyRecycleBinW devuelve E_UNEXPECTED (0x8000FFFF)
        if items_before == 0 && bytes_before == 0 {
            return result;
        }

        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        let hr = unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), flags) };
        if hr.is_err() {
            result.error = Some(format!("Error vaciando papelera: {:?}", hr));
        }

        // Lo liberado es lo que de verdad desapareció, no lo que había antes de intentar.
        let (bytes_after, items_after) =
            query(&mut rb_info).unwrap_or((bytes_before, items_before));
        result.bytes_freed = bytes_before.saturating_sub(bytes_after);
        result.files_removed = items_before.saturating_sub(items_after);
        result
    }

    pub fn run_full_cleanup(
        min_age_hours: u64,
        clean_recycle: bool,
        cancel: &AtomicBool,
    ) -> Vec<CategoryCleanResult> {
        Self::run_full_cleanup_with_progress(min_age_hours, clean_recycle, cancel, None)
    }

    pub fn run_full_cleanup_with_progress(
        min_age_hours: u64,
        clean_recycle: bool,
        cancel: &AtomicBool,
        progress: Option<&Arc<Mutex<CleanProgress>>>,
    ) -> Vec<CategoryCleanResult> {
        let mut results = Vec::new();
        let mut skipped_notes: Vec<CategoryCleanResult> = Vec::new();
        let min_age = Duration::from_secs(min_age_hours.saturating_mul(3600));

        // (nombre, ruta, profundidad mínima segura)
        let mut targets: Vec<(&'static str, PathBuf, usize)> = Vec::new();
        if let Ok(local_app_data) = Self::get_known_folder(&FOLDERID_LocalAppData) {
            targets.push(("Temporales de Usuario", local_app_data.join("Temp"), 3));
        }
        if let Ok(win_dir) = Self::get_known_folder(&FOLDERID_Windows) {
            targets.push(("Temporales de Windows", win_dir.join("Temp"), 2));
            // Prefetch no se toca: Windows lo mantiene solo y vaciarlo hace más lentos los próximos arranques.
            // La caché de Windows Update solo se limpia con el servicio y BITS detenidos, para no romper una
            // actualización en curso.
            let wu_idle = is_service_stopped("wuauserv").unwrap_or(false)
                && is_service_stopped("BITS").unwrap_or(false);
            if wu_idle {
                targets.push((
                    "Descargas de Windows Update",
                    win_dir.join("SoftwareDistribution").join("Download"),
                    3,
                ));
            } else {
                skipped_notes.push(CategoryCleanResult {
                    category: "Descargas de Windows Update".into(),
                    bytes_freed: 0,
                    files_removed: 0,
                    skipped_recent_or_locked: 0,
                    error: Some(
                        "Omitido: Windows Update está activo; se limpiará cuando esté detenido"
                            .into(),
                    ),
                });
            }
        }
        if let Ok(inet_cache) = Self::get_known_folder(&FOLDERID_InternetCache) {
            targets.push(("Caché de Internet", inet_cache, 3));
        }

        let total_steps = targets.len() + usize::from(clean_recycle);
        let update_step = |step: usize, label: &str| {
            if let Some(p_arc) = progress {
                if let Ok(mut p) = p_arc.lock() {
                    p.current_step = step;
                    p.total_steps = total_steps.max(1);
                    p.current_category = label.to_string();
                    p.percent = ((step * 100) / total_steps.max(1)).min(99) as u8;
                }
            }
        };

        for (idx, (cat_name, path, min_comp)) in targets.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return results;
            }
            update_step(idx, cat_name);
            results.push(Self::clean_temp_directory_with_depth(
                cat_name, &path, min_age, min_comp, cancel, progress,
            ));
        }

        if clean_recycle && !cancel.load(Ordering::Relaxed) {
            update_step(total_steps.saturating_sub(1), "Papelera de reciclaje");
            let rb_res = Self::clean_recycle_bin();
            if let Some(p_arc) = progress {
                if let Ok(mut p) = p_arc.lock() {
                    p.bytes_freed_so_far += rb_res.bytes_freed;
                    p.files_removed_so_far += rb_res.files_removed;
                }
            }
            results.push(rb_res);
        }

        results.extend(skipped_notes);

        if let Some(p_arc) = progress {
            if let Ok(mut p) = p_arc.lock() {
                p.current_step = total_steps;
                p.total_steps = total_steps.max(1);
                p.current_category = "Limpieza completada".into();
                p.percent = 100;
            }
        }

        results
    }
}
