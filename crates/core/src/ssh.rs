//! Launch an SSH session to a VM in an external terminal window.
//! Uses the system OpenSSH client (built into Windows 10/11).

use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Result};

use crate::validate;
use crate::vm::SshConfig;

/// ssh arguments for the target: port, accept-new host keys, user@host.
/// The host and the user reach a command line, so they are checked first.
pub fn build_ssh_args(cfg: &SshConfig) -> Result<Vec<String>> {
    let host = validate::ssh_host(&cfg.host)?;
    let user = validate::ssh_user(&cfg.user)?;
    let port = validate::port(cfg.port)?;
    Ok(vec![
        "-p".into(),
        port.to_string(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        format!("{user}@{host}"),
    ])
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
    let args = build_ssh_args(cfg)?;

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
        // The client is started directly, not through cmd.exe: a shell would
        // interpret the target as a command (audit F-07).
        let ssh = find_in_path("ssh").unwrap_or_else(|| PathBuf::from("ssh"));
        Command::new(ssh)
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
        let args = build_ssh_args(&cfg).unwrap();
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

    /// The host and the user must not be able to become an option of the
    /// ssh client (audit F-07).
    #[test]
    fn ssh_args_reject_option_like_targets() {
        for (host, user) in [
            ("-oProxyCommand=calc", "root"),
            ("127.0.0.1", "-oProxyCommand=calc"),
            ("127.0.0.1 & calc", "root"),
            ("127.0.0.1", "root;calc"),
        ] {
            let cfg = SshConfig {
                host: host.into(),
                port: 22,
                user: user.into(),
            };
            assert!(
                build_ssh_args(&cfg).is_err(),
                "must reject host {host:?} user {user:?}"
            );
        }
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
