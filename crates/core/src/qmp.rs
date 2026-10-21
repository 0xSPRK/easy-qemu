use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

const IO_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Async QMP client (one connection = one session).
pub struct Qmp {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Qmp {
    pub async fn connect(addr: SocketAddr) -> Result<Qmp> {
        let raw = timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await??;
        raw.set_nodelay(true).ok();
        let (r, w) = raw.into_split();
        let mut reader = BufReader::new(r);
        // Server greeting.
        read_msg(&mut reader).await?;
        let mut q = Qmp { reader, writer: w };
        // qmp_capabilities must be the first command.
        q.exec("qmp_capabilities", None).await?;
        Ok(q)
    }

    pub async fn exec(&mut self, cmd: &str, args: Option<Value>) -> Result<Value> {
        let req = match args {
            Some(a) => json!({ "execute": cmd, "arguments": a }),
            None => json!({ "execute": cmd }),
        };
        let line = serde_json::to_string(&req)?;
        write_line(&mut self.writer, &line).await?;

        loop {
            let v = read_msg(&mut self.reader).await?;
            if v.get("event").is_some() {
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(anyhow!("QMP error: {err}"));
            }
            if v.get("return").is_some() {
                return Ok(v["return"].clone());
            }
        }
    }

    /// Actual VNC port from `query-vnc` (the `service` field).
    pub async fn query_vnc_port(&mut self) -> Result<Option<u16>> {
        let ret = match self.exec("query-vnc", None).await {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        if ret.get("enabled").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let port = ret
            .get("service")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u16>().ok());
        Ok(port.or_else(|| ret.get("service").and_then(Value::as_u64).map(|p| p as u16)))
    }

    /// Guest status: running / paused / prelaunch / restore-vm / shutdown etc.
    pub async fn query_status(&mut self) -> Result<String> {
        let ret = self.exec("query-status", None).await?;
        Ok(ret
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }

    // ------------------------------------------------------------------
    // Guest control
    // ------------------------------------------------------------------

    pub async fn pause(&mut self) -> Result<()> {
        self.exec("stop", None).await.map(|_| ())
    }
    pub async fn cont(&mut self) -> Result<()> {
        self.exec("cont", None).await.map(|_| ())
    }
    pub async fn reset(&mut self) -> Result<()> {
        self.exec("system_reset", None).await.map(|_| ())
    }
    pub async fn powerdown(&mut self) -> Result<()> {
        self.exec("system_powerdown", None).await.map(|_| ())
    }
    /// Suspend to RAM (S3); wake with `system_wakeup`.
    pub async fn suspend(&mut self) -> Result<()> {
        self.exec("suspend", None).await.map(|_| ())
    }
    pub async fn wakeup(&mut self) -> Result<()> {
        self.exec("system_wakeup", None).await.map(|_| ())
    }
    pub async fn nmi(&mut self) -> Result<()> {
        self.exec("nmi", Some(json!({"cpu": 0}))).await.map(|_| ())
    }
    /// Graceful QEMU termination (used after a completed outgoing migration).
    pub async fn quit(&mut self) -> Result<()> {
        self.exec("quit", None).await.map(|_| ())
    }

    /// Send a key combo like "ctrl-alt-delete" or "ctrl-alt-f2".
    pub async fn send_key(&mut self, combo: &str) -> Result<()> {
        let keys = parse_key_combo(combo)?;
        self.exec("send-key", Some(json!({ "keys": keys })))
            .await
            .map(|_| ())
    }

    /// Save a screenshot. Tries PNG (QEMU >= 7.1), falls back to PPM.
    /// Returns the format actually written.
    pub async fn screendump(&mut self, filename: &str) -> Result<&'static str> {
        let png = self
            .exec(
                "screendump",
                Some(json!({"filename": filename, "format": "png"})),
            )
            .await;
        if png.is_ok() {
            return Ok("png");
        }
        self.exec("screendump", Some(json!({"filename": filename})))
            .await
            .map(|_| "ppm")
    }

    // ------------------------------------------------------------------
    // Memory balloon
    // ------------------------------------------------------------------

    /// Current balloon value in bytes (`query-balloon.actual`).
    pub async fn query_balloon_bytes(&mut self) -> Result<Option<u64>> {
        let ret = match self.exec("query-balloon", None).await {
            Ok(r) => r,
            Err(_) => return Ok(None),
        };
        Ok(ret.get("actual").and_then(Value::as_u64))
    }

