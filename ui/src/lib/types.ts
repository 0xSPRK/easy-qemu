// Types mirrored from easy-qemu-core DTOs.

export type Accel = "auto" | "kvm" | "whpx" | "tcg" | "none";
export type DisplayMode = "none" | "vnc" | "gtk" | "sdl";
export type MachineType = "auto" | "q35" | "i440fx" | "microvm";
export type Firmware = "bios" | "uefi";
export type CpuModel = "auto" | "max" | "host";
export type NetMode = "nat" | "bridged" | "none";
export type NetModel = "auto" | "virtio" | "e1000" | "rtl8139";
export type FwdProto = "tcp" | "udp";
export type DiskBus = "virtio-blk" | "nvme" | "sata" | "ide";
export type CacheMode = "auto" | "none" | "writeback" | "writethrough" | "unsafe";
export type AioMode = "auto" | "threads" | "native" | "io-uring";
export type VgaModel = "auto" | "std" | "virtio" | "qxl" | "ramfb" | "none";
export type RtcBase = "utc" | "localtime";
export type BootFirst = "disk" | "cd" | "network";
/** Platform-specific variants are filtered in the UI at runtime. */
export type AudioBackend =
  | "off"
  | "auto"
  | "sdl"
  | "spice"
  | "wasapi"
  | "direct-sound"
  | "pulseaudio"
  | "alsa";
export type VmStatus = "running" | "paused" | "shutoff" | "unknown";

export interface HostFwd {
  proto: FwdProto;
  host_port: number;
  guest_port: number;
}

export interface SshConfig {
  host: string;
  port: number;
  user: string;
}

export const DEFAULT_SSH: SshConfig = { host: "127.0.0.1", port: 22, user: "root" };

export interface DiskOpts {
  bus: DiskBus;
  cache: CacheMode;
  aio: AioMode;
  discard_unmap: boolean;
  iothread: boolean;
}

export const DEFAULT_DISK_OPTS: DiskOpts = {
  bus: "virtio-blk",
  cache: "auto",
  aio: "auto",
  discard_unmap: false,
  iothread: false,
};

export interface DiskEntry {
  id: string;
  path: string;
  size_gb: number;
  owned: boolean;
  opts: DiskOpts;
}

export type DiskSpec =
  | ({ kind: "new"; size_gb: number; folder: string | null } & DiskOpts)
  | ({ kind: "existing"; path: string } & DiskOpts);

export interface Vm {
  id: string;
  name: string;
  memory_mb: number;
  cpus: number;
  disks: DiskEntry[];
  iso: string | null;
  accel: Accel;
  display: DisplayMode;
  vnc_display: number | null;
  machine: MachineType;
  firmware: Firmware;
  cpu: CpuModel;
  smp_sockets: number | null;
  smp_cores: number | null;
  smp_threads: number | null;
  maxcpus: number | null;
  rng: boolean;
  usb_tablet: boolean;
  vga: VgaModel;
  vgamem_mb: number | null;
  gl: boolean;
  mem_prealloc: boolean;
  mem_path: string | null;
  rtc: RtcBase;
  audio: AudioBackend;
  vnc_password_auth: boolean;
  boot_first: BootFirst;
  boot_menu: boolean;
  net_mode: NetMode;
  net_model: NetModel;
  mac_address: string | null;
  tap_ifname: string | null;
  tap_br: string | null;
  hostfwd: HostFwd[];
  ssh: SshConfig;
}

export interface VmListItem extends Vm {
  status: VmStatus;
}

export interface DiskStat {
  id: string;
  path: string;
  rd_bytes: number;
  wr_bytes: number;
  rd_ops: number;
  wr_ops: number;
}

export interface VmStats {
  cpu_pct: number | null;
  balloon_mb: number | null;
  disks: DiskStat[];
}

export interface VmUpdate {
  id: string;
  name: string;
  status: VmStatus;
  pid: number | null;
  qmp_port: number | null;
  vnc_port: number | null;
  console_port: number | null;
  started_at: number | null;
  stats?: VmStats | null;
}

export interface ConsoleInfo {
  ws_port: number;
  vnc_port: number;
}

export interface RunningInfo {
  id: string;
  pid: number;
  qmp_port: number;
  vnc_display: number | null;
  vnc_port: number | null;
  display: DisplayMode;
  started_at: number | null;
}

export interface SnapInfo {
  name: string;
  tag: string | null;
  date_time: string | null;
}

export interface VmDraft {
  name: string;
  memory_mb: number;
  cpus: number;
  iso: string | null;
  accel: Accel;
  display: DisplayMode;
  machine: MachineType;
  firmware: Firmware;
  cpu: CpuModel;
  net_mode: NetMode;
  net_model: NetModel;
  mac_address: string | null;
  tap_ifname: string | null;
  tap_br: string | null;
  hostfwd: HostFwd[];
  ssh: SshConfig;
  boot_first: BootFirst;
  boot_menu: boolean;
  smp_sockets: number | null;
  smp_cores: number | null;
  smp_threads: number | null;
  maxcpus: number | null;
  rng: boolean;
  usb_tablet: boolean;
  vga: VgaModel;
  vgamem_mb: number | null;
  gl: boolean;
  mem_prealloc: boolean;
  mem_path: string | null;
  rtc: RtcBase;
  audio: AudioBackend;
  vnc_password_auth: boolean;
  disks: DiskSpec[];
}

export interface AppConfig {
  qemu_binary: string | null;
  qemu_img: string | null;
  storage_dir: string | null;
  vnc_bind: string | null;
  hostfwd_bind: string | null;
  theme: string | null;
}

export interface EnvProbe {
  qemu_system: string | null;
  qemu_img: string | null;
  accels: string[];
}

export interface DeleteReport {
  json_removed: boolean;
  pidfile_removed: boolean;
  vars_removed: boolean;
  log_removed: boolean;
  sav_removed: boolean;
  disk_attempted: boolean;
  disks_deleted: number;
  errors: string[];
}

export type SnapOp =
  | "list"
  | "create"
  | "apply"
  | "delete"
  | "live_list"
  | "live_create"
  | "live_apply"
  | "live_delete";

export function isWindows(): boolean {
  return navigator.platform.toLowerCase().includes("win") ||
    navigator.userAgent.toLowerCase().includes("windows");
}

export function audioBackends(): { value: AudioBackend; label: string }[] {
  const base: { value: AudioBackend; label: string }[] = [
    { value: "off", label: "off (no audio device)" },
    { value: "auto", label: "auto (intel hda)" },
    { value: "sdl", label: "sdl" },
    { value: "spice", label: "spice" },
  ];
  if (isWindows()) {
    return [...base,
      { value: "wasapi", label: "wasapi (windows)" },
      { value: "direct-sound", label: "dsound (windows)" }];
  }
  return [...base,
    { value: "pulseaudio", label: "pulseaudio" },
    { value: "alsa", label: "alsa" }];
}
