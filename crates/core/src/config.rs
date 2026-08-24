use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Config {
    pub qemu_binary: Option<PathBuf>,
    pub qemu_img: Option<PathBuf>,
    pub storage_dir: Option<PathBuf>,
    pub vnc_bind: Option<String>,
    pub theme: Option<String>,
}

/// Config + warnings about file problems (shown in the UI settings).
pub struct LoadedConfig {
    pub config: Config,
    pub warnings: Vec<String>,
}

pub fn default_base() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("easy-qemu"))
        .unwrap_or_else(|| PathBuf::from(".easy-qemu"))
}

impl Config {
    pub fn path(base: &Path) -> PathBuf {
        base.join("config.toml")
    }

    pub fn load(base: &Path) -> LoadedConfig {
        let p = Config::path(base);
        if !p.exists() {
            return LoadedConfig {
                config: Config::default(),
                warnings: vec![],
            };
        }
        match std::fs::read_to_string(&p) {
            Err(e) => LoadedConfig {
                config: Config::default(),
                warnings: vec![format!("Failed to read config.toml: {e}")],
            },
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(c) => LoadedConfig {
                    config: c,
                    warnings: vec![],
                },
                Err(e) => LoadedConfig {
                    config: Config::default(),
                    warnings: vec![format!(
                        "Failed to parse config.toml ({e}); using default values"
                    )],
                },
            },
        }
    }

    /// Atomic write: tmp + rename.
    pub fn save(&self, base: &Path) -> Result<()> {
        let p = Config::path(base);
        let tmp = base.join(".config.toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn vnc_bind(&self) -> String {
        self.vnc_bind.clone().unwrap_or_else(|| "127.0.0.1".into())
    }

    pub fn storage_dir(&self, base: &Path) -> PathBuf {
        self.storage_dir
            .clone()
            .unwrap_or_else(|| base.join("disks"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_base() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eq-cfg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_load_roundtrip() {
        let base = tmp_base();
        let cfg = Config {
            qemu_binary: None,
            qemu_img: None,
            storage_dir: None,
            vnc_bind: Some("127.0.0.1".into()),
            theme: Some("dark".into()),
        };
        cfg.save(&base).unwrap();
        let loaded = Config::load(&base);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.config.vnc_bind.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn broken_toml_yields_warning_and_default() {
        let base = tmp_base();
        std::fs::write(Config::path(&base), "not [valid tomL !!!").unwrap();
        let loaded = Config::load(&base);
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("config.toml"));
    }

    #[test]
    fn missing_file_no_warnings() {
        let base = tmp_base();
        let loaded = Config::load(&base);
        assert!(loaded.warnings.is_empty());
    }
}