    /// Request a new balloon target (bytes).
    pub async fn set_balloon_bytes(&mut self, bytes: u64) -> Result<()> {
        self.exec("balloon", Some(json!({"value": bytes})))
            .await
            .map(|_| ())
    }

    // ------------------------------------------------------------------
    // Media management (runtime ISO swap)
    // ------------------------------------------------------------------

    /// All block devices (`query-block`) — used to find the CD-ROM drive.
    pub async fn query_block(&mut self) -> Result<Value> {
        self.exec("query-block", None).await
    }

    /// Find the CD-ROM device id by matching its current image or name.
    pub async fn find_cdrom_device(&mut self, current_iso: Option<&str>) -> Result<String> {
        let blocks = self.query_block().await?;
        let arr = blocks
            .as_array()
            .ok_or_else(|| anyhow!("unexpected query-block reply"))?;

        // Prefer an empty tray or a device whose image matches the record's ISO.
        for b in arr {
            let dev = b.get("device").and_then(Value::as_str).unwrap_or("");
            if !dev.contains("cd") && !dev.contains("cdrom") {
                continue;
            }
            let inserted = b.get("inserted").and_then(|i| i.get("image"));
            let file = inserted
                .and_then(|i| i.get("filename"))
                .and_then(Value::as_str);
            match current_iso {
                Some(cur) if file == Some(cur) => return Ok(dev.to_string()),
                _ => {}
            }
            if inserted.is_none() || file.is_none() {
                return Ok(dev.to_string()); // empty tray candidate
            }
        }
        // Fallback: any device with a removable medium flag.
        for b in arr {
            if b.get("removable").and_then(Value::as_bool) == Some(true) {
                if let Some(dev) = b.get("device").and_then(Value::as_str) {
                    return Ok(dev.to_string());
                }
            }
        }
        Err(anyhow!("No CD-ROM device found in this VM"))
    }

    /// Insert/replace the CD image at runtime.
    pub async fn change_medium(&mut self, device: &str, filename: &str) -> Result<()> {
        self.exec(
            "blockdev-change-medium",
            Some(json!({
                "id": device,
                "filename": filename,
                "format": "raw"
            })),
        )
        .await
        .map(|_| ())
    }

    /// Eject the CD from the virtual tray.
    pub async fn eject_medium(&mut self, device: &str) -> Result<()> {
        self.exec("blockdev-open-tray", Some(json!({"id": device})))
            .await?;
        let _ = self
            .exec("blockdev-remove-medium", Some(json!({"id": device})))
            .await; // already empty is fine
        self.exec("blockdev-close-tray", Some(json!({"id": device})))
            .await
            .ok();
        Ok(())
    }

    // ------------------------------------------------------------------
    // VNC password
    // ------------------------------------------------------------------

    pub async fn set_password(&mut self, protocol: &str, password: &str) -> Result<()> {
        self.exec(
            "set_password",
            Some(json!({"protocol": protocol, "password": password})),
        )
        .await
        .map(|_| ())
    }

    // ------------------------------------------------------------------
    // Statistics
    // ------------------------------------------------------------------

    /// Per-node block statistics (`query-blockstats` with query-nodes=true).
    /// Nodes carry our `node-name=<did>` ids.
    pub async fn query_blockstats(&mut self) -> Result<Value> {
        self.exec("query-blockstats", Some(json!({"query-nodes": true})))
            .await
    }

    // ------------------------------------------------------------------
    // Internal snapshots (live)
    // ------------------------------------------------------------------

    pub async fn snapshot_save(
        &mut self,
        job_id: &str,
        tag: &str,
        devices: &[String],
    ) -> Result<()> {
        self.exec(
            "snapshot-save",
            Some(json!({"job-id": job_id, "tag": tag, "devices": devices})),
        )
        .await
        .map(|_| ())
    }

    pub async fn snapshot_load(
        &mut self,
        job_id: &str,
        tag: &str,
        devices: &[String],
    ) -> Result<()> {
        self.exec(
            "snapshot-load",
            Some(json!({"job-id": job_id, "tag": tag, "devices": devices})),
        )
        .await
        .map(|_| ())
    }

    pub async fn snapshot_delete(
        &mut self,
        job_id: &str,
        tag: &str,
        devices: &[String],
    ) -> Result<()> {
        self.exec(
            "snapshot-delete",
            Some(json!({"job-id": job_id, "tag": tag, "devices": devices})),
        )
        .await
        .map(|_| ())
    }

