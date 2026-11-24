use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use windows::core::GUID;
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, HANDLE, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, SetInterfaceDnsSettings,
    DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1, DNS_SETTING_IPV6,
    DNS_SETTING_NAMESERVER, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
#[cfg(target_pointer_width = "32")]
use windows::Win32::NetworkManagement::IpHelper::ICMP_ECHO_REPLY as EchoReplyStruct;
#[cfg(target_pointer_width = "64")]
use windows::Win32::NetworkManagement::IpHelper::ICMP_ECHO_REPLY32 as EchoReplyStruct;
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::AF_UNSPEC;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::core::elevation::ElevationManager;
use crate::core::error::{CoreError, CoreResult};
use crate::core::power::parse_guid_strict;
use crate::core::safe_fs::{atomic_write_new, delete_verified_file, read_bounded_bytes};
use crate::core::win_common::{to_wide_null, NamedMutex, SafeLibrary};

const DNS_BACKUP_MUTEX: &str = r"Global\OtterSwiss_DnsBackup_Lock";
const MAX_DNS_BACKUP_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub ipv4_primary: &'static str,
    pub ipv4_secondary: &'static str,
    pub ipv6_primary: &'static str,
    pub ipv6_secondary: &'static str,
    pub privacy_notice: &'static str,
}

pub const DNS_PRESETS: &[DnsPreset] = &[
    DnsPreset {
        id: "cloudflare",
        name: "Cloudflare (1.1.1.1)",
        ipv4_primary: "1.1.1.1",
        ipv4_secondary: "1.0.0.1",
        ipv6_primary: "2606:4700:4700::1111",
        ipv6_secondary: "2606:4700:4700::1001",
        privacy_notice: "Las consultas DNS se envían a Cloudflare (DNS plano UDP/TCP 53).",
    },
    DnsPreset {
        id: "quad9",
        name: "Quad9 (9.9.9.9)",
        ipv4_primary: "9.9.9.9",
        ipv4_secondary: "149.112.112.112",
        ipv6_primary: "2620:fe::fe",
        ipv6_secondary: "2620:fe::9",
        privacy_notice:
            "Las consultas DNS se envían a Quad9 (filtrado de dominios maliciosos en resolver).",
    },
    DnsPreset {
        id: "google",
        name: "Google Public DNS (8.8.8.8)",
        ipv4_primary: "8.8.8.8",
        ipv4_secondary: "8.8.4.4",
        ipv6_primary: "2001:4860:4860::8888",
        ipv6_secondary: "2001:4860:4860::8844",
        privacy_notice: "Las consultas DNS se envían a Google Public DNS.",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsBenchmarkResult {
    pub preset_id: &'static str,
    pub avg_latency_ms: Option<u32>,
    pub method: &'static str,
}

struct IcmpHandle(HANDLE);
impl Drop for IcmpHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0 .0.is_null() {
            unsafe {
                let _ = IcmpCloseHandle(self.0);
            }
        }
    }
}

fn ping_ipv4_once(ip: Ipv4Addr, timeout_ms: u32) -> Option<(u32, &'static str)> {
    if let Ok(handle) = unsafe { IcmpCreateFile() } {
        let guard = IcmpHandle(handle);
        let dest_addr = u32::from_ne_bytes(ip.octets());
        let payload = [0x4F_u8; 32];
        let reply_size = std::mem::size_of::<EchoReplyStruct>() + payload.len() + 16;
        let mut reply_buf = vec![0u8; reply_size];

        let count = unsafe {
            IcmpSendEcho(
                guard.0,
                dest_addr,
                payload.as_ptr() as *const _,
                payload.len() as u16,
                None,
                reply_buf.as_mut_ptr() as *mut _,
                reply_size as u32,
                timeout_ms,
            )
        };

        if count > 0 {
            let reply = unsafe { &*(reply_buf.as_ptr() as *const EchoReplyStruct) };
            if reply.Status == 0 {
                return Some((reply.RoundTripTime, "ICMP"));
            }
        }
    }

    // Fallback TCP/53 si el router o firewall bloquea ICMP
    let addr = SocketAddr::new(IpAddr::V4(ip), 53);
    let started = Instant::now();
    if TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms as u64)).is_ok() {
        return Some((started.elapsed().as_millis() as u32, "TCP:53"));
    }

    None
}

