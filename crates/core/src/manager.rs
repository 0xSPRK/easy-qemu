use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::accel::{self, AccelSupport};
use crate::config::Config;
use crate::process;
use crate::proxy::ProxyPool;
use crate::qemu;
use crate::qmp::Qmp;
use crate::snapshots::{self, SnapInfo};
use crate::ssh;
use crate::stats::{self, DiskStat, VmStats};
use crate::store::{DeleteReport, RunningInfo, Store};
use crate::vm::{DisplayMode, SshConfig, Vm, VmDraft};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Paused,
    Shutoff,
    Unknown,
}

impl Status {
    /// Transitional QEMU states (prelaunch, restore-vm, save-vm, postmigrate,
    /// io-error) must NOT be reported as Shutoff — the poller would otherwise
    /// consider a live VM dead during startup or migration.
    fn from_qmp(s: &str) -> Status {
        match s {
            "running" | "watchdog" => Status::Running,
            "paused" | "prelaunch" | "restore-vm" | "save-vm" | "postmigrate" | "io-error" => {
                Status::Paused
            }
            "shutdown" => Status::Shutoff,
            _ => Status::Unknown,
        }
    }
    fn is_off(s: &str) -> bool {
        matches!(s, "shutdown")
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
    /// Attached on every third poll (~4.5 s); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<VmStats>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConsoleInfo {
    pub ws_port: u16,
    pub vnc_port: u16,
}

/// Guest control actions routed through QMP.
pub enum QmpAction {
    Pause,
    Resume,
    Reset,
    Shutdown,
    Suspend,
    Wakeup,
    Nmi,
}

impl QmpAction {
    async fn apply(&self, q: &mut Qmp) -> Result<()> {
        match self {
            QmpAction::Pause => q.pause().await,
            QmpAction::Resume => q.cont().await,
            QmpAction::Reset => q.reset().await,
            QmpAction::Shutdown => q.powerdown().await,
            QmpAction::Suspend => q.suspend().await,
            QmpAction::Wakeup => q.wakeup().await,
            QmpAction::Nmi => q.nmi().await,
        }
    }
}

/// CPU sample cache entry for delta computation.
struct CpuSample {
    cpu_ns: u64,
    wall: Instant,
}

pub struct Manager {
    base: PathBuf,
    store: Store,
    cfg: RwLock<Config>,
    /// Config load warnings (broken TOML etc.), cleared when settings are saved.
    config_warnings: RwLock<Vec<String>>,
    accel_cache: RwLock<Option<(PathBuf, AccelSupport)>>,
    running: Mutex<HashMap<String, RunningInfo>>,
    statuses: Mutex<HashMap<String, Status>>,
    proxies: Mutex<ProxyPool>,
    /// Latest statistics snapshot per VM (refreshed every third poll).
    stats_cache: RwLock<HashMap<String, VmStats>>,
    cpu_samples: Mutex<HashMap<String, CpuSample>>,
    poll_ticks: AtomicUsize,
}

/// How often `refresh()` collects heavy stats (ticks of ~1.5 s).
const STATS_EVERY_TICKS: usize = 3;

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
            base,
            store,
            cfg: RwLock::new(cfg),
            config_warnings: RwLock::new(loaded.warnings),
            accel_cache: RwLock::new(None),
            running: Mutex::new(running),
            statuses: Mutex::new(HashMap::new()),
            proxies: Mutex::new(ProxyPool::new()),
            stats_cache: RwLock::new(HashMap::new()),
            cpu_samples: Mutex::new(HashMap::new()),
            poll_ticks: AtomicUsize::new(0),
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
    /// A VM whose QMP port is unreachable but whose process is still alive is
    /// NOT considered dead (protects against transient hiccups).
    pub async fn refresh(&self) -> Result<Vec<VmUpdate>> {
        let probes: Vec<(String, u16, i32)> = self
            .running
            .lock()
            .await
            .iter()
            .map(|(id, r)| (id.clone(), r.qmp_port, r.pid))
            .collect();

        let mut changed = false;
        for (id, port, pid) in probes {
            let status = probe_status(port).await;
            match status {
                Some(st) if !Status::is_off(&st) => {
                    self.statuses.lock().await.insert(id, Status::from_qmp(&st));
                }
                _ => {
                    // QMP unreachable or guest shut down — verify the process.
                    if !process::is_alive(pid, "qemu-system") {
                        self.mark_dead(&id).await;
                        changed = true;
                    } else {
                        // Alive but not answering right now: keep previous status.
                    }
                }
            }
        }
        if changed {
            let run = self.running.lock().await;
            let _ = self.store.save_running(&run);
        }

        // Heavy stats every third tick.
        let tick = self.poll_ticks.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(STATS_EVERY_TICKS) {
            self.collect_stats().await;
        }

        Ok(self.snapshot_updates().await)
    }