    /// Snapshot list of a RUNNING VM via `query-snapshots` (QEMU >= 6.0).
    pub async fn query_snapshots_json(&mut self) -> Result<Value> {
        self.exec("query-snapshots", None).await
    }

    /// Wait until a snapshot/migration job finishes. Returns error text on failure.
    pub async fn wait_job(&mut self, job_id: &str, max_wait: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + max_wait;
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let jobs = self.exec("query-jobs", None).await?;
            if let Some(arr) = jobs.as_array() {
                for j in arr {
                    if j.get("id").and_then(Value::as_str) != Some(job_id) {
                        continue;
                    }
                    let status = j.get("status").and_then(Value::as_str).unwrap_or("");
                    match status {
                        "concluded" => return Ok(()),
                        s if s.contains("error") => {
                            let err = j
                                .get("error")
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "unknown job error".into());
                            return Err(anyhow!("Job {job_id} failed: {err}"));
                        }
                        _ => {}
                    }
                }
            }
            if tokio::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Job {job_id} timed out after {} s",
                    max_wait.as_secs()
                ));
            }
        }
    }

    // ------------------------------------------------------------------
    // Migration (save/restore state)
    // ------------------------------------------------------------------

    pub async fn migrate(&mut self, uri: &str) -> Result<()> {
        self.exec("migrate", Some(json!({"uri": uri})))
            .await
            .map(|_| ())
    }

    pub async fn query_migrate(&mut self) -> Result<Value> {
        self.exec("query-migrate", None).await
    }

    /// Poll migration status until completed/failed.
    pub async fn wait_migration(&mut self, max_wait: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + max_wait;
        loop {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let m = self.query_migrate().await?;
            let status = m
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            match status.as_str() {
                "completed" => return Ok(()),
                "failed" | "cancelled" => {
                    let err = m
                        .get("error-desc")
                        .and_then(Value::as_str)
                        .unwrap_or(&status);
                    return Err(anyhow!("Migration {status}: {err}"));
                }
                _ => {}
            }
            if tokio::time::Instant::now() > deadline {
                return Err(anyhow!(
                    "Migration timed out after {} s",
                    max_wait.as_secs()
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Key combos
// ---------------------------------------------------------------------------

fn parse_key_combo(combo: &str) -> Result<Vec<Value>> {
    let tokens: Vec<&str> = combo
        .split(['-', '+'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Err(anyhow!("Empty key combination"));
    }
    if tokens.len() > 6 {
        return Err(anyhow!("Too many keys in combination (max 6)"));
    }
    Ok(tokens
        .iter()
        .map(|t| json!({"type": "qcode", "data": qcode(t)}))
        .collect())
}

/// Map common names to QEMU qcodes; unknown alphanumerics pass through
/// (qcodes for letters/digits/F-keys share their names).
fn qcode(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => "ret".into(),
        "esc" | "escape" => "esc".into(),
        "space" => "spc".into(),
        "del" | "delete" => "delete".into(),
        "pgup" => "pgup".into(),
        "pagedown" | "pgdn" => "pgdn".into(),
        "win" | "super" | "meta" | "cmd" => "meta".into(),
        "control" => "ctrl".into(),
        other => other.to_string(),
    }
}

async fn write_line(w: &mut OwnedWriteHalf, line: &str) -> Result<()> {
    timeout(IO_TIMEOUT, async {
        w.write_all(line.as_bytes()).await?;
        w.write_all(b"\n").await?;
        w.flush().await
    })
    .await??;
    Ok(())
}

async fn read_msg<R>(r: &mut R) -> Result<Value>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        let n = timeout(IO_TIMEOUT, r.read_line(&mut line)).await??;
        if n == 0 {
            return Err(anyhow!("QMP connection closed"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(serde_json::from_str(trimmed)?);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_combo_parses_to_qcodes() {
        let keys = parse_key_combo("ctrl-alt-delete").unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0]["data"], "ctrl");
        assert_eq!(keys[1]["data"], "alt");
        assert_eq!(keys[2]["data"], "delete");

        let keys = parse_key_combo("enter").unwrap();
        assert_eq!(keys[0]["data"], "ret");

        let keys = parse_key_combo("ctrl+alt+f2").unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[2]["data"], "f2");
    }

    #[test]
    fn key_combo_rejects_garbage() {
        assert!(parse_key_combo("").is_err());
        assert!(parse_key_combo("a-b-c-d-e-f-g").is_err());
    }
}
