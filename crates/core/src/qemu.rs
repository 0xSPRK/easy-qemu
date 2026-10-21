use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::accel;
use crate::config::Config;
use crate::qmp::Qmp;
use crate::store::{RunningInfo, Store};
use crate::validate;
use crate::vm::{
    Accel, AudioBackend, BootFirst, CpuModel, DiskBus, DisplayMode, Firmware, MachineType, RtcBase,
    VgaModel, Vm,
};

/// QMP chardev/monitor + pidfile flags.
fn qmp_args(qmp_port: u16, pidfile: &Path) -> Vec<String> {
    vec![
        "-chardev".into(),
        format!(
            "socket,host=127.0.0.1,port={},server=on,wait=off,id=qmp0",
            qmp_port
        ),
        "-mon".into(),
        "chardev=qmp0,mode=control".into(),
        "-pidfile".into(),
        pidfile.to_string_lossy().to_string(),
    ]
}

pub fn resolve_binary(name: &str, override_path: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Ok(p.clone());
        }
        return Err(anyhow!("Binary '{name}' not found at: {}", p.display()));
    }

    if let Some(p) = search_in_path(name) {
        return Ok(p);
    }

    #[cfg(windows)]
    {
        let prog = PathBuf::from("C:\\Program Files\\qemu").join(name);
        if prog.is_file() {
            return Ok(prog);
        }
    }

    Err(anyhow!(
        "Could not find '{name}'. Add it to PATH or set the path in settings."
    ))
}

