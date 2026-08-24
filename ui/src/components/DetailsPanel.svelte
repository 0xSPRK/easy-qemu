<script lang="ts">
  import { fade } from "svelte/transition";
  import { statuses } from "../lib/state";
  import { api, humanMb, uptimeStr } from "../lib/api";
  import { toast } from "../lib/toast";
  import LogsModal from "./LogsModal.svelte";
  import SnapshotsModal from "./SnapshotsModal.svelte";
  import type { VmListItem, VmStatus } from "../lib/types";

  let { vm, status }: { vm: VmListItem; status: VmStatus } = $props();

  const upd = $derived($statuses[vm.id]);
  const uptime = $derived(upd?.started_at ? uptimeStr(upd.started_at) : "—");
  const vncPort = $derived(upd?.vnc_port ?? (vm.vnc_display !== null ? 5900 + vm.vnc_display : null));

  let showLogs = $state(false);
  let showSnaps = $state(false);

  async function openConsole() {
    try {
      await api.openConsole(vm.id);
    } catch (e) {
      toast(String(e), "error");
    }
  }

  async function openSsh() {
    try {
      const t = await api.sshConnect(vm.id);
      toast(`SSH ${t.user}@${t.host}:${t.port} opened in a terminal`, "success");
    } catch (e) {
      toast(String(e).replace(/^[^:]*Error:\s*/, "").trim(), "error");
    }
  }
</script>

<aside class="panel details">
  <header>
    <h3>{vm.name}</h3>
    <span class="dot {status}" title={status}></span>
  </header>

  <div class="body">
    {#key vm.id}
      <div class="grid" in:fade={{ duration: 120 }}>
      <div class="kv"><span>vCPU</span><b>{vm.cpus}</b></div>
      <div class="kv"><span>Memory</span><b>{humanMb(vm.memory_mb)}</b></div>
      <div class="kv"><span>Platform</span><b>{vm.machine}</b></div>
      <div class="kv"><span>Firmware</span><b>{vm.firmware.toUpperCase()}</b></div>
      <div class="kv"><span>CPU model</span><b>{vm.cpu}</b></div>
      <div class="kv"><span>Accel</span><b>{vm.accel}</b></div>
      <div class="kv"><span>Network</span><b>{vm.net_mode} · {vm.net_model}</b></div>
      <div class="kv"><span>Display</span><b>{vm.display}</b></div>

      {#if vm.hostfwd.length > 0}
        <div class="kv wide"><span>Port forwarding</span>
          <b class="mono">
            {#each vm.hostfwd as hf}
              <div>{hf.proto} :{hf.host_port} → guest :{hf.guest_port}</div>
            {/each}
          </b>
        </div>
      {/if}

      <div class="kv wide"><span>Disk</span><b class="mono wrap">{vm.disk_path}</b></div>
      {#if vm.iso}
        <div class="kv wide"><span>ISO</span><b class="mono wrap">{vm.iso}</b></div>
      {/if}
      <div class="kv wide">
        <span>SSH</span>
        <b class="mono wrap">{vm.ssh.user}@{vm.ssh.host}:{vm.ssh.port}</b>
      </div>

      <div class="sep"></div>

      <div class="kv"><span>Uptime</span><b>{uptime}</b></div>
      <div class="kv"><span>PID</span><b class="mono">{upd?.pid ?? "—"}</b></div>
      <div class="kv"><span>VNC port</span><b class="mono">{vncPort ?? "—"}</b></div>
      </div>
    {/key}
  </div>

  <footer>
    <button onclick={() => showLogs = true}>Log</button>
    <button onclick={() => showSnaps = true} disabled={status === 'running' || status === 'paused'}>
      Snapshots
    </button>
    <span style="flex:1"></span>
    <button
      onclick={openSsh}
      disabled={status !== 'running'}
      title={status !== 'running' ? "Start the VM first" : "Open an SSH session in a terminal"}
    >
      SSH
    </button>
    <button
      class="primary"
      onclick={openConsole}
      disabled={vm.display !== 'vnc' || status !== 'running'}
      title={vm.display !== 'vnc'
        ? "The console is only available for VMs with a VNC display"
        : "Open the noVNC console window"}
    >
      Console
    </button>
  </footer>
</aside>

{#if showLogs}
  <LogsModal {vm} onclose={() => (showLogs = false)} />
{/if}
{#if showSnaps}
  <SnapshotsModal {vm} onclose={() => (showSnaps = false)} />
{/if}

<style>
  .details {
    width: 340px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
  }
  .body { flex: 1; overflow: auto; padding: 14px 16px; }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 14px;
  }
  .kv span {
    display: block;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg3);
    margin-bottom: 2px;
  }
  .kv b { font-weight: 600; word-break: break-all; }
  .kv.wide { grid-column: 1 / -1; }
  .wrap { word-break: break-all; font-weight: 500 !important; color: var(--fg2); }
  .sep { grid-column: 1 / -1; height: 1px; background: var(--line); margin: 4px 0; }
  footer {
    display: flex; gap: 8px; align-items: center;
    padding: 12px 16px;
    border-top: 1px solid var(--line);
  }
</style>
