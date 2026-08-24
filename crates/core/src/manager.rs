use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::accel::{self, AccelSupport};
use crate::config::Config;
use crate::process;
use crate::proxy::ProxyPool;
use crate::qemu;
use crate::qmp::Qmp;
use crate::snapshots;
use crate::store::{DeleteReport, RunningInfo, Store};
use crate::vm::{DisplayMode, Vm, VmDraft};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Paused,
    Shutoff,
    Unknown,
}

impl Status {
    fn from_qmp(s: &str) -> Status {
        match s {
            "running" => Status::Running,
            "paused" => Status::Paused,
            _ => Status::Shutoff,
        }
    }
}

/// VM + status for the list view.
#[derive(Serialize, Clone)]
pub struct VmListItem {
    #[serde(flatten)]
    pub vm: Vm,
    pub status: Status,
}

/// State snapshot of a single VM for a UI event.
#[derive(Serialize, Clone, Debug)]
pub struct VmUpdate {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub pid: Option<i32>,
    pub qmp_port: Option<u16>,
    pub vnc_port: Option<u16>,
    pub console_port: Option<u16>,
    pub started_at: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConsoleInfo {
    pub ws_port: u16,
    pub vnc_port: u16,
}

enum QmpAction {
    Pause,
    Resume,
    Reset,
    Shutdown,
}

impl QmpAction {
    fn cmd(&self) -> &'static str {
        match self {
            QmpAction::Pause => "stop",
            QmpAction::Resume => "cont",
            QmpAction::Reset => "system_reset",
            QmpAction::Shutdown => "system_powerdown",
        }
    }
}

pub struct Manager {
    base: PathBuf,
    storage: PathBuf,
    store: Store,
    cfg: RwLock<Config>,
    /// Config load warnings (broken TOML etc.), cleared when settings are saved.
    config_warnings: RwLock<Vec<String>>,
    accel_cache: RwLock<Option<(PathBuf, AccelSupport)>>,
    running: Mutex<HashMap<String, RunningInfo>>,
    statuses: Mutex<HashMap<String, Status>>,
    proxies: Mutex<ProxyPool>,
}