fn search_in_path(name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(&exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Free TCP port for QMP. Any race is closed by the connect retry loop.
pub fn free_tcp_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn qmp_addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

const VNC_PORT_SEARCH_RANGE: u16 = 20;
const MAX_DISKS: usize = 8;

/// Everything the arg builder needs besides the VM record itself.
/// Manager-owned paths live in the store base directory (single source of truth):
/// pidfile, OVMF vars, logs — never inside user disk folders.
#[derive(Clone, Debug)]
pub struct QemuLaunch {
    pub qmp_port: u16,
    pub pidfile: PathBuf,
    /// OVMF VARS flash image for this VM (UEFI only).
    pub vars_fd: PathBuf,
    pub vnc_bind: String,
    /// Bind address for hostfwd rules; empty means all interfaces.
    pub hostfwd_bind: String,
    pub accel_resolved: Accel,
    pub daemonize: bool,
    /// Restore saved VM state (`-incoming file:...`) instead of fresh boot.
    pub incoming: Option<PathBuf>,
}

// Resolved-binary hint so find_ovmf can search portable install dirs nearby.
static BINARY_HINT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

fn remember_binary_dir(p: &Path) {
    if let Ok(mut h) = BINARY_HINT.lock() {
        *h = p.parent().map(Path::to_path_buf);
    }
}

fn last_binary_dir() -> Option<PathBuf> {
    BINARY_HINT.lock().ok().and_then(|h| h.clone())
}

pub fn build_qemu_args(vm: &Vm, l: &QemuLaunch) -> Result<Vec<String>> {
    if vm.disks.is_empty() {
        bail!("A VM needs at least one disk");
    }
    if vm.disks.len() > MAX_DISKS {
        bail!("Too many disks (max {MAX_DISKS})");
    }

    // -object entries collected while building sections, emitted afterwards.
    let mut objects: Vec<String> = vec![];
    let mut machine_extra: Vec<String> = vec![];

    let mut a: Vec<String> = vec![
        "-name".into(),
        vm.name.clone(),
        "-m".into(),
        vm.memory_mb.to_string(),
    ];

    // --- memory backend / huge pages ---
    if let Some(mp) = vm
        .mem_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // The path is embedded in a comma-separated option, so a comma in it
        // has to be escaped (audit F-08).
        let prealloc = if vm.mem_prealloc { "on" } else { "off" };
        objects.push(format!(
            "memory-backend-file,id=mem,size={m}M,mem-path={mp},share=on,prealloc={prealloc}",
            m = vm.memory_mb,
            mp = validate::escape_commas(mp)
        ));
        machine_extra.push("memory-backend=mem".into());
    } else if vm.mem_prealloc {
        a.push("-mem-prealloc".into());
    }

    // --- machine ---
    let mut machine_parts: Vec<String> = vec![];
    if let Some(flag) = vm.machine.qemu_flag() {
        machine_parts.push(flag.to_string());
    }
    machine_parts.append(&mut machine_extra);
    if !machine_parts.is_empty() {
        a.extend(["-machine".into(), machine_parts.join(",")]);
    }

    a.extend(smp_args(vm));
    a.extend(cpu_accel_args(vm, l.accel_resolved));
    a.extend(disks_args(vm)?);
    a.extend(["-nic".into(), net_arg(vm, &l.hostfwd_bind)?]);
    a.extend(video_display_args(vm, &l.vnc_bind));

    // --- input / entropy ---
    if vm.usb_tablet && vm.machine != MachineType::MicroVm {
        a.extend(["-usb".into(), "-device".into(), "usb-tablet".into()]);
    }
    if vm.rng {
        objects.push("rng-builtin,id=rng0".into());
        let dev = if vm.machine == MachineType::MicroVm {
            "virtio-rng-device"
        } else {
            "virtio-rng-pci"
        };
        a.extend(["-device".into(), format!("{dev},rng=rng0")]);
    }

    // --- audio ---
    a.extend(audio_args(vm));

    // --- rtc / boot ---
    a.extend([
        "-rtc".into(),
        match vm.rtc {
            RtcBase::Utc => "base=utc".into(),
            RtcBase::LocalTime => "base=localtime".into(),
        },
    ]);
    if let Some(b) = boot_args(vm) {
        a.extend(["-boot".into(), b]);
    }

    // --- firmware ---
    if vm.firmware == Firmware::Uefi {
        setup_uefi(vm, l, &mut a)?;
    }

    // --- cdrom ---
    if let Some(iso) = &vm.iso {
        a.extend(["-cdrom".into(), iso.display().to_string()]);
    }

    // --- objects (iothread, memory backend, rng) ---
    if vm.disks.iter().any(|d| d.opts.iothread) {
        objects.push("iothread,id=io0".into());
    }
    for o in &objects {
        a.extend(["-object".into(), o.clone()]);
    }

    // --- control plumbing ---
    a.extend(qmp_args(l.qmp_port, &l.pidfile));

    if let Some(state) = &l.incoming {
        a.extend(["-incoming".into(), format!("file:{}", state.display())]);
    }

    if l.daemonize {
        #[cfg(target_os = "linux")]
        a.push("-daemonize".into());
    }

    Ok(a)
}

fn smp_args(vm: &Vm) -> Vec<String> {
    let mut s = vm.cpus.to_string();
    if let Some(v) = vm.smp_sockets {
        s.push_str(&format!(",sockets={v}"));
    }
    if let Some(v) = vm.smp_cores {
        s.push_str(&format!(",cores={v}"));
    }
    if let Some(v) = vm.smp_threads {
        s.push_str(&format!(",threads={v}"));
    }
    if let Some(v) = vm.maxcpus {
        s.push_str(&format!(",maxcpus={v}"));
    }
    vec!["-smp".into(), s]
}

/// `-cpu` / accelerator flags. `-cpu host` is only valid under KVM;
/// max is used for WHPX/TCG. TCG gets MTTCG (`thread=multi`) for multi-vCPU guests.
fn cpu_accel_args(vm: &Vm, accel_resolved: Accel) -> Vec<String> {
    let mut a = vec![];
    match vm.cpu {
        CpuModel::Auto => {}
        CpuModel::Max => a.extend(["-cpu".into(), "max".into()]),
        CpuModel::Host => {
            let model = if accel_resolved == Accel::Kvm {
                "host"
            } else {
                "max"
            };
            a.extend(["-cpu".into(), model.into()]);
        }
    }
    match accel_resolved {
        Accel::Kvm => a.extend(["-accel".into(), "kvm".into()]),
        Accel::Whpx => a.extend(["-accel".into(), "whpx".into()]),
        Accel::Tcg => {
            let v = if vm.cpus > 1 {
                "tcg,thread=multi"
            } else {
                "tcg"
            };
            a.extend(["-accel".into(), v.into()]);
        }
        Accel::Auto | Accel::None => {}
    }
    a
}

/// Drive options shared by every attach flavor.
fn drive_opts(disk_opts: &crate::vm::DiskOpts) -> String {
    let mut s = String::new();
    if let Some(c) = disk_opts.cache.qemu_str() {
        s.push_str(&format!(",cache={c}"));
    }
    if let Some(ai) = disk_opts.aio.qemu_str() {
        s.push_str(&format!(",aio={ai}"));
    }
    if disk_opts.discard_unmap {
        s.push_str(",discard=unmap");
    }
    s
}

/// Every disk is attached explicitly (`-drive if=none,id=<did>` + `-device`)
/// so iothread pinning works uniformly and NVMe is possible.
fn disks_args(vm: &Vm) -> Result<Vec<String>> {
    let mut a: Vec<String> = vec![];
    let mut port: usize = 0;
    let microvm = vm.machine == MachineType::MicroVm;

    for (i, d) in vm.disks.iter().enumerate() {
        // A disk id and a disk path end up inside comma-separated option
        // strings, so a comma in them would start a new option (audit F-08).
        let did = validate::escape_commas(&d.id);
        let mut drv = format!(
            "file={},format=qcow2,id={did},if=none,node-name={did}",
            validate::qemu_path(&d.path)
        );
        drv.push_str(&drive_opts(&d.opts));
        if d.opts.iothread {
            drv.push_str(",iothread=io0");
        }
        a.extend(["-drive".into(), drv]);

        let dev = match d.opts.bus {
            DiskBus::VirtioBlk => {
                if microvm {
                    format!("virtio-blk-device,drive={did}")
                } else {
                    format!("virtio-blk-pci,drive={did}")
                }
            }
            DiskBus::Nvme => format!("nvme,drive={did},serial={}", short_serial(&vm.id, i)),
            DiskBus::Sata => {
                if port >= 6 {
                    bail!("q35 AHCI has 6 ports — too many sata disks");
                }
                let dev = format!("ide-hd,drive={did},bus=ide.{port}");
                port += 1;
                dev
            }
            DiskBus::Ide => {
                if port >= 4 {
                    bail!("PIIX IDE has 4 slots (2 buses x master/slave) — too many ide disks");
                }
                let (bus, unit) = (port / 2, port % 2);
                port += 1;
                format!("ide-hd,drive={did},bus=ide.{bus},unit={unit}")
            }
        };
        a.extend(["-device".into(), dev]);
    }
    Ok(a)
}

fn short_serial(id: &str, i: usize) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    id.hash(&mut h);
    i.hash(&mut h);
    format!("eq{:016x}", h.finish())
}

