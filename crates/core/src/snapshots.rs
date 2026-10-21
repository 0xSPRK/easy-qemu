use std::path::Path;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::qemu::hide_window;

#[derive(Serialize, Clone, Debug)]
pub struct SnapInfo {
    pub name: String,
    #[serde(rename = "id")]
    pub tag: Option<String>,
    #[serde(default)]
    pub date_time: Option<String>,
}

#[derive(Deserialize)]
struct RawList {
    #[serde(default)]
    snapshots: Vec<RawSnap>,
}

/// Accepts both qemu-img JSON (`date-sec`) and QMP SnapshotInfo
/// (`creation-sec`) field spellings.
#[derive(Deserialize)]
struct RawSnap {
    name: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, alias = "creation-sec", rename = "date-sec")]
    date_sec: Option<i64>,
    #[serde(default, alias = "creation-nsec", rename = "date-nsec")]
    date_nsec: Option<i64>,
}

/// Epoch seconds -> "YYYY-MM-DD HH:MM" UTC (no external crates).
pub fn format_epoch(sec: i64) -> String {
    let days = sec.div_euclid(86_400);
    let tod = sec.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60
    )
}

/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn snap_date(sec: i64, _nsec: i64) -> String {
    format_epoch(sec)
}

async fn run_qemu_img(img_bin: &Path, args: &[&str]) -> Result<(String, String)> {
    let mut cmd = tokio::process::Command::new(img_bin);
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_window(&mut cmd);
    let out = cmd.output().await?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        return Err(anyhow!("qemu-img: {}", stderr.trim()));
    }
    Ok((stdout, stderr))
}

fn parse_list(stdout: &str) -> Result<Vec<SnapInfo>> {
    let raw: RawList = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow!("Failed to parse the snapshot list: {e}"))?;
    Ok(raw
        .snapshots
        .into_iter()
        .map(|s| SnapInfo {
            date_time: s
                .date_sec
                .map(|sec| snap_date(sec, s.date_nsec.unwrap_or(0))),
            name: s.name,
            tag: s.id,
        })
        .collect())
}

pub async fn list(img_bin: &Path, disk: &Path) -> Result<Vec<SnapInfo>> {
    let disk_s = disk.to_string_lossy().to_string();
    let (stdout, _) = run_qemu_img(img_bin, &["snapshot", "-l", "--output=json", &disk_s]).await?;
    parse_list(&stdout)
}

fn valid_snapshot_name(name: &str) -> Result<&str> {
    // The rules live in the validation module, so the snapshot name is
    // checked in the same way everywhere (audit F-18).
    crate::validate::snapshot_name(name)
}

pub async fn create(img_bin: &Path, disk: &Path, name: &str) -> Result<()> {
    let name = valid_snapshot_name(name)?;
    let disk_s = disk.to_string_lossy().to_string();
    run_qemu_img(img_bin, &["snapshot", "-c", name, &disk_s]).await?;
    Ok(())
}

pub async fn apply(img_bin: &Path, disk: &Path, name: &str) -> Result<()> {
    let name = valid_snapshot_name(name)?;
    let disk_s = disk.to_string_lossy().to_string();
    run_qemu_img(img_bin, &["snapshot", "-a", name, &disk_s]).await?;
    Ok(())
}

pub async fn delete(img_bin: &Path, disk: &Path, name: &str) -> Result<()> {
    let name = valid_snapshot_name(name)?;
    let disk_s = disk.to_string_lossy().to_string();
    run_qemu_img(img_bin, &["snapshot", "-d", name, &disk_s]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_formats_to_utc_datetime() {
        assert_eq!(format_epoch(0), "1970-01-01 00:00");
        // 2000-03-01 00:00 UTC = 951868800
        assert_eq!(format_epoch(951_868_800), "2000-03-01 00:00");
        // Leap-year boundary: 2024-02-29 12:34 UTC = 1709210040
        assert_eq!(format_epoch(1_709_210_040), "2024-02-29 12:34");
    }

    #[test]
    fn parses_qemu_img_json() {
        let s = r#"{"snapshots":[{"name":"snap1","id":"1","date-sec":1700000000,"date-nsec":0,"vm-state-size":0}]}"#;
        let snaps = parse_list(s).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].name, "snap1");
        assert_eq!(snaps[0].tag.as_deref(), Some("1"));
        assert!(snaps[0]
            .date_time
            .as_deref()
            .unwrap_or("")
            .starts_with("2023-"));
    }

    #[test]
    fn parses_qmp_snapshot_info() {
        let s = r#"{"snapshots":[{"name":"live","id":"2","creation-sec":1700000000,"creation-nsec":0,"vm-state-size":12345}]}"#;
        let snaps = parse_list(s).unwrap();
        assert_eq!(snaps.len(), 1);
        assert!(snaps[0].date_time.is_some());
    }

    #[test]
    fn snapshot_names_validated() {
        assert!(valid_snapshot_name("clean-install").is_ok());
        assert!(valid_snapshot_name("  ").is_err());
        assert!(valid_snapshot_name("bad/name").is_err());
        assert!(valid_snapshot_name(&"x".repeat(61)).is_err());
    }
}
