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
    /// Manager-owned auxiliary files removed from the base dir
    /// (pidfile, OVMF vars, log, saved state).
    pub pidfile_removed: bool,
    pub vars_removed: bool,
    pub log_removed: bool,
    pub sav_removed: bool,
    /// Whether a disk file removal was attempted.
    pub disk_attempted: bool,
    /// Number of disk files successfully removed.
    pub disks_deleted: usize,
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

    // ---------- manager-owned per-VM files ----------

    pub fn pidfile(&self, id: &str) -> PathBuf {
        self.base.join(format!("{id}.pid"))
    }
    pub fn vars_fd(&self, id: &str) -> PathBuf {
        self.base.join(format!("{id}-vars.fd"))
    }
    pub fn log(&self, id: &str) -> PathBuf {
        self.base.join(format!("{id}.log"))
    }
    /// Saved VM state produced by live migration (`migrate file:`).
    pub fn saved_state(&self, id: &str) -> PathBuf {
        self.base.join(format!("{id}.sav"))
    }

    // ---------- vm records ----------

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
        match Self::parse_vm(&s) {
            Ok(vm) => Some(vm),
            Err(e) => {
                eprintln!("skip broken vm file {}: {e}", path.display());
                None
            }
        }
    }

    /// Parse + normalize (legacy single-disk records become `disks` entries).
    pub fn parse_vm(s: &str) -> Result<Vm> {
        let mut vm: Vm = serde_json::from_str(s)?;
        vm.normalize();
        Ok(vm)
    }

    pub fn get_vm(&self, id: &str) -> Result<Option<Vm>> {
        let p = self.vms_dir.join(format!("{id}.json"));
        if !p.exists() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
        Ok(Some(Self::parse_vm(&s)?))
    }

    /// Atomic write: tmp + rename.
    pub fn save_vm(&self, vm: &Vm) -> Result<()> {
        let p = self.vms_dir.join(format!("{}.json", vm.id));
        let tmp = self.vms_dir.join(format!(".{}.json.tmp", vm.id));
        std::fs::write(&tmp, serde_json::to_string_pretty(vm)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// Delete a VM record and manager-owned files. Disk files are touched ONLY
    /// when they were created by the manager (`disk.owned`) and the user
    /// confirmed deletion. Never fails hard: problems land in `report.errors`.
    pub fn delete_vm(&self, id: &str, delete_disks: bool) -> Result<DeleteReport> {
        let mut report = DeleteReport {
            json_removed: false,
            pidfile_removed: false,
            vars_removed: false,
            log_removed: false,
            sav_removed: false,
            disk_attempted: false,
            disks_deleted: 0,
            errors: vec![],
        };
        let vm = self.get_vm(id)?;

        if let Some(ref vm) = vm {
            if delete_disks {
                for d in vm.disks.iter().filter(|d| d.owned) {
                    report.disk_attempted = true;
                    if !d.path.exists() {
                        continue; // already absent — goal achieved
                    }
                    match std::fs::remove_file(&d.path) {
                        Ok(_) => report.disks_deleted += 1,
                        Err(e) => report
                            .errors
                            .push(format!("Failed to delete disk {}: {e}", d.path.display())),
                    }
                }
            }
            report.pidfile_removed = self.remove_aux(&self.pidfile(id), &mut report.errors);
            report.vars_removed = self.remove_aux(&self.vars_fd(id), &mut report.errors);
            report.log_removed = self.remove_aux(&self.log(id), &mut report.errors);
            report.sav_removed = self.remove_aux(&self.saved_state(id), &mut report.errors);

            // Legacy UEFI vars next to the first disk (pre-multi-disk layout).
            if let Some(old_vars) = vm
                .disks
                .first()
                .and_then(|d| d.path.parent())
                .map(|p| p.join(format!("{id}-vars.fd")))
                .filter(|p| p.is_file())
            {
                let _ = std::fs::remove_file(&old_vars);
            }
        } else {
            // No record — still clean up whatever aux files exist.
            report.pidfile_removed = self.remove_aux(&self.pidfile(id), &mut report.errors);
            report.sav_removed = self.remove_aux(&self.saved_state(id), &mut report.errors);
        }

        report.json_removed = self.remove_vm_record(id, &mut report);
        Ok(report)
    }

    fn remove_aux(&self, path: &std::path::Path, errors: &mut Vec<String>) -> bool {
        if !path.exists() {
            return true;
        }
        match std::fs::remove_file(path) {
            Ok(_) => true,
            Err(e) => {
                errors.push(format!("Failed to remove {}: {e}", path.display()));
                false
            }
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

    // ---------- running state ----------

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{DiskBus, DiskEntry, DiskOpts};

    fn tmp_base() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eq-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn vm_with_disks(base: &std::path::Path) -> Vm {
        let d0 = base.join("main.qcow2");
        let d1 = base.join("extra.qcow2");
        for p in [&d0, &d1] {
            std::fs::write(p, b"fake").unwrap();
        }
        Vm {
            id: "vmx".into(),
            name: "x".into(),
            memory_mb: 256,
            cpus: 1,
            disks: vec![
                DiskEntry {
                    id: "d0".into(),
                    path: d0,
                    size_gb: 10,
                    owned: true,
                    opts: DiskOpts {
                        bus: DiskBus::VirtioBlk,
                        ..Default::default()
                    },
                },
                DiskEntry {
                    id: "d1".into(),
                    path: d1,
                    size_gb: 0,
                    owned: false,
                    opts: DiskOpts {
                        bus: DiskBus::Nvme,
                        cache: crate::vm::CacheMode::None,
                        ..Default::default()
                    },
                },
            ],
            iso: None,
            accel: Default::default(),
            display: DisplayMode::None,
            vnc_display: None,
            machine: Default::default(),
            firmware: Default::default(),
            cpu: Default::default(),
            smp_sockets: None,
            smp_cores: None,
            smp_threads: None,
            maxcpus: None,
            rng: true,
            usb_tablet: true,
            vga: Default::default(),
            vgamem_mb: None,
            gl: false,
            mem_prealloc: false,
            mem_path: None,
            rtc: Default::default(),
            audio: Default::default(),
            vnc_password_auth: false,
            boot_first: Default::default(),
            boot_menu: false,
            net_mode: Default::default(),
            net_model: Default::default(),
            mac_address: None,
            tap_ifname: None,
            tap_br: None,
            hostfwd: vec![],
            ssh: Default::default(),
            legacy_disk_path: None,
            legacy_disk_size_gb: None,
            legacy_disk_owned: None,
        }
    }

    #[test]
    fn roundtrip_and_multi_disk_delete() {
        let base = tmp_base();
        let store = Store::new(base.clone()).unwrap();
        let vm = vm_with_disks(&base);
        store.save_vm(&vm).unwrap();

        let loaded = store.get_vm("vmx").unwrap().unwrap();
        assert_eq!(loaded.disks.len(), 2);
        assert_eq!(loaded.disks[1].opts.cache, crate::vm::CacheMode::None);

        let report = store.delete_vm("vmx", true).unwrap();
        assert!(report.json_removed);
        assert!(report.pidfile_removed); // absent counts as removed
        assert_eq!(report.disks_deleted, 1); // only the owned one
        assert!(!vm.disks[0].path.exists());
        assert!(vm.disks[1].path.exists());
        let _ = std::fs::remove_file(&vm.disks[1].path);
    }

    #[test]
    fn keep_disks_flag_preserves_files() {
        let base = tmp_base();
        let store = Store::new(base.clone()).unwrap();
        let vm = vm_with_disks(&base);
        store.save_vm(&vm).unwrap();
        let report = store.delete_vm("vmx", false).unwrap();
        assert!(!report.disk_attempted);
        assert!(vm.disks[0].path.exists());
        assert!(vm.disks[1].path.exists());
        let _ = std::fs::remove_file(&vm.disks[0].path);
        let _ = std::fs::remove_file(&vm.disks[1].path);
    }

    #[test]
    fn legacy_record_parses_into_disks() {
        let json = r#"{"id":"a","name":"old","memory_mb":512,"cpus":1,
            "disk_size_gb":5,"disk_path":"/d/o.qcow2","disk_owned":false,
            "iso":null,"accel":"auto","display":"none","vnc_display":null}"#;
        let vm = Store::parse_vm(json).unwrap();
        assert_eq!(vm.disks.len(), 1);
        assert!(!vm.disks[0].owned);
    }
}