impl Manager {
    pub fn new(base: PathBuf) -> Result<Manager> {
        std::fs::create_dir_all(&base)?;
        let loaded = Config::load(&base);
        let cfg = loaded.config;
        if !Config::path(&base).exists() {
            let _ = cfg.save(&base);
        }
        let storage = cfg.storage_dir(&base);
        std::fs::create_dir_all(&storage)?;
        let store = Store::new(base.clone())?;
        let running = store.load_running().unwrap_or_default();
        Ok(Manager {
            storage,
            base,
            store,
            cfg: RwLock::new(cfg),
            config_warnings: RwLock::new(loaded.warnings),
            accel_cache: RwLock::new(None),
            running: Mutex::new(running),
            statuses: Mutex::new(HashMap::new()),
            proxies: Mutex::new(ProxyPool::new()),
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    // ---------- config ----------

    pub async fn get_config(&self) -> Config {
        self.cfg.read().await.clone()
    }

    pub async fn get_config_warnings(&self) -> Vec<String> {
        self.config_warnings.read().await.clone()
    }

    pub async fn set_config(&self, new_cfg: Config) -> Result<()> {
        new_cfg.save(&self.base)?;
        *self.cfg.write().await = new_cfg;
        *self.config_warnings.write().await = Vec::new();
        Ok(())
    }

    async fn accel_support_for(&self, bin: &Path) -> AccelSupport {
        let mut cache = self.accel_cache.write().await;
        if let Some((cached_bin, sup)) = cache.as_ref() {
            if cached_bin == bin {
                return sup.clone();
            }
        }
        let sup = accel::probe(bin);
        *cache = Some((bin.to_path_buf(), sup.clone()));
        sup
    }

    // ---------- list and statuses ----------

    pub async fn list(&self) -> Result<Vec<VmListItem>> {
        let vms = self.store.list_vms()?;
        let st = self.statuses.lock().await;
        Ok(vms
            .into_iter()
            .map(|vm| {
                let status = st.get(&vm.id).copied().unwrap_or(Status::Unknown);
                VmListItem { vm, status }
            })
            .collect())
    }

    /// Full state snapshot of all VMs. Also cleans up dead entries.
    pub async fn refresh(&self) -> Result<Vec<VmUpdate>> {
        let probes: Vec<(String, u16)> = self
            .running
            .lock()
            .await
            .iter()
            .map(|(id, r)| (id.clone(), r.qmp_port))
            .collect();

        let mut changed = false;
        for (id, port) in probes {
            let st = match Qmp::connect(qemu::qmp_addr(port)).await {
                Ok(mut q) => match q.query_status().await {
                    Ok(s) => Status::from_qmp(&s),
                    Err(_) => Status::Shutoff,
                },
                Err(_) => Status::Shutoff,
            };
            {
                let mut run = self.running.lock().await;
                let mut stat = self.statuses.lock().await;
                stat.insert(id.clone(), st);
                if st == Status::Shutoff && run.remove(&id).is_some() {
                    changed = true;
                    let _ = std::fs::remove_file(self.storage.join(format!("{id}.pid")));
                }
            }
            if st == Status::Shutoff {
                let _ = self.proxies.lock().await.stop(&id);
            }
        }
        if changed {
            let run = self.running.lock().await;
            let _ = self.store.save_running(&run);
        }

        Ok(self.snapshot_updates().await)
    }

    /// Current state without touching QMP.
    async fn snapshot_updates(&self) -> Vec<VmUpdate> {
        let vms = self.store.list_vms().unwrap_or_default();
        let run = self.running.lock().await;
        let stat = self.statuses.lock().await;
        let proxies = self.proxies.lock().await;
        vms.into_iter()
            .map(|vm| {
                let id = vm.id.clone();
                let name = vm.name.clone();
                let status = stat.get(&id).copied().unwrap_or(Status::Unknown);
                let info = run.get(&id);
                let console_port = proxies.port_of(&id);
                VmUpdate {
                    id,
                    name,
                    status,
                    pid: info.map(|i| i.pid),
                    qmp_port: info.map(|i| i.qmp_port),
                    vnc_port: info.and_then(RunningInfo::effective_vnc_port),
                    console_port,
                    started_at: info.and_then(|i| i.started_at),
                }
            })
            .collect()
    }

    // ---------- VM lifecycle ----------

    async fn find_vm(&self, id: &str) -> Result<Vm> {
        self.store
            .get_vm(id)?
            .ok_or_else(|| anyhow!("VM '{id}' not found"))
    }

    pub async fn create(&self, draft: VmDraft) -> Result<Vm> {
        let v = draft.validate()?;
        let cfg = self.cfg.read().await;

        let (disk_path, disk_size_gb) = match &v.disk {
            crate::vm::DiskSpec::New { size_gb, folder } => {
                let folder = match folder.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(f) => PathBuf::from(f).join(&v.name),
                    None => cfg.storage_dir(&self.base).join(&v.name),
                };
                std::fs::create_dir_all(&folder)
                    .map_err(|e| anyhow!("Failed to create disk folder {}: {e}", folder.display()))?;
                let id_tmp = gen_id();
                (folder.join(format!("{id_tmp}.qcow2")), *size_gb)
            }
            crate::vm::DiskSpec::Existing { path } => {
                let p = PathBuf::from(path.trim());
                let is_qcow2 = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("qcow2"))
                    .unwrap_or(false);
                if !is_qcow2 {
                    bail!("Only disks in the qcow2 format are supported");
                }
                (p, 0)
            }
        };

        let id = gen_id();
        let owned = matches!(v.disk, crate::vm::DiskSpec::New { .. });

        if let crate::vm::DiskSpec::New { size_gb, .. } = &v.disk {
            let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img)?;
            qemu::create_disk(&img, &disk_path, *size_gb).await?;
        }

        let vm = Vm {
            id: id.clone(),
            name: v.name,
            memory_mb: v.memory_mb,
            cpus: v.cpus,
            disk_size_gb,
            disk_path,
            disk_owned: owned,
            iso: v.iso,
            accel: draft.accel,
            display: draft.display,
            vnc_display: None,
            machine: draft.machine,
            firmware: draft.firmware,
            cpu: draft.cpu,
            net_mode: draft.net_mode,
            net_model: draft.net_model,
            hostfwd: v.hostfwd,
        };
        self.store.save_vm(&vm)?;
        Ok(vm)
    }

