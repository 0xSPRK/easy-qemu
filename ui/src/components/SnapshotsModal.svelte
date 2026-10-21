<script lang="ts">
  import { api, cleanErr } from "../lib/api";
  import { toast } from "../lib/toast";
  import Modal from "./Modal.svelte";
  import type { SnapInfo, VmListItem } from "../lib/types";

  let { vm, onclose }: { vm: VmListItem; onclose: () => void } = $props();

  const running = vm.status === "running" || vm.status === "paused";

  let snaps = $state<SnapInfo[]>([]);
  let newName = $state("");
  let busy = $state(false);
  // Offline mode operates per-disk; live snapshots cover all disks at once.
  let diskId = $state(vm.disks[0]?.id ?? "");

  function clean(e: unknown): string {
    return cleanErr(e);
  }

  async function refresh() {
    try {
      snaps = running
        ? await api.snapshotOp(vm.id, "live_list")
        : await api.snapshotList(vm.id, diskId || undefined);
    } catch (e) {
      toast(clean(e), "error");
      snaps = [];
    }
  }

  async function run(fn: () => Promise<unknown>, ok: string) {
    busy = true;
    try {
      await fn();
      toast(ok, "success");
      await refresh();
    } catch (e) {
      toast(clean(e), "error");
    } finally {
      busy = false;
    }
  }

  const create = () =>
    run(
      () =>
        running
          ? api.snapshotOp(vm.id, "live_create", newName.trim())
          : api.snapshotCreate(vm.id, newName.trim(), diskId || undefined),
      `Snapshot "${newName}" created`
    );
  const apply = (name: string) =>
    run(
      () =>
        running
          ? api.snapshotOp(vm.id, "live_apply", name)
          : api.snapshotApply(vm.id, name, diskId || undefined),
      `Restored "${name}"`
    );
  const del = (name: string) =>
    run(
      () =>
        running
          ? api.snapshotOp(vm.id, "live_delete", name)
          : api.snapshotDelete(vm.id, name, diskId || undefined),
      `Deleted "${name}"`
    );

  $effect(() => {
    if (!running) refresh();
    else refresh();
  });
</script>

<Modal title={`Snapshots — ${vm.name}`} width="640px" {onclose}>
  <div class="body">
    <div class="mode-row">
      {#if !running && vm.disks.length > 1}
        <b class="lbl">Disk</b>
        {#each vm.disks as d}
          <button
            type="button"
            class="chip"
            class:active={diskId === d.id}
            onclick={() => (diskId = d.id)}
            title={d.path}
          >
            {d.id}{d.owned ? "" : " (ext)"}
          </button>
        {/each}
      {:else if !running}
        <span class="lbl mono dim">{vm.disks[0]?.path}</span>
      {:else}
        <span class="live-badge">LIVE</span>
        <span class="dim">Running VM — internal snapshots across all {vm.disks.length} disk(s)</span>
      {/if}
    </div>

    <div style="display:flex; gap:6px; margin-bottom:14px">
      <input type="text" bind:value={newName} placeholder="snapshot name" style="flex:1" />
      <button class="primary" disabled={busy || !newName.trim()} onclick={create}>
        Create
      </button>
    </div>

    {#if snaps.length === 0}
      <p class="none">No snapshots.</p>
    {:else}
      <table>
        <thead>
          <tr><th>Name</th><th>Tag</th><th>Date</th><th></th></tr>
        </thead>
        <tbody>
          {#each snaps as s (s.name + (s.tag ?? ""))}
            <tr>
              <td>{s.name}</td>
              <td class="mono">{s.tag ?? "—"}</td>
              <td class="mono">{s.date_time ?? "—"}</td>
              <td style="text-align:right; white-space:nowrap">
                <button class="ghost" disabled={busy} title="Restore this snapshot" onclick={() => apply(s.name)}>
                  Restore
                </button>
                <button class="ghost danger-text" disabled={busy} onclick={() => del(s.name)}>
                  Delete
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if running}
      <p class="hint">
        Live snapshots require QEMU ≥ 6.0. The guest is paused briefly while the
        state is written into the qcow2 images.
      </p>
    {:else}
      <p class="hint">Snapshots are stored inside the qcow2 disk image.</p>
    {/if}
  </div>
</Modal>

<style>
  .mode-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .lbl { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--fg3); }
  .dim { color: var(--fg3); font-size: 12px; }
  .mono { word-break: break-all; }
  .live-badge {
    background: color-mix(in srgb, var(--ok) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--ok) 40%, transparent);
    color: var(--ok);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: .08em;
    padding: 2px 8px;
    border-radius: 999px;
  }
  table { width: 100%; border-collapse: collapse; }
  th {
    text-align: left;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .05em;
    color: var(--fg3);
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
  }
  td { padding: 7px 8px; border-bottom: 1px solid var(--line); }
  .none { color: var(--fg3); }
  .hint { color: var(--fg3); font-size: 12px; margin-bottom: 0; }
  .danger-text:hover:not(:disabled) { color: var(--err); }
</style>
