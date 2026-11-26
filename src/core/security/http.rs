//! Cliente HTTPS mínimo sobre WinInet: una sola implementación para las descargas verificadas y el actualizador
//! (CL-03). No valida contenido: eso lo hace quien lee (`download.rs` con SHA-256 y Authenticode, el actualizador
//! con su tope de tamaño). Sale por el proxy de la máquina, nunca por el del usuario (SEC-32).

use windows::Win32::Foundation::{GlobalFree, HGLOBAL};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_PROXY_INFO, WinHttpGetDefaultProxyConfiguration,
};
use windows::Win32::Networking::WinInet::{
    HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE, HttpOpenRequestW,
    HttpQueryInfoW, HttpSendRequestW, INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD,
    INTERNET_FLAG_SECURE, INTERNET_OPEN_TYPE_DIRECT, INTERNET_OPEN_TYPE_PROXY,
    INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT,
    INTERNET_OPTION_URL, INTERNET_SERVICE_HTTP, InternetConnectW, InternetOpenUrlW, InternetOpenW,
    InternetQueryOptionW, InternetReadFile, InternetSetOptionW,
};
use windows::core::{PCWSTR, PWSTR};

use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::handles::WinInetHandle;
use crate::core::platform::wide::to_wide_null;

/// Tiempos de espera de una petición.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect_ms: u32,
    pub io_ms: u32,
}

/// Cómo sale una sesión a internet (SEC-32).
#[derive(Debug, PartialEq, Eq)]
enum SessionProxy {
    Direct,
    Named { proxy: String, bypass: String },
}