    /// Update VM settings. Disk fields are immutable.
    pub async fn update(&self, patch: Vm) -> Result<Vm> {
        let mut vm = self.find_vm(&patch.id).await?;
        if self.running.lock().await.contains_key(&patch.id) {
            bail!("Stop the VM before editing");
        }
        vm.name = patch.name;
        vm.memory_mb = patch.memory_mb;
        vm.cpus = patch.cpus;
        vm.iso = patch.iso;
        vm.accel = patch.accel;
        vm.display = patch.display;
        vm.vnc_display = patch.vnc_display;
        vm.machine = patch.machine;
        vm.firmware = patch.firmware;
        vm.cpu = patch.cpu;
        vm.net_mode = patch.net_mode;
        vm.net_model = patch.net_model;
        vm.hostfwd = patch.hostfwd;
        self.store.save_vm(&vm)?;
        Ok(vm)
    }

    pub async fn delete(&self, id: &str, delete_disk: bool) -> Result<DeleteReport> {
        if self.running.lock().await.contains_key(id) {
            self.force_stop(id).await?;
        }
        let report = self.store.delete_vm(id, delete_disk)?;
        let _ = self.proxies.lock().await.stop(id);
        self.statuses.lock().await.remove(id);
        Ok(report)
    }

    pub async fn start(&self, id: &str) -> Result<RunningInfo> {
        {
            let run = self.running.lock().await;
            if run.contains_key(id) {
                bail!("VM is already running");
            }
        }
        let vm = self.find_vm(id).await?;
        if !vm.disk_path.exists() {
            bail!("Disk file not found: {}", vm.disk_path.display());
        }
        let cfg = self.cfg.read().await.clone();
        let bin = qemu::resolve_binary("qemu-system-x86_64", &cfg.qemu_binary)?;
        let support = self.accel_support_for(&bin).await;

        let info = qemu::start_vm(&self.store, &vm, &cfg, &support).await?;
        {
            let mut run = self.running.lock().await;
            run.insert(id.to_string(), info.clone());
            self.statuses.lock().await.insert(id.to_string(), Status::Running);
            let _ = self.store.save_running(&run);
        }
        Ok(info)
    }

    pub async fn qmp_action(&self, id: &str, action: QmpAction) -> Result<()> {
        let info = {
            let run = self.running.lock().await;
            run.get(id).cloned().ok_or_else(|| anyhow!("VM is not running"))?
        };
        let mut q = Qmp::connect(qemu::qmp_addr(info.qmp_port)).await?;
        q.exec(action.cmd(), None).await?;
        Ok(())
    }

    pub async fn force_stop(&self, id: &str) -> Result<()> {
        let info = {
            let mut run = self.running.lock().await;
            run.remove(id).ok_or_else(|| anyhow!("VM is not running"))?
        };
        let res = process::kill_force(info.pid, "qemu-system");
        let _ = self.proxies.lock().await.stop(id);
        {
            let run = self.running.lock().await;
            self.statuses.lock().await.insert(id.to_string(), Status::Shutoff);
            let _ = self.store.save_running(&run);
        }
        let _ = std::fs::remove_file(self.storage.join(format!("{id}.pid")));
        res
    }

    // ---------- console / proxy ----------

