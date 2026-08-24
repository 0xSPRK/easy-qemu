<script lang="ts">
  import { statuses, visibleVms, selectedId } from "../lib/state";
  import type { VmListItem, VmStatus } from "../lib/types";
  import { humanMb } from "../lib/api";

  let {
    statusOf,
    start,
    pause,
    resume,
    reset,
    shutdown,
    forceStop,
    openConsole,
    openEdit,
    askDelete,
  }: {
    statusOf: (vm: VmListItem) => VmStatus;
    start: (vm: VmListItem) => void;
    pause: (vm: VmListItem) => void;
    resume: (vm: VmListItem) => void;
    reset: (vm: VmListItem) => void;
    shutdown: (vm: VmListItem) => void;
    forceStop: (vm: VmListItem) => void;
    openConsole: (vm: VmListItem) => void;
    openEdit: (vm: VmListItem) => void;
    askDelete: (vm: VmListItem) => void;
  } = $props();

  const displayLabel: Record<string, string> = {
    none: "headless",
    vnc: "vnc",
    gtk: "gtk",
    sdl: "sdl",
  };
</script>

<section class="panel table-wrap">
  <table>
    <thead>
      <tr>
        <th class="c-name">Name</th>
        <th class="c-status">Status</th>
        <th class="c-num">vCPU</th>
        <th class="c-num">RAM</th>
        <th class="c-num">Disk</th>
        <th>Display</th>
        <th class="c-actions"></th>
      </tr>
    </thead>
    <tbody>
      {#each $visibleVms as vm (vm.id)}
        {@const st = statusOf(vm)}
        {@const sel = vm.id === $selectedId}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <tr
          class={sel ? "row selected" : "row"}
          onclick={() => selectedId.set(vm.id)}
          tabindex="0"
          ondblclick={() => openConsole(vm)}
        >
          <td class="c-name">{vm.name}</td>
          <td class="c-status"><span class="dot {st}"></span>{st}</td>
          <td class="c-num">{vm.cpus}</td>
          <td class="c-num">{humanMb(vm.memory_mb)}</td>
          <td class="c-num">{vm.disk_size_gb > 0 ? `${vm.disk_size_gb} GB` : "—"}</td>
          <td>{displayLabel[vm.display] ?? vm.display}</td>
          <td class="c-actions">
            <div class="acts" class:visible={sel}>
              <button class="ghost" title="Edit" onclick={(e) => { e.stopPropagation(); openEdit(vm); }}>✎</button>
              {#if st === "running"}
                <button class="ghost" title="Pause" onclick={(e) => { e.stopPropagation(); pause(vm); }}>❚❚</button>
                <button class="ghost" title="Reset (hard)" onclick={(e) => { e.stopPropagation(); reset(vm); }}>⟳</button>
                <button class="ghost" title="ACPI shutdown" onclick={(e) => { e.stopPropagation(); shutdown(vm); }}>&#9211;</button>
                <button class="ghost danger-text" title="Force stop" onclick={(e) => { e.stopPropagation(); forceStop(vm); }}>⏻</button>
                <button class="ghost accent-text" title="Open console" onclick={(e) => { e.stopPropagation(); openConsole(vm); }}>⛶</button>
              {:else if st === "paused"}
                <button class="ghost" title="Resume" onclick={(e) => { e.stopPropagation(); resume(vm); }}>▶</button>
                <button class="ghost danger-text" title="Force stop" onclick={(e) => { e.stopPropagation(); forceStop(vm); }}>⏻</button>
              {:else}
                <button class="ghost" title="Start" onclick={(e) => { e.stopPropagation(); start(vm); }}>▶</button>
                <button class="ghost danger-text" title="Delete…" onclick={(e) => { e.stopPropagation(); askDelete(vm); }}>🗑</button>
              {/if}
            </div>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  {#if $visibleVms.length === 0}
    <div class="empty-filter">Nothing matches the filter.</div>
  {/if}
</section>

<style>
  .table-wrap {
    flex: 1;
    overflow: auto;
    min-width: 0;
    position: relative;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--bg1);
    text-align: left;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg3);
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    z-index: 1;
  }
  tbody tr {
    border-bottom: 1px solid var(--line);
    transition: background var(--t-fast) ease;
  }
  td { padding: 8px 12px; white-space: nowrap; }
  tr.row:hover { background: var(--bg2); }
  tr.selected { background: var(--bg3); box-shadow: inset 2px 0 0 var(--accent); }
  .c-name { min-width: 160px; width: 30%; font-weight: 600; }
  .c-status { min-width: 120px; }
  .c-num { text-align: right; font-family: var(--mono); font-size: 12px; }
  th.c-num { text-align: right; }

  /* actions appear on row hover (or when the row is selected) */
  .c-actions { width: 1%; }
  .c-actions .acts {
    display: inline-flex;
    gap: 2px;
    opacity: 0;
    transform: translateX(4px);
    transition: opacity var(--t-fast) ease, transform var(--t-fast) var(--ease-out);
  }
  tr:hover .acts, .acts.visible { opacity: 1; transform: none; }
  .acts button { padding: 2px 7px; font-size: 12px; line-height: 1.5; }
  .danger-text:hover:not(:disabled) { color: var(--err); }
  .accent-text:hover:not(:disabled) { color: var(--accent); }

  .empty-filter {
    position: absolute;
    inset: 52px 0 0;
    display: grid;
    place-items: center;
    color: var(--fg3);
  }
</style>
