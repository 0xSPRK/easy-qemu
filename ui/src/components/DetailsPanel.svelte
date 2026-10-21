<script lang="ts">
  import { fade } from "svelte/transition";
  import { statuses } from "../lib/state";
  import { api, humanMb, humanBytes, uptimeStr, cleanErr } from "../lib/api";
  import { toast } from "../lib/toast";
  import LogsModal from "./LogsModal.svelte";
  import SnapshotsModal from "./SnapshotsModal.svelte";
  import type { VmListItem, VmStatus } from "../lib/types";

  let { vm, status }: { vm: VmListItem; status: VmStatus } = $props();

  const upd = $derived($statuses[vm.id]);
  const uptime = $derived(upd?.started_at ? uptimeStr(upd.started_at) : "—");
  const vncPort = $derived(upd?.vnc_port ?? (vm.vnc_display !== null ? 5900 + vm.vnc_display : null));
  const stats = $derived(upd?.stats ?? null);

  let showLogs = $state(false);
  let showSnaps = $state(false);
  let busyAct = $state(false);
  let moreOpen = $state(false);

  // saved-state (migration) availability
  let hasSav = $state(false);
  $effect(() => {
    api.hasSavedState(vm.id).then((v) => (hasSav = v)).catch(() => (hasSav = false));
  });

  // balloon target
  let balloonTarget = $state<number>(vm.memory_mb);
  $effect(() => {
    if (stats?.balloon_mb) balloonTarget = stats.balloon_mb;
  });

  // runtime action inputs
  let keyCombo = $state("");
  let isoInput = $state(vm.iso ?? "");

  async function act(fn: () => Promise<unknown>, ok?: string) {
    if (busyAct) return;
    busyAct = true;
    try {
      await fn();
      if (ok) toast(ok, "success");
    } catch (e) {
      toast(cleanErr(e), "error");
    } finally {
      busyAct = false;
    }
  }

  async function openConsole() {
    await act(() => api.openConsole(vm.id));
  }
  async function openSsh() {
    await act(async () => {
      const t = await api.sshConnect(vm.id);
      toast(`SSH ${t.user}@${t.host}:${t.port} opened in a terminal`, "success");
    });
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
        <div class="kv"><span>Display</span><b>{vm.display}</b>

        </div>
        <div class="kv"><span>Video</span><b>{vm.vga}{vm.gl ? " + gl" : ""}</b></div>
        <div class="kv"><span>Boot</span><b>{vm.boot_first}{vm.boot_menu ? " · menu" : ""}</b></div>

        {#if vm.hostfwd.length > 0}
          <div class="kv wide"><span>Port forwarding</span>
            <b class="mono">
              {#each vm.hostfwd as hf}
                <div>{hf.proto} :{hf.host_port} → guest :{hf.guest_port}</div>
              {/each}
            </b>
          </div>
        {/if}

        <div class="kv wide"><span>Disk{vm.disks.length > 1 ? `s (${vm.disks.length})` : ""}</span>
          <b class="mono wrap">
            {#each vm.disks as d}
              <div>{d.id} · {d.opts.bus}{d.owned ? "" : " · external"} — {d.path}</div>
            {/each}
          </b>
        </div>
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

      <!-- live statistics -->
      {#if stats && (stats.cpu_pct !== null || stats.disks.length > 0)}
        <div class="stats" in:fade={{ duration: 120 }}>
          <div class="stat-head">Live stats</div>
          <div class="stat-row">
            <span>CPU</span>
            <b class="mono" class:hot={(stats.cpu_pct ?? 0) > 90}>
              {stats.cpu_pct !== null ? `${stats.cpu_pct}%` : "—"}
            </b>
          </div>
          {#if stats.balloon_mb !== null}
            <div class="balloon">
              <div class="stat-row">
                <span>Balloon</span>
                <b class="mono">{humanMb(stats.balloon_mb)}</b>
              </div>
              <input
                type="range"
                min="128"
                max={vm.memory_mb}
                step="64"
                bind:value={balloonTarget}
              />
              <button
                class="ghost tiny"
                disabled={busyAct || balloonTarget === stats.balloon_mb}
                onclick={() => act(() => api.setBalloon(vm.id, balloonTarget), "Balloon updated")}
              >
                Apply {humanMb(balloonTarget)}
              </button>
            </div>
          {/if}
          {#each stats.disks as d}
            <div class="stat-row" title={d.path}>
              <span>{d.id} I/O</span>
              <b class="mono">↓{humanBytes(d.rd_bytes)} ↑{humanBytes(d.wr_bytes)}</b>
            </div>
          {/each}
        </div>
      {/if}

      <!-- runtime actions -->
      {#if status === "running" || status === "paused"}
        <details class="more" bind:open={moreOpen}>
          <summary>More actions…</summary>
          <div class="more-body">
            <div class="act-grid">
              <button disabled={busyAct} onclick={() => act(() => api.vmAction(vm.id, "suspend"), "Suspended")}>Suspend</button>
              <button disabled={busyAct || status !== "paused"} onclick={() => act(() => api.vmAction(vm.id, "wakeup"), "Woke up")}>Wake up</button>
              <button disabled={busyAct} onclick={() => act(() => api.vmAction(vm.id, "nmi"), "NMI sent")}>NMI</button>
              <button disabled={busyAct} onclick={() =>
                act(async () => {
                  const p = await api.screendump(vm.id);
                  toast(`Screenshot saved: ${p}`, "success");
                })}>Screenshot</button>
            </div>

            <label class="inline"><span>Send keys</span>
              <div style="display:flex; gap:6px">
                <input
                  type="text"
                  bind:value={keyCombo}
                  placeholder="ctrl-alt-delete"
                  style="flex:1"
                />
                <button
                  disabled={busyAct || !keyCombo.trim()}
                  onclick={() =>
                    act(() => api.sendKey(vm.id, keyCombo.trim()), `Sent ${keyCombo}`)}
                >Send</button>
              </div>
            </label>

            <label class="inline"><span>CD-ROM image</span>
              <div style="display:flex; gap:6px">
                <input type="text" bind:value={isoInput} placeholder=".iso path" style="flex:1" />
                <button
                  disabled={busyAct || !isoInput.trim()}
                  title="Insert / replace at runtime"
                  onclick={() => act(() => api.changeIso(vm.id, isoInput.trim()), "ISO inserted")}
                >Insert</button>
                <button
                  disabled={busyAct}
                  title="Eject the medium"
                  onclick={() => act(() => api.changeIso(vm.id, null), "ISO ejected")}
                >Eject</button>
              </div>
            </label>

            {#if vm.vnc_password_auth && vm.display === "vnc"}
              <label class="inline"><span>VNC password</span>
                <div style="display:flex; gap:6px">
                  <input type="password" id="vnvpw" style="flex:1" placeholder="min 4 chars" />
                  <button
                    disabled={busyAct}
                    onclick={() =>
                      act(async () => {
                        const el = document.getElementById("vnvpw") as HTMLInputElement | null;
                        const pw = el?.value ?? "";
                        await api.setVncPassword(vm.id, pw);
                        if (el) el.value = "";
                        toast("VNC password set", "success");
                      })}
                  >Set</button>
                </div>
              </label>
            {:else if vm.display === "vnc"}
              <p class="hint">Enable “VNC password auth” in the VM settings to protect the console.</p>
            {/if}

            <div class="sav-block">
              {#if hasSav}
                <p class="hint ok-text">A saved state exists — starting will resume it.</p>
                <button disabled={busyAct} onclick={() =>
                  act(async () => {
                    await api.discardSavedState(vm.id);
                    hasSav = false;
                    toast("Saved state discarded", "success");
                  })}>Discard saved state</button>
              {:else}
                <button
                  disabled={busyAct}
                  title="Save the live state to disk and stop the VM"
                  onclick={() =>
                    act(async () => {
                      await api.saveVmState(vm.id);
                      hasSav = true;
                      toast("VM state saved and VM stopped", "success");
                    })}
                >Save state & stop</button>
              {/if}
            </div>
          </div>
        </details>
      {:else if hasSav}
        <p class="hint ok-text">Saved state present — press Start to resume this VM.</p>
      {/if}
    {/key}
  </div>

  <footer>
    <button onclick={() => (showLogs = true)}>Log</button>
    <button onclick={() => (showSnaps = true)}>
      Snapshots
    </button>
    <span style="flex:1"></span>
    <button
      onclick={openSsh}
      disabled={status !== "running"}
      title={status !== "running" ? "Start the VM first" : "Open an SSH session in a terminal"}
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
  .wrap { word-break: break-all; font-weight: 500 !important; color: var(--fg2); font-size: 12px; }
  .sep { grid-column: 1 / -1; height: 1px; background: var(--line); margin: 4px 0; }

  .stats {
    margin-top: 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm, 8px);
    padding: 10px 12px;
  }
  .stat-head {
    font-size: 10.5px; text-transform: uppercase; letter-spacing: .06em;
    color: var(--fg3); margin-bottom: 6px;
  }
  .stat-row { display: flex; justify-content: space-between; align-items: baseline; padding: 2px 0; }
  .stat-row span { font-size: 11.5px; color: var(--fg3); }
  .stat-row b { font-size: 12px; font-weight: 600; }
  .hot { color: var(--err); }
  .balloon { margin: 6px 0; }
  .balloon input[type="range"] { width: 100%; margin: 6px 0; }
  .tiny { padding: 3px 10px; font-size: 11.5px; }

  .more { margin-top: 12px; border-top: 1px dashed var(--line); padding-top: 8px; }
  .more summary {
    cursor: pointer; user-select: none;
    font-size: 12px; color: var(--fg3);
  }
  .more-body { display: flex; flex-direction: column; gap: 10px; margin-top: 10px; }
  .act-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .inline { display: block; }
  .inline > span {
    display: block; font-size: 10.5px; text-transform: uppercase;
    letter-spacing: .05em; color: var(--fg3); margin-bottom: 4px;
  }
  .hint { color: var(--fg3); font-size: 11.5px; margin: 0; }
  .ok-text { color: var(--ok); }
  .sav-block { display: flex; gap: 8px; align-items: center; justify-content: space-between; }

  footer {
    display: flex; gap: 8px; align-items: center;
    padding: 12px 16px;
    border-top: 1px solid var(--line);
  }
</style>
