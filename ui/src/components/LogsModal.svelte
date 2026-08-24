<script lang="ts">
  import { api } from "../lib/api";
  import type { VmListItem } from "../lib/types";

  let { vm, onclose }: { vm: VmListItem; onclose: () => void } = $props();

  let text = $state("loading…");

  $effect(() => {
    api.readLog(vm.id, 600).then((t) => {
      text = t.trim() || "(log is empty)";
    });
  });
</script>

<div class="overlay">
  <div class="modal" style="width: 760px; height: 70vh">
    <header>
      <h3>QEMU log — {vm.name}</h3>
      <button class="ghost" onclick={onclose}>✕</button>
    </header>
    <pre class="body mono">{text}</pre>
  </div>
</div>

<style>
  pre.body {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--fg2);
    background: var(--bg0);
    flex: 1;
  }
</style>