    /// Forget a dead VM: drop its running record, pidfile and console proxy.
    async fn mark_dead(&self, id: &str) {
        {
            let mut run = self.running.lock().await;
            if run.remove(id).is_some() {
                if let Ok(p) = self.store.pidfile(id) {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        self.statuses
            .lock()
            .await
            .insert(id.to_string(), Status::Shutoff);
        self.stats_cache.write().await.remove(id);
        self.cpu_samples.lock().await.remove(id);
        let _ = self.proxies.lock().await.stop(id);
    }

    /// Current state without touching QMP.
    async fn snapshot_updates(&self) -> Vec<VmUpdate> {
        let vms = self.store.list_vms().unwrap_or_default();
        let run = self.running.lock().await;
        let stat = self.statuses.lock().await;
        let proxies = self.proxies.lock().await;
        let sc = self.stats_cache.read().await;
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
                    stats: sc.get(vm.id.as_str()).cloned(),
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

        let id = gen_id();
        let mut entries: Vec<crate::vm::DiskEntry> = vec![];

        for (i, spec) in v.disks.iter().enumerate() {
            match spec {
                crate::vm::DiskSpec::New {
                    size_gb,
                    folder,
                    opts,
                } => {
                    let parent = match folder.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                        Some(f) => PathBuf::from(f),
                        None => cfg.storage_dir(&self.base),
                    };
                    let folder = disk_folder(&parent, &v.name)?;
                    std::fs::create_dir_all(&folder).map_err(|e| {
                        anyhow!("Failed to create disk folder {}: {e}", folder.display())
                    })?;
                    ensure_inside(&parent, &folder)?;
                    let path = folder.join(format!("{id}-d{i}.qcow2"));
                    let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img)?;
                    qemu::create_disk(&img, &path, *size_gb).await?;
                    entries.push(crate::vm::DiskEntry {
                        id: format!("d{i}"),
                        path,
                        size_gb: *size_gb,
                        owned: true,
                        opts: *opts,
                    });
                }
                crate::vm::DiskSpec::Existing { path, opts } => {
                    let p = PathBuf::from(path.trim());
                    crate::validate::disk_extension(&p)?;
                    if !p.is_file() {
                        bail!("Disk file not found: {}", p.display());
                    }
                    entries.push(crate::vm::DiskEntry {
                        id: format!("d{i}"),
                        path: p,
                        size_gb: 0,
                        owned: false,
                        opts: *opts,
                    });
                }
            }
        }

        let vm = Vm {
            id: id.clone(),
            name: v.name,
            memory_mb: v.memory_mb,
            cpus: v.cpus,
            disks: entries,
            iso: v.iso,
            accel: draft.accel,
            display: draft.display,
            vnc_display: None,
            machine: draft.machine,
            firmware: draft.firmware,
            cpu: draft.cpu,
            smp_sockets: draft.smp_sockets,
            smp_cores: draft.smp_cores,
            smp_threads: draft.smp_threads,
            maxcpus: draft.maxcpus,
            rng: draft.rng,
            usb_tablet: draft.usb_tablet,
            vga: draft.vga,
            vgamem_mb: draft.vgamem_mb,
            gl: draft.gl,
            mem_prealloc: draft.mem_prealloc,
            mem_path: draft.mem_path,
            rtc: draft.rtc,
            audio: draft.audio,
            vnc_password_auth: draft.vnc_password_auth,
            boot_first: draft.boot_first,
            boot_menu: draft.boot_menu,
            net_mode: draft.net_mode,
            net_model: draft.net_model,
            mac_address: draft.mac_address.map(|s| s.trim().to_string()),
            tap_ifname: draft.tap_ifname.map(|s| s.trim().to_string()),
            tap_br: draft.tap_br.map(|s| s.trim().to_string()),
            hostfwd: v.hostfwd,
            ssh: draft.ssh,
            legacy_disk_path: None,
            legacy_disk_size_gb: None,
            legacy_disk_owned: None,
        };
        self.store.save_vm(&vm)?;
        Ok(vm)
    }

    /// Update VM settings. The full record is revalidated; disk files are
    /// never created or deleted here (orphans stay on disk by design).
    pub async fn update(&self, mut patch: Vm) -> Result<Vm> {
        patch.normalize();
        patch.validate()?;
        let mut vm = self.find_vm(&patch.id).await?;
        if self.running.lock().await.contains_key(&patch.id) {
            bail!("Stop the VM before editing");
        }
        vm.name = patch.name;
        vm.memory_mb = patch.memory_mb;
        vm.cpus = patch.cpus;
        vm.disks = patch.disks;
        vm.iso = patch.iso;
        vm.accel = patch.accel;
        vm.display = patch.display;
        vm.vnc_display = patch.vnc_display;
        vm.machine = patch.machine;
        vm.firmware = patch.firmware;
        vm.cpu = patch.cpu;
        vm.smp_sockets = patch.smp_sockets;
        vm.smp_cores = patch.smp_cores;
        vm.smp_threads = patch.smp_threads;
        vm.maxcpus = patch.maxcpus;
        vm.rng = patch.rng;
        vm.usb_tablet = patch.usb_tablet;
        vm.vga = patch.vga;
        vm.vgamem_mb = patch.vgamem_mb;
        vm.gl = patch.gl;
        vm.mem_prealloc = patch.mem_prealloc;
        vm.mem_path = patch.mem_path;
        vm.rtc = patch.rtc;
        vm.audio = patch.audio;
        vm.vnc_password_auth = patch.vnc_password_auth;
        vm.boot_first = patch.boot_first;
        vm.boot_menu = patch.boot_menu;
        vm.net_mode = patch.net_mode;
        vm.net_model = patch.net_model;
        vm.mac_address = patch.mac_address;
        vm.tap_ifname = patch.tap_ifname;
        vm.tap_br = patch.tap_br;
        vm.hostfwd = patch.hostfwd;
        vm.ssh = patch.ssh;
        vm.validate()?;
        self.store.save_vm(&vm)?;
        Ok(vm)
    }

    pub async fn delete(&self, id: &str, delete_disk: bool) -> Result<DeleteReport> {
        if self.running.lock().await.contains_key(id) {
            self.force_stop(id).await?;
        }
        let report = self.store.delete_vm(id, delete_disk)?;
        self.stats_cache.write().await.remove(id);
        self.cpu_samples.lock().await.remove(id);
        let _ = self.proxies.lock().await.stop(id);
        self.statuses.lock().await.remove(id);
        Ok(report)
    }

    pub async fn has_saved_state(&self, id: &str) -> bool {
        let Ok(p) = self.store.saved_state(id) else {
            return false;
        };
        p.is_file() && !self.running.lock().await.contains_key(id)
    }

    pub async fn discard_saved_state(&self, id: &str) -> Result<()> {
        let p = self.store.saved_state(id)?;
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| anyhow!("Failed to remove saved state: {e}"))?;
        }
        Ok(())
    }

    /// Save the live VM state to `<base>/<id>.sav` via migration to a file,
    /// then terminate the source gracefully. The VM stops afterwards.
    pub async fn save_state(&self, id: &str) -> Result<()> {
        let info = {
            let run = self.running.lock().await;
            run.get(id)
                .cloned()
                .ok_or_else(|| anyhow!("VM is not running"))?
        };
        let sav = self.store.saved_state(id)?;
        let _ = std::fs::remove_file(&sav);

        let mut q = Qmp::connect(qemu::qmp_addr(info.qmp_port)).await?;
        q.migrate(&format!("file:{}", sav.display())).await?;
        q.wait_migration(Duration::from_secs(600)).await?;
        // Source is paused post-migration: shut it down cleanly.
        let _ = q.quit().await;

        {
            let mut run = self.running.lock().await;
            run.remove(id);
            let _ = self.store.save_running(&run);
        }
        self.finish_stopped(id).await;
        Ok(())
    }

    pub async fn start(&self, id: &str) -> Result<RunningInfo> {
        {
            let run = self.running.lock().await;
            if run.contains_key(id) {
                bail!("VM is already running");
            }
        }
        let vm = self.find_vm(id).await?;
        for d in &vm.disks {
            if !d.path.exists() {
                bail!("Disk file not found: {}", d.path.display());
            }
        }
        let resume = {
            let sav = self.store.saved_state(id)?;
            if sav.is_file() {
                Some(sav)
            } else {
                None
            }
        };
        let cfg = self.cfg.read().await.clone();
        let bin = qemu::resolve_binary("qemu-system-x86_64", &cfg.qemu_binary)?;
        let support = self.accel_support_for(&bin).await;

        let info = qemu::start_vm(&self.store, &vm, &cfg, &support, resume.as_deref()).await?;

        // Restored successfully — the state file served its purpose.
        if resume.is_some() {
            if let Ok(p) = self.store.saved_state(id) {
                let _ = std::fs::remove_file(p);
            }
        }
        {
            let mut run = self.running.lock().await;
            run.insert(id.to_string(), info.clone());
            self.statuses
                .lock()
                .await
                .insert(id.to_string(), Status::Running);
            let _ = self.store.save_running(&run);
        }
        Ok(info)
    }

    pub async fn qmp_action(&self, id: &str, action: QmpAction) -> Result<()> {
        let mut q = self.connect_running(id).await?;
        action.apply(&mut q).await
    }

    pub async fn force_stop(&self, id: &str) -> Result<()> {
        let info = {
            let mut run = self.running.lock().await;
            run.remove(id).ok_or_else(|| anyhow!("VM is not running"))?
        };
        let res = process::kill_force(info.pid, "qemu-system");
        self.finish_stopped(id).await;
        res
    }

    /// Shared cleanup after a VM stopped (forced or migrated away).
    async fn finish_stopped(&self, id: &str) {
        let _ = self.proxies.lock().await.stop(id);
        {
            let run = self.running.lock().await;
            let _ = self.store.save_running(&run);
        }
        self.statuses
            .lock()
            .await
            .insert(id.to_string(), Status::Shutoff);
        self.stats_cache.write().await.remove(id);
        self.cpu_samples.lock().await.remove(id);
        if let Ok(p) = self.store.pidfile(id) {
            let _ = std::fs::remove_file(p);
        }
    }

    async fn connect_running(&self, id: &str) -> Result<Qmp> {
        let info = {
            let run = self.running.lock().await;
            run.get(id)
                .cloned()
                .ok_or_else(|| anyhow!("VM is not running"))?
        };
        Qmp::connect(qemu::qmp_addr(info.qmp_port)).await
    }

    // ---------- runtime control extras ----------

    /// Send a key combo (e.g. "ctrl-alt-delete") to the guest.
    pub async fn send_key(&self, id: &str, combo: &str) -> Result<()> {
        let mut q = self.connect_running(id).await?;
        q.send_key(combo).await
    }

    /// Save a screenshot into the base dir; returns the file path.
    pub async fn screendump(&self, id: &str) -> Result<String> {
        let shots = self.base.join("screenshots");
        std::fs::create_dir_all(&shots)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let png = shots.join(format!("{id}-{ts}.png"));
        let mut q = self.connect_running(id).await?;
        match q.screendump(&png.to_string_lossy()).await? {
            "png" => Ok(png.to_string_lossy().into_owned()),
            _ => {
                let ppm = png.with_extension("ppm");
                let _ = std::fs::rename(&png, &ppm);
                Ok(ppm.to_string_lossy().into_owned())
            }
        }
    }

    /// Adjust guest memory at runtime through the virtio-balloon.
    pub async fn set_balloon(&self, id: &str, mb: u64) -> Result<()> {
        if mb == 0 {
            bail!("Balloon target must be greater than 0 MB");
        }
        let mut q = self.connect_running(id).await?;
        q.set_balloon_bytes(mb * 1024 * 1024).await
    }

    /// Runtime ISO swap. `None` ejects the medium.
    pub async fn change_iso(&self, id: &str, iso: Option<String>) -> Result<()> {
        let vm = self.find_vm(id).await?;
        let mut q = self.connect_running(id).await?;
        let current = vm.iso.as_ref().map(|p| p.display().to_string());
        let dev = q.find_cdrom_device(current.as_deref()).await?;
        match iso.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => q.eject_medium(&dev).await,
            Some(path) => {
                if !Path::new(path).is_file() {
                    bail!("ISO not found: {path}");
                }
                q.change_medium(&dev, path).await
            }
        }
    }

    /// Set a VNC password (requires `vnc_password_auth` enabled on the VM).
    pub async fn set_vnc_password(&self, id: &str, password: &str) -> Result<()> {
        if password.len() < 4 {
            bail!("VNC password must be at least 4 characters");
        }
        let vm = self.find_vm(id).await?;
        if !vm.vnc_password_auth || vm.display != DisplayMode::Vnc {
            bail!("Enable VNC password authentication in the VM settings first");
        }
        let mut q = self.connect_running(id).await?;
        q.set_password("vnc", password).await
    }

    // ---------- console / proxy ----------

    pub async fn open_console(&self, id: &str) -> Result<ConsoleInfo> {
        let info = {
            let run = self.running.lock().await;
            run.get(id)
                .cloned()
                .ok_or_else(|| anyhow!("VM is not running"))?
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

    // ---------- SSH ----------

    /// Open an SSH session to the VM in an external terminal.
    pub fn ssh_connect(&self, id: &str) -> Result<SshConfig> {
        let vm = self
            .store
            .get_vm(id)?
            .ok_or_else(|| anyhow!("VM '{id}' not found"))?;
        ssh::launch(&vm.ssh)?;
        Ok(vm.ssh)
    }

    // ---------- logs ----------

    pub fn log_path(&self, id: &str) -> Result<PathBuf> {
        self.store.log(id)
    }

    pub fn log_tail(&self, id: &str, max_lines: usize) -> String {
        match self.log_path(id) {
            Ok(p) => qemu::log_tail(&p, max_lines),
            Err(_) => String::new(),
        }
    }
}

// ---------- statistics ----------

impl Manager {
    /// Collect per-VM stats: host CPU% delta, block totals via QMP, balloon.
    async fn collect_stats(&self) {
        let vms = self.store.list_vms().unwrap_or_default();
        let targets: Vec<(Vm, i32, u16)> = {
            let run = self.running.lock().await;
            vms.into_iter()
                .filter_map(|vm| run.get(&vm.id).map(|i| (vm, i.pid, i.qmp_port)))
                .collect()
        };
        for (vm, pid, qmp_port) in targets {
            let mut st = VmStats::default();

            // CPU% from host process deltas.
            if let Some(now_ns) = stats::process_cpu_ns(pid) {
                let mut samples = self.cpu_samples.lock().await;
                match samples.get(&vm.id) {
                    Some(prev) => {
                        st.cpu_pct = stats::cpu_pct(prev.cpu_ns, prev.wall, now_ns);
                        samples.insert(
                            vm.id.clone(),
                            CpuSample {
                                cpu_ns: now_ns,
                                wall: Instant::now(),
                            },
                        );
                    }
                    None => {
                        samples.insert(
                            vm.id.clone(),
                            CpuSample {
                                cpu_ns: now_ns,
                                wall: Instant::now(),
                            },
                        );
                    }
                }
            }

            // Block + balloon over a short-lived QMP connection.
            if let Ok(mut q) = Qmp::connect(qemu::qmp_addr(qmp_port)).await {
                if let Ok(nodes) = q.query_blockstats().await {
                    if let Some(arr) = nodes.as_array() {
                        for n in arr {
                            let node = n
                                .get("node-name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default();
                            let Some(disk) = vm.disks.iter().find(|d| d.id == node) else {
                                continue;
                            };
                            let s = n.get("stats").cloned().unwrap_or_default();
                            let g =
                                |k: &str| s.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
                            st.disks.push(DiskStat {
                                id: disk.id.clone(),
                                path: disk.path.display().to_string(),
                                rd_bytes: g("read_bytes"),
                                wr_bytes: g("write_bytes"),
                                rd_ops: g("read_operations"),
                                wr_ops: g("write_operations"),
                            });
                        }
                    }
                }
                st.balloon_mb = q
                    .query_balloon_bytes()
                    .await
                    .ok()
                    .flatten()
                    .map(|b| b / 1024 / 1024);
            }

            self.stats_cache.write().await.insert(vm.id.clone(), st);
        }
    }
}

// ---------- snapshots (offline via qemu-img, live via QMP) ----------

impl Manager {
    async fn img_bin_offline_vm(&self, id: &str) -> Result<(PathBuf, Vm)> {
        if self.running.lock().await.contains_key(id) {
            bail!("Snapshot operations require the VM to be powered off");
        }
        let vm = self.find_vm(id).await?;
        let cfg = self.cfg.read().await;
        let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img)?;
        Ok((img, vm))
    }

    pub async fn snapshot_list(&self, id: &str, disk_id: Option<String>) -> Result<Vec<SnapInfo>> {
        let (_, vm) = self.img_bin_offline_vm(id).await?;
        let disk = resolve_snapshot_disk(&vm, disk_id)?;
        let cfg = self.cfg.read().await;
        let img = qemu::resolve_binary("qemu-img", &cfg.qemu_img)?;
        snapshots::list(&img, &disk).await
    }

    pub async fn snapshot_create(
        &self,
        id: &str,
        name: &str,
        disk_id: Option<String>,
    ) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        let disk = resolve_snapshot_disk(&vm, disk_id)?;
        snapshots::create(&img, &disk, name).await
    }

    pub async fn snapshot_apply(
        &self,
        id: &str,
        name: &str,
        disk_id: Option<String>,
    ) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        let disk = resolve_snapshot_disk(&vm, disk_id)?;
        snapshots::apply(&img, &disk, name).await
    }

    pub async fn snapshot_delete(
        &self,
        id: &str,
        name: &str,
        disk_id: Option<String>,
    ) -> Result<()> {
        let (img, vm) = self.img_bin_offline_vm(id).await?;
        let disk = resolve_snapshot_disk(&vm, disk_id)?;
        snapshots::delete(&img, &disk, name).await
    }

    // ----- live variants (running VM, internal snapshots on all disks) -----

    fn live_devices(vm: &Vm) -> Vec<String> {
        vm.disks.iter().map(|d| d.id.clone()).collect()
    }

    pub async fn snapshot_live_list(&self, id: &str) -> Result<Vec<SnapInfo>> {
        let mut q = self.connect_running(id).await?;
        let arr = q.query_snapshots_json().await?;
        // query-snapshots returns a bare array; reuse the shared parser.
        let wrapped = serde_json::json!({ "snapshots": arr });
        parse_snap_list(&wrapped.to_string())
    }

    pub async fn snapshot_live_create(&self, id: &str, name: &str) -> Result<()> {
        let vm = self.find_vm(id).await?;
        let devices = Self::live_devices(&vm);
        let mut q = self.connect_running(id).await?;
        let job = format!("eq-{name}-{}", gen_id());
        q.snapshot_save(&job, name, &devices).await?;
        q.wait_job(&job, Duration::from_secs(300)).await
    }

    pub async fn snapshot_live_apply(&self, id: &str, name: &str) -> Result<()> {
        let vm = self.find_vm(id).await?;
        let devices = Self::live_devices(&vm);
        let mut q = self.connect_running(id).await?;
        let job = format!("eq-load-{}", gen_id());
        q.snapshot_load(&job, name, &devices).await?;
        q.wait_job(&job, Duration::from_secs(300)).await
    }

    pub async fn snapshot_live_delete(&self, id: &str, name: &str) -> Result<()> {
        let vm = self.find_vm(id).await?;
        let devices = Self::live_devices(&vm);
        let mut q = self.connect_running(id).await?;
        let job = format!("eq-del-{}", gen_id());
        q.snapshot_delete(&job, name, &devices).await?;
        q.wait_job(&job, Duration::from_secs(300)).await
    }
}