/// `-nic` value: user/tap/none with optional model, MAC and hostfwd rules.
fn net_arg(vm: &Vm, hostfwd_bind: &str) -> Result<String> {
    use crate::vm::NetMode;
    if vm.net_mode == NetMode::None {
        return Ok("none".to_string());
    }
    let mut nic = match vm.net_mode {
        NetMode::Nat => "user".to_string(),
        _ => "tap".to_string(),
    };
    if let Some(m) = vm.net_model.qemu_model() {
        nic.push_str(&format!(",model={m}"));
    }
    nic.push_str(&format!(
        ",mac={}",
        validate::escape_commas(&vm.effective_mac())
    ));

    match vm.net_mode {
        NetMode::Nat => {
            for hf in &vm.hostfwd {
                nic.push(',');
                nic.push_str(&hf.qemu_fragment(hostfwd_bind));
            }
        }
        NetMode::Bridged => {
            if let Some(ifname) = vm
                .tap_ifname
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                // The interface name is limited to what Linux accepts, so it
                // can never end the option (audit F-08).
                nic.push_str(&format!(",ifname={}", validate::tap_name(ifname)?));
            }
            if let Some(br) = vm
                .tap_br
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                nic.push_str(&format!(",br={}", validate::escape_commas(br)));
            }
            nic.push_str(",script=no,downscript=no");
        }
        NetMode::None => {}
    }
    Ok(nic)
}

