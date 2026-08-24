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

#[derive(Deserialize)]
struct RawSnap {
    name: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "date-sec")]
    date_sec: Option<i64>,
    #[serde(default, rename = "date-nsec")]
    date_nsec: Option<i64>,
}

fn format_date(sec: i64, nsec: i64) -> Option<String> {
    // Без внешних крейтов: ISO из epoch через chrono-free расчёт невозможен точно
    // (високосные годы), поэтому используем простой формат epoch-секунд.
    // Для UI достаточно сортируемого представления.
    let _ = nsec;
    Some(format!("{sec}"))
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

pub async fn list(img_bin: &Path, disk: &Path) -> Result<Vec<SnapInfo>> {
    let disk_s = disk.to_string_lossy().to_string();
    let (stdout, _) =
        run_qemu_img(img_bin, &["snapshot", "-l", "--output=json", &disk_s]).await?;
    let raw: RawList = serde_json::from_str(stdout.trim())
        .map_err(|e| anyhow!("Не удалось разобрать список снапшотов: {e}"))?;
    Ok(raw
        .snapshots
        .into_iter()
        .map(|s| SnapInfo {
            date_time: match (s.date_sec, s.date_nsec) {
                (Some(sec), nsec) => format_date(sec, nsec.unwrap_or(0)),
                (None, _) => None,
            },
            name: s.name,
            tag: s.id,
        })
        .collect())
}

fn valid_snapshot_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("Имя снапшота не может быть пустым"));
    }
    if name.len() > 60 || !name.chars().all(|c| c.is_alphanumeric() || "-_ .".contains(c)) {
        return Err(anyhow!(
            "Имя снапшота: буквы, цифры, пробел и символы - _ . (до 60 символов)"
        ));
    }
    Ok(name)
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
