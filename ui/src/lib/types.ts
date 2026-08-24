// Types mirrored from easy-qemu-core DTOs.

export type Accel = "auto" | "kvm" | "whpx" | "tcg" | "none";
export type DisplayMode = "none" | "vnc" | "gtk" | "sdl";
export type MachineType = "auto" | "q35" | "i440fx" | "microvm";
export type Firmware = "bios" | "uefi";
export type CpuModel = "auto" | "max" | "host";
export type NetMode = "nat" | "bridged" | "none";
export type NetModel = "auto" | "virtio" | "e1000" | "rtl8139";
export type FwdProto = "tcp" | "udp";
export type VmStatus = "running" | "paused" | "shutoff" | "unknown";

export interface HostFwd {
  proto: FwdProto;
  host_port: number;
  guest_port: number;
}

export interface Vm {
  id: string;
  name: string;
  memory_mb: number;
  cpus: number;
  disk_size_gb: number;
  disk_path: string;
  disk_owned: boolean;
  iso: string | null;
  accel: Accel;
  display: DisplayMode;
  vnc_display: number | null;
  machine: MachineType;
  firmware: Firmware;
  cpu: CpuModel;
  net_mode: NetMode;
  net_model: NetModel;
  hostfwd: HostFwd[];
}

export interface VmListItem extends Vm {
  status: VmStatus;
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

export type DiskSpec =
  | { kind: "new"; size_gb: number; folder: string | null }
  | { kind: "existing"; path: string };

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
  hostfwd: HostFwd[];
  disk: DiskSpec;
}

export interface AppConfig {
  qemu_binary: string | null;
  qemu_img: string | null;
  storage_dir: string | null;
  vnc_bind: string | null;
  theme: string | null;
}

export interface DeleteReport {
  json_removed: boolean;
  pidfile_removed: boolean;
  disk_attempted: boolean;
  disk_deleted: boolean;
  errors: string[];
}
