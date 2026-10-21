use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::validate;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Accel {
    #[default]
    Auto,
    Kvm,
    Whpx,
    Tcg,
    None,
}

impl Accel {
    pub const ALL: &'static [Accel] = &[
        Accel::Auto,
        Accel::Kvm,
        Accel::Whpx,
        Accel::Tcg,
        Accel::None,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Accel::Auto => "auto",
            Accel::Kvm => "kvm",
            Accel::Whpx => "whpx",
            Accel::Tcg => "tcg",
            Accel::None => "none",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    #[default]
    None,
    Vnc,
    Gtk,
    Sdl,
}

impl DisplayMode {
    pub const ALL: &'static [DisplayMode] = &[
        DisplayMode::None,
        DisplayMode::Vnc,
        DisplayMode::Gtk,
        DisplayMode::Sdl,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DisplayMode::None => "headless",
            DisplayMode::Vnc => "vnc",
            DisplayMode::Gtk => "gtk window",
            DisplayMode::Sdl => "sdl window",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum MachineType {
    /// Don't pass `-machine` (QEMU default).
    #[default]
    Auto,
    /// Modern ICH9 chipset: PCIe, AHCI, UEFI, virtio.
    Q35,
    /// Legacy PIIX chipset (old guests only).
    I440Fx,
    /// Minimal platform (virtio-mmio only, headless).
    MicroVm,
}

impl MachineType {
    pub const ALL: &'static [MachineType] = &[
        MachineType::Q35,
        MachineType::I440Fx,
        MachineType::MicroVm,
        MachineType::Auto,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MachineType::Auto => "auto (QEMU default)",
            MachineType::Q35 => "q35 (modern)",
            MachineType::I440Fx => "i440fx (legacy)",
            MachineType::MicroVm => "microvm (minimal)",
        }
    }

    pub fn qemu_flag(&self) -> Option<&'static str> {
        match self {
            MachineType::Auto => None,
            MachineType::Q35 => Some("q35"),
            MachineType::I440Fx => Some("pc"),
            MachineType::MicroVm => Some("microvm"),
        }
    }

    pub fn supports_vnc(&self) -> bool {
        !matches!(self, MachineType::MicroVm)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Firmware {
    #[default]
    Bios,
    Uefi,
}

impl Firmware {
    pub const ALL: &'static [Firmware] = &[Firmware::Bios, Firmware::Uefi];

    pub fn label(&self) -> &'static str {
        match self {
            Firmware::Bios => "BIOS (SeaBIOS)",
            Firmware::Uefi => "UEFI (OVMF)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CpuModel {
    #[default]
    Auto,
    Max,
    Host,
}

impl CpuModel {
    pub const ALL: &'static [CpuModel] = &[CpuModel::Auto, CpuModel::Max, CpuModel::Host];

    pub fn label(&self) -> &'static str {
        match self {
            CpuModel::Auto => "auto",
            CpuModel::Max => "max",
            CpuModel::Host => "host",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    /// SLIRP user networking (NAT through the host, no admin rights needed).
    #[default]
    Nat,
    /// Bridge into the LAN via a host TAP adapter.
    Bridged,
    /// No NIC attached.
    None,
}

impl NetMode {
    pub const ALL: &'static [NetMode] = &[NetMode::Nat, NetMode::Bridged, NetMode::None];

    pub fn label(&self) -> &'static str {
        match self {
            NetMode::Nat => "nat (user)",
            NetMode::Bridged => "bridged (tap)",
            NetMode::None => "none",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetModel {
    /// No `model=` — QEMU default (e1000), keeps legacy behavior.
    #[default]
    Auto,
    /// Paravirtual virtio NIC (Linux guests).
    Virtio,
    /// Intel e1000 (Windows without virtio drivers).
    E1000,
    /// Realtek rtl8139 (old guests).
    Rtl8139,
}

impl NetModel {
    pub const ALL: &'static [NetModel] = &[
        NetModel::Auto,
        NetModel::Virtio,
        NetModel::E1000,
        NetModel::Rtl8139,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            NetModel::Auto => "auto (e1000)",
            NetModel::Virtio => "virtio-net",
            NetModel::E1000 => "e1000",
            NetModel::Rtl8139 => "rtl8139",
        }
    }

    pub fn qemu_model(&self) -> Option<&'static str> {
        match self {
            NetModel::Auto => None,
            NetModel::Virtio => Some("virtio-net-pci"),
            NetModel::E1000 => Some("e1000"),
            NetModel::Rtl8139 => Some("rtl8139"),
        }
    }
}

/// Storage bus of a drive.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DiskBus {
    /// Paravirtual virtio-blk — fastest; needs guest drivers.
    #[default]
    VirtioBlk,
    /// NVMe controller — fast, works without virtio drivers on modern OS.
    Nvme,
    /// SATA/AHCI (q35 only) — native drivers everywhere.
    Sata,
    /// Legacy IDE (i440fx / q35 compat) — slowest but universal.
    Ide,
}

impl DiskBus {
    pub const ALL: &'static [DiskBus] = &[
        DiskBus::VirtioBlk,
        DiskBus::Nvme,
        DiskBus::Sata,
        DiskBus::Ide,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DiskBus::VirtioBlk => "virtio-blk (fast)",
            DiskBus::Nvme => "nvme (fast)",
            DiskBus::Sata => "sata/ahci",
            DiskBus::Ide => "ide (compat)",
        }
    }

    /// `-drive if=` value.
    pub fn qemu_if(&self) -> &'static str {
        match self {
            DiskBus::VirtioBlk => "virtio",
            DiskBus::Nvme => "nvme", // handled via -device in the builder
            DiskBus::Sata => "sata",
            DiskBus::Ide => "ide",
        }
    }
}

