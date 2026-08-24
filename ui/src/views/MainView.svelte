<script lang="ts">
  import { onMount } from "svelte";
  import { initEvents, reloadVms, statuses, visibleVms, selectedVm, selectedId, filter, vms } from "../lib/state";
  import { api } from "../lib/api";
  import { toast } from "../lib/toast";
  import VmTable from "../components/VmTable.svelte";
  import DetailsPanel from "../components/DetailsPanel.svelte";
  import VmForm from "../components/VmForm.svelte";
  import SettingsModal from "../components/SettingsModal.svelte";
  import Toasts from "../components/Toasts.svelte";
  import type { VmListItem, VmStatus } from "../lib/types";

  let showForm = $state(false);
  let editTarget = $state<VmListItem | null>(null);
  let showSettings = $state(false);
  let confirmDelete = $state<VmListItem | null>(null);
  let deleteDisk = $state(true);
  let busy = $state(false);

  const runningCount = $derived(
    Object.values($statuses).filter((s) => s.status === "running").length
  );

  onMount(async () => {
    await initEvents();
    await reloadVms();
    if (!($selectedId) && $visibleVms.length > 0) selectedId.set($visibleVms[0].id);
  });

  function statusOf(vm: VmListItem): VmStatus {
    return $statuses[vm.id]?.status ?? vm.status ?? "unknown";
  }

  async function withBusy(fn: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try {
      await fn();
    } catch (e) {
      toast(String(e).replace(/^.*Error:? */, ""), "error");
    } finally {
      busy = false;
    }
  }

  const start = (vm: VmListItem) =>
    withBusy(async () => {
      toast(`Starting "${vm.name}"…`);
      await api.startVm(vm.id);
    });
  const pause = (vm: VmListItem) => withBusy(() => api.vmAction(vm.id, "pause"));
  const resume = (vm: VmListItem) => withBusy(() => api.vmAction(vm.id, "resume"));
  const reset = (vm: VmListItem) => withBusy(() => api.vmAction(vm.id, "reset"));
  const shutdown = (vm: VmListItem) => withBusy(() => api.vmAction(vm.id, "shutdown"));
  const forceStop = (vm: VmListItem) => withBusy(() => api.forceStop(vm.id));

  function openCreate() {
    editTarget = null;
    showForm = true;
  }
  function openEdit(vm: VmListItem) {
    if (statusOf(vm) === "running" || statusOf(vm) === "paused") {
      toast("Stop the VM before editing", "info");
      return;
    }
    editTarget = vm;
    showForm = true;
  }

  function askDelete(vm: VmListItem) {
    confirmDelete = vm;
    deleteDisk = vm.disk_owned;
  }

  const doDelete = () =>
    withBusy(async () => {
      const vm = confirmDelete!;
      confirmDelete = null;
      const report = await api.deleteVm(vm.id, deleteDisk);
      for (const e of report.errors) toast(e, "error");
      if (!report.disk_attempted && deleteDisk && !vm.disk_owned) {
        toast("External disk kept (not owned by Easy QEMU)", "info");
      }
      await reloadVms();
      toast(`VM "${vm.name}" deleted`, "success");
    });

  async function openConsole(vm: VmListItem) {
    if (statusOf(vm) !== "running") {
      toast("Start the VM first", "info");
      return;
    }
    try {
      await api.openConsole(vm.id);
    } catch (e) {
      toast(String(e), "error");
    }
  }
</script>

<div class="layout">
  <header class="topbar">
    <div class="brand">
      <span class="brand-mark"></span>
      Easy QEMU
    </div>
    <div class="top-stats">
      <span class="stat">{$visibleVms.length} of {$vms.length} shown</span>
      <span class="sep">·</span>
      <span class="stat"><span class="dot running"></span>{runningCount} running</span>
    </div>
    <div class="top-actions">
      <input
        type="text"
        placeholder="Filter…"
        bind:value={$filter}
        style="width: 180px"
      />
      <button onclick={() => (showSettings = true)} title="Settings">Settings</button>
      <button class="primary" onclick={openCreate}>New VM</button>
    </div>
  </header>

  <main class="content">
    <VmTable {statusOf} {start} {pause} {resume} {reset} {shutdown} {forceStop} {openConsole} {openEdit} {askDelete} />
    {#if $selectedVm}
      <DetailsPanel vm={$selectedVm} status={statusOf($selectedVm)} />
    {:else}
      <aside class="panel empty-hint">
        Select a virtual machine to see details.
      </aside>
    {/if}
  </main>
</div>

{#if showForm}
  <VmForm
    existing={editTarget?.vm ?? null}
    onclose={() => (showForm = false)}
  />
{/if}

{#if showSettings}
  <SettingsModal onclose={() => (showSettings = false)} />
{/if}

{#if confirmDelete}
  <div class="overlay" role="presentation" onkeydown={() => (confirmDelete = null)}>
    <div class="modal">
      <header><h3>Delete “{confirmDelete.name}”?</h3></header>
      <div class="body">
        {#if confirmDelete.disk_owned}
          <label style="display:flex; gap:8px; align-items:center; cursor:pointer">
            <input type="checkbox" bind:checked={deleteDisk} />
            Also delete the disk file ({confirmDelete.disk_path})
          </label>
          {#if !deleteDisk}
            <p style="color: var(--fg3); margin: 8px 0 0">The disk file will be kept.</p>
          {/if}
        {:else}
          <p style="margin:0">
            This VM uses an external disk — the disk file will NOT be removed.
          </p>
        {/if}
      </div>
      <footer>
        <button onclick={() => (confirmDelete = null)}>Cancel</button>
        <button class="danger" onclick={doDelete}>Delete</button>
      </footer>
    </div>
  </div>
{/if}

<Toasts />

<style>
  .layout {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--bg1);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .brand-mark {
    width: 14px; height: 14px;
    background: var(--accent);
    clip-path: polygon(0 0, 100% 50%, 0 100%);
  }
  .top-stats { color: var(--fg3); display: flex; gap: 10px; align-items: center; }
  .sep { color: var(--line2); }
  .top-actions { margin-left: auto; display: flex; gap: 8px; align-items: center; }
  .content {
    flex: 1;
    display: flex;
    gap: 12px;
    padding: 12px 16px;
    min-height: 0;
  }
  .empty-hint {
    width: 320px;
    display: grid;
    place-items: center;
    color: var(--fg3);
    text-align: center;
    padding: 24px;
  }
</style>
