<script lang="ts">
  import { api, humanMb } from "../lib/api";
  import { toast } from "../lib/toast";
  import { reloadVms } from "../lib/state";
  import type { HostFwd, Vm, VmDraft, FwdProto, Accel, DisplayMode, MachineType, Firmware, CpuModel, NetMode, NetModel } from "../lib/types";

  let { existing = null, onclose }: { existing: Vm | null; onclose: () => void } = $props();

  let name = $state(existing?.name ?? "");
  let memMb = $state(existing?.memory_mb ?? 1024);
  let cpus = $state(existing?.cpus ?? 2);
  let iso = $state(existing?.iso ?? "");
  let accel = $state<Accel>(existing?.accel ?? "auto");
  let display = $state<DisplayMode>(existing?.display ?? "vnc");
  let machine = $state<MachineType>(existing?.machine ?? "q35");
  let firmware = $state<Firmware>(existing?.firmware ?? "bios");
  let cpuModel = $state<CpuModel>(existing?.cpu ?? "auto");
  let netMode = $state<NetMode>(existing?.net_mode ?? "nat");
  let netModel = $state<NetModel>(existing?.net_model ?? "auto");

  // disk (locked when editing)
  let diskNew = $state(true);
  let diskGb = $state(20);
  let diskFolder = $state("");
  let diskExistingPath = $state("");

  if (!existing) {
    api.pickPath("folder").then((p) => {
      if (p && !diskFolder) diskFolder = "";
    });
    api.getConfig().then((c) => {
      if (!diskFolder && c.storage_dir) diskFolder = c.storage_dir;
    });
  } else {
    diskNew = false;
    diskExistingPath = existing.disk_path;
    diskGb = existing.disk_size_gb || 0;
  }

  // hostfwd
  let fwd = $state<HostFwd[]>(existing ? (JSON.parse(JSON.stringify(existing.hostfwd)) as HostFwd[]) : []);
  function addFwd() {
    fwd.push({ proto: "tcp" as FwdProto, host_port: 0, guest_port: 0 });
  }
  function removeFwd(i: number) {
    fwd.splice(i, 1);
  }

  let busy = $state(false);

  const isEdit = !!existing;

  async function pickIso() {
    const p = await api.pickPath("iso");
    if (p) iso = p;
  }
  async function pickQcow2() {
    const p = await api.pickPath("qcow2");
    if (p) diskExistingPath = p;
  }
  async function pickFolder() {
    const p = await api.pickPath("folder");
    if (p) diskFolder = p;
  }

  async function submit() {
    busy = true;
    try {
      const draft: VmDraft = {
        name,
        memory_mb: Number(memMb),
        cpus: Number(cpus),
        iso: iso.trim() ? iso.trim() : null,
        accel,
        display,
        machine,
        firmware,
        cpu: cpuModel,
        net_mode: netMode,
        net_model: netModel,
        hostfwd: netMode === "nat"
          ? fwd.map((f) => ({ proto: f.proto, host_port: Number(f.host_port), guest_port: Number(f.guest_port) }))
          : [],
        disk: isEdit
          ? { kind: "existing", path: diskExistingPath || existing!.disk_path }
          : diskNew
            ? { kind: "new", size_gb: Number(diskGb), folder: diskFolder.trim() || null }
            : { kind: "existing", path: diskExistingPath.trim() },
      };
      if (isEdit) {
        const merged = { ...existing!, ...draft, id: existing!.id, disk_path: existing!.disk_path, disk_owned: existing!.disk_owned, disk_size_gb: existing!.disk_size_gb, vnc_display: existing!.vnc_display };
        await api.updateVm(merged as Vm);
        toast(`VM "${name}" updated`, "success");
      } else {
        await api.createVm(draft);
        toast(`VM "${name}" created`, "success");
      }
      await reloadVms();
      onclose();
    } catch (e) {
      toast(cleanErr(e), "error");
    } finally {
      busy = false;
    }
  }

  function cleanErr(e: unknown): string {
    return String(e).replace(/^[^:]*Error:\s*/, "").trim();
  }

  const numInput = (bind: number, set: (v: number) => void, min = 1, max = 1_000_000) => ({
    type: "number",
    value: String(bind),
    oninput: (e: Event) => {
      const v = Number((e.target as HTMLInputElement).value);
      if (!Number.isNaN(v)) set(Math.min(max, Math.max(min, Math.trunc(v))));
    },
  });
</script>