/// Host cache mode of a drive (`cache=`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CacheMode {
    /// Omit `cache=` (QEMU default: writeback).
    #[default]
    Auto,
    None,
    Writeback,
    Writethrough,
    /// Unsafe cache: no flushes on guest shutdown — faster, risky on power loss.
    Unsafe,
}

impl CacheMode {
    pub const ALL: &'static [CacheMode] = &[
        CacheMode::Auto,
        CacheMode::None,
        CacheMode::Writeback,
        CacheMode::Writethrough,
        CacheMode::Unsafe,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CacheMode::Auto => "auto",
            CacheMode::None => "none (host page cache off)",
            CacheMode::Writeback => "writeback (default)",
            CacheMode::Writethrough => "writethrough",
            CacheMode::Unsafe => "unsafe (skip flushes)",
        }
    }

    pub fn qemu_str(&self) -> Option<&'static str> {
        match self {
            CacheMode::Auto => None,
            CacheMode::None => Some("none"),
            CacheMode::Writeback => Some("writeback"),
            CacheMode::Writethrough => Some("writethrough"),
            CacheMode::Unsafe => Some("unsafe"),
        }
    }
}

/// AIO backend of a drive (`aio=`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AioMode {
    /// Omit `aio=`.
    #[default]
    Auto,
    Threads,
    /// Linux native AIO (O_DIRECT implied with cache=none).
    Native,
    /// Linux io_uring (fastest on modern kernels, Linux only).
    IoUring,
}

impl AioMode {
    pub const ALL: &'static [AioMode] = &[
        AioMode::Auto,
        AioMode::Threads,
        AioMode::Native,
        AioMode::IoUring,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AioMode::Auto => "auto",
            AioMode::Threads => "threads",
            AioMode::Native => "native (linux)",
            AioMode::IoUring => "io_uring (linux)",
        }
    }

    pub fn qemu_str(&self) -> Option<&'static str> {
        match self {
            AioMode::Auto => None,
            AioMode::Threads => Some("threads"),
            AioMode::Native => Some("native"),
            AioMode::IoUring => Some("io_uring"),
        }
    }
}

/// Video adapter (`-vga`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VgaModel {
    /// QEMU default (std VGA / cirrus on old machines).
    #[default]
    Auto,
    Std,
    /// Paravirtual virtio-gpu (needs drivers; best perf on Windows w/ drivers).
    Virtio,
    Qxl,
    /// Minimal framebuffer for UEFI-only heads.
    Ramfb,
    /// No video at all (headless microvm style).
    None,
}

impl VgaModel {
    pub const ALL: &'static [VgaModel] = &[
        VgaModel::Auto,
        VgaModel::Std,
        VgaModel::Virtio,
        VgaModel::Qxl,
        VgaModel::Ramfb,
        VgaModel::None,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            VgaModel::Auto => "auto",
            VgaModel::Std => "std vga",
            VgaModel::Virtio => "virtio-gpu",
            VgaModel::Qxl => "qxl (spice era)",
            VgaModel::Ramfb => "ramfb (uefi fb)",
            VgaModel::None => "off",
        }
    }
}

