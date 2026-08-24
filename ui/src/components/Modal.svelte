<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, scale } from "svelte/transition";

  let {
    title,
    width = "480px",
    danger = false,
    onclose,
    children,
  }: {
    title: string;
    width?: string;
    danger?: boolean;
    onclose: () => void;
    children: Snippet;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onclose();
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="overlay"
  transition:fade={{ duration: 130 }}
  onclick={onBackdropClick}
>
  <div
    class="modal"
    style="width: {width}"
    transition:scale={{ start: 0.97, duration: 150 }}
  >
    <header>
      <h3 class:danger>{title}</h3>
      <button class="ghost" onclick={onclose} aria-label="Close">✕</button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  h3.danger { color: var(--err); }
</style>
