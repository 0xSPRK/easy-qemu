use std::collections::HashSet;
use std::path::Path;

use crate::vm::Accel;

/// Какие акселераторы поддерживает конкретный бинарник QEMU.
#[derive(Clone, Debug, Default)]
pub struct AccelSupport {
    known: HashSet<String>,
}

impl AccelSupport {
    pub fn supports(&self, accel: &str) -> bool {
        self.known.contains(accel)
    }
}

/// Разбор вывода `qemu-system-x86_64 -accel help`.
/// Строки вида: `kvm`, `tcg` либо `whpx: Windows Hypervisor Platform support`.
pub fn parse_accel_help(output: &str) -> AccelSupport {
    let mut known = HashSet::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let token = line.split(':').next().unwrap_or("").trim().to_lowercase();
        if token.is_empty() {
            continue;
        }
        // Заголовки вроде "Accelerators supported by QEMU binary:" отфильтруются
        // проверкой на одиночное слово без пробелов.
        if token.chars().all(|c| c.is_ascii_alphanumeric()) && token.len() <= 16 {
            known.insert(token);
        }
    }
    AccelSupport { known }
}

/// Запрос поддержки у бинарника QEMU. При ошибке запуска — пустой набор.
pub fn probe(qemu_bin: &Path) -> AccelSupport {
    let out = std::process::Command::new(qemu_bin)
        .args(["-accel", "help"])
        .output();
    match out {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            parse_accel_help(&text)
        }
        Err(_) => AccelSupport::default(),
    }
}

/// Разрешение `Auto` с учётом платформы и реально поддерживаемых акселераторов.
pub fn effective(accel: Accel, support: &AccelSupport) -> Accel {
    match accel {
        Accel::Auto => {
            #[cfg(target_os = "linux")]
            {
                if std::path::Path::new("/dev/kvm").exists() && support.supports("kvm") {
                    Accel::Kvm
                } else {
                    Accel::Tcg
                }
            }
            #[cfg(target_os = "windows")]
            {
                if support.supports("whpx") {
                    Accel::Whpx
                } else {
                    Accel::Tcg
                }
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            {
                Accel::Tcg
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Accelerators supported by QEMU binary:\n  kvm\n  whpx: Windows Hypervisor Platform support\n  tcg: TCG (software) accelerator\n";

    #[test]
    fn parses_standard_output() {
        let s = parse_accel_help(SAMPLE);
        assert!(s.supports("kvm"));
        assert!(s.supports("whpx"));
        assert!(s.supports("tcg"));
        assert!(!s.supports("xen"));
    }

    #[test]
    fn empty_output_supports_nothing() {
        let s = parse_accel_help("");
        assert!(!s.supports("tcg"));
    }

    #[test]
    fn auto_on_windows_prefers_whpx_when_present() {
        #[cfg(target_os = "windows")]
        {
            let mut s = parse_accel_help(SAMPLE);
            s.known.insert("whpx".into());
            assert_eq!(effective(Accel::Auto, &s), Accel::Whpx);
        }
    }

    #[test]
    fn explicit_accel_passthrough() {
        let s = parse_accel_help("");
        assert_eq!(effective(Accel::Tcg, &s), Accel::Tcg);
    }
}
