import { listen } from "@tauri-apps/api/event";
import { writable, derived, get } from "svelte/store";
import type { VmListItem, VmUpdate } from "./types";
import { api } from "./api";

/** Live status snapshot for every VM (pushed by the backend poller). */
export const statuses = writable<Record<string, VmUpdate>>({});

/** Full VM records (loaded on demand). */
export const vms = writable<VmListItem[]>([]);

/** Currently selected VM id in the table. */
export const selectedId = writable<string | null>(null);

/** Case-insensitive name filter. */
export const filter = writable("");

export const visibleVms = derived([vms, filter], ([$vms, $filter]) => {
  const q = $filter.trim().toLowerCase();
  if (!q) return $vms;
  return $vms.filter((v) => v.name.toLowerCase().includes(q));
});

export const selectedVm = derived([vms, selectedId], ([$vms, $sel]) =>
  $vms.find((v) => v.id === $sel) ?? null
);

export async function reloadVms() {
  vms.set(await api.listVms());
  const sel = get(selectedId);
  if (sel && !get(vms).some((v) => v.id === sel)) {
    selectedId.set(get(visibleVms)[0]?.id ?? null);
  }
}

export function applyStatuses(updates: VmUpdate[]) {
  const map: Record<string, VmUpdate> = {};
  for (const u of updates) map[u.id] = u;
  statuses.set(map);
}

let unlisten: (() => void) | null = null;

export async function initEvents() {
  if (unlisten) return;
  unlisten = await listen<VmUpdate[]>("vm:statuses", (e) => {
    applyStatuses(e.payload);
  });
  // initial pull so the UI is not empty until the first tick
  try {
    applyStatuses(await api.getStatuses());
  } catch {
    /* poller will fill it in */
  }
}
