<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import RFB from "@novnc/novnc";
  import { api } from "../lib/api";
  import type { ConsoleInfo } from "../lib/types";

  let { vmId }: { vmId: string } = $props();

  let container = $state<HTMLDivElement | null>(null);
  let info = $state<ConsoleInfo | null>(null);
  let connected = $state(false);
  let errorText = $state("");
  let toolbarVisible = $state(true);
  let rfb: RFB | null = null;
  let idleTimer: ReturnType<typeof setTimeout> | null = null;

  function wakeToolbar() {
    toolbarVisible = true;
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      // hide only while the session is up — errors must stay readable
      if (connected) toolbarVisible = false;
    }, 2200);
  }

  async function connect() {
    errorText = "";
    try {
      const ci = await api.openConsole(vmId);
      info = ci;
      if (!container) return;
      const url = `ws://127.0.0.1:${ci.ws_port}/websockify`;
      rfb = new RFB(container, url, { shared: true });
      rfb.scaleViewport = true;
      rfb.resizeSession = true;
      rfb.background = "#0a0a0b";
      rfb.addEventListener("connect", () => {
        connected = true;
        wakeToolbar();
      });
      rfb.addEventListener("disconnect", (e) => {
        connected = false;
        toolbarVisible = true;
        const detail = (e as CustomEvent).detail as { clean?: boolean };
        if (!detail?.clean) errorText = "Connection lost.";
      });
    } catch (e) {
      errorText = String(e).replace(/^[^:]*Error:\s*/, "").trim();
    }
  }

  function disconnect() {
    try {
      rfb?.disconnect();
      void api.closeConsole(vmId);
    } finally {
      window.close();
    }
  }

  onMount(() => {
    void connect();
    wakeToolbar();
    return () => {
      if (idleTimer) clearTimeout(idleTimer);
      try {
        rfb?.disconnect();
      } catch {
        /* ignore */
      }
    };
  });

  function sendCtrlAltDel() {
    rfb?.sendCtrlAltDel();
  }

  async function fullscreen() {
    await document.documentElement.requestFullscreen().catch(() => {});
  }
</script>

<svelte:window onmousemove={wakeToolbar} />

<div class="console">
  <header
    class="bar"
    class:hidden={!toolbarVisible}
    transition:fade={{ duration: 150 }}
  >
    <span class="dot" class:running={connected}></span>
    <span class="status">
      {#if info}
        VNC :{info.vnc_port} · {connected ? "connected" : "connecting…"}
      {:else}
        connecting…
      {/if}
    </span>
    <span style="flex:1"></span>
    <button onclick={sendCtrlAltDel} disabled={!connected}>Ctrl+Alt+Del</button>
    <button onclick={fullscreen}>Fullscreen</button>
    <button class="danger" onclick={disconnect}>Close</button>
  </header>

  <div class="screen" bind:this={container}></div>

  {#if !connected}
    <div class="veil" transition:fade={{ duration: 150 }}>
      {#if errorText}
        <div class="msg err">{errorText}</div>
        <button class="primary" onclick={() => { errorText = ""; void connect(); }}>Retry</button>
      {:else}
        <div class="msg">Connecting to the VM display…</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .console {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg0);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg1);
    user-select: none;
    transition:
      transform var(--t-med) var(--ease-out),
      opacity var(--t-med) ease;
  }
  .bar.hidden {
    transform: translateY(-100%);
    opacity: 0;
    pointer-events: none;
    position: absolute;
    top: 0; left: 0; right: 0;
    z-index: 5;
  }
  .status { color: var(--fg2); font-size: 12.5px; font-family: var(--mono); }
  .screen {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .screen :global(div) { width: 100%; height: 100%; }
  .veil {
    position: absolute;
    inset: 0;
    background: rgba(10, 10, 11, 0.85);
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    z-index: 4;
  }
  .msg { color: var(--fg2); }
  .msg.err { color: var(--err); }
</style>
