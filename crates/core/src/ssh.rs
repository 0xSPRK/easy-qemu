//! Launch an SSH session to a VM in an external terminal window.
//! Uses the system OpenSSH client (built into Windows 10/11).

use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Result};

use crate::vm::SshConfig;

/// ssh arguments for the target: port, accept-new host keys, user@host.
pub fn build_ssh_args(cfg: &SshConfig) -> Vec<String> {
    vec![
        "-p".into(),
        cfg.port.to_string(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        format!("{}@{}", cfg.user, cfg.host),
    ]
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&exe))
        .find(|p| p.is_file())
}

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Open a new terminal window running ssh to the target.
/// Windows: Windows Terminal when available, otherwise a plain console.
pub fn launch(cfg: &SshConfig) -> Result<()> {
    if cfg.user.trim().is_empty() || cfg.host.trim().is_empty() {
        return Err(anyhow!("SSH user/host must not be empty"));
    }
    let args = build_ssh_args(cfg);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Some(wt) = find_in_path("wt") {
            Command::new(wt)
                .arg("ssh")
                .args(&args)
                .spawn()
                .map_err(|e| anyhow!("failed to launch Windows Terminal: {e}"))?;
            return Ok(());
        }
        Command::new("cmd")
            .args(["/C", "ssh"])
            .args(&args)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| anyhow!("failed to launch ssh (is OpenSSH client installed?): {e}"))?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let target = args.last().cloned().unwrap_or_default();
        let port_args = ["-p", &cfg.port.to_string()];
        let candidates: &[&[&str]] = &[
            &["gnome-terminal", "--"],
            &["konsole", "-e"],
            &["xfce4-terminal", "-x"],
            &["x-terminal-emulator", "-e"],
            &["xterm", "-e"],
        ];
        for term in candidates {
            if let Some(bin) = find_in_path(term[0]) {
                let mut cmd = Command::new(bin);
                cmd.args(&term[1..]).arg("ssh").args(port_args).arg(&target);
                if cmd.spawn().is_ok() {
                    return Ok(());
                }
            }
        }
        Err(anyhow!("no terminal emulator found to run ssh"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_args_shape() {
        let cfg = SshConfig {
            host: "127.0.0.1".into(),
            port: 2222,
            user: "root".into(),
        };
        let args = build_ssh_args(&cfg);
        assert_eq!(
            args,
            vec![
                "-p",
                "2222",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "root@127.0.0.1"
            ]
        );
    }

    #[test]
    fn default_config_is_local_root_22() {
        let cfg = SshConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 22);
        assert_eq!(cfg.user, "root");
    }

    #[test]
    fn launch_rejects_empty_fields() {
        let cfg = SshConfig {
            host: String::new(),
            port: 22,
            user: "root".into(),
        };
        assert!(launch(&cfg).is_err());
    }
}