/// Hace ping en paralelo a todos los presets DNS (3 muestras cada uno) y devuelve su latencia promedio.
pub fn benchmark_dns_presets() -> Vec<DnsBenchmarkResult> {
    let handles: Vec<_> = DNS_PRESETS
        .iter()
        .map(|preset| {
            let id = preset.id;
            let ip_str = preset.ipv4_primary;
            thread::spawn(move || {
                let Ok(ipv4) = ip_str.parse::<Ipv4Addr>() else {
                    return DnsBenchmarkResult {
                        preset_id: id,
                        avg_latency_ms: None,
                        method: "N/A",
                    };
                };

                let mut samples = Vec::new();
                let mut used_method = "N/A";
                for _ in 0..3 {
                    if let Some((ms, method)) = ping_ipv4_once(ipv4, 800) {
                        samples.push(ms);
                        used_method = method;
                    }
                }

                let avg = if samples.is_empty() {
                    None
                } else {
                    let sum: u32 = samples.iter().sum();
                    Some((sum / samples.len() as u32).max(1))
                };

                DnsBenchmarkResult {
                    preset_id: id,
                    avg_latency_ms: avg,
                    method: used_method,
                }
            })
        })
        .collect();

    handles
        .into_iter()
        .filter_map(|h| h.join().ok())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapterInfo {
    pub interface_guid: String,
    pub friendly_name: String,
    pub description: String,
    pub if_type: u32,
    pub current_ipv4_dns: String,
    pub current_ipv6_dns: String,
    pub is_virtual_or_vpn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsAdapterSnapshot {
    pub interface_guid: String,
    pub friendly_name: String,
    pub previous_ipv4_nameserver: String,
    pub previous_ipv6_nameserver: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsBackupDocument {
    pub version: u32,
    pub adapters: Vec<DnsAdapterSnapshot>,
}

pub fn list_active_adapters() -> CoreResult<Vec<NetworkAdapterInfo>> {
    let mut size: u32 = 16 * 1024;
    let mut buf = vec![0u8; size as usize];

    loop {
        let ret = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if ret == NO_ERROR.0 {
            break;
        }
        if ret == ERROR_BUFFER_OVERFLOW.0 {
            buf.resize(size as usize, 0);
            continue;
        }
        return Err(CoreError::Dns {
            message: format!("GetAdaptersAddresses falló con código {}", ret),
            source: None,
        });
    }

    let mut adapters = Vec::new();
    let mut current = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;

    while !current.is_null() {
        let item = unsafe { &*current };
        current = item.Next;

        if item.IfType == 24 || item.OperStatus != IfOperStatusUp {
            continue;
        }

        let guid_raw = unsafe { item.AdapterName.to_string() }.unwrap_or_default();
        let guid_clean = guid_raw.trim_matches(|c| c == '{' || c == '}').to_string();
        if parse_guid_strict(&guid_clean).is_err() {
            continue;
        }

        let friendly = unsafe { item.FriendlyName.to_string() }.unwrap_or_default();
        let desc = unsafe { item.Description.to_string() }.unwrap_or_default();
        let desc_lower = desc.to_ascii_lowercase();
        let friendly_lower = friendly.to_ascii_lowercase();

        let is_virtual_or_vpn = desc_lower.contains("hyper-v")
            || desc_lower.contains("vmware")
            || desc_lower.contains("virtualbox")
            || desc_lower.contains("wireguard")
            || desc_lower.contains("openvpn")
            || desc_lower.contains("tap-windows")
            || desc_lower.contains("wintun")
            || desc_lower.contains("vpn")
            || friendly_lower.contains("vethernet");

        let (v4_dns, v6_dns) = read_adapter_nameservers_from_registry(&guid_clean);

        adapters.push(NetworkAdapterInfo {
            interface_guid: guid_clean,
            friendly_name: friendly,
            description: desc,
            if_type: item.IfType,
            current_ipv4_dns: v4_dns,
            current_ipv6_dns: v6_dns,
            is_virtual_or_vpn,
        });
    }

    Ok(adapters)
}

fn read_adapter_nameservers_from_registry(guid_no_braces: &str) -> (String, String) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let braced = format!("{{{}}}", guid_no_braces);

    let v4_path = format!(
        r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{}",
        braced
    );
    let v4 = hklm
        .open_subkey_with_flags(&v4_path, KEY_READ)
        .ok()
        .and_then(|k| k.get_value::<String, _>("NameServer").ok())
        .map(|s| normalize_dns_list(&s))
        .unwrap_or_default();

    let v6_path = format!(
        r"SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters\Interfaces\{}",
        braced
    );
    let v6 = hklm
        .open_subkey_with_flags(&v6_path, KEY_READ)
        .ok()
        .and_then(|k| k.get_value::<String, _>("NameServer").ok())
        .map(|s| normalize_dns_list(&s))
        .unwrap_or_default();

    (v4, v6)
}

fn normalize_dns_list(raw: &str) -> String {
    raw.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn validate_dns_input(
    ipv4_primary: Option<&str>,
    ipv4_secondary: Option<&str>,
    ipv6_primary: Option<&str>,
    ipv6_secondary: Option<&str>,
) -> CoreResult<(String, String)> {
    let p4 = ipv4_primary.map(str::trim).filter(|s| !s.is_empty());
    let s4 = ipv4_secondary.map(str::trim).filter(|s| !s.is_empty());
    let p6 = ipv6_primary.map(str::trim).filter(|s| !s.is_empty());
    let s6 = ipv6_secondary.map(str::trim).filter(|s| !s.is_empty());

    if p4.is_none() && s4.is_some() {
        return Err(CoreError::Validation(
            "No se puede configurar DNS IPv4 secundario sin primario".into(),
        ));
    }
    if p6.is_none() && s6.is_some() {
        return Err(CoreError::Validation(
            "No se puede configurar DNS IPv6 secundario sin primario".into(),
        ));
    }
    if p4.is_none() && p6.is_none() {
        return Err(CoreError::Validation(
            "Debe especificar al menos un servidor DNS primario (IPv4 o IPv6)".into(),
        ));
    }

    let mut v4_list = Vec::new();
    if let Some(ip_str) = p4 {
        validate_ipv4_dns(ip_str)?;
        v4_list.push(ip_str);
    }
    if let Some(ip_str) = s4 {
        validate_ipv4_dns(ip_str)?;
        v4_list.push(ip_str);
    }

    let mut v6_list = Vec::new();
    if let Some(ip_str) = p6 {
        validate_ipv6_dns(ip_str)?;
        v6_list.push(ip_str);
    }
    if let Some(ip_str) = s6 {
        validate_ipv6_dns(ip_str)?;
        v6_list.push(ip_str);
    }

    Ok((v4_list.join(","), v6_list.join(",")))
}

fn validate_ipv4_dns(s: &str) -> CoreResult<()> {
    let ip: Ipv4Addr = s
        .parse()
        .map_err(|_| CoreError::Validation(format!("IPv4 inválida: '{}'", s)))?;
    if ip.is_unspecified() || ip.is_broadcast() || ip.is_loopback() || ip.is_multicast() {
        return Err(CoreError::Validation(format!(
            "Dirección IPv4 no válida como servidor DNS: '{}'",
            s
        )));
    }
    Ok(())
}

fn validate_ipv6_dns(s: &str) -> CoreResult<()> {
    let ip: Ipv6Addr = s
        .parse()
        .map_err(|_| CoreError::Validation(format!("IPv6 inválida: '{}'", s)))?;
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return Err(CoreError::Validation(format!(
            "Dirección IPv6 no válida como servidor DNS: '{}'",
            s
        )));
    }
    Ok(())
}

pub struct DnsManager {
    dns_backup_path: PathBuf,
}

impl DnsManager {
    pub fn new(secure_data_dir: &Path) -> Self {
        Self {
            dns_backup_path: secure_data_dir.join("dns_backup.json"),
        }
    }

    pub fn apply_dns_to_selected(
        &self,
        selected_guids: &[String],
        ipv4_csv: &str,
        ipv6_csv: &str,
    ) -> CoreResult<usize> {
        ElevationManager::require_elevated("Modificar servidores DNS de red")?;
        if selected_guids.is_empty() {
            return Err(CoreError::Validation(
                "Debe seleccionar al menos un adaptador de red".into(),
            ));
        }

        let active = list_active_adapters()?;
        let mut modified_count = 0usize;

        for raw_guid in selected_guids {
            let clean_guid = raw_guid.trim_matches(|c| c == '{' || c == '}');
            let guid_struct = parse_guid_strict(clean_guid)?;
            let adapter = active
                .iter()
                .find(|a| a.interface_guid.eq_ignore_ascii_case(clean_guid))
                .ok_or_else(|| {
                    CoreError::Validation(format!(
                        "El adaptador '{}' no está activo o no existe",
                        clean_guid
                    ))
                })?;

            self.record_adapter_before_apply(DnsAdapterSnapshot {
                interface_guid: adapter.interface_guid.clone(),
                friendly_name: adapter.friendly_name.clone(),
                previous_ipv4_nameserver: adapter.current_ipv4_dns.clone(),
                previous_ipv6_nameserver: adapter.current_ipv6_dns.clone(),
            })?;

            apply_interface_dns_typed(guid_struct, ipv4_csv, ipv6_csv)?;
            modified_count += 1;
        }

        flush_dns_cache();
        Ok(modified_count)
    }

    pub fn restore_dns(&self) -> CoreResult<usize> {
        ElevationManager::require_elevated("Restaurar servidores DNS previos")?;
        let _lock = NamedMutex::acquire_blocking(DNS_BACKUP_MUTEX)?;

        if !self.dns_backup_path.exists() {
            return Err(CoreError::Backup {
                message: "No existe respaldo DNS activo".into(),
                source: None,
            });
        }

        let bytes = read_bounded_bytes(&self.dns_backup_path, MAX_DNS_BACKUP_BYTES)?;
        let doc: DnsBackupDocument =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Backup {
                message: format!("Error leyendo dns_backup.json: {}", e),
                source: Some(Box::new(e)),
            })?;

        let mut restored = 0usize;
        for snap in &doc.adapters {
            let guid_struct = parse_guid_strict(&snap.interface_guid)?;
            apply_interface_dns_typed(
                guid_struct,
                &snap.previous_ipv4_nameserver,
                &snap.previous_ipv6_nameserver,
            )?;
            restored += 1;
        }

        flush_dns_cache();
        delete_verified_file(&self.dns_backup_path)?;
        Ok(restored)
    }

    fn record_adapter_before_apply(&self, snap: DnsAdapterSnapshot) -> CoreResult<()> {
        let _lock = NamedMutex::acquire_blocking(DNS_BACKUP_MUTEX)?;
        let mut doc = if self.dns_backup_path.exists() {
            read_bounded_bytes(&self.dns_backup_path, MAX_DNS_BACKUP_BYTES)
                .ok()
                .and_then(|b| serde_json::from_slice::<DnsBackupDocument>(&b).ok())
                .unwrap_or(DnsBackupDocument {
                    version: 1,
                    adapters: Vec::new(),
                })
        } else {
            DnsBackupDocument {
                version: 1,
                adapters: Vec::new(),
            }
        };

        if !doc
            .adapters
            .iter()
            .any(|a| a.interface_guid.eq_ignore_ascii_case(&snap.interface_guid))
        {
            doc.adapters.push(snap);
        }

        if let Some(parent) = self.dns_backup_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&doc).map_err(|e| CoreError::Backup {
            message: format!("Error serializando dns_backup.json: {}", e),
            source: Some(Box::new(e)),
        })?;
        atomic_write_new(&self.dns_backup_path, &json)
    }
}

