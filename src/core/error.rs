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
        source: Option<BoxedSource>,
    },

    #[error("Error de servicio '{service}': {message}")]
    Service {
        service: String,
        message: String,
        #[source]
        source: Option<BoxedSource>,
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

    /// AR-06: no hay adaptadores de red físicos activos a los que aplicar DNS.
    #[error("No hay adaptadores de red físicos activos")]
    NoActiveAdapters,

    #[error("Error de validación: {0}")]
    Validation(String),

    #[error("Error interno inesperado (panic capturado): {0}")]
    Panic(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

/// UI-11: qué clase de fallo es, para que la UI ponga el título en el idioma del usuario. El detalle técnico sigue
/// siendo el mensaje de cada variante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    NotElevated,
    Cancelled,
    AlreadyRunning,
    Registry,
    Service,
    Power,
    Dns,
    Trust,
    Backup,
    Archive,
    Uninstaller,
    SystemTool,
    NoActiveAdapters,
    Validation,
    Panic,
}

/// Error de origen con tipo borrado, para el campo `source` de las variantes.
pub type BoxedSource = Box<dyn std::error::Error + Send + Sync>;

impl CoreError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Io(_) => ErrorKind::Io,
            Self::NotElevated(_) => ErrorKind::NotElevated,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::AlreadyRunning => ErrorKind::AlreadyRunning,
            Self::Registry { .. } => ErrorKind::Registry,
            Self::Service { .. } => ErrorKind::Service,
            Self::Power { .. } => ErrorKind::Power,
            Self::Dns { .. } => ErrorKind::Dns,
            Self::Trust { .. } => ErrorKind::Trust,
            Self::Backup { .. } => ErrorKind::Backup,
            Self::Archive { .. } => ErrorKind::Archive,
            Self::Uninstaller { .. } => ErrorKind::Uninstaller,
            Self::SystemTool { .. } => ErrorKind::SystemTool,
            Self::NoActiveAdapters => ErrorKind::NoActiveAdapters,
            Self::Validation(_) => ErrorKind::Validation,
            Self::Panic(_) => ErrorKind::Panic,
        }
    }

    /// Detalle técnico sin el título de la variante (UI-11: el título lo traduce la UI). Vacío si la variante no
    /// tiene más que su título.
    pub fn detail(&self) -> String {
        match self {
            Self::Io(e) => e.to_string(),
            Self::NotElevated(m) | Self::Validation(m) | Self::Panic(m) => m.clone(),
            Self::Registry { path, message, .. } => format!("{path}: {message}"),
            Self::Service {
                service, message, ..
            } => format!("{service}: {message}"),
            Self::Power { message, .. }
            | Self::Dns { message, .. }
            | Self::Trust { message, .. }
            | Self::Backup { message, .. }
            | Self::Archive { message, .. }
            | Self::Uninstaller { message, .. }
            | Self::SystemTool { message, .. } => message.clone(),
            Self::Cancelled | Self::AlreadyRunning | Self::NoActiveAdapters => String::new(),
        }
    }

    /// AR-06: adaptador para `map_err` que conserva el error de origen (HRESULT, `io::ErrorKind`) en `source`.
    /// `prefix` es el contexto ya redactado, por ejemplo `"OpenProcessToken falló: "`.
    pub fn sys<E>(prefix: &'static str) -> impl FnOnce(E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        move |e| Self::SystemTool {
            message: format!("{prefix}{e}"),
            source: Some(Box::new(e)),
        }
    }

    /// Igual que `sys`, para errores de descarga y verificación.
    pub fn trust<E>(prefix: &'static str) -> impl FnOnce(E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        move |e| Self::Trust {
            message: format!("{prefix}{e}"),
            source: Some(Box::new(e)),
        }
    }

    /// Una API devolvió un handle nulo o inválido sin fijar un error propio: no se lee `GetLastError`, que
    /// sería de otra llamada.
    pub fn invalid_handle(context: impl Into<String>) -> Self {
        Self::SystemTool {
            message: format!("{}: handle inválido", context.into()),
            source: None,
        }
    }

    pub fn from_win32(context: impl Into<String>) -> Self {
        let err = io::Error::last_os_error();
        Self::SystemTool {
            message: format!("{}: {}", context.into(), err),
            source: Some(Box::new(err)),
        }
    }
}
