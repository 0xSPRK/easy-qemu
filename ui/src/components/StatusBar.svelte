<script lang="ts">
  import { statuses, visibleVms, vms } from "../lib/state";

  let {
    warnings = [],
    onOpenSettings,
  }: {
    warnings?: string[];
    onOpenSettings?: () => void;
  } = $props();

  const running = $derived(
    Object.values($statuses).filter((s) => s.status === "running").length
  );
  const paused = $derived(
    Object.values($statuses).filter((s) => s.status === "paused").length
  );
</script>

<footer class="statusbar">
  <span class="cell">
    <span class="dot running"></span>{running} running{#if paused} · {paused} paused{/if}
  </span>
  <span class="cell dim">{$visibleVms.length}/{$vms.length} shown</span>
  <span class="hint">
    <b>N</b> new · <b>Ctrl+F</b> filter · <b>Del</b> delete · <b>dbl-click</b> console
  </span>
  <span style="flex:1"></span>
  {#if warnings.length > 0}
    <button
      class="ghost warn"
      title={warnings.join("\n")}
      onclick={onOpenSettings}
    >
      ⚠ config warning — click to review
    </button>
  {/if}
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 6px 16px;
    border-top: 1px solid var(--line);
    background: var(--bg1);
    font-size: 12px;
    color: var(--fg2);
    user-select: none;
  }
  .dim { color: var(--fg3); }
  .hint {
    margin-left: auto;
    color: var(--fg3);
  }
  .hint b {
    color: var(--fg2);
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
  }
  .warn { color: var(--warn); font-size: 12px; }
</style>
