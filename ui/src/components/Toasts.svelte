<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import { toasts, dismiss } from "../lib/toast";
</script>

<div class="toasts">
  {#each $toasts as t (t.id)}
    <button
      type="button"
      class="toast {t.kind}"
      role="status"
      in:fly={{ y: 14, duration: 160 }}
      out:fade={{ duration: 120 }}
      onclick={() => dismiss(t.id)}
    >
      {t.text}
    </button>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 44px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    z-index: 200;
    max-width: min(680px, calc(100vw - 40px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    background: var(--bg2);
    border: 1px solid var(--line2);
    border-radius: var(--radius);
    padding: 9px 16px;
    font-size: 13px;
    cursor: pointer;
    max-width: 100%;
    text-align: center;
  }
  .toast.success { border-color: color-mix(in srgb, var(--ok) 45%, transparent); }
  .toast.error { border-color: color-mix(in srgb, var(--err) 55%, transparent); color: var(--err); }
</style>
