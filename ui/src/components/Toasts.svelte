<script lang="ts">
  import { toasts, dismiss } from "../lib/toast";
</script>

<div class="toasts">
  {#each $toasts as t (t.id)}
    <button type="button" class="toast {t.kind}" role="status" onclick={() => dismiss(t.id)}>
      {t.text}
    </button>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    z-index: 200;
    max-width: min(680px, calc(100vw - 40px));
  }
  .toast {
    background: var(--bg2);
    border: 1px solid var(--line2);
    border-radius: var(--radius);
    padding: 9px 16px;
    font-size: 13px;
    cursor: pointer;
    animation: slide-up 0.18s ease-out;
    max-width: 100%;
  }
  .toast.success { border-color: color-mix(in srgb, var(--ok) 45%, transparent); }
  .toast.error { border-color: color-mix(in srgb, var(--err) 55%, transparent); color: var(--err); }
  @keyframes slide-up {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