fn resolve_snapshot_disk(vm: &Vm, disk_id: Option<String>) -> Result<PathBuf> {
    match disk_id.as_deref() {
        Some(want) => vm
            .disks
            .iter()
            .find(|d| d.id == want)
            .map(|d| d.path.clone())
            .ok_or_else(|| anyhow!("Disk '{want}' not found on this VM")),
        None => vm
            .disks
            .first()
            .map(|d| d.path.clone())
            .ok_or_else(|| anyhow!("VM has no disks")),
    }
}

/// Parse either qemu-img or QMP snapshot lists.
fn parse_snap_list(s: &str) -> Result<Vec<SnapInfo>> {
    #[derive(serde::Deserialize)]
    struct RawList {
        #[serde(default)]
        snapshots: Vec<RawSnap>,
    }
    #[derive(serde::Deserialize)]
    struct RawSnap {
        name: String,
        #[serde(default)]
        id: Option<String>,
        #[serde(default, alias = "creation-sec")]
        date_sec: Option<i64>,
        #[serde(default)]
        _date_nsec: Option<i64>,
    }
    let raw: RawList = serde_json::from_str(s.trim())
        .map_err(|e| anyhow!("Failed to parse the snapshot list: {e}"))?;
    Ok(raw
        .snapshots
        .into_iter()
        .map(|s| SnapInfo {
            date_time: s.date_sec.map(snapshots::format_epoch),
            name: s.name,
            tag: s.id,
        })
        .collect())
}

