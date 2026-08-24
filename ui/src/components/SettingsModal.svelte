<script lang="ts">
  import { api } from "../lib/api";
  import { toast } from "../lib/toast";
  import type { AppConfig, EnvProbe } from "../lib/types";
  import Modal from "./Modal.svelte";

  let {
    onclose,
    onsaved,
  }: {
    onclose: () => void;
    onsaved?: () => void | Promise<void>;
  } = $props();

  let cfg = $state<AppConfig>({
    qemu_binary: null,
    qemu_img: null,
    storage_dir: null,
    vnc_bind: null,
    theme: "dark",
  });
  let warnings = $state<string[]>([]);
  let probe = $state<EnvProbe | null>(null);
  let busy = $state(false);

  $effect(() => {
    (async () => {
      cfg = await api.getConfig();
      warnings = await api.getConfigWarnings();
      probe = await api.probeEnv().catch(() => null);
    })();
  });

  async function pick(kind: "iso" | "qcow2" | "folder" | "any", field: keyof AppConfig) {
    const p = await api.pickPath(kind);
    if (p) (cfg as Record<string, unknown>)[field] = p;
  }

  async function save() {
    busy = true;
    try {
      for (const k of ["qemu_binary", "qemu_img", "storage_dir", "vnc_bind"] as const) {
        if (typeof cfg[k] === "string" && (cfg[k] as string).trim() === "") cfg[k] = null;
      }
      await api.setConfig(cfg);
      warnings = [];
      probe = await api.probeEnv().catch(() => probe);
      toast("Settings saved", "success");
      await onsaved?.();
    } catch (e) {
      toast(String(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Settings" width="620px" {onclose}>
  <div class="body">
    {#each warnings as w}
      <div class="warnbox">{w}</div>
    {/each}

    <label class="field"><span>qemu-system-x86_64</span>
      <div style="display:flex; gap:6px">
        <input type="text" bind:value={cfg.qemu_binary} placeholder="(auto — search in PATH)" style="flex:1" />
        <button onclick={() => pick("any", "qemu_binary")}>…</button>
      </div>
    </label>

    <label class="field" style="margin-top:12px"><span>qemu-img</span>
      <div style="display:flex; gap:6px">
        <input type="text" bind:value={cfg.qemu_img} placeholder="(auto — search in PATH)" style="flex:1" />
        <button onclick={() => pick("any", "qemu_img")}>…</button>
      </div>
    </label>

    <label class="field" style="margin-top:12px"><span>Storage directory</span>
      <div style="display:flex; gap:6px">
        <input type="text" bind:value={cfg.storage_dir} placeholder="(default: config dir / disks)" style="flex:1" />
        <button onclick={() => pick("folder", "storage_dir")}>…</button>
      </div>
    </label>

    <label class="field" style="margin-top:12px"><span>VNC bind address</span>
      <input type="text" bind:value={cfg.vnc_bind} placeholder="127.0.0.1" />
    </label>

    <!-- detected environment -->
    <div class="sep"></div>
    <b style="font-size:11px; text-transform:uppercase; letter-spacing:.06em; color:var(--fg3)">Detected environment</b>
    {#if probe === null}
      <p class="note">Probing…</p>
    {:else}
      <div class="env">
        <div class="env-row">
          <span>qemu-system</span>
          {#if probe.qemu_system}
            <b class="mono ok-text">{probe.qemu_system}</b>
          {:else}
            <b class="err-text">not found — add to PATH or set above</b>
          {/if}
        </div>
        <div class="env-row">
          <span>qemu-img</span>
          {#if probe.qemu_img}
            <b class="mono ok-text">{probe.qemu_img}</b>
          {:else}
            <b class="err-text">not found</b>
          {/if}
        </div>
        <div class="env-row">
          <span>accelerators</span>
          <b>
            {#if probe.accels.length === 0}
              <span class="err-text">—</span>
            {:else}
              {#each probe.accels as a}
                <span class="chip active accel-chip">{a}</span>
              {/each}
            {/if}
          </b>
        </div>
      </div>
    {/if}
  </div>
  <footer>
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={save} disabled={busy}>Save</button>
  </footer>
</Modal>

<style>
  .sep { height: 1px; background: var(--line); margin: 16px 0 12px; }
  .note { color: var(--fg3); font-size: 12px; margin: 8px 0 0; }
  .warnbox {
    background: color-mix(in srgb, var(--warn) 10%, var(--bg2));
    border: 1px solid color-mix(in srgb, var(--warn) 35%, transparent);
    color: var(--warn);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    font-size: 12.5px;
    margin-bottom: 14px;
  }
  .env { display: flex; flex-direction: column; gap: 7px; margin-top: 10px; }
  .env-row { display: grid; grid-template-columns: 110px 1fr; gap: 10px; align-items: baseline; }
  .env-row > span { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--fg3); }
  .env-row b { font-weight: 500; word-break: break-all; font-size: 12px; }
  .ok-text { color: var(--ok); }
  .err-text { color: var(--err); }
  .accel-chip { display: inline-block; margin-right: 4px; border: 1px solid var(--line2); }
</style>
