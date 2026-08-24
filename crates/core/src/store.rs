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
        let entries = match std::fs::read_dir(&self.vms_dir) {
            Ok(e) => e,
            Err(_) => return Ok(vec![]),
        };
        let mut out: Vec<Vm> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| Self::read_vm_file(&e.path()))
            .collect();
        out.sort_by_key(|vm| vm.name.to_lowercase());
        Ok(out)
    }

    /// Reads one VM record; broken files are skipped with a note on stderr.
    fn read_vm_file(path: &std::path::Path) -> Option<Vm> {
        let s = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str::<Vm>(&s) {
            Ok(vm) => Some(vm),
            Err(e) => {
                eprintln!("skip broken vm file {}: {e}", path.display());
                None
            }
        }
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
            Self::try_delete_disk(vm, delete_disk, &mut report);
            let _ = std::fs::remove_file(vm.pidfile_path());
            report.pidfile_removed = true;
        }

        report.json_removed = self.remove_vm_record(id, &mut report);
        Ok(report)
    }

    /// Removes the qcow2 file when the VM owns its disk and deletion was requested.
    fn try_delete_disk(vm: &Vm, requested: bool, report: &mut DeleteReport) {
        report.disk_attempted = requested && vm.disk_owned;
        if !report.disk_attempted {
            return;
        }
        if !vm.disk_path.exists() {
            report.disk_deleted = true; // already absent — goal achieved
            return;
        }
        match std::fs::remove_file(&vm.disk_path) {
            Ok(_) => report.disk_deleted = true,
            Err(e) => report.errors.push(format!(
                "Failed to delete disk {}: {e}",
                vm.disk_path.display()
            )),
        }
    }

    fn remove_vm_record(&self, id: &str, report: &mut DeleteReport) -> bool {
        let p = self.vms_dir.join(format!("{id}.json"));
        if !p.exists() {
            return true;
        }
        match std::fs::remove_file(&p) {
            Ok(_) => true,
            Err(e) => {
                report
                    .errors
                    .push(format!("Failed to remove the VM record: {e}"));
                false
            }
        }
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