<div class="overlay">
  <div class="modal" style="width: 640px">
    <header>
      <h3>{isEdit ? `Edit VM — ${existing!.name}` : "Create a virtual machine"}</h3>
      <button class="ghost" onclick={onclose}>✕</button>
    </header>

    <div class="body">
      <!-- basics -->
      <div class="row3">
        <label class="field"><span>Name</span>
          <input type="text" bind:value={name} placeholder="my-vm" />
        </label>
        <label class="field"><span>Memory, MB</span>
          <input type="number" min="16" step="64" bind:value={memMb} />
        </label>
        <label class="field"><span>vCPUs</span>
          <input type="number" min="1" max="128" bind:value={cpus} />
        </label>
      </div>

      <div class="row3" style="margin-top:12px">
        <label class="field"><span>Accel</span>
          <select bind:value={accel}>
            {#each ["auto", "kvm", "whpx", "tcg", "none"] as a}<option value={a}>{a}</option>{/each}
          </select>
        </label>
        <label class="field"><span>Display</span>
          <select bind:value={display} disabled={machine === "microvm"}>
            {#each ["none", "vnc", "gtk", "sdl"] as d}<option value={d}>{d}</option>{/each}
          </select>
        </label>
        <label class="field"><span>Firmware</span>
          <select bind:value={firmware}>
            <option value="bios">BIOS (SeaBIOS)</option>
            <option value="uefi">UEFI (OVMF)</option>
          </select>
        </label>
      </div>

      <div class="row3" style="margin-top:12px">
        <label class="field"><span>Machine</span>
          <select bind:value={machine}>
            <option value="q35">q35 (modern)</option>
            <option value="i440fx">i440fx (legacy)</option>
            <option value="microvm">microvm</option>
            <option value="auto">auto (default)</option>
          </select>
        </label>
        <label class="field"><span>CPU model</span>
          <select bind:value={cpuModel}>
            {#each ["auto", "max", "host"] as c}<option value={c}>{c}</option>{/each}
          </select>
        </label>
        <div class="field"><span>ISO</span>
          <div style="display:flex; gap:6px">
            <input type="text" bind:value={iso} placeholder=".iso path" style="flex:1" />
            <button onclick={pickIso}>…</button>
          </div>
        </div>
      </div>

      <div class="sep"></div>

      <!-- disk -->
      {#if !isEdit}
        <div class="row" style="gap:14px; align-items:center; margin-bottom:10px">
          <b style="font-size:12px; text-transform:uppercase; letter-spacing:.05em; color:var(--fg3)">Disk</b>
          <button class="ghost" class:active={diskNew} onclick={() => (diskNew = true)}>Create new</button>
          <button class="ghost" class:active={!diskNew} onclick={() => (diskNew = false)}>Use existing</button>
        </div>

        {#if diskNew}
          <div class="row2">
            <label class="field"><span>Size, GB</span>
              <input type="number" min="1" max="16384" bind:value={diskGb} />
            </label>
            <div class="field" style="grid-column: span 2"><span>Folder</span>
              <div style="display:flex; gap:6px">
                <input type="text" bind:value={diskFolder} placeholder="default storage dir" style="flex:1" />
                <button onclick={pickFolder}>…</button>
              </div>
            </div>
          </div>
        {:else}
          <div class="field"><span>qcow2 file</span>
            <div style="display:flex; gap:6px">
              <input type="text" bind:value={diskExistingPath} placeholder="path to .qcow2" style="flex:1" />
              <button onclick={pickQcow2}>…</button>
            </div>
          </div>
          <p class="note">An external disk is never deleted together with the VM.</p>
        {/if}
      {:else}
        <p class="note" style="margin-top:0">Disk settings cannot be changed after creation.</p>
      {/if}

      <div class="sep"></div>

      <!-- network -->
      <div class="row3">
        <label class="field"><span>Network mode</span>
          <select bind:value={netMode}>
            <option value="nat">NAT (user)</option>
            <option value="bridged">Bridged (tap)</option>
            <option value="none">None</option>
          </select>
        </label>
        <label class="field"><span>NIC model</span>
          <select bind:value={netModel} disabled={netMode === 'none'}>
            {#each [["auto", "auto (e1000)"], ["virtio", "virtio-net"], ["e1000", "e1000"], ["rtl8139", "rtl8139"]] as [v, l]}
              <option value={v}>{l}</option>
            {/each}
          </select>
        </label>
      </div>

      {#if netMode === "nat"}
        <div style="margin-top:10px">
          <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:6px">
            <b style="font-size:11px; text-transform:uppercase; letter-spacing:.06em; color:var(--fg3)">Port forwarding (hostfwd)</b>
            <button class="ghost" onclick={addFwd}>+ Add rule</button>
          </div>
          {#if fwd.length === 0}
            <p class="note">No rules. Example: tcp 2222 → guest 22 gives SSH access.</p>
          {/if}
          {#each fwd as f, i}
            <div style="display:flex; gap:6px; margin-bottom:6px; align-items:center">
              <select bind:value={f.proto} style="width:76px">
                <option value="tcp">tcp</option>
                <option value="udp">udp</option>
              </select>
              <input type="number" min="1" max="65535" placeholder="host port" bind:value={f.host_port} style="width:110px" />
              <span style="color:var(--fg3)">→ guest :</span>
              <input type="number" min="1" max="65535" placeholder="guest port" bind:value={f.guest_port} style="width:110px" />
              <button class="ghost danger-text" title="Remove" onclick={() => removeFwd(i)}>✕</button>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <footer>
      <button onclick={onclose}>Cancel</button>
      <button class="primary" onclick={submit} disabled={busy || !name.trim()}>
        {isEdit ? "Save changes" : "Create VM"}
      </button>
    </footer>
  </div>
</div>

<style>
  .row2 { display: grid; grid-template-columns: 130px 1fr; gap: 10px; }
  .row3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .sep { height: 1px; background: var(--line); margin: 16px 0; }
  .note { color: var(--fg3); font-size: 12px; margin: 8px 0 0; }
  button.active {
    background: var(--accent);
    color: #101013;
    border-color: var(--accent);
    font-weight: 600;
  }
</style>
