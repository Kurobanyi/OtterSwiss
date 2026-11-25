//! Cliente HTTPS mínimo sobre WinInet: una sola implementación para las descargas verificadas y el actualizador
//! (CL-03). No valida contenido: eso lo hace quien lee (`download.rs` con SHA-256 y Authenticode, el actualizador
//! con su tope de tamaño).

use windows::core::PCWSTR;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetOpenUrlW, InternetOpenW, InternetQueryOptionW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
    INTERNET_OPTION_SEND_TIMEOUT, INTERNET_OPTION_URL,
};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::WinInetHandle;
use crate::core::platform::wide::to_wide_null;

/// Tiempos de espera de una petición.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect_ms: u32,
    pub io_ms: u32,
}

/// Respuesta abierta de una petición GET por HTTPS. La sesión vive mientras viva la respuesta.
pub struct HttpsResponse {
    request: WinInetHandle,
    _session: WinInetHandle,
}

fn trust_err(message: String) -> CoreError {
    CoreError::Trust {
        message,
        source: None,
    }
}

/// Abre `url` (solo `https://`) sin caché. Los redireccionamientos los sigue WinInet: quien llama tiene que
/// comprobar `final_url` contra su lista de hosts permitidos.
pub fn get(url: &str, timeouts: Timeouts) -> CoreResult<HttpsResponse> {
    if !url.starts_with("https://") {
        return Err(trust_err("Solo se permiten conexiones HTTPS".into()));
    }
    let agent_w = to_wide_null(format!("OtterSwiss/{}", env!("CARGO_PKG_VERSION")));
    // SAFETY: `agent_w` termina en NUL y vive durante la llamada; los punteros nulos son opcionales.
    let session = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            INTERNET_OPEN_TYPE_PRECONFIG.0,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })
    .ok_or_else(|| trust_err("No se pudo inicializar WinInet (InternetOpenW)".into()))?;

    for (option, value) in [
        (INTERNET_OPTION_CONNECT_TIMEOUT, timeouts.connect_ms),
        (INTERNET_OPTION_SEND_TIMEOUT, timeouts.io_ms),
        (INTERNET_OPTION_RECEIVE_TIMEOUT, timeouts.io_ms),
    ] {
        // SAFETY: `value` es un u32 vivo durante la llamada y se pasa su tamaño exacto.
        unsafe {
            let _ = InternetSetOptionW(
                Some(session.raw()),
                option,
                Some(&value as *const u32 as *const _),
                4,
            );
        }
    }

    let url_w = to_wide_null(url);
    // SAFETY: `session` es una sesión válida y `url_w` termina en NUL.
    let request = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            session.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            0,
        )
    })
    .ok_or_else(|| trust_err(format!("No se pudo conectar con '{url}'")))?;

    Ok(HttpsResponse {
        request,
        _session: session,
    })
}

impl HttpsResponse {
    fn query_number(&self, query: u32) -> Option<u32> {
        let mut value: u32 = 0;
        let mut len: u32 = std::mem::size_of::<u32>() as u32;
        // SAFETY: `value` y `len` viven durante la llamada; se pide el número como u32.
        unsafe {
            HttpQueryInfoW(
                self.request.raw(),
                query | HTTP_QUERY_FLAG_NUMBER,
                Some(&mut value as *mut u32 as *mut _),
                &mut len,
                None,
            )
        }
        .ok()
        .map(|()| value)
    }

    /// Código de estado HTTP.
    pub fn status(&self) -> CoreResult<u32> {
        self.query_number(HTTP_QUERY_STATUS_CODE)
            .ok_or_else(|| trust_err("No se pudo leer el código de estado HTTP".into()))
    }

    /// Falla si el estado no es 200.
    pub fn require_ok(&self) -> CoreResult<()> {
        match self.status()? {
            200 => Ok(()),
            code => Err(trust_err(format!("Respuesta HTTP no permitida: {code}"))),
        }
    }

    /// `Content-Length`, si el servidor lo envió.
    pub fn content_length(&self) -> Option<u64> {
        self.query_number(HTTP_QUERY_CONTENT_LENGTH).map(u64::from)
    }

    /// URL final después de los redireccionamientos.
    pub fn final_url(&self) -> CoreResult<String> {
        let mut len: u32 = 0;
        // SAFETY: primera llamada solo para conocer el tamaño necesario.
        let _ = unsafe {
            InternetQueryOptionW(
                Some(self.request.raw()),
                INTERNET_OPTION_URL,
                None,
                &mut len,
            )
        };
        if len == 0 {
            return Err(trust_err(
                "No se pudo verificar la URL final tras redirecciones".into(),
            ));
        }
        let mut buf = vec![0u16; (len as usize / 2) + 1];
        // SAFETY: `buf` tiene al menos `len` bytes y vive durante la llamada.
        unsafe {
            InternetQueryOptionW(
                Some(self.request.raw()),
                INTERNET_OPTION_URL,
                Some(buf.as_mut_ptr() as *mut _),
                &mut len,
            )
        }
        .map_err(|e| {
            trust_err(format!(
                "InternetQueryOptionW(INTERNET_OPTION_URL) falló: {e}"
            ))
        })?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..end]))
    }

    /// Lee el siguiente bloque en `buf`. `Ok(0)` al terminar.
    pub fn read_chunk(&self, buf: &mut [u8]) -> CoreResult<usize> {
        let mut read: u32 = 0;
        // SAFETY: `buf` es escribible por `buf.len()` bytes durante la llamada.
        unsafe {
            InternetReadFile(
                self.request.raw(),
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut read,
            )
        }
        .map_err(|e| trust_err(format!("InternetReadFile falló: {e}")))?;
        Ok(read as usize)
    }

    /// Lee todo el cuerpo, fallando si pasa de `max_bytes`.
    pub fn read_to_end_capped(&self, max_bytes: usize) -> CoreResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = self.read_chunk(&mut chunk)?;
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(&chunk[..n]);
            if out.len() > max_bytes {
                return Err(trust_err(format!(
                    "La respuesta excede el tamaño máximo esperado ({max_bytes} bytes)"
                )));
            }
        }
    }
}
