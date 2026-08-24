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

  function rowClass(vm: VmListItem, selected: boolean) {
    return selected ? "row selected" : "row";
  }

  const displayLabel: Record<string, string> = {
    none: "headless",
    vnc: "vnc",
    gtk: "gtk",
    sdl: "sdl",
  };
</script>

<section class="panel table-wrap">
  {#if $visibleVms.length === 0}
    <div class="empty">
      No virtual machines yet.<br />Click <b>New VM</b> to create one.
    </div>
  {:else}
    <table>
      <thead>
        <tr>
          <th class="c-name">Name</th>
          <th class="c-status">Status</th>
          <th class="c-num">vCPU</th>
          <th class="c-num">RAM</th>
          <th class="c-num">Disk</th>
          <th>Display</th>
        </tr>
      </thead>
      <tbody>
        {#each $visibleVms as vm (vm.id)}
          {@const st = statusOf(vm)}
          {@const sel = vm.id === $selectedId}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <tr
            class={rowClass(vm, sel)}
            onclick={() => selectedId.set(vm.id)}
            onkeydown={(e) => e.key === "Enter" && selectedId.set(vm.id)}
            tabindex="0"
            ondblclick={() => openConsole(vm)}
          >
            <td class="c-name">{vm.name}</td>
            <td class="c-status">
              <span class="dot {st}"></span>{st}
              {#if st === "running" || st === "paused"}
                <span class="mini-actions">
                  {#if st === "running"}
                    <button class="ghost" title="Pause" onclick={() => pause(vm)}>❚❚</button>
                    <button class="ghost" title="ACPI shutdown" onclick={() => shutdown(vm)}>&#9211;</button>
                    <button class="ghost danger-text" title="Force stop" onclick={() => forceStop(vm)}>✕</button>
                  {:else}
                    <button class="ghost" title="Resume" onclick={() => resume(vm)}>▶</button>
                  {/if}
                </span>
              {:else}
                <span class="mini-actions">
                  <button class="ghost" title="Start" onclick={() => start(vm)}>▶</button>
                </span>
              {/if}
            </td>
            <td class="c-num">{vm.cpus}</td>
            <td class="c-num">{humanMb(vm.memory_mb)}</td>
            <td class="c-num">{vm.disk_size_gb > 0 ? `${vm.disk_size_gb} GB` : "—"}</td>
            <td>{displayLabel[vm.display] ?? vm.display}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .table-wrap {
    flex: 1;
    overflow: auto;
    min-width: 0;
  }
  .empty {
    height: 100%;
    display: grid;
    place-items: center;
    text-align: center;
    color: var(--fg3);
    line-height: 2;
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
    cursor: default;
    border-bottom: 1px solid var(--line);
  }
  td { padding: 8px 12px; white-space: nowrap; }
  tr.row:hover { background: var(--bg2); }
  tr.selected { background: var(--bg3); box-shadow: inset 2px 0 0 var(--accent); }
  .c-name { min-width: 160px; width: 32%; font-weight: 600; }
  .c-status { min-width: 190px; }
  .c-num { text-align: right; font-family: var(--mono); font-size: 12px; }
  th.c-num { text-align: right; }
  .mini-actions { margin-left: 8px; display: inline-flex; gap: 2px; vertical-align: middle; }
  .mini-actions button { padding: 1px 6px; font-size: 11px; line-height: 1.4; }
  .danger-text { color: var(--err); }
</style>