/// Video adapter + display flags. VirGL couples the two: `gl=on` on the
/// display plus a `virtio-*-gl` device instead of plain `-vga`.
fn video_display_args(vm: &Vm, vnc_bind: &str) -> Vec<String> {
    let mut a: Vec<String> = vec![];

    match vm.display {
        DisplayMode::None => a.extend(["-display".into(), "none".into()]),
        DisplayMode::Vnc => {
            let start = vm.vnc_display.unwrap_or(0);
            let mut v = format!(
                "{vb}:{d},to={rng}",
                vb = validate::escape_commas(vnc_bind),
                d = start,
                rng = VNC_PORT_SEARCH_RANGE
            );
            if vm.vnc_password_auth {
                v.push_str(",password=on");
            }
            a.extend(["-vnc".into(), v]);
        }
        DisplayMode::Gtk => a.extend([
            "-display".into(),
            if vm.gl {
                "gtk,gl=on".into()
            } else {
                "gtk".into()
            },
        ]),
        DisplayMode::Sdl => a.extend([
            "-display".into(),
            if vm.gl {
                "sdl,gl=on".into()
            } else {
                "sdl".into()
            },
        ]),
    }

    if vm.gl {
        a.extend(["-device".into(), "virtio-vga-gl".into()]);
        return a;
    }
    // Headless + default video: emit nothing (legacy behavior).
    if vm.vga == VgaModel::Auto && vm.display == DisplayMode::None {
        return a;
    }
    match vm.vga {
        VgaModel::Ramfb => a.extend(["-device".into(), "ramfb".into()]),
        VgaModel::None => {} // explicit no-video
        VgaModel::Std | VgaModel::Qxl | VgaModel::Virtio | VgaModel::Auto => {
            let mut v = match vm.vga {
                VgaModel::Qxl => "qxl".to_string(),
                VgaModel::Virtio => "virtio".to_string(),
                _ => "std".to_string(),
            };
            if matches!(vm.vga, VgaModel::Std | VgaModel::Qxl) {
                if let Some(mb) = vm.vgamem_mb {
                    v.push_str(&format!(",vgamem_mb={mb}"));
                }
            }
            a.extend(["-vga".into(), v]);
        }
    }
    a
}

/// `-audiodev` + Intel HDA card. Microvm has no PCI — skipped silently.
fn audio_args(vm: &Vm) -> Vec<String> {
    if vm.audio == AudioBackend::Off || vm.machine == MachineType::MicroVm {
        return vec![];
    }
    let driver = match vm.audio {
        AudioBackend::Sdl => "sdl",
        AudioBackend::Spice => "spice",
        #[cfg(windows)]
        AudioBackend::Wasapi => "wasapi",
        #[cfg(windows)]
        AudioBackend::DirectSound => "dsound",
        #[cfg(not(windows))]
        AudioBackend::PulseAudio => "pulseaudio",
        #[cfg(not(windows))]
        AudioBackend::Alsa => "alsa",
        AudioBackend::Off | AudioBackend::Auto => auto_audio_driver(),
    };
    let ctrl = if vm.machine == MachineType::Q35 {
        "ich9-intel-hda"
    } else {
        "intel-hda"
    };
    vec![
        "-audiodev".into(),
        format!("{driver},id=snd0"),
        "-device".into(),
        ctrl.into(),
        "-device".into(),
        "hda-duplex,audiodev=snd0".into(),
    ]
}