/// Guest RTC base clock.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum RtcBase {
    #[default]
    Utc,
    /// Windows guests usually expect hardware clock set to local time.
    LocalTime,
}

impl RtcBase {
    pub const ALL: &'static [RtcBase] = &[RtcBase::Utc, RtcBase::LocalTime];

    pub fn label(&self) -> &'static str {
        match self {
            RtcBase::Utc => "utc (linux default)",
            RtcBase::LocalTime => "localtime (windows)",
        }
    }
}

/// First boot device.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum BootFirst {
    /// First hard disk (legacy letter `c`).
    #[default]
    Disk,
    /// First CD-ROM (`d`).
    Cd,
    /// Network PXE (`n`).
    Network,
}

impl BootFirst {
    pub const ALL: &'static [BootFirst] = &[BootFirst::Disk, BootFirst::Cd, BootFirst::Network];

    pub fn label(&self) -> &'static str {
        match self {
            BootFirst::Disk => "disk",
            BootFirst::Cd => "cdrom",
            BootFirst::Network => "network (pxe)",
        }
    }

    pub fn qemu_letter(&self) -> char {
        match self {
            BootFirst::Disk => 'c',
            BootFirst::Cd => 'd',
            BootFirst::Network => 'n',
        }
    }
}

/// Host audio backend used by `-audiodev`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AudioBackend {
    /// Emit no audio flags (current behavior: QEMU defaults apply).
    #[default]
    Off,
    /// Pick a backend based on the host OS and attach an Intel HDA card.
    Auto,
    Sdl,
    Spice,
    #[cfg(windows)]
    Wasapi,
    #[cfg(windows)]
    DirectSound,
    #[cfg(not(windows))]
    PulseAudio,
    #[cfg(not(windows))]
    Alsa,
}

impl AudioBackend {
    pub const ALL: &'static [AudioBackend] = &[
        AudioBackend::Off,
        AudioBackend::Auto,
        AudioBackend::Sdl,
        AudioBackend::Spice,
        #[cfg(windows)]
        AudioBackend::Wasapi,
        #[cfg(windows)]
        AudioBackend::DirectSound,
        #[cfg(not(windows))]
        AudioBackend::PulseAudio,
        #[cfg(not(windows))]
        AudioBackend::Alsa,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AudioBackend::Off => "off (no audio device)",
            AudioBackend::Auto => "auto (intel hda)",
            AudioBackend::Sdl => "sdl",
            AudioBackend::Spice => "spice",
            #[cfg(windows)]
            AudioBackend::Wasapi => "wasapi (windows)",
            #[cfg(windows)]
            AudioBackend::DirectSound => "dsound (windows)",
            #[cfg(not(windows))]
            AudioBackend::PulseAudio => "pulseaudio",
            #[cfg(not(windows))]
            AudioBackend::Alsa => "alsa",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FwdProto {
    #[default]
    Tcp,
    Udp,
}

impl FwdProto {
    pub fn qemu_str(&self) -> &'static str {
        match self {
            FwdProto::Tcp => "tcp",
            FwdProto::Udp => "udp",
        }
    }
}

/// Port forwarding rule for user-NET: `-nic user,hostfwd=tcp::2222-:22`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct HostFwd {
    pub proto: FwdProto,
    pub host_port: u16,
    pub guest_port: u16,
}

impl HostFwd {
    pub fn qemu_fragment(&self, bind: &str) -> String {
        format!(
            "hostfwd={p}:{b}:{hp}-:{gp}",
            p = self.proto.qemu_str(),
            b = bind,
            hp = self.host_port,
            gp = self.guest_port
        )
    }
}

/// Per-VM SSH connection target, remembered across sessions.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        SshConfig {
            host: "127.0.0.1".into(),
            port: 22,
            user: "root".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Disks
// ---------------------------------------------------------------------------

/// Tunables shared by every disk (draft + stored form).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DiskOpts {
    #[serde(default)]
    pub bus: DiskBus,
    #[serde(default)]
    pub cache: CacheMode,
    #[serde(default)]
    pub aio: AioMode,
    /// Pass TRIM/discard through to the host image (discard=unmap).
    #[serde(default)]
    pub discard_unmap: bool,
    /// Pin block I/O to a dedicated iothread.
    #[serde(default)]
    pub iothread: bool,
}

/// One attached disk of a VM (stored form).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiskEntry {
    /// Stable per-drive id inside the QEMU command line ("d0", "d1", ...).
    pub id: String,
    pub path: PathBuf,
    /// Size in GB; 0 when unknown (imported external disk).
    pub size_gb: u32,
    /// The disk was created by this manager and may be deleted with the VM.
    pub owned: bool,
    #[serde(flatten)]
    pub opts: DiskOpts,
}

