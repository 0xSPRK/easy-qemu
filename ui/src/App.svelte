<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import MainView from "./views/MainView.svelte";
  import ConsoleView from "./views/ConsoleView.svelte";

  const label = getCurrentWindow().label;
  const consoleVmId = label.startsWith("console_") ? label.slice("console_".length) : null;
</script>

{#if consoleVmId}
  <ConsoleView vmId={consoleVmId} />
{:else}
  <MainView />
{/if}
