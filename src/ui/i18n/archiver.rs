//! IN-02: textos de la vista archiver. Un campo por texto; el compilador exige los dos idiomas.

pub struct ArchiverStrings {
    pub archive_path_zip_7z_tar: &'static str,
    pub compress: &'static str,
    pub compress_now: &'static str,
    pub extract: &'static str,
    pub extract_now: &'static str,
    pub format: &'static str,
    pub n_100_open_source_rust_compression: &'static str,
    pub output_archive_file: &'static str,
    pub output_folder: &'static str,
    pub secure_archiver_zip_7z_tar: &'static str,
    pub source_file_or_folder: &'static str,
    pub source_path_does_not_exist: &'static str,
    pub done_processed: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
}

pub static ES: ArchiverStrings = ArchiverStrings {
    archive_path_zip_7z_tar: "Archivo comprimido (.zip, .7z, .tar, .tar.gz):",
    compress: "Comprimir",
    compress_now: "Comprimir",
    extract: "Descomprimir",
    extract_now: "Descomprimir",
    format: "Formato:",
    n_100_open_source_rust_compression: "Compresión y extracción libre 100% Rust",
    output_archive_file: "Archivo destino:",
    output_folder: "Carpeta destino:",
    secure_archiver_zip_7z_tar: "Archivador Seguro (ZIP / 7Z / TAR.GZ)",
    source_file_or_folder: "Archivo o carpeta origen:",
    source_path_does_not_exist: "La ruta de origen no existe.",
    done_processed: |a0, dest_display| format!("Completado ({a0} procesados) → {dest_display}"),
};

pub static EN: ArchiverStrings = ArchiverStrings {
    archive_path_zip_7z_tar: "Archive path (.zip, .7z, .tar, .tar.gz):",
    compress: "Compress",
    compress_now: "Compress Now",
    extract: "Extract",
    extract_now: "Extract Now",
    format: "Format:",
    n_100_open_source_rust_compression: "100% open-source Rust compression & extraction",
    output_archive_file: "Output archive file:",
    output_folder: "Output folder:",
    secure_archiver_zip_7z_tar: "Secure Archiver (ZIP / 7Z / TAR.GZ)",
    source_file_or_folder: "Source file or folder:",
    source_path_does_not_exist: "Source path does not exist.",
    done_processed: |a0, dest_display| format!("Done ({a0} processed) → {dest_display}"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.archive_path_zip_7z_tar,
                s.compress,
                s.compress_now,
                s.extract,
                s.extract_now,
                s.format,
                s.n_100_open_source_rust_compression,
                s.output_archive_file,
                s.output_folder,
                s.secure_archiver_zip_7z_tar,
                s.source_file_or_folder,
                s.source_path_does_not_exist,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