/// Respuesta abierta de una petición GET por HTTPS. La sesión vive mientras viva la respuesta.
pub struct HttpsResponse {
    // Orden de los campos = orden de cierre: petición, conexión y sesión.
    request: WinInetHandle,
    _connection: Option<WinInetHandle>,
    _session: WinInetHandle,
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

fn trust_err(message: String) -> CoreError {
    CoreError::trust_msg(message)
}

/// Abre `url` (solo `https://`) sin caché. Los redireccionamientos los sigue WinInet: quien llama tiene que
/// comprobar `final_url` contra su lista de hosts permitidos.
pub fn get(url: &str, timeouts: Timeouts) -> CoreResult<HttpsResponse> {
    get_with_agent(url, &default_agent(), timeouts)
}

/// Igual que [`get`] con otro user agent (algún servicio de fabricante solo responde a su cliente).
pub fn get_with_agent(url: &str, agent: &str, timeouts: Timeouts) -> CoreResult<HttpsResponse> {
    if !url.starts_with("https://") {
        return Err(trust_err("Solo se permiten conexiones HTTPS".into()));
    }
    let session = open_session(agent, timeouts)?;
    let url_w = to_wide_null(url);
    // SAFETY: `session` es una sesión válida y `url_w` termina en NUL.
    let request = WinInetHandle::new(unsafe {
        InternetOpenUrlW(
            session.raw(),
            PCWSTR(url_w.as_ptr()),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            None,
        )
    })
    .ok_or_else(|| trust_err(format!("No se pudo conectar con '{url}'")))?;

    Ok(HttpsResponse {
        request,
        _connection: None,
        _session: session,
    })
}

/// AR-21: `POST` de un formulario (`application/x-www-form-urlencoded`) a `https://{host}{path}`. Mismas reglas que
/// [`get`]: sin caché, con timeouts, y quien llama valida estado y `final_url`.
pub fn post_form(
    host: &str,
    path: &str,
    body: &str,
    timeouts: Timeouts,
) -> CoreResult<HttpsResponse> {
    let session = open_session(&default_agent(), timeouts)?;
    let host_w = to_wide_null(host);
    // SAFETY: `session` es válida y `host_w` termina en NUL; usuario y contraseña nulos son opcionales.
    let connection = WinInetHandle::new(unsafe {
        InternetConnectW(
            session.raw(),
            PCWSTR(host_w.as_ptr()),
            443,
            PCWSTR::null(),
            PCWSTR::null(),
            INTERNET_SERVICE_HTTP,
            0,
            None,
        )
    })
    .ok_or_else(|| trust_err(format!("No se pudo conectar con '{host}'")))?;

    let verb_w = to_wide_null("POST");
    let path_w = to_wide_null(path);
    // SAFETY: `connection` es válida y las cadenas terminan en NUL; versión, referer y tipos aceptados son opcionales.
    let request = WinInetHandle::new(unsafe {
        HttpOpenRequestW(
            connection.raw(),
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            INTERNET_FLAG_SECURE | INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE,
            None,
        )
    })
    .ok_or_else(|| trust_err(format!("No se pudo abrir la petición a '{host}{path}'")))?;

    let headers_w = to_wide_null("Content-Type: application/x-www-form-urlencoded\r\n");
    let body_bytes = body.as_bytes();
    // SAFETY: la petición es válida; las cabeceras (sin el NUL final) y el cuerpo viven durante la llamada.
    unsafe {
        HttpSendRequestW(
            request.raw(),
            Some(&headers_w[..headers_w.len() - 1]),
            Some(body_bytes.as_ptr() as *const _),
            body_bytes.len() as u32,
        )
    }
    .map_err(|e| CoreError::Trust {
        message: format!("No se pudo enviar el formulario a '{host}{path}'"),
        source: Some(Box::new(e)),
    })?;

    Ok(HttpsResponse {
        request,
        _connection: Some(connection),
        _session: session,
    })
}

fn default_agent() -> String {
    format!("OtterSwiss/{}", env!("CARGO_PKG_VERSION"))
}

/// Sesión WinInet con los tres timeouts aplicados.
/// SEC-32: la app corre elevada pero comparte HKCU con el usuario; un programa sin privilegios podría poner ahí un
/// proxy y ver o cambiar lo que se descarga. Se usa el proxy de WinHTTP de la máquina (HKLM, lo fija un administrador
/// con `netsh winhttp set proxy`) o la conexión directa.
fn machine_proxy() -> SessionProxy {
    let mut info = WINHTTP_PROXY_INFO::default();
    // SAFETY: estructura de salida local; si la llamada tiene éxito, WinHTTP reserva las cadenas con GlobalAlloc.
    if unsafe { WinHttpGetDefaultProxyConfiguration(&mut info) }.is_err() {
        return SessionProxy::Direct;
    }
    let proxy = take_global_string(info.lpszProxy);
    let bypass = take_global_string(info.lpszProxyBypass);
    proxy_choice(
        info.dwAccessType == WINHTTP_ACCESS_TYPE_NAMED_PROXY,
        proxy,
        bypass,
    )
}

/// Copia y libera una cadena que reservó WinHTTP con `GlobalAlloc`.
fn take_global_string(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: cadena terminada en NUL reservada por WinHTTP; se copia antes de liberarla.
    let text = unsafe { p.to_string() }.ok();
    // SAFETY: la memoria la reservó WinHTTP con GlobalAlloc y se libera una sola vez.
    let _ = unsafe { GlobalFree(Some(HGLOBAL(p.0.cast()))) };
    text
}

/// Con proxy de máquina, ese proxy; sin él, directo. Sin lista de exclusión explícita se usa `<local>`: con `NULL`
/// WinInet leería la del usuario.
fn proxy_choice(named: bool, proxy: Option<String>, bypass: Option<String>) -> SessionProxy {
    match proxy.filter(|p| named && !p.trim().is_empty()) {
        Some(proxy) => SessionProxy::Named {
            proxy,
            bypass: bypass
                .filter(|b| !b.trim().is_empty())
                .unwrap_or_else(|| "<local>".into()),
        },
        None => SessionProxy::Direct,
    }
}

fn open_session(agent: &str, timeouts: Timeouts) -> CoreResult<WinInetHandle> {
    let agent_w = to_wide_null(agent);
    let (access, proxy_w, bypass_w) = match machine_proxy() {
        SessionProxy::Direct => (INTERNET_OPEN_TYPE_DIRECT, None, None),
        SessionProxy::Named { proxy, bypass } => (
            INTERNET_OPEN_TYPE_PROXY,
            Some(to_wide_null(&proxy)),
            Some(to_wide_null(&bypass)),
        ),
    };
    let ptr = |w: &Option<Vec<u16>>| w.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));
    // SAFETY: las cadenas terminan en NUL y viven durante la llamada; los punteros nulos son opcionales.
    let session = WinInetHandle::new(unsafe {
        InternetOpenW(
            PCWSTR(agent_w.as_ptr()),
            access.0,
            ptr(&proxy_w),
            ptr(&bypass_w),
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
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SEC-32: sin proxy de máquina se sale directo, nunca por el proxy del usuario.
    #[test]
    fn proxy_de_la_maquina_o_directo() {
        assert_eq!(proxy_choice(false, None, None), SessionProxy::Direct);
        assert_eq!(
            proxy_choice(false, Some("10.0.0.1:8080".into()), None),
            SessionProxy::Direct
        );
        assert_eq!(
            proxy_choice(true, Some("  ".into()), None),
            SessionProxy::Direct
        );
        assert_eq!(
            proxy_choice(true, Some("10.0.0.1:8080".into()), None),
            SessionProxy::Named {
                proxy: "10.0.0.1:8080".into(),
                bypass: "<local>".into()
            }
        );
    }

    /// Lee la configuración real de la máquina sin fallar (en un equipo sin `netsh winhttp set proxy`, directo).
    #[test]
    fn lee_el_proxy_de_la_maquina() {
        let _ = machine_proxy();
    }
}