/// Disk spec from the creation/edit form.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DiskSpec {
    New {
        size_gb: u32,
        folder: Option<String>,
        #[serde(flatten)]
        opts: DiskOpts,
    },
    Existing {
        path: String,
        #[serde(flatten)]
        opts: DiskOpts,
    },
}

impl DiskSpec {
    pub fn opts(&self) -> &DiskOpts {
        match self {
            DiskSpec::New { opts, .. } => opts,
            DiskSpec::Existing { opts, .. } => opts,
        }
    }
}

// ---------------------------------------------------------------------------
// VM
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Vm {
    pub id: String,
    pub name: String,
    pub memory_mb: u32,
    pub cpus: u32,

    // --- storage ---
    #[serde(default)]
    pub disks: Vec<DiskEntry>,
    /// ISO attached to the emulated CD-ROM.
    pub iso: Option<PathBuf>,

    // --- platform ---
    pub accel: Accel,
    pub display: DisplayMode,
    /// Preferred VNC display number (port 5900+N). The actual port is resolved via QMP.
    pub vnc_display: Option<u16>,
    #[serde(default)]
    pub machine: MachineType,
    #[serde(default)]
    pub firmware: Firmware,
    #[serde(default)]
    pub cpu: CpuModel,

    // --- performance ---
    /// SMP topology parts appended to `-smp` when set.
    #[serde(default)]
    pub smp_sockets: Option<u32>,
    #[serde(default)]
    pub smp_cores: Option<u32>,
    #[serde(default)]
    pub smp_threads: Option<u32>,
    #[serde(default)]
    pub maxcpus: Option<u32>,
    /// virtio-rng paravirtual entropy device.
    #[serde(default = "true_default")]
    pub rng: bool,
    /// USB tablet for pointer integration in VNC/GTK/SDL windows.
    #[serde(default = "true_default")]
    pub usb_tablet: bool,
    #[serde(default)]
    pub vga: VgaModel,
    /// VGA memory MB (std/qxl only).
    #[serde(default)]
    pub vgamem_mb: Option<u32>,
    /// VirGL 3D acceleration (Linux hosts; gtk/sdl displays only).
    #[serde(default)]
    pub gl: bool,
    /// Preallocate all guest memory at start (-mem-prealloc).
    #[serde(default)]
    pub mem_prealloc: bool,
    /// Memory backend file path, e.g. /dev/hugepages (huge pages).
    #[serde(default)]
    pub mem_path: Option<String>,

    // --- platform extras ---
    #[serde(default)]
    pub rtc: RtcBase,
    #[serde(default)]
    pub audio: AudioBackend,
    /// Require VNC password auth (set via QMP after start).
    #[serde(default)]
    pub vnc_password_auth: bool,

    // --- boot ---
    #[serde(default)]
    pub boot_first: BootFirst,
    #[serde(default)]
    pub boot_menu: bool,

    // --- network ---
    #[serde(default)]
    pub net_mode: NetMode,
    #[serde(default)]
    pub net_model: NetModel,
    #[serde(default)]
    pub mac_address: Option<String>,
    /// TAP interface name (bridged mode).
    #[serde(default)]
    pub tap_ifname: Option<String>,
    /// Host bridge to attach the TAP to (bridged mode).
    #[serde(default)]
    pub tap_br: Option<String>,
    #[serde(default)]
    pub hostfwd: Vec<HostFwd>,
    /// SSH target; older records get the default via serde.
    #[serde(default)]
    pub ssh: SshConfig,

    // --- legacy fields of pre-multi-disk records ---
    // Deserialized under their original JSON keys for compatibility, converted
    // into `disks` by normalize(), never serialized back.
    #[doc(hidden)]
    #[serde(default, skip_serializing, rename = "disk_path")]
    pub legacy_disk_path: Option<PathBuf>,
    #[doc(hidden)]
    #[serde(default, skip_serializing, rename = "disk_size_gb")]
    pub legacy_disk_size_gb: Option<u32>,
    #[doc(hidden)]
    #[serde(default, skip_serializing, rename = "disk_owned")]
    pub legacy_disk_owned: Option<bool>,
}

fn true_default() -> bool {
    true
}

