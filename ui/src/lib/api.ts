import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  ConsoleInfo,
  DeleteReport,
  RunningInfo,
  SnapInfo,
  Vm,
  VmDraft,
  VmListItem,
  VmUpdate,
} from "./types";

export const api = {
  listVms: () => invoke<VmListItem[]>("list_vms"),
  getStatuses: () => invoke<VmUpdate[]>("get_statuses"),

  createVm: (draft: VmDraft) => invoke<Vm>("create_vm", { draft }),
  updateVm: (vm: Vm) => invoke<Vm>("update_vm", { vm }),
  deleteVm: (id: string, deleteDisk: boolean) =>
    invoke<DeleteReport>("delete_vm", { id, deleteDisk }),

  startVm: (id: string) => invoke<RunningInfo>("start_vm", { id }),
  vmAction: (id: string, action: "pause" | "resume" | "reset" | "shutdown") =>
    invoke<void>("vm_action", { id, action }),
  forceStop: (id: string) => invoke<void>("force_stop", { id }),

  openConsole: (id: string) => invoke<ConsoleInfo>("open_console", { id }),
  closeConsole: (id: string) => invoke<void>("close_console", { id }),

  readLog: (id: string, lines?: number) =>
    invoke<string>("read_log", { id, lines: lines ?? null }),

  snapshotList: (id: string) => invoke<SnapInfo[]>("snapshot_list", { id }),
  snapshotCreate: (id: string, name: string) =>
    invoke<void>("snapshot_create", { id, name }),
  snapshotApply: (id: string, name: string) =>
    invoke<void>("snapshot_apply", { id, name }),
  snapshotDelete: (id: string, name: string) =>
    invoke<void>("snapshot_delete", { id, name }),

  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (cfg: AppConfig) => invoke<void>("set_config", { cfg }),
  getConfigWarnings: () => invoke<string[]>("get_config_warnings"),

  pickPath: (kind: "iso" | "qcow2" | "folder" | "any") =>
    invoke<string | null>("pick_path", { kind }),
};

export function humanMb(mb: number): string {
  if (mb >= 1024 && mb % 1024 === 0) return `${mb / 1024} GB`;
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
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