// ---------- helpers for the UI ----------

impl Manager {
    pub fn uptime_secs(started_at: Option<u64>) -> u64 {
        started_at
            .and_then(|t| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|n| n.as_secs().saturating_sub(t))
            })
            .unwrap_or(0)
    }
}

fn gen_id() -> String {
    format!("vm{}", uuid::Uuid::new_v4().simple())
}

/// The folder that holds the disks of a new VM: a direct child of `parent`
/// named after the VM. The name is validated first, so it can neither escape
/// the parent nor replace it with an absolute path.
fn disk_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = crate::validate::vm_name(name)?;
    let folder = parent.join(name);
    if folder.parent() != Some(parent) {
        bail!("Invalid disk folder name");
    }
    Ok(folder)
}

/// Defense in depth: resolve both paths (symlinks included) and require the
/// child to stay inside its parent.
fn ensure_inside(parent: &Path, child: &Path) -> Result<()> {
    let parent = parent
        .canonicalize()
        .map_err(|e| anyhow!("Failed to resolve {}: {e}", parent.display()))?;
    let child = child
        .canonicalize()
        .map_err(|e| anyhow!("Failed to resolve {}: {e}", child.display()))?;
    if !child.starts_with(&parent) {
        bail!(
            "Disk folder {} is outside {}",
            child.display(),
            parent.display()
        );
    }
    Ok(())
}

