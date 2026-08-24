use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

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
    pub const ALL: &'static [Accel] = &[Accel::Auto, Accel::Kvm, Accel::Whpx, Accel::Tcg, Accel::None];

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
    pub const ALL: &'static [DisplayMode] =
        &[DisplayMode::None, DisplayMode::Vnc, DisplayMode::Gtk, DisplayMode::Sdl];

    pub fn label(&self) -> &'static str {
        match self {
            DisplayMode::None => "none (headless)",
            DisplayMode::Vnc => "vnc (remote)",
            DisplayMode::Gtk => "gtk (window)",
            DisplayMode::Sdl => "sdl (window)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum MachineType {
    /// Не передавать `-machine` (дефолт QEMU).
    #[default]
    Auto,
    /// Современный чипсет ICH9: PCIe, AHCI, UEFI, virtio.
    Q35,
    /// Легаси-чипсет PIIX (только для старых гостей).
    I440Fx,
    /// Минималистичная платформа (только headless/virtio).
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
            MachineType::Auto => "авто (дефолт QEMU)",
            MachineType::Q35 => "q35 (современный)",
            MachineType::I440Fx => "i440fx (легаси)",
            MachineType::MicroVm => "microvm (мини)",
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
            CpuModel::Auto => "авто",
            CpuModel::Max => "max",
            CpuModel::Host => "host",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    /// Пользовательская сеть SLIRP (NAT через хост, без прав администратора).
    #[default]
    Nat,
    /// Мост в локальную сеть через TAP-адаптер хоста.
    Bridged,
    /// Сетевая карта не добавляется.
    None,
}

impl NetMode {
    pub const ALL: &'static [NetMode] = &[NetMode::Nat, NetMode::Bridged, NetMode::None];

    pub fn label(&self) -> &'static str {
        match self {
            NetMode::Nat => "nat (user)",
            NetMode::Bridged => "мост (tap)",
            NetMode::None => "нет",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetModel {
    /// Без `model=` — дефолт QEMU (e1000), сохраняет старое поведение.
    #[default]
    Auto,
    /// Паравиртуальная virtio-сеть (Linux).
    Virtio,
    /// Intel e1000 (Windows без драйверов virtio).
    E1000,
    /// Realtek rtl8139 (старые гости).
    Rtl8139,
}

impl NetModel {
    pub const ALL: &'static [NetModel] =
        &[NetModel::Auto, NetModel::Virtio, NetModel::E1000, NetModel::Rtl8139];

    pub fn label(&self) -> &'static str {
        match self {
            NetModel::Auto => "авто (e1000)",
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

/// Правило проброса порта для user-NET: `-nic user,hostfwd=tcp::2222-:22`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct HostFwd {
    pub proto: FwdProto,
    pub host_port: u16,
    pub guest_port: u16,
}

impl HostFwd {
    pub fn qemu_fragment(&self) -> String {
        format!("hostfwd={p}::{hp}-:{gp}", p = self.proto.qemu_str(), hp = self.host_port, gp = self.guest_port)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Vm {
    pub id: String,
    pub name: String,
    pub memory_mb: u32,
    pub cpus: u32,
    pub disk_size_gb: u32,
    pub disk_path: PathBuf,
    /// Диск создан этим менеджером и его можно удалять вместе с ВМ.
    /// У записей до v0.2 поле отсутствует — для совместимости считается true.
    #[serde(default = "true_default")]
    pub disk_owned: bool,
    pub iso: Option<PathBuf>,
    pub accel: Accel,
    pub display: DisplayMode,
    /// Предпочитаемый номер VNC-дисплея (порт 5900+N). Реальный порт уточняется через QMP.
    pub vnc_display: Option<u16>,
    #[serde(default)]
    pub machine: MachineType,
    #[serde(default)]
    pub firmware: Firmware,
    #[serde(default)]
    pub cpu: CpuModel,
    #[serde(default)]
    pub net_mode: NetMode,
    #[serde(default)]
    pub net_model: NetModel,
    #[serde(default)]
    pub hostfwd: Vec<HostFwd>,
}

fn true_default() -> bool {
    true
}

impl Vm {
    pub fn pidfile_path(&self) -> PathBuf {
        self.disk_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{}.pid", self.id))
    }
}

/// Данные формы создания ВМ до валидации.
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
    pub hostfwd: Vec<HostFwd>,
    pub disk: DiskSpec,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DiskSpec {
    New { size_gb: u32, folder: Option<String> },
    Existing { path: String },
}

/// Проверенная часть драфта, из которой собирается ВМ.
#[derive(Clone, Debug)]
pub struct ValidatedDraft {
    pub name: String,
    pub memory_mb: u32,
    pub cpus: u32,
    pub iso: Option<PathBuf>,
    pub hostfwd: Vec<HostFwd>,
    pub disk: DiskSpec,
}

impl VmDraft {
    /// Валидация полей формы. Ошибки — человекочитаемые, на русском (показываются в UI).
    pub fn validate(&self) -> Result<ValidatedDraft> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            bail!("Имя не может быть пустым");
        }
        if self.memory_mb == 0 {
            bail!("Память (МБ) должна быть числом > 0");
        }
        if self.cpus == 0 {
            bail!("Число vCPU должно быть > 0");
        }
        if self.display == DisplayMode::Vnc && !self.machine.supports_vnc() {
            bail!("microvm не поддерживает VNC-дисплей. Выберите другую платформу.");
        }
        if self.net_mode != NetMode::Nat && !self.hostfwd.is_empty() {
            bail!("Проброс портов доступен только для сети nat (user)");
        }
        let mut seen = std::collections::HashSet::new();
        for hf in &self.hostfwd {
            if hf.host_port == 0 || hf.guest_port == 0 {
                bail!("Порты проброса должны быть больше 0");
            }
            if !seen.insert((hf.proto, hf.host_port)) {
                bail!("Дублирующийся hostfwd: {} порт {}", hf.proto.qemu_str(), hf.host_port);
            }
        }

        let iso = match self.iso.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(p) => {
                let path = PathBuf::from(p);
                if !path.is_file() {
                    bail!("ISO не найден: {}", path.display());
                }
                Some(path)
            }
            None => None,
        };

        Ok(ValidatedDraft {
            name,
            memory_mb: self.memory_mb,
            cpus: self.cpus,
            iso,
            hostfwd: self.hostfwd.clone(),
            disk: self.disk.clone(),
        })
    }
}
