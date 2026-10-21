import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  ConsoleInfo,
  DeleteReport,
  EnvProbe,
  RunningInfo,
  SnapInfo,
  SnapOp,
  SshConfig,
  Vm,
  VmDraft,
  VmListItem,
  VmUpdate,
} from "./types";

export type VmActionName =
  | "pause"
  | "resume"
  | "reset"
  | "shutdown"
  | "suspend"
  | "wakeup"
  | "nmi";

export const api = {
  listVms: () => invoke<VmListItem[]>("list_vms"),
  getStatuses: () => invoke<VmUpdate[]>("get_statuses"),

  createVm: (draft: VmDraft) => invoke<Vm>("create_vm", { draft }),
  updateVm: (vm: Vm) => invoke<Vm>("update_vm", { vm }),
  deleteVm: (id: string, deleteDisk: boolean) =>
    invoke<DeleteReport>("delete_vm", { id, deleteDisk }),

  startVm: (id: string) => invoke<RunningInfo>("start_vm", { id }),
  vmAction: (id: string, action: VmActionName) =>
    invoke<void>("vm_action", { id, action }),
  forceStop: (id: string) => invoke<void>("force_stop", { id }),

  sendKey: (id: string, keys: string) => invoke<void>("vm_sendkey", { id, keys }),
  screendump: (id: string) => invoke<string>("vm_screendump", { id }),
  setBalloon: (id: string, mb: number) => invoke<void>("vm_set_balloon", { id, mb }),
  changeIso: (id: string, iso: string | null) =>
    invoke<void>("vm_change_iso", { id, iso }),
  setVncPassword: (id: string, password: string) =>
    invoke<void>("vm_set_vnc_password", { id, password }),

  // saved state (migration to file)
  hasSavedState: (id: string) => invoke<boolean>("has_saved_state", { id }),
  discardSavedState: (id: string) => invoke<void>("discard_saved_state", { id }),
  saveVmState: (id: string) => invoke<void>("save_vm_state", { id }),

  openConsole: (id: string) => invoke<ConsoleInfo>("open_console", { id }),
  closeConsole: (id: string) => invoke<void>("close_console", { id }),

  sshConnect: (id: string) => invoke<SshConfig>("ssh_connect", { id }),

  readLog: (id: string, lines?: number) =>
    invoke<string>("read_log", { id, lines: lines ?? null }),

  snapshotOp: (
    id: string,
    op: SnapOp,
    name?: string,
    diskId?: string
  ) =>
    invoke<SnapInfo[]>("snapshot_op", {
      id,
      op,
      name: name ?? null,
      diskId: diskId ?? null,
    }),
  snapshotList: (id: string, diskId?: string) => api.snapshotOp(id, "list", undefined, diskId),
  snapshotCreate: (id: string, name: string, diskId?: string) =>
    api.snapshotOp(id, "create", name, diskId),
  snapshotApply: (id: string, name: string, diskId?: string) =>
    api.snapshotOp(id, "apply", name, diskId),
  snapshotDelete: (id: string, name: string, diskId?: string) =>
    api.snapshotOp(id, "delete", name, diskId),

  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (cfg: AppConfig) => invoke<void>("set_config", { cfg }),
  getConfigWarnings: () => invoke<string[]>("get_config_warnings"),
  probeEnv: () => invoke<EnvProbe>("probe_env"),

  pickPath: (kind: "iso" | "qcow2" | "folder" | "any") =>
    invoke<string | null>("pick_path", { kind }),
};

export function humanMb(mb: number): string {
  if (mb >= 1024 && mb % 1024 === 0) return `${mb / 1024} GB`;
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

export function humanBytes(b: number): string {
  if (!b) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log2(b) / 10));
  const v = b / 2 ** (10 * i);
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export function uptimeStr(startedAt: number | null): string {
  if (!startedAt) return "—";
  const now = Math.floor(Date.now() / 1000);
  const s = Math.max(0, now - startedAt);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

export function cleanErr(e: unknown): string {
  return String(e).replace(/^[^:]*Error:\s*/, "").trim();
}
