//! Host environment probe: resolved QEMU binaries and supported accelerators.
//! Used by the Settings screen ("Detected environment") and the status bar.

use serde::Serialize;

use crate::accel;
use crate::config::Config;
use crate::qemu;

#[cfg(test)]
use std::path::PathBuf;

#[derive(Serialize, Clone, Debug)]
pub struct EnvProbe {
    /// Resolved path to qemu-system-x86_64, if found.
    pub qemu_system: Option<String>,
    /// Resolved path to qemu-img, if found.
    pub qemu_img: Option<String>,
    /// Accelerators supported by the QEMU binary (subset of kvm/whpx/tcg).
    pub accels: Vec<String>,
}

pub fn probe(cfg: &Config) -> EnvProbe {
    let sys = qemu::resolve_binary("qemu-system-x86_64", &cfg.qemu_binary).ok();
    let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img).ok();

    let accels: Vec<String> = match &sys {
        Some(bin) => {
            let support = accel::probe(bin);
            let mut names: Vec<String> = ["kvm", "whpx", "tcg"]
                .into_iter()
                .filter(|a| support.supports(a))
                .map(str::to_string)
                .collect();
            names.sort();
            names
        }
        None => Vec::new(),
    };

    EnvProbe {
        qemu_system: sys.map(|p| p.display().to_string()),
        qemu_img: img.map(|p| p.display().to_string()),
        accels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binaries_probe_to_none() {
        let cfg = Config {
            qemu_binary: Some(PathBuf::from("Z:/definitely/not/here/qemu.exe")),
            qemu_img: Some(PathBuf::from("Z:/definitely/not/here/qemu-img.exe")),
            ..Default::default()
        };
        let p = probe(&cfg);
        assert!(p.qemu_system.is_none());
        assert!(p.qemu_img.is_none());
        assert!(p.accels.is_empty());
    }
}
