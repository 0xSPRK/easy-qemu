use std::process::Command;

use anyhow::{anyhow, Result};

/// Имя образа процесса по PID (для проверки перед kill).
/// Windows: `tasklist /FI "PID eq N"`; Unix: `/proc/<pid>/comm`.
fn process_image_name(pid: i32) -> Option<String> {
    #[cfg(windows)]
    {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let first = text.lines().next()?.trim();
        if first.starts_with("\"") {
            let name = first.trim_matches('"');
            if name.eq_ignore_ascii_case("INFO:") {
                return None;
            }
            return Some(name.to_string());
        }
        None
    }
    #[cfg(not(windows))]
    {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|s| s.trim().to_string())
    }
}

pub fn is_alive(pid: i32, expect_prefix: &str) -> bool {
    process_image_name(pid)
        .map(|n| n.to_lowercase().starts_with(&expect_prefix.to_lowercase()))
        .unwrap_or(false)
}

/// Принудительное завершение процесса ТОЛЬКО если это ожидаемый бинарник.
/// Защита от убийства чужого процесса после переиспользования PID.
pub fn kill_force(pid: i32, expect_prefix: &str) -> Result<()> {
    match process_image_name(pid) {
        Some(name) => {
            if !name.to_lowercase().starts_with(&expect_prefix.to_lowercase()) {
                return Err(anyhow!(
                    "PID {pid} теперь принадлежит '{name}', а не '{expect_prefix}*'. \
                     Процесс не будет остановлен."
                ));
            }
        }
        None => {
            // Процесс уже не существует — считать успехом.
            return Ok(());
        }
    }

    #[cfg(windows)]
    let status = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status()?;
    #[cfg(not(windows))]
    let status = Command::new("kill").args(["-9", &pid.to_string()]).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Не удалось завершить процесс {pid}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_pid_is_not_alive() {
        assert!(!is_alive(4_000_000, "qemu-system"));
    }

    #[test]
    fn kill_dead_pid_is_ok() {
        assert!(kill_force(4_000_000, "qemu-system").is_ok());
    }

    #[test]
    fn own_process_name_mismatch_is_refused() {
        // Текущий процесс — cargo/test, не qemu: kill должен отказаться.
        let pid = std::process::id() as i32;
        assert!(!is_alive(pid, "qemu-system"));
        assert!(kill_force(pid, "qemu-system").is_err());
    }
}
