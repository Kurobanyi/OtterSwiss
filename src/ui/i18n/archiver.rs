//! IN-02: textos de la vista archiver. Un campo por texto; el compilador exige los dos idiomas.

pub struct ArchiverStrings {
    pub operation: &'static str,
    pub extract: &'static str,
    pub compress: &'static str,
    pub extract_now: &'static str,
    pub compress_now: &'static str,
    pub extracting: &'static str,
    pub compressing: &'static str,
    pub archive_path: &'static str,
    pub archive_path_hint: &'static str,
    pub extract_destination: &'static str,
    pub extract_destination_hint: &'static str,
    pub source_path: &'static str,
    pub source_path_hint: &'static str,
    pub output_archive: &'static str,
    pub output_archive_hint: &'static str,
    pub format: &'static str,
    pub supported_formats: &'static str,
    pub source_path_does_not_exist: &'static str,
    pub done_processed: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
}

pub static ES: ArchiverStrings = ArchiverStrings {
    operation: "Operación",
    extract: "Descomprimir",
    compress: "Comprimir",
    extract_now: "Descomprimir",
    compress_now: "Comprimir",
    extracting: "Descomprimiendo archivo…",
    compressing: "Comprimiendo…",
    archive_path: "Archivo comprimido",
    archive_path_hint: "Ruta de un archivo .zip, .7z, .tar o .tar.gz.",
    extract_destination: "Carpeta de destino",
    extract_destination_hint: "Si la carpeta ya existe, se crea una nueva, como «nombre (2)».",
    source_path: "Archivo o carpeta de origen",
    source_path_hint: "Ruta de lo que quieres comprimir.",
    output_archive: "Archivo de destino",
    output_archive_hint: "Ruta completa del archivo que se crea, con su extensión.",
    format: "Formato",
    supported_formats: "Al descomprimir: ZIP, 7Z, TAR y TAR.GZ. Al comprimir: ZIP, 7Z y TAR.GZ.",
    source_path_does_not_exist: "La ruta de origen no existe.",
    done_processed: |a0, dest_display| format!("Listo: {a0} procesados. Destino: {dest_display}"),
};

pub static EN: ArchiverStrings = ArchiverStrings {
    operation: "Operation",
    extract: "Extract",
    compress: "Compress",
    extract_now: "Extract",
    compress_now: "Compress",
    extracting: "Extracting archive…",
    compressing: "Compressing…",
    archive_path: "Archive",
    archive_path_hint: "Path to a .zip, .7z, .tar or .tar.gz file.",
    extract_destination: "Destination folder",
    extract_destination_hint: "If the folder already exists, a new one is created, like “name (2)”.",
    source_path: "Source file or folder",
    source_path_hint: "Path of what you want to compress.",
    output_archive: "Output archive",
    output_archive_hint: "Full path of the archive to create, including its extension.",
    format: "Format",
    supported_formats: "When extracting: ZIP, 7Z, TAR and TAR.GZ. When compressing: ZIP, 7Z and TAR.GZ.",
    source_path_does_not_exist: "Source path does not exist.",
    done_processed: |a0, dest_display| format!("Done: {a0} processed. Destination: {dest_display}"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.operation,
                s.extract,
                s.compress,
                s.extract_now,
                s.compress_now,
                s.extracting,
                s.compressing,
                s.archive_path,
                s.archive_path_hint,
                s.extract_destination,
                s.extract_destination_hint,
                s.source_path,
                s.source_path_hint,
                s.output_archive,
                s.output_archive_hint,
                s.format,
                s.supported_formats,
                s.source_path_does_not_exist,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
