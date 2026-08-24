<script lang="ts">
  import { api } from "../lib/api";
  import Modal from "./Modal.svelte";
  import type { VmListItem } from "../lib/types";

  let { vm, onclose }: { vm: VmListItem; onclose: () => void } = $props();

  let text = $state("loading…");

  $effect(() => {
    api.readLog(vm.id, 600).then((t) => {
      text = t.trim() || "(log is empty)";
    });
  });
</script>

<Modal title={`QEMU log — ${vm.name}`} width="760px" {onclose}>
  <pre class="body mono">{text}</pre>
</Modal>

<style>
  pre.body {
    margin: 0;
    min-height: 320px;
    max-height: calc(70vh - 100px);
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--fg2);
    background: var(--bg0);
    flex: 1;
  }
</style>
