<script lang="ts">
  import { api } from "../lib/api";
  import { toast } from "../lib/toast";
  import type { SnapInfo, VmListItem } from "../lib/types";

  let { vm, onclose }: { vm: VmListItem; onclose: () => void } = $props();

  let snaps = $state<SnapInfo[]>([]);
  let newName = $state("");
  let busy = $state(false);

  async function refresh() {
    try {
      snaps = await api.snapshotList(vm.id);
    } catch (e) {
      toast(clean(e), "error");
      snaps = [];
    }
  }

  function clean(e: unknown): string {
    return String(e).replace(/^[^:]*Error:\s*/, "").trim();
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

  $effect(() => {
    refresh();
  });
</script>

<div class="overlay">
  <div class="modal" style="width: 560px">
    <header>
      <h3>Snapshots — {vm.name}</h3>
      <button class="ghost" onclick={onclose}>✕</button>
    </header>
    <div class="body">
      <div style="display:flex; gap:6px; margin-bottom:14px">
        <input
          type="text"
          bind:value={newName}
          placeholder="snapshot name"
          style="flex:1"
        />
        <button
          class="primary"
          disabled={busy || !newName.trim()}
          onclick={() => run(() => api.snapshotCreate(vm.id, newName.trim()), `Snapshot "${newName}" created`)}
        >
          Create
        </button>
      </div>

      {#if snaps.length === 0}
        <p style="color:var(--fg3)">No snapshots.</p>
      {:else}
        <table>
          <thead>
            <tr><th>Name</th><th>Tag</th><th>Epoch date</th><th></th></tr>
          </thead>
          <tbody>
            {#each snaps as s (s.name + (s.tag ?? ""))}
              <tr>
                <td>{s.name}</td>
                <td class="mono">{s.tag ?? "—"}</td>
                <td class="mono">{s.date_time ?? "—"}</td>
                <td style="text-align:right; white-space:nowrap">
                  <button class="ghost" disabled={busy} title="Restore this snapshot"
                    onclick={() => run(() => api.snapshotApply(vm.id, s.name), `Restored "${s.name}"`)}>
                    Restore
                  </button>
                  <button class="ghost danger-text" disabled={busy}
                    onclick={() => run(() => api.snapshotDelete(vm.id, s.name), `Deleted "${s.name}"`)}>
                    Delete
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      <p class="hint">Snapshots are stored inside the qcow2 disk. The VM must be powered off to change them.</p>
    </div>
  </div>
</div>

<style>
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
  .hint { color: var(--fg3); font-size: 12px; margin-bottom: 0; }
</style>