fn apply_interface_dns_typed(
    interface_guid: GUID,
    ipv4_csv: &str,
    ipv6_csv: &str,
) -> CoreResult<()> {
    let mut v4_applied = false;
    if !ipv4_csv.is_empty() || ipv6_csv.is_empty() {
        let v4_w = to_wide_null(ipv4_csv);
        let settings_v4 = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            Flags: DNS_SETTING_NAMESERVER as u64,
            NameServer: windows::core::PWSTR(v4_w.as_ptr() as *mut _),
            ..Default::default()
        };
        let err = unsafe { SetInterfaceDnsSettings(interface_guid, &settings_v4) };
        if err.0 != 0 {
            return Err(CoreError::Dns {
                message: format!("SetInterfaceDnsSettings(IPv4) falló (código {})", err.0),
                source: None,
            });
        }
        v4_applied = true;
    }

    if !ipv6_csv.is_empty() {
        let v6_w = to_wide_null(ipv6_csv);
        let settings_v6 = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            // DNS_SETTING_IPV6 = 0x0001, DNS_SETTING_NAMESERVER = 0x0002
            Flags: (DNS_SETTING_NAMESERVER | DNS_SETTING_IPV6) as u64,
            NameServer: windows::core::PWSTR(v6_w.as_ptr() as *mut _),
            ..Default::default()
        };
        let err = unsafe { SetInterfaceDnsSettings(interface_guid, &settings_v6) };
        if err.0 != 0 {
            // Si IPv6 está deshabilitado en el adaptador (87 o 1168) y IPv4 ya se aplicó, no abortar
            if !(v4_applied && (err.0 == 87 || err.0 == 1168)) {
                return Err(CoreError::Dns {
                    message: format!("SetInterfaceDnsSettings(IPv6) falló (código {})", err.0),
                    source: None,
                });
            }
        }
    }

    Ok(())
}

fn flush_dns_cache() {
    if let Ok(lib) = SafeLibrary::load_system("dnsapi.dll") {
        unsafe {
            if let Some(proc) = lib.get_proc(b"DnsFlushResolverCache\0") {
                let func: unsafe extern "system" fn() -> i32 = std::mem::transmute(proc);
                let _ = func();
            }
        }
    }
}