impl Vm {
    /// Convert legacy single-disk fields into the `disks` list.
    /// Called after every deserialization (store load, tauri update payload).
    pub fn normalize(&mut self) {
        if self.disks.is_empty() {
            if let Some(p) = self.legacy_disk_path.take() {
                let size_gb = self.legacy_disk_size_gb.take().unwrap_or(0);
                let owned = self.legacy_disk_owned.take().unwrap_or(true);
                self.disks.push(DiskEntry {
                    id: "d0".into(),
                    path: p,
                    size_gb,
                    owned,
                    opts: DiskOpts::default(),
                });
            }
        } else {
            self.legacy_disk_path = None;
            self.legacy_disk_size_gb = None;
            self.legacy_disk_owned = None;
        }
        for (i, d) in self.disks.iter_mut().enumerate() {
            if d.id.is_empty() {
                d.id = format!("d{i}");
            }
        }
    }

    /// Normalize + validate everything that does not depend on the host FS layout.
    pub fn validate(&mut self) -> Result<()> {
        self.normalize();
        validate::vm_name(&self.name)?;
        if self.memory_mb == 0 {
            bail!("Memory (MB) must be a number > 0");
        }
        if self.cpus == 0 || self.cpus > 512 {
            bail!("vCPU count must be between 1 and 512");
        }
        if self.disks.is_empty() {
            bail!("A VM needs at least one disk");
        }
        if self.display == DisplayMode::Vnc && !self.machine.supports_vnc() {
            bail!("microvm does not support a VNC display. Pick another platform.");
        }

        // disks
        let mut seen = std::collections::HashSet::new();
        for d in &self.disks {
            if !seen.insert(d.path.clone()) {
                bail!("Duplicate disk path: {}", d.path.display());
            }
            if matches!(self.machine, MachineType::MicroVm) && d.opts.bus != DiskBus::VirtioBlk {
                bail!("microvm supports only the virtio-blk disk bus");
            }
            if d.opts.bus == DiskBus::Sata && !matches!(self.machine, MachineType::Q35) {
                bail!("The sata/ahci bus requires the q35 machine type");
            }
            if d.opts.aio == AioMode::IoUring && !cfg!(target_os = "linux") {
                bail!("aio=io_uring is only available on Linux hosts");
            }
        }

        // video
        if let Some(mb) = self.vgamem_mb {
            if !matches!(self.vga, VgaModel::Std | VgaModel::Qxl) {
                bail!("VGA memory can only be tuned for std or qxl video models");
            }
            if mb == 0 || mb > 512 || !mb.is_power_of_two() {
                bail!("VGA memory must be a power of two between 16 and 512 MB");
            }
        }
        if self.gl {
            if !matches!(self.display, DisplayMode::Gtk | DisplayMode::Sdl) {
                bail!("3D acceleration (virgl) requires the GTK or SDL display");
            }
            if matches!(self.machine, MachineType::MicroVm) {
                bail!("3D acceleration is not supported on microvm");
            }
        }
        if self.vga == VgaModel::Ramfb && self.firmware != Firmware::Uefi {
            bail!("ramfb video makes sense only together with UEFI firmware");
        }

        // network
        if self.net_mode != NetMode::Nat && !self.hostfwd.is_empty() {
            bail!("Port forwarding is available only for nat (user) networking");
        }
        if self.net_mode != NetMode::Bridged
            && (self.tap_ifname.as_deref().map(str::trim).is_some()
                || self.tap_br.as_deref().map(str::trim).is_some())
        {
            bail!("TAP interface/bridge options apply only to bridged networking");
        }
        let mut fwd_seen = std::collections::HashSet::new();
        for hf in &self.hostfwd {
            if hf.host_port == 0 || hf.guest_port == 0 {
                bail!("Forwarding ports must be greater than 0");
            }
            if !fwd_seen.insert((hf.proto, hf.host_port)) {
                bail!(
                    "Duplicate hostfwd rule: {} port {}",
                    hf.proto.qemu_str(),
                    hf.host_port
                );
            }
        }
        if let Some(mac) = &self.mac_address {
            Self::validate_mac(mac)?;
        }

        // memory backend
        if let Some(mp) = &self.mem_path {
            if mp.trim().is_empty() {
                bail!("Memory path must not be empty");
            }
        }
        Ok(())
    }

    pub fn validate_mac(mac: &str) -> Result<()> {
        let parts: Vec<&str> = mac.split(':').collect();
        let ok = parts.len() == 6
            && parts
                .iter()
                .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
        if !ok {
            bail!("MAC address must look like 52:54:00:12:34:56");
        }
        Ok(())
    }

