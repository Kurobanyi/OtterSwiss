use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Error de entrada/salida: {0}")]
    Io(#[from] io::Error),

    #[error("Se requieren privilegios de administrador para esta operación: {0}")]
    NotElevated(String),

    #[error("Operación cancelada por el usuario o por señal de cancelación")]
    Cancelled,

    #[error("Otra instancia de OtterSwiss ya está ejecutando cambios en el sistema")]
    AlreadyRunning,

    #[error("Error de registro en '{path}': {message}")]
    Registry {
        path: String,
        message: String,
        #[source]
        source: Option<io::Error>,
    },

    #[error("Error de servicio '{service}': {message}")]
    Service {
        service: String,
        message: String,
        #[source]
        source: Option<io::Error>,
    },

    #[error("Error de administración de energía: {message}")]
    Power {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de configuración DNS: {message}")]
    Dns {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de confianza o descarga segura: {message}")]
    Trust {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de respaldo y reversión: {message}")]
    Backup {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de archivo comprimido: {message}")]
    Archive {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error del desinstalador: {message}")]
    Uninstaller {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error en herramienta del sistema: {message}")]
    SystemTool {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de sensores de hardware: {message}")]
    Sensor {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Error de validación: {0}")]
    Validation(String),

    #[error("Error interno inesperado (panic capturado): {0}")]
    Panic(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

impl CoreError {
    pub fn from_win32(context: impl Into<String>) -> Self {
        let err = io::Error::last_os_error();
        Self::SystemTool {
            message: format!("{}: {}", context.into(), err),
            source: Some(Box::new(err)),
        }
    }
}
