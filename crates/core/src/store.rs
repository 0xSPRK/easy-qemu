use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::vm::{DisplayMode, Vm};

#[derive(Serialize, Deserialize, Clone)]
pub struct RunningInfo {
    pub id: String,
    pub pid: i32,
    pub qmp_port: u16,
    /// Legacy display-number hint (port 5900+N) from older records.
    pub vnc_display: Option<u16>,
    /// Actual VNC port obtained via QMP query-vnc after start.
    #[serde(default)]
    pub vnc_port: Option<u16>,
    pub display: DisplayMode,
    /// Process start time in epoch seconds (for uptime).
    #[serde(default)]
    pub started_at: Option<u64>,
}

impl RunningInfo {
    pub fn effective_vnc_port(&self) -> Option<u16> {
        self.vnc_port.or(self.vnc_display.map(|d| 5900 + d))
    }
}

/// VM deletion result — what actually happened to the files.
#[derive(Serialize, Clone, Debug)]
pub struct DeleteReport {
    pub json_removed: bool,
    pub pidfile_removed: bool,
    /// Whether a disk file removal was attempted.
    pub disk_attempted: bool,
    pub disk_deleted: bool,
    /// Deletion errors (e.g. file locked by a process) — shown in the UI.
    pub errors: Vec<String>,
}

pub struct Store {
    pub base: PathBuf,
    vms_dir: PathBuf,
    running_file: PathBuf,
}

impl Store {
    pub fn new(base: PathBuf) -> Result<Store> {
        let vms_dir = base.join("vms");
        let running_file = base.join("running.json");
        std::fs::create_dir_all(&vms_dir).context("create store dir")?;
        Ok(Store {
            base,
            vms_dir,
            running_file,
        })
    }

    pub fn list_vms(&self) -> Result<Vec<Vm>> {
        let mut out = vec![];
        if let Ok(entries) = std::fs::read_dir(&self.vms_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "json").unwrap_or(false) {
                    if let Ok(s) = std::fs::read_to_string(&p) {
                        match serde_json::from_str::<Vm>(&s) {
                            Ok(vm) => out.push(vm),
                            Err(e) => eprintln!("skip broken vm file {}: {e}", p.display()),
                        }
                    }
                }
            }
        }
        out.sort_by_key(|vm| vm.name.to_lowercase());
        Ok(out)
    }

    pub fn get_vm(&self, id: &str) -> Result<Option<Vm>> {
        let p = self.vms_dir.join(format!("{id}.json"));
        if !p.exists() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(Some(serde_json::from_str(&s)?))
    }

    /// Atomic write: tmp + rename.
    pub fn save_vm(&self, vm: &Vm) -> Result<()> {
        let p = self.vms_dir.join(format!("{}.json", vm.id));
        let tmp = self.vms_dir.join(format!(".{}.json.tmp", vm.id));
        std::fs::write(&tmp, serde_json::to_string_pretty(vm)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// Delete a VM. The disk file is touched ONLY if it was created by the
    /// manager (`disk_owned`) and the user confirmed deletion.
    pub fn delete_vm(&self, id: &str, delete_disk: bool) -> Result<DeleteReport> {
        let mut report = DeleteReport {
            json_removed: false,
            pidfile_removed: false,
            disk_attempted: false,
            disk_deleted: false,
            errors: vec![],
        };
        let vm = self.get_vm(id)?;

        if let Some(ref vm) = vm {
            report.disk_attempted = delete_disk && vm.disk_owned;
            if report.disk_attempted {
                if vm.disk_path.exists() {
                    match std::fs::remove_file(&vm.disk_path) {
                        Ok(_) => report.disk_deleted = true,
                        Err(e) => report.errors.push(format!(
                            "Failed to delete disk {}: {e}",
                            vm.disk_path.display()
                        )),
                    }
                } else {
                    report.disk_deleted = true; // already absent — goal achieved
                }
            }
        }

        if let Some(ref vm) = vm {
            let _ = std::fs::remove_file(vm.pidfile_path());
            report.pidfile_removed = true;
        }

        let p = self.vms_dir.join(format!("{id}.json"));
        if p.exists() {
            match std::fs::remove_file(&p) {
                Ok(_) => report.json_removed = true,
                Err(e) => report
                    .errors
                    .push(format!("Failed to remove the VM record: {e}")),
            }
        } else {
            report.json_removed = true;
        }

        Ok(report)
    }

    pub fn load_running(&self) -> Result<HashMap<String, RunningInfo>> {
        if !self.running_file.exists() {
            return Ok(HashMap::new());
        }
        let s = std::fs::read_to_string(&self.running_file)?;
        let m: HashMap<String, RunningInfo> = serde_json::from_str(&s).unwrap_or_default();
        Ok(m)
    }

    pub fn save_running(&self, m: &HashMap<String, RunningInfo>) -> Result<()> {
        let tmp = self.base.join(".running.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(m)?)?;
        std::fs::rename(&tmp, &self.running_file)?;
        Ok(())
    }
}
