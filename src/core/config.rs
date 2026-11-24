use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safe_fs::{atomic_write_new, read_bounded_bytes};

const MAX_CONFIG_BYTES: u64 = 256 * 1024;
const MIN_SENSOR_POLL_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DefaultArchiveFormat {
    #[default]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    TarGz,
}

impl DefaultArchiveFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "zip" => Some(Self::Zip),
            "7z" => Some(Self::SevenZ),
            "tar.gz" | "tgz" => Some(Self::TarGz),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::TarGz => "tar.gz",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub sensor_poll_interval_ms: u64,
    pub default_archive_format: DefaultArchiveFormat,
    pub clean_recent_hours_threshold: u64,
    pub auto_create_restore_backup: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sensor_poll_interval_ms: 1500,
            default_archive_format: DefaultArchiveFormat::Zip,
            clean_recent_hours_threshold: 24,
            auto_create_restore_backup: true,
        }
    }
}

impl AppConfig {
    pub fn sanitize(&mut self) {
        if self.sensor_poll_interval_ms < MIN_SENSOR_POLL_MS {
            self.sensor_poll_interval_ms = MIN_SENSOR_POLL_MS;
        }
        if self.clean_recent_hours_threshold == 0 {
            self.clean_recent_hours_threshold = 24;
        }
    }

    pub fn load_from(path: &Path) -> CoreResult<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let bytes = read_bounded_bytes(path, MAX_CONFIG_BYTES)?;
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(mut cfg) => {
                cfg.sanitize();
                Ok(cfg)
            }
            Err(parse_err) => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let bak_path = path.with_extension(format!("json.{}.corrupt.bak", ts));
                let _ = std::fs::rename(path, &bak_path);
                Err(CoreError::Validation(format!(
                    "config.json corrupto (respaldado en '{}'): {}",
                    bak_path.display(),
                    parse_err
                )))
            }
        }
    }

    pub fn save_to(&mut self, path: &Path) -> CoreResult<()> {
        self.sanitize();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| CoreError::Validation(format!("Error serializando AppConfig: {}", e)))?;
        atomic_write_new(path, &json)
    }

    pub fn default_config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.json")
    }
}