    /// Deterministic MAC derived from the VM id, so guests keep one address
    /// across reboots unless an explicit one is configured.
    pub fn effective_mac(&self) -> String {
        if let Some(m) = &self.mac_address {
            return m.clone();
        }
        let h = md5ish_hash(self.id.as_bytes());
        format!("52:54:{:02x}:{:02x}:{:02x}:{:02x}", h[0], h[1], h[2], h[3])
    }

    /// First disk path (kept for diagnostics/UI summaries).
    pub fn primary_disk(&self) -> Option<&DiskEntry> {
        self.disks.first()
    }
}

/// Small deterministic hash (FNV-ish) used for MAC generation — not security.
fn md5ish_hash(bytes: &[u8]) -> [u8; 4] {
    let mut a: u32 = 0x811c9dc5;
    let mut b: u32 = 0x01000193;
    for (i, byte) in bytes.iter().enumerate() {
        a ^= (*byte as u32).wrapping_mul(i as u32 + 7);
        a = a.rotate_left(5).wrapping_add(b);
        b = b.wrapping_mul(31).wrapping_add(*byte as u32);
    }
    a ^= b >> 3;
    b = b.wrapping_add(a << 7);
    [a as u8, (a >> 8) as u8, b as u8, (b >> 8) as u8]
}

// ---------------------------------------------------------------------------
// Draft (create form)
// ---------------------------------------------------------------------------

/// VM creation form data before validation.
#[derive(Deserialize, Clone, Debug)]
pub struct VmDraft {
    pub name: String,
    pub memory_mb: u32,
    pub cpus: u32,
    pub iso: Option<String>,
    pub accel: Accel,
    pub display: DisplayMode,
    pub machine: MachineType,
    pub firmware: Firmware,
    pub cpu: CpuModel,
    pub net_mode: NetMode,
    pub net_model: NetModel,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub tap_ifname: Option<String>,
    #[serde(default)]
    pub tap_br: Option<String>,
    #[serde(default)]
    pub hostfwd: Vec<HostFwd>,
    #[serde(default)]
    pub ssh: SshConfig,
    #[serde(default)]
    pub boot_first: BootFirst,
    #[serde(default)]
    pub boot_menu: bool,
    #[serde(default)]
    pub smp_sockets: Option<u32>,
    #[serde(default)]
    pub smp_cores: Option<u32>,
    #[serde(default)]
    pub smp_threads: Option<u32>,
    #[serde(default)]
    pub maxcpus: Option<u32>,
    #[serde(default = "true_default")]
    pub rng: bool,
    #[serde(default = "true_default")]
    pub usb_tablet: bool,
    #[serde(default)]
    pub vga: VgaModel,
    #[serde(default)]
    pub vgamem_mb: Option<u32>,
    #[serde(default)]
    pub gl: bool,
    #[serde(default)]
    pub mem_prealloc: bool,
    #[serde(default)]
    pub mem_path: Option<String>,
    #[serde(default)]
    pub rtc: RtcBase,
    #[serde(default)]
    pub audio: AudioBackend,
    #[serde(default)]
    pub vnc_password_auth: bool,
    /// At least one entry required.
    #[serde(default)]
    pub disks: Vec<DiskSpec>,
}

/// Validated draft used to build a VM.
#[derive(Clone, Debug)]
pub struct ValidatedDraft {
    pub name: String,
    pub memory_mb: u32,
    pub cpus: u32,
    pub iso: Option<PathBuf>,
    pub hostfwd: Vec<HostFwd>,
    pub ssh: SshConfig,
    pub disks: Vec<DiskSpec>,
}