/// Query guest status over a short-lived QMP connection.
/// None = QMP unreachable.
async fn probe_status(qmp_port: u16) -> Option<String> {
    match Qmp::connect(qemu::qmp_addr(qmp_port)).await {
        Ok(mut q) => q.query_status().await.ok(),
        Err(_) => None,
    }
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
            mac_address: None,
            tap_ifname: None,
            tap_br: None,
            hostfwd: vec![],
            ssh: SshConfig::default(),
            boot_first: BootFirst::Disk,
            boot_menu: false,
            smp_sockets: None,
            smp_cores: None,
            smp_threads: None,
            maxcpus: None,
            rng: false,
            usb_tablet: true,
            vga: VgaModel::Auto,
            vgamem_mb: None,
            gl: false,
            mem_prealloc: false,
            mem_path: None,
            rtc: RtcBase::Utc,
            audio: AudioBackend::Off,
            vnc_password_auth: false,
            disks: vec![DiskSpec::New {
                size_gb: 1,
                folder: None,
                opts: DiskOpts::default(),
            }],
        }
    }

    /// A hostile id must never reach the filesystem through the manager
    /// either (audit F-02).
    #[tokio::test]
    async fn delete_rejects_a_hostile_id() {
        let base = tmp_base();
        let mgr = Manager::new(base.clone()).unwrap();
        let victim = base.join("victim.json");
        std::fs::write(&victim, b"{}").unwrap();
        assert!(mgr.delete("../victim", true).await.is_err());
        assert!(victim.exists(), "the file outside the store must survive");
        let _ = std::fs::remove_file(&victim);
    }

    /// Creating a disk needs a real `qemu-img`. CI runners do not ship QEMU,
    /// so the tests that create disks are skipped when it is missing.
    fn qemu_img_available() -> bool {
        let found = qemu::resolve_binary("qemu-img", &None).is_ok();
        if !found {
            eprintln!("skipping: qemu-img not found on this host");
        }
        found
    }

    #[tokio::test]
    async fn create_and_list_roundtrip() {
        if !qemu_img_available() {
            return;
        }
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("alpha")).await.unwrap();
        assert!(vm.disks[0].owned);
        assert!(vm.primary_disk().unwrap().path.exists());
        let items = mgr.list().await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].vm.name, "alpha");
    }

    #[tokio::test]
    async fn delete_owned_disk_removes_file_and_record() {
        if !qemu_img_available() {
            return;
        }
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("beta")).await.unwrap();
        let disk = vm.disks[0].path.clone();
        let report = mgr.delete(&vm.id, true).await.unwrap();
        assert!(report.json_removed);
        assert_eq!(report.disks_deleted, 1);
        assert!(!disk.exists());
        assert!(mgr.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_keep_disk_preserves_file() {
        if !qemu_img_available() {
            return;
        }
        let mgr = Manager::new(tmp_base()).unwrap();
        let vm = mgr.create(draft("gamma")).await.unwrap();
        let disk = vm.disks[0].path.clone();
        let report = mgr.delete(&vm.id, false).await.unwrap();
        assert!(!report.disk_attempted);
        assert!(disk.exists());
        let _ = std::fs::remove_file(&disk);
    }

    /// A VM name must never become a path component that escapes the disk
    /// directory (audit F-01).
    #[tokio::test]
    async fn create_rejects_names_that_escape_the_disk_dir() {
        let mgr = Manager::new(tmp_base()).unwrap();
        for name in ["..", "../victim", "..\\victim", "C:\\Windows\\Temp", "/etc"] {
            assert!(
                mgr.create(draft(name)).await.is_err(),
                "must reject the name {name:?}"
            );
        }
    }

    /// The disk of a new VM lives inside the configured storage directory.
    #[tokio::test]
    async fn create_keeps_disks_inside_the_storage_dir() {
        if !qemu_img_available() {
            return;
        }
        let base = tmp_base();
        let mgr = Manager::new(base.clone()).unwrap();
        let vm = mgr.create(draft("inside")).await.unwrap();
        let disk = vm.disks[0].path.clone();
        let storage = base.join("disks");
        assert!(
            disk.starts_with(&storage),
            "disk {} must live under {}",
            disk.display(),
            storage.display()
        );
    }

    #[tokio::test]
    async fn validation_error_surfaces() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let mut d = draft("");
        d.name = "  ".into();
        assert!(mgr.create(d).await.is_err());
    }

    /// update() must re-validate: microvm + VNC used to slip through.
    #[tokio::test]
    async fn update_validates_record() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let mut vm = mgr.create(draft("delta")).await.unwrap();

        // Invalid combo must be rejected without touching the record.
        let mut bad = vm.clone();
        bad.machine = MachineType::MicroVm;
        bad.display = DisplayMode::Vnc;
        assert!(mgr.update(bad).await.is_err());

        // Valid rename works and keeps disks.
        vm.name = "delta2".into();
        let updated = mgr.update(vm).await.unwrap();
        assert_eq!(updated.name, "delta2");
        assert_eq!(updated.disks.len(), 1);
    }

    #[tokio::test]
    async fn update_rejects_duplicate_hostfwd() {
        let mgr = Manager::new(tmp_base()).unwrap();
        let mut vm = mgr.create(draft("eps")).await.unwrap();
        vm.hostfwd = vec![
            HostFwd {
                proto: FwdProto::Tcp,
                host_port: 2222,
                guest_port: 22,
            },
            HostFwd {
                proto: FwdProto::Tcp,
                host_port: 2222,
                guest_port: 80,
            },
        ];
        assert!(mgr.update(vm).await.is_err());
    }

    #[test]
    fn transitional_statuses_are_not_shutoff() {
        assert_eq!(Status::from_qmp("prelaunch"), Status::Paused);
        assert_eq!(Status::from_qmp("restore-vm"), Status::Paused);
        assert_eq!(Status::from_qmp("postmigrate"), Status::Paused);
        assert_eq!(Status::from_qmp("shutdown"), Status::Shutoff);
        assert_eq!(Status::from_qmp("running"), Status::Running);
        assert_eq!(Status::from_qmp("paused"), Status::Paused);
        assert_eq!(Status::from_qmp("weird-future-state"), Status::Unknown);
    }

    #[test]
    fn gen_id_unique() {
        assert_ne!(gen_id(), gen_id());
    }

    #[test]
    fn snap_list_parses_both_shapes() {
        let img = r#"{"snapshots":[{"name":"a","id":"1","date-sec":1700000000,"date-nsec":0}]}"#;
        assert_eq!(parse_snap_list(img).unwrap()[0].name, "a");
        let qmp =
            r#"{"snapshots":[{"name":"b","id":"2","creation-sec":1700000000,"vm-state-size":5}]}"#;
        assert_eq!(parse_snap_list(qmp).unwrap()[0].name, "b");
    }
}
