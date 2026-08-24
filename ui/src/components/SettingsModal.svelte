<script lang="ts">
  import { api } from "../lib/api";
  import { toast } from "../lib/toast";
  import type { AppConfig } from "../lib/types";

  let { onclose }: { onclose: () => void } = $props();

  let cfg = $state<AppConfig>({
    qemu_binary: null,
    qemu_img: null,
    storage_dir: null,
    vnc_bind: null,
    theme: "dark",
  });
  let warnings = $state<string[]>([]);
  let busy = $state(false);

  $effect(() => {
    (async () => {
      cfg = await api.getConfig();
      warnings = await api.getConfigWarnings();
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
      toast("Settings saved", "success");
      onclose();
    } catch (e) {
      toast(String(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay">
  <div class="modal" style="width: 620px">
    <header>
      <h3>Settings</h3>
      <button class="ghost" onclick={onclose}>✕</button>
    </header>
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
    </div>
    <footer>
      <button onclick={onclose}>Cancel</button>
      <button class="primary" onclick={save} disabled={busy}>Save</button>
    </footer>
  </div>
</div>

<style>
  .warnbox {
    background: color-mix(in srgb, var(--warn) 10%, var(--bg2));
    border: 1px solid color-mix(in srgb, var(--warn) 35%, transparent);
    color: var(--warn);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    font-size: 12.5px;
    margin-bottom: 14px;
  }
</style>