    pub async fn open_console(&self, id: &str) -> Result<ConsoleInfo> {
        let info = {
            let run = self.running.lock().await;
            run.get(id).cloned().ok_or_else(|| anyhow!("VM is not running"))?
        };
        if info.display != DisplayMode::Vnc {
            bail!("The console is available only for VMs with a VNC display");
        }
        let vnc_port = info
            .effective_vnc_port()
            .ok_or_else(|| anyhow!("VNC port unknown — wait a couple of seconds"))?;
        let ws_port = self
            .proxies
            .lock()
            .await
            .ensure(id, "127.0.0.1".into(), vnc_port)
            .await?;
        Ok(ConsoleInfo { ws_port, vnc_port })
    }

    pub async fn close_console(&self, id: &str) -> Result<()> {
        let _ = self.proxies.lock().await.stop(id);
        Ok(())
    }

    // ---------- logs ----------

    pub fn log_path(&self, id: &str) -> PathBuf {
        self.base.join(format!("{id}.log"))
    }

    pub fn log_tail(&self, id: &str, max_lines: usize) -> String {
        qemu::log_tail(&self.log_path(id), max_lines)
    }

    // ---------- snapshots ----------

    async fn img_bin_offline_vm(&self, id: &str) -> Result<(PathBuf, Vm)> {
        if self.running.lock().await.contains_key(id) {
            bail!("Snapshot operations require the VM to be powered off");
        }
        let vm = self.find_vm(id).await?;
        let cfg = self.cfg.read().await;
        let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img)?;
        Ok((img, vm))
    }

    pub async fn snapshot_list(&self, id: &str) -> Result<Vec<snapshots::SnapInfo>> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        snapshots::list(&img, &vm.disk_path).await
    }

    pub async fn snapshot_create(&self, id: &str, name: &str) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        snapshots::create(&img, &vm.disk_path, name).await
    }

    pub async fn snapshot_apply(&self, id: &str, name: &str) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        snapshots::apply(&img, &vm.disk_path, name).await
    }

    pub async fn snapshot_delete(&self, id: &str, name: &str) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        snapshots::delete(&img, &vm.disk_path, name).await
    }

    // ---------- helpers for the UI ----------

    pub fn uptime_secs(started_at: Option<u64>) -> u64 {
        started_at
            .and_then(|t| SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|n| n.as_secs() - t))
            .unwrap_or(0)
    }
}

fn gen_id() -> String {
    format!("vm{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::*;

    fn tmp_base() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eq-mgr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn draft(name: &str) -> VmDraft {
        VmDraft {
            name: name.into(),
            memory_mb: 512,
            cpus: 1,
            iso: None,
            accel: Accel::Auto,
            display: DisplayMode::None,
            machine: MachineType::Auto,
            firmware: Firmware::Bios,
            cpu: CpuModel::Auto,
            net_mode: NetMode::Nat,
            net_model: NetModel::Auto,
            hostfwd: vec![],
            disk: DiskSpec::New { size_gb: 1, folder: None },
        }
    }

    #[tokio::test]
    async fn create_and_list_roundtrip() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("alpha")).await.unwrap();
        assert!(vm.disk_owned);
        assert!(vm.disk_path.exists());
        let items = mgr.list().await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].vm.name, "alpha");
    }

    #[tokio::test]
    async fn delete_owned_disk_removes_file_and_record() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("beta")).await.unwrap();
        let disk = vm.disk_path.clone();
        let report = mgr.delete(&vm.id, true).await.unwrap();
        assert!(report.json_removed);
        assert!(report.disk_deleted);
        assert!(!disk.exists());
        assert!(mgr.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_keep_disk_preserves_file() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("gamma")).await.unwrap();
        let disk = vm.disk_path.clone();
        let report = mgr.delete(&vm.id, false).await.unwrap();
        assert!(!report.disk_attempted);
        assert!(disk.exists());
        let _ = std::fs::remove_file(&disk);
    }

    #[tokio::test]
    async fn validation_error_surfaces() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let mut d = draft("");
        d.name = "  ".into();
        assert!(mgr.create(d).await.is_err());
    }

    #[test]
    fn gen_id_unique() {
        assert_ne!(gen_id(), gen_id());
    }
}