fn auto_audio_driver() -> &'static str {
    if cfg!(target_os = "windows") {
        "wasapi"
    } else if cfg!(target_os = "macos") {
        "coreaudio"
    } else {
        "pulseaudio"
    }
}

/// `-boot` value: order / menu / once-cdrom semantics.
/// Returns None when everything is default (keeps the command line clean).
fn boot_args(vm: &Vm) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    match vm.boot_first {
        BootFirst::Disk => {
            // Fresh ISO present: boot the CD once, then fall back to disk.
            if vm.iso.is_some() {
                parts.push("once=d".into());
            }
        }
        other => parts.push(format!("order={}", other.qemu_letter())),
    }
    if vm.boot_menu {
        parts.push("menu=on".into());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

/// Locate OVMF (UEFI) images: CODE (read-only) and VARS (template copied per VM).
/// Also searches relative to the resolved QEMU binary so portable installs work.
fn find_ovmf(bin_dir: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
    let candidates: &[(&str, &str)] = &[
        ("OVMF_CODE.fd", "OVMF_VARS.fd"),
        ("ovmf_CODE.fd", "ovmf_VARS.fd"),
        ("edk2-x86_64-code.fd", "edk2-i386-vars.fd"),
        ("edk2-x86_64-secure-code.fd", "edk2-i386-vars.fd"),
    ];
    let mut dirs: Vec<PathBuf> = [
        "C:\\Program Files\\qemu\\share",
        "C:\\Program Files\\qemu\\share\\edk2-x86_64",
        "/usr/share/OVMF",
        "/usr/share/ovmf",
        "/usr/share/edk2/ovmf",
        "/usr/share/edk2",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(dir) = bin_dir {
        dirs.push(dir.to_path_buf());
        dirs.push(dir.join("share"));
        dirs.push(dir.join("share").join("edk2-x86_64"));
    }
    for d in &dirs {
        for (code, vars) in candidates {
            let code_p = d.join(code);
            let vars_p = d.join(vars);
            if code_p.is_file() && vars_p.is_file() {
                return Some((code_p, vars_p));
            }
        }
    }
    None
}

fn setup_uefi(vm: &Vm, l: &QemuLaunch, a: &mut Vec<String>) -> Result<()> {
    let (code, vars_tmpl) = find_ovmf(last_binary_dir().as_deref()).ok_or_else(|| {
        anyhow!("UEFI requested but OVMF was not found (OVMF_CODE.fd / OVMF_VARS.fd).")
    })?;

    let vars_path = &l.vars_fd;
    if !vars_path.exists() {
        // Migrate pre-multi-disk layouts: vars used to live next to the disk.
        let legacy = vm
            .disks
            .first()
            .and_then(|d| d.path.parent())
            .map(|p| p.join(format!("{}-vars.fd", vm.id)))
            .filter(|p| p.is_file());
        if let Some(old) = legacy {
            std::fs::copy(&old, vars_path)
                .with_context(|| format!("failed to migrate OVMF VARS from {}", old.display()))?;
        } else {
            std::fs::copy(&vars_tmpl, vars_path)
                .map_err(|e| anyhow!("Failed to copy OVMF VARS: {e}"))?;
        }
    }

    a.extend([
        "-drive".into(),
        format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
        "-drive".into(),
        format!("if=pflash,format=raw,file={}", vars_path.display()),
    ]);
    Ok(())
}

fn open_log(path: &Path) -> std::fs::File {
    if let Ok(f) = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
    {
        return f;
    }
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new()
            .write(true)
            .open("NUL")
            .unwrap_or_else(|_| {
                std::fs::File::create(path.parent().unwrap_or(Path::new(".")).join("fallback.log"))
                    .unwrap()
            })
    }
    #[cfg(not(windows))]
    {
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/null")
            .unwrap()
    }
}

/// Last `max_lines` lines of a log file (used to surface startup errors).
pub fn log_tail(path: &Path, max_lines: usize) -> String {
    let Ok(s) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

#[cfg(windows)]
const DETACHED_FLAGS: u32 = 0x0000_0008 | 0x0000_0200; // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP

/// Spawn QEMU and wait until it is ready (pidfile + QMP).
/// If the process dies early the error includes the tail of its log.
pub async fn start_vm(
    store: &Store,
    vm: &Vm,
    cfg: &Config,
    support: &accel::AccelSupport,
    resume_state: Option<&Path>,
) -> Result<RunningInfo> {
    let bin = resolve_binary("qemu-system-x86_64", &cfg.qemu_binary)?;
    remember_binary_dir(&bin);
    let resolved = accel::effective(vm.accel, support);
    let qmp_port = free_tcp_port()?;
    let launch = QemuLaunch {
        qmp_port,
        pidfile: store.base.join(format!("{}.pid", vm.id)),
        vars_fd: store.base.join(format!("{}-vars.fd", vm.id)),
        vnc_bind: cfg.vnc_bind(),
        hostfwd_bind: cfg.hostfwd_bind(),
        accel_resolved: resolved,
        daemonize: true,
        incoming: resume_state.map(Path::to_path_buf),
    };
    let args = build_qemu_args(vm, &launch)?;
    let log_path = store.base.join(format!("{}.log", vm.id));

    let stderr = open_log(&log_path);
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(&args)
        .stdout(stderr.try_clone().unwrap_or_else(|_| open_log(&log_path)))
        .stderr(stderr);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(DETACHED_FLAGS);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to launch {}", bin.display()))?;

    let pid = wait_for_pidfile(&mut child, &launch.pidfile, &log_path).await?;

    // Wait for QMP readiness and resolve the actual VNC port.
    let vnc_port = wait_qmp_vnc(qmp_port, vm.display == DisplayMode::Vnc).await;

    Ok(RunningInfo {
        id: vm.id.clone(),
        pid,
        qmp_port,
        vnc_display: vm.vnc_display,
        vnc_port,
        display: vm.display,
        started_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ),
    })
}

/// Retry-connect to QMP for up to 5 s; when `want_vnc` is set, ask it for the
/// actual VNC port (QEMU searched a port range, so the hint may be off).
async fn wait_qmp_vnc(qmp_port: u16, want_vnc: bool) -> Option<u16> {
    for _ in 0..10 {
        if let Ok(mut q) = Qmp::connect(qmp_addr(qmp_port)).await {
            return if want_vnc {
                q.query_vnc_port().await.ok().flatten()
            } else {
                None
            };
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    None
}

async fn wait_for_pidfile(
    child: &mut std::process::Child,
    pidfile: &Path,
    log_path: &Path,
) -> Result<i32> {
    for _ in 0..40 {
        if let Ok(Some(status)) = child.try_wait() {
            let tail = log_tail(log_path, 30);
            let tail_str = if tail.trim().is_empty() {
                String::new()
            } else {
                format!("\n\nLog tail:\n{tail}")
            };
            anyhow::bail!("QEMU exited immediately after start ({status}).{tail_str}");
        }
        if pidfile.exists() {
            return read_pidfile(pidfile);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let _ = child.kill();
    anyhow::bail!(
        "QEMU did not create a pidfile within 10 seconds.\nLog tail:\n{}",
        log_tail(log_path, 30)
    )
}

fn read_pidfile(path: &Path) -> Result<i32> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read pidfile {}", path.display()))?;
    s.lines()
        .next()
        .and_then(|l| l.trim().parse::<i32>().ok())
        .ok_or_else(|| anyhow!("pidfile does not contain a valid PID"))
}

/// Create a qcow2 disk via qemu-img (async, no console window on Windows).
pub async fn create_disk(img_bin: &Path, path: &Path, size_gb: u32) -> Result<()> {
    let mut cmd = tokio::process::Command::new(img_bin);
    cmd.args(["create", "-f", "qcow2"])
        .arg(path)
        .arg(format!("{size_gb}G"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_window(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "qemu-img failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub fn hide_window(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
pub fn hide_window(_cmd: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests;