impl VmDraft {
    /// Form field validation. Errors are human-readable and shown in the UI.
    pub fn validate(&self) -> Result<ValidatedDraft> {
        let name = validate::vm_name(&self.name)?;
        if self.memory_mb == 0 {
            bail!("Memory (MB) must be a number > 0");
        }
        if self.cpus == 0 || self.cpus > 512 {
            bail!("vCPU count must be between 1 and 512");
        }
        if self.display == DisplayMode::Vnc && !self.machine.supports_vnc() {
            bail!("microvm does not support a VNC display. Pick another platform.");
        }
        if self.net_mode != NetMode::Nat && !self.hostfwd.is_empty() {
            bail!("Port forwarding is available only for nat (user) networking");
        }
        if self.net_mode != NetMode::Bridged
            && (self.tap_ifname.as_deref().map(str::trim).is_some()
                || self.tap_br.as_deref().map(str::trim).is_some())
        {
            bail!("TAP interface/bridge options apply only to bridged networking");
        }
        let mut seen = std::collections::HashSet::new();
        for hf in &self.hostfwd {
            if hf.host_port == 0 || hf.guest_port == 0 {
                bail!("Forwarding ports must be greater than 0");
            }
            if !seen.insert((hf.proto, hf.host_port)) {
                bail!(
                    "Duplicate hostfwd rule: {} port {}",
                    hf.proto.qemu_str(),
                    hf.host_port
                );
            }
        }
        if let Some(mac) = &self.mac_address {
            if !mac.trim().is_empty() {
                Vm::validate_mac(mac)?;
            }
        }
        if self.gl && !matches!(self.display, DisplayMode::Gtk | DisplayMode::Sdl) {
            bail!("3D acceleration (virgl) requires the GTK or SDL display");
        }
        if self.gl && matches!(self.machine, MachineType::MicroVm) {
            bail!("3D acceleration is not supported on microvm");
        }
        if let Some(mb) = self.vgamem_mb {
            if !matches!(self.vga, VgaModel::Std | VgaModel::Qxl) {
                bail!("VGA memory can only be tuned for std or qxl video models");
            }
            if mb == 0 || mb > 512 || !mb.is_power_of_two() {
                bail!("VGA memory must be a power of two between 16 and 512 MB");
            }
        }
        if self.vga == VgaModel::Ramfb && self.firmware != Firmware::Uefi {
            bail!("ramfb video makes sense only together with UEFI firmware");
        }
        for spec in &self.disks {
            if matches!(self.machine, MachineType::MicroVm) && spec.opts().bus != DiskBus::VirtioBlk
            {
                bail!("microvm supports only the virtio-blk disk bus");
            }
            if spec.opts().bus == DiskBus::Sata && !matches!(self.machine, MachineType::Q35) {
                bail!("The sata/ahci bus requires the q35 machine type");
            }
            if spec.opts().aio == AioMode::IoUring && !cfg!(target_os = "linux") {
                bail!("aio=io_uring is only available on Linux hosts");
            }
            if let DiskSpec::Existing { path, .. } = spec {
                if PathBuf::from(path.trim()).extension() != Some(std::ffi::OsStr::new("qcow2"))
                    && PathBuf::from(path.trim()).extension() != Some(std::ffi::OsStr::new("qcow"))
                {
                    // extension check is advisory here; existence is checked in create()
                }
            }
        }
        if self.disks.is_empty() {
            bail!("Add at least one disk to the VM");
        }

        let iso = match self.iso.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(p) => {
                let path = PathBuf::from(p);
                if !path.is_file() {
                    bail!("ISO not found: {}", path.display());
                }
                Some(path)
            }
            None => None,
        };
        Ok(ValidatedDraft {
            name: name.to_string(),
            memory_mb: self.memory_mb,
            cpus: self.cpus,
            iso,
            hostfwd: self.hostfwd.clone(),
            ssh: self.ssh.clone(),
            disks: self.disks.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vm() -> Vm {
        let mut vm = Vm {
            id: "test".into(),
            name: "demo".into(),
            memory_mb: 2048,
            cpus: 2,
            disks: vec![DiskEntry {
                id: "d0".into(),
                path: PathBuf::from("/tmp/demo.qcow2"),
                size_gb: 20,
                owned: true,
                opts: DiskOpts::default(),
            }],
            iso: Some(PathBuf::from("/tmp/boot.iso")),
            accel: Accel::Auto,
            display: DisplayMode::Vnc,
            vnc_display: Some(3),
            machine: MachineType::Auto,
            firmware: Firmware::Bios,
            cpu: CpuModel::Auto,
            smp_sockets: None,
            smp_cores: None,
            smp_threads: None,
            maxcpus: None,
            rng: true,
            usb_tablet: true,
            vga: VgaModel::Auto,
            vgamem_mb: None,
            gl: false,
            mem_prealloc: false,
            mem_path: None,
            rtc: RtcBase::Utc,
            audio: AudioBackend::Off,
            vnc_password_auth: false,
            boot_first: BootFirst::Disk,
            boot_menu: false,
            net_mode: NetMode::Nat,
            net_model: NetModel::Auto,
            mac_address: None,
            tap_ifname: None,
            tap_br: None,
            hostfwd: vec![],
            ssh: SshConfig::default(),
            legacy_disk_path: None,
            legacy_disk_size_gb: None,
            legacy_disk_owned: None,
        };
        vm.normalize();
        vm
    }

    fn legacy_json() -> String {
        r#"{
            "id": "vmabc", "name": "old", "memory_mb": 1024, "cpus": 1,
            "disk_size_gb": 20, "disk_path": "/disks/old.qcow2", "disk_owned": true,
            "iso": null, "accel": "auto", "display": "vnc", "vnc_display": null
        }"#
        .to_string()
    }

    #[test]
    fn legacy_record_becomes_multi_disk() {
        let mut vm: Vm = serde_json::from_str(&legacy_json()).unwrap();
        vm.normalize();
        assert_eq!(vm.disks.len(), 1);
        assert_eq!(vm.disks[0].path, PathBuf::from("/disks/old.qcow2"));
        assert_eq!(vm.disks[0].size_gb, 20);
        assert!(vm.disks[0].owned);
        assert_eq!(vm.disks[0].id, "d0");
        assert_eq!(vm.disks[0].opts.bus, DiskBus::VirtioBlk);

        // Round trip keeps canonical form and drops legacy fields.
        let s = serde_json::to_string(&vm).unwrap();
        assert!(!s.contains("disk_path"));
        let mut rt: Vm = serde_json::from_str(&s).unwrap();
        rt.normalize();
        assert_eq!(rt.disks.len(), 1);
        assert_eq!(rt.disks[0].path, vm.disks[0].path);
    }

    #[test]
    fn validation_rejects_bad_combos() {
        let mut vm = sample_vm();
        vm.name = "".into();
        assert!(vm.validate().is_err());

        let mut vm = sample_vm();
        vm.machine = MachineType::MicroVm;
        vm.disks[0].opts.bus = DiskBus::Nvme;
        assert!(vm.validate().is_err());

        let mut vm = sample_vm();
        vm.machine = MachineType::I440Fx;
        vm.disks[0].opts.bus = DiskBus::Sata;
        assert!(vm.validate().is_err());

        let mut vm = sample_vm();
        vm.gl = true;
        assert!(vm.validate().is_err()); // gl needs gtk/sdl

        let mut vm = sample_vm();
        vm.mac_address = Some("zz".into());
        assert!(vm.validate().is_err());
    }

    #[test]
    fn valid_vm_passes() {
        let mut vm = sample_vm();
        vm.validate().unwrap();
    }

    #[test]
    fn deterministic_mac_is_stable_and_valid() {
        let vm = sample_vm();
        let m1 = vm.effective_mac();
        let m2 = vm.effective_mac();
        assert_eq!(m1, m2);
        Vm::validate_mac(&m1).unwrap();
        assert!(m1.starts_with("52:54:"));
    }

    #[test]
    fn draft_requires_at_least_one_disk() {
        let d = VmDraft {
            name: "x".into(),
            memory_mb: 256,
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
            disks: vec![],
        };
        assert!(d.validate().is_err());
    }

    #[test]
    fn draft_accepts_flattened_disk_opts() {
        let json = r#"{
            "name":"x","memory_mb":256,"cpus":1,"iso":null,
            "accel":"auto","display":"none","machine":"q35","firmware":"bios",
            "cpu":"auto","net_mode":"nat","net_model":"auto",
            "disks":[{"kind":"new","size_gb":10,"folder":null,
                      "bus":"nvme","cache":"none","aio":"threads",
                      "discard_unmap":true,"iothread":true}]
        }"#;
        let d: VmDraft = serde_json::from_str(json).unwrap();
        let v = d.validate().unwrap();
        assert_eq!(v.disks[0].opts().bus, DiskBus::Nvme);
        assert_eq!(v.disks[0].opts().cache, CacheMode::None);
        assert!(v.disks[0].opts().discard_unmap);
        assert!(v.disks[0].opts().iothread);
    }

    #[test]
    fn vgamem_rules() {
        let mut vm = sample_vm();
        vm.vga = VgaModel::Virtio;
        vm.vgamem_mb = Some(64);
        assert!(vm.validate().is_err());
        vm.vga = VgaModel::Std;
        vm.validate().unwrap();
        vm.vgamem_mb = Some(48);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn ramfb_needs_uefi() {
        let mut vm = sample_vm();
        vm.firmware = Firmware::Bios;
        vm.vga = VgaModel::Ramfb;
        assert!(vm.validate().is_err());
        vm.firmware = Firmware::Uefi;
        vm.validate().unwrap();
    }
}
