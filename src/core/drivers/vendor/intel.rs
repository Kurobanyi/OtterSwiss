//! Drivers de Intel desde el catálogo de Driver & Support Assistant, emparejados por Hardware ID.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::core::drivers::hwid::is_version_newer;
use crate::core::drivers::{DriverManager, InstalledDriverDevice, VendorDriverUpdate, fetch_bytes};
use crate::core::error::{CoreError, CoreResult};
use crate::core::security::download::validate_https_host;

const INTEL_DSA_HOSTS: &[&str] = &["dsadata.intel.com"];
pub(super) const INTEL_DOWNLOAD_HOSTS: &[&str] =
    &["downloadmirror.intel.com", "download.intel.com"];
pub(super) const INTEL_SIGNER: &str = "Intel Corporation|Intel(R) Corporation|Intel(R) Software";

impl DriverManager {
    pub fn check_intel_updates(
        devices: &[InstalledDriverDevice],
    ) -> CoreResult<Vec<VendorDriverUpdate>> {
        let mut results = Vec::new();

        let mut local_intel: HashMap<String, (Option<String>, bool)> = HashMap::new();
        for d in devices {
            if let Some((ven, dev, _)) = d.extract_ven_dev()
                && (ven == "8086" || ven == "8087")
            {
                let key = format!("{}_{}", ven, dev);
                let entry = local_intel
                    .entry(key)
                    .or_insert_with(|| (d.driver_version.clone(), d.has_problem));
                if d.has_problem {
                    entry.1 = true;
                }
                if entry.0.is_none() && d.driver_version.is_some() {
                    entry.0 = d.driver_version.clone();
                }
            }
        }

        if local_intel.is_empty() {
            return Ok(results);
        }

        let bytes = fetch_bytes(
            "https://dsadata.intel.com/data/en",
            INTEL_DSA_HOSTS,
            None,
            32 * 1024 * 1024,
        )?;

        let root = serde_json::from_slice::<Value>(&bytes).map_err(|e| CoreError::SystemTool {
            message: "Respuesta inesperada del servicio de drivers".into(),
            source: Some(Box::new(e)),
        })?;

        let mut seen_urls = HashSet::new();
        collect_intel_matches(&root, &local_intel, &mut seen_urls, &mut results);
        Ok(results)
    }
}

fn collect_intel_matches(
    node: &Value,
    local_intel: &HashMap<String, (Option<String>, bool)>,
    seen_urls: &mut HashSet<String>,
    out: &mut Vec<VendorDriverUpdate>,
) {
    match node {
        Value::Array(arr) => {
            for item in arr {
                collect_intel_matches(item, local_intel, seen_urls, out);
            }
        }
        Value::Object(map) => {
            let blob = node.to_string().to_ascii_uppercase();
            let mut matched_local: Option<(String, Option<String>, bool)> = None;

            for (key, (inst_ver, has_prob)) in local_intel {
                let parts: Vec<&str> = key.split('_').collect();
                if parts.len() == 2 {
                    let pci_pat = format!("VEN_{}&DEV_{}", parts[0], parts[1]);
                    let usb_pat = format!("VID_{}&PID_{}", parts[0], parts[1]);
                    if blob.contains(&pci_pat) || blob.contains(&usb_pat) {
                        matched_local = Some((
                            format!("{}:{}", parts[0], parts[1]),
                            inst_ver.clone(),
                            *has_prob,
                        ));
                        break;
                    }
                }
            }

            if let Some((hw_tag, inst_ver, has_prob)) = matched_local
                && let Some(dl_url) = find_intel_download_url(node)
                && seen_urls.insert(dl_url.clone())
            {
                let title = find_json_str(map, &["Title", "Name", "PackageName"])
                    .unwrap_or_else(|| "Intel® Driver Package".into());
                let version = find_json_str(map, &["Version", "DriverVersion"])
                    .unwrap_or_else(|| "Latest".into());
                let rel_date = find_json_str(map, &["ReleaseDate", "Date", "PublishedDate"])
                    .unwrap_or_default();

                let cur_ver_str = inst_ver.clone().unwrap_or_else(|| "Sin driver".into());
                let should_offer =
                    has_prob || inst_ver.is_none() || is_version_newer(&version, &cur_ver_str);

                if should_offer {
                    let is_zip = dl_url.to_ascii_lowercase().ends_with(".zip");
                    out.push(VendorDriverUpdate {
                        id: format!("intel_{}_{}", hw_tag, version),
                        vendor: "Intel".into(),
                        title,
                        current_version: cur_ver_str,
                        latest_version: version,
                        release_date: rel_date,
                        download_url: dl_url,
                        size_label: "Oficial Intel".into(),
                        expected_signer_cn: Some(INTEL_SIGNER.into()),
                        allowed_hosts: INTEL_DOWNLOAD_HOSTS.iter().map(|h| (*h).into()).collect(),
                        is_zip_inf: is_zip,
                        matched_hw_id: hw_tag,
                    });
                    return;
                }
            }

            for val in map.values() {
                if matches!(val, Value::Array(_) | Value::Object(_)) {
                    collect_intel_matches(val, local_intel, seen_urls, out);
                }
            }
        }
        _ => {}
    }
}

fn find_json_str(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for target in keys {
        for (k, v) in map {
            if k.eq_ignore_ascii_case(target)
                && let Some(s) = v.as_str()
            {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

fn find_intel_download_url(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let low = s.to_ascii_lowercase();
            if s.starts_with("https://")
                && validate_https_host(&low, INTEL_DOWNLOAD_HOSTS).is_ok()
                && (low.ends_with(".exe") || low.ends_with(".zip"))
            {
                return Some(s.clone());
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(find_intel_download_url),
        Value::Object(map) => map.values().find_map(find_intel_download_url),
        _ => None,
    }
}
