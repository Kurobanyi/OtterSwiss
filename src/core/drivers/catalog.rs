//! Microsoft Update Catalog: búsqueda por Hardware ID (raspando el HTML) e instalación de un CAB firmado por Microsoft
//! (SEC-33).

use super::install::{ScratchDir, download_to_file};
use super::{
    CatalogDriverMatch, DATA_TIMEOUTS, DriverInstallReport, DriverManager, PNPUTIL_TIMEOUT,
    ensure_driver_updates_allowed, fetch_bytes, url_encode,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::platform::process::{run_captured, system32_dir};
use crate::core::security::acl::create_admin_only_dir;
use crate::core::security::download::{
    DownloadPolicy, reopen_verified_for_exec, validate_https_host,
};
use crate::core::security::http;

const CATALOG_HOSTS: &[&str] = &["www.catalog.update.microsoft.com"];
/// Hosts de los que se aceptan CAB del Microsoft Update Catalog (SEC-33).
const CATALOG_CAB_HOSTS: &[&str] = &["download.windowsupdate.com", "*.download.windowsupdate.com"];
const MICROSOFT_SIGNER: &str =
    "Microsoft Corporation|Microsoft Windows|Microsoft Windows Hardware Compatibility Publisher";
const MAX_CATALOG_CAB_BYTES: u64 = 350 * 1024 * 1024;

impl DriverManager {
    pub fn search_microsoft_catalog(hw_id_query: &str) -> CoreResult<Vec<CatalogDriverMatch>> {
        let url = format!(
            "https://www.catalog.update.microsoft.com/Search.aspx?q={}",
            url_encode(hw_id_query)
        );
        let bytes = fetch_bytes(&url, CATALOG_HOSTS, None, 4 * 1024 * 1024)?;
        let html = String::from_utf8_lossy(&bytes);

        let mut matches = Vec::new();
        let mut cursor = 0usize;

        // El Catálogo marca cada resultado con `<a id='<guid>_link' …>` (comillas simples o dobles).
        while let Some(pos) = html[cursor..].find("_link") {
            let abs_pos = cursor + pos;
            cursor = abs_pos + 5;
            if !matches!(html.as_bytes().get(abs_pos + 5), Some(b'\'' | b'"')) {
                continue;
            }

            let mut back = abs_pos.saturating_sub(64);
            while !html.is_char_boundary(back) {
                back += 1;
            }
            let search_back = &html[back..abs_pos];
            let Some(id_start) = search_back.rfind("id=") else {
                continue;
            };
            let guid = search_back[id_start + 3..]
                .trim_start_matches(['\'', '"'])
                .trim();
            if !is_guid(guid) {
                continue;
            }

            let row_start = html[..abs_pos].rfind("<tr").unwrap_or(abs_pos);
            let Some(row_end_rel) = html[abs_pos..].find("</tr>") else {
                continue;
            };
            let row_html = &html[row_start..abs_pos + row_end_rel];

            let mut cells = Vec::new();
            let mut td_cur = 0usize;
            while let Some(td_open) = row_html[td_cur..].find("<td") {
                let td_abs = td_cur + td_open;
                let Some(td_close) = row_html[td_abs..].find("</td>") else {
                    break;
                };
                let cell_text = strip_html_tags(&row_html[td_abs..td_abs + td_close]);
                cells.push(cell_text);
                td_cur = td_abs + td_close + 5;
            }

            if cells.len() >= 7 {
                let title = cells[1].clone();
                if title.is_empty() {
                    continue;
                }
                let last_updated = cells[4].clone();
                let version = cells[5].clone();
                let size_str = cells[6].clone();

                matches.push(CatalogDriverMatch {
                    update_id: guid.to_string(),
                    title,
                    last_updated,
                    version,
                    size_str,
                    query_hw_id: hw_id_query.to_string(),
                });
            }

            if matches.len() >= 8 {
                break;
            }
        }

        Ok(matches)
    }

    /// SEC-33: instala un driver del Microsoft Update Catalog. El `update_id` tiene que ser un GUID; el enlace del CAB
    /// se acepta solo si su host es de Windows Update y termina en `.cab`; el CAB se baja con el cliente HTTPS común
    /// (host final validado), tiene que estar firmado por Microsoft antes de expandirse y todo vive en una carpeta
    /// temporal que se borra al terminar.
    pub(super) fn install_catalog_driver_inner(update_id: &str) -> CoreResult<DriverInstallReport> {
        ensure_driver_updates_allowed()?;
        if !is_guid(update_id) {
            return Err(CoreError::Validation(
                "UpdateID de catálogo inválido".into(),
            ));
        }

        let download_url = catalog_cab_url(update_id)?;

        let scratch = ScratchDir::new("Catalog", update_id)?;
        let cab_path = scratch.dir.join("driver.cab");
        download_to_file(
            &download_url,
            CATALOG_CAB_HOSTS,
            &cab_path,
            MAX_CATALOG_CAB_BYTES,
        )?;

        let policy = DownloadPolicy {
            url: &download_url,
            allowed_final_hosts: CATALOG_CAB_HOSTS,
            expected_sha256_hex: None,
            expected_cert_thumbprint_sha1: None,
            expected_signer_cn: Some(MICROSOFT_SIGNER),
            max_bytes: MAX_CATALOG_CAB_BYTES,
        };
        // El handle de solo lectura impide cambiar el CAB entre la verificación y `expand`.
        let _cab_lock = reopen_verified_for_exec(&cab_path, &policy)?;

        let extract_dir = scratch.dir.join("extracted");
        create_admin_only_dir(&extract_dir)?;
        let system32 = system32_dir()?;
        let exp_out = run_captured(
            &system32.join("expand.exe"),
            &[
                cab_path.as_os_str(),
                std::ffi::OsStr::new("-F:*"),
                extract_dir.as_os_str(),
            ],
            PNPUTIL_TIMEOUT,
        )?;
        if !exp_out.success {
            return Err(CoreError::system_tool_msg(format!(
                "Error extrayendo .CAB: {}",
                exp_out.stderr.trim()
            )));
        }

        let inf_pattern = extract_dir.join("*.inf");
        let pnp_out = run_captured(
            &system32.join("pnputil.exe"),
            &[
                std::ffi::OsStr::new("/add-driver"),
                inf_pattern.as_os_str(),
                std::ffi::OsStr::new("/install"),
            ],
            PNPUTIL_TIMEOUT,
        )?;
        let code = pnp_out.code.unwrap_or(-1);
        if pnp_out.success || code == 3010 {
            return Ok(DriverInstallReport::single(code == 3010));
        }
        Err(CoreError::system_tool_msg(format!(
            "pnputil devolvió código {}: {}",
            code,
            pnp_out.stdout.trim()
        )))
    }
}

/// Enlace del CAB de una actualización del Catálogo, pedido a `DownloadDialog.aspx` (`update_id` ya validado como GUID).
fn catalog_cab_url(update_id: &str) -> CoreResult<String> {
    let json_payload = format!(
        r#"[{{"size":0,"languages":"","uidInfo":"{}","updateID":"{}"}}]"#,
        update_id, update_id
    );
    let form_body = format!("updateIDs={}", url_encode(&json_payload));
    let resp = http::post_form(
        CATALOG_HOSTS[0],
        "/DownloadDialog.aspx",
        &form_body,
        DATA_TIMEOUTS,
    )?;
    resp.require_ok()?;
    validate_https_host(&resp.final_url()?, CATALOG_HOSTS)?;
    let dialog_html =
        String::from_utf8_lossy(&resp.read_to_end_capped(4 * 1024 * 1024)?).into_owned();
    find_catalog_cab_url(&dialog_html).ok_or_else(|| {
        CoreError::system_tool_msg(
            "No se encontró enlace de descarga .CAB válido en Microsoft Catalog.",
        )
    })
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `true` si `s` es un GUID `8-4-4-4-12` en hexadecimal (forma en que el Catálogo nombra sus actualizaciones).
fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, len)| p.len() == len && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Primer enlace a un `.cab` de Windows Update en el HTML de `DownloadDialog.aspx`, pasado a HTTPS. Se descarta lo
/// que no sea `.cab` (antes se bajaban `.msu` y `.exe` para rechazarlos después, ERR-22) y lo que no tenga un host de
/// [`CATALOG_CAB_HOSTS`] exacto.
fn find_catalog_cab_url(html: &str) -> Option<String> {
    for marker in ["https://", "http://"] {
        let mut cur = 0usize;
        while let Some(p) = html[cur..].find(marker) {
            let abs = cur + p;
            let tail = &html[abs..];
            let end = tail
                .find(['\'', '"', '<', ' '])
                .unwrap_or(tail.len().min(512));
            let candidate = &tail[..end];
            let secure = match candidate.strip_prefix("http://") {
                Some(rest) => format!("https://{rest}"),
                None => candidate.to_string(),
            };
            let path = secure.split(['?', '#']).next().unwrap_or_default();
            if path.to_ascii_lowercase().ends_with(".cab")
                && validate_https_host(&secure, CATALOG_CAB_HOSTS).is_ok()
            {
                return Some(secure);
            }
            cur = abs + marker.len();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CL-22: el texto de una celda del Catálogo, sin etiquetas, entidades comunes decodificadas y espacios colapsados.
    #[test]
    fn celda_html_a_texto() {
        assert_eq!(
            strip_html_tags(
                "<td class='x'>
  Intel&nbsp;-&nbsp;Display <b>&amp;</b> Audio  </td>"
            ),
            "Intel - Display & Audio"
        );
        assert_eq!(
            strip_html_tags("<a href=\"#\">&lt;x&gt; &quot;y&quot;</a>"),
            "<x> \"y\""
        );
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn update_id_solo_guid() {
        assert!(is_guid("0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(!is_guid("C:xxx-0000-0000-0000-000000000000"));
        assert!(!is_guid("0a1b2c3d-4e5f-6789-abcd-ef012345678\""));
        assert!(!is_guid(""));
    }

    #[test]
    fn enlace_cab_del_catalogo() {
        let ok = r#"x 'http://catalog.s.download.windowsupdate.com/d/msdownload/abc_123.cab' y"#;
        assert_eq!(
            find_catalog_cab_url(ok).as_deref(),
            Some("https://catalog.s.download.windowsupdate.com/d/msdownload/abc_123.cab")
        );
        // Subcadena del host en otro dominio, traversal en el nombre y paquetes que no son CAB.
        assert_eq!(
            find_catalog_cab_url("'https://evil.example/download.windowsupdate.com/a.cab'"),
            None
        );
        assert_eq!(
            find_catalog_cab_url("'https://download.windowsupdate.com.evil.example/a.cab'"),
            None
        );
        assert_eq!(
            find_catalog_cab_url("'https://download.windowsupdate.com/a.msu'"),
            None
        );
    }

    /// Red real: busca en el Catálogo y resuelve el enlace del CAB del primer resultado (no lo descarga).
    #[test]
    #[ignore = "usa la red"]
    fn catalogo_en_vivo() {
        let found =
            DriverManager::search_microsoft_catalog(r"PCI\VEN_8086&DEV_A0F0").expect("búsqueda");
        let first = found.first().expect("algún resultado");
        let url = catalog_cab_url(&first.update_id).expect("enlace del CAB");
        assert!(url.to_ascii_lowercase().ends_with(".cab"));
    }
}
