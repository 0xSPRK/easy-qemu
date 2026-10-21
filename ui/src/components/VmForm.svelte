<script lang="ts">
  import { api, cleanErr } from "../lib/api";
  import { toast } from "../lib/toast";
  import { reloadVms } from "../lib/state";
  import Modal from "./Modal.svelte";
  import type {
    HostFwd, Vm, VmDraft, FwdProto, DiskSpec, DiskOpts, DiskEntry,
    Accel, DisplayMode, MachineType, Firmware, CpuModel, NetMode, NetModel,
    AioMode, CacheMode, DiskBus, VgaModel, RtcBase, AudioBackend, BootFirst,
  } from "../lib/types";
  import { DEFAULT_SSH, DEFAULT_DISK_OPTS, audioBackends } from "../lib/types";

  let { existing = null, onclose }: { existing: Vm | null; onclose: () => void } = $props();

  const isEdit = !!existing;

  // ---------- tabs ----------
  type TabId = "general" | "disks" | "perf" | "net" | "adv";
  const TABS: { id: TabId; label: string }[] = [
    { id: "general", label: "General" },
    { id: "disks", label: "Disks" },
    { id: "perf", label: "Performance" },
    { id: "net", label: "Network" },
    { id: "adv", label: "Advanced" },
  ];
  let tab = $state<TabId>("general");

  // ---------- basics ----------
  let name = $state(existing?.name ?? "");
  let memMb = $state(existing?.memory_mb ?? 1024);
  let cpus = $state(existing?.cpus ?? 2);
  let iso = $state(existing?.iso ?? "");
  let accel = $state<Accel>(existing?.accel ?? "auto");
  let display = $state<DisplayMode>(existing?.display ?? "vnc");
  let machine = $state<MachineType>(existing?.machine ?? "q35");
  let firmware = $state<Firmware>(existing?.firmware ?? "bios");
  let cpuModel = $state<CpuModel>(existing?.cpu ?? "auto");

  // ---------- performance ----------
  let smpSockets = $state<number | "">(existing?.smp_sockets ?? "");
  let smpCores = $state<number | "">(existing?.smp_cores ?? "");
  let smpThreads = $state<number | "">(existing?.smp_threads ?? "");
  let maxcpus = $state<number | "">(existing?.maxcpus ?? "");
  let rng = $state(existing?.rng ?? true);
  let usbTablet = $state(existing?.usb_tablet ?? true);
  let memPrealloc = $state(existing?.mem_prealloc ?? false);
  let memPath = $state(existing?.mem_path ?? "");
  let vga = $state<VgaModel>(existing?.vga ?? "auto");
  let vgamemMb = $state<number | "">(existing?.vgamem_mb ?? "");
  let gl = $state(existing?.gl ?? false);
  let audio = $state<AudioBackend>(existing?.audio ?? "off");

  // ---------- network ----------
  let netMode = $state<NetMode>(existing?.net_mode ?? "nat");
  let netModel = $state<NetModel>(existing?.net_model ?? "auto");
  let macAddress = $state(existing?.mac_address ?? "");
  let tapIfname = $state(existing?.tap_ifname ?? "");
  let tapBr = $state(existing?.tap_br ?? "");

  // ---------- advanced ----------
  let bootFirst = $state<BootFirst>(existing?.boot_first ?? "disk");
  let bootMenu = $state(existing?.boot_menu ?? false);
  let rtc = $state<RtcBase>(existing?.rtc ?? "utc");
  let vncPasswordAuth = $state(existing?.vnc_password_auth ?? false);

  // ---------- disks ----------
  interface DiskRow extends DiskOpts {
    mode: "new" | "existing";
    /** Set when the row mirrors an already-stored disk entry (edit mode). */
    origId: string | null;
    path: string;
    sizeGb: number;
    folder: string;
    owned: boolean;
  }

  function rowFromEntry(e: DiskEntry): DiskRow {
    return {
      mode: "existing",
      origId: e.id,
      path: e.path,
      sizeGb: e.size_gb || 20,
      folder: "",
      owned: e.owned,
      ...e.opts,
    };
  }
  function newRow(asExisting = false): DiskRow {
    return {
      mode: asExisting ? "existing" : "new",
      origId: null,
      path: "",
      sizeGb: 20,
      folder: "",
      owned: false,
      ...DEFAULT_DISK_OPTS,
    };
  }

  let disks = $state<DiskRow[]>(
    isEdit ? existing!.disks.map(rowFromEntry) : [newRow()]
  );

  if (!isEdit) {
    api.getConfig().then((c) => {
      const def = c.storage_dir;
      for (const d of disks) if (!d.folder && def) d.folder = def;
    });
  }

  function addDisk() {
    const last = disks[disks.length - 1];
    // Editing never creates new files — attach an existing image instead.
    const base = last ?? newRow(isEdit);
    disks.push({ ...base, mode: "existing", origId: null, owned: false });
  }
  async function pickQcow2(i: number) {
    const p = await api.pickPath("qcow2");
    if (p) {
      disks[i].mode = "existing";
      disks[i].path = p;
      disks[i].owned = false;
    }
  }
  async function pickFolder(i: number) {
    const p = await api.pickPath("folder");
    if (p) disks[i].folder = p;
  }
  function pickIso() {
    api.pickPath("iso").then((p) => p && (iso = p));
  }

  // ---------- hostfwd rules / ssh ----------
  let fwd = $state<HostFwd[]>(
    existing ? (JSON.parse(JSON.stringify(existing.hostfwd)) as HostFwd[]) : []
  );
  let sshHost = $state(existing?.ssh?.host ?? DEFAULT_SSH.host);
  let sshPort = $state(existing?.ssh?.port ?? DEFAULT_SSH.port);
  let sshUser = $state(existing?.ssh?.user ?? DEFAULT_SSH.user);

  function resetSsh() {
    sshHost = DEFAULT_SSH.host;
    sshPort = DEFAULT_SSH.port;
    sshUser = DEFAULT_SSH.user;
  }

  /** NAT hides the guest behind the host: a hostfwd rule's host port is the
   *  reachable SSH port, so offer one-click fill from the rules. */
  const fwdSshRules = $derived(
    netMode === "nat"
      ? fwd.filter((r) => validPort(r.host_port) && validPort(r.guest_port))
      : []
  );
  function addFwd() {
    fwd.push({ proto: "tcp" as FwdProto, host_port: 8000, guest_port: 80 });
  }
  function removeFwd(i: number) {
    fwd.splice(i, 1);
  }

  // ---------- validation ----------
  const MEM_PRESETS = [512, 1024, 2048, 4096, 8192];

  function validPort(p: unknown): boolean {
    const n = Number(p);
    return Number.isInteger(n) && n >= 1 && n <= 65535;
  }
  const fwdInvalid = $derived(
    fwd.map((f) => !validPort(f.host_port) || !validPort(f.guest_port))
  );
  const fwdDuplicate = $derived.by(() => {
    const seen = new Set<string>();
    for (const f of fwd) {
      const key = `${f.proto}:${Number(f.host_port)}`;
      if (seen.has(key)) return true;
      seen.add(key);
    }
    return false;
  });
  const macValid = $derived(
    macAddress.trim() === "" ||
      /^([0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}$/.test(macAddress.trim())
  );
  const disksValid = $derived(
    disks.length > 0 &&
      disks.every((d) => (d.mode === "existing" ? d.path.trim().length > 0 : d.sizeGb >= 1)) &&
      new Set(disks.map((d) => d.path.trim()).filter(Boolean)).size ===
        disks.filter((d) => d.path.trim()).length
  );
  const generalInvalid = $derived(
    !name.trim() || Number(memMb) <= 0 || Number(cpus) <= 0
  );

  const tabInvalid: Record<TabId, boolean> = $derived({
    general: generalInvalid,
    disks: !disksValid,
    perf: false,
    net: fwdInvalid.some(Boolean) || fwdDuplicate || !macValid,
    adv: false,
  });
  const firstInvalidLabel = $derived(
    TABS.find((t) => tabInvalid[t.id])?.label ?? null
  );

  let busy = $state(false);
  const canSubmit = $derived(
    !!name.trim() && !busy && !fwdDuplicate && !fwdInvalid.some(Boolean) && macValid && disksValid
  );

  let nameEl = $state<HTMLInputElement | null>(null);
  $effect(() => {
    queueMicrotask(() => nameEl?.focus());
  });

  function num(v: number | ""): number | null {
    return v === "" ? null : Number(v);
  }

  function buildDraft(): VmDraft {
    const specDisks: DiskSpec[] = disks.map((d) =>
      d.mode === "new"
        ? {
            kind: "new",
            size_gb: Number(d.sizeGb),
            folder: d.folder.trim() || null,
            bus: d.bus,
            cache: d.cache,
            aio: d.aio,
            discard_unmap: d.discard_unmap,
            iothread: d.iothread,
          }
        : {
            kind: "existing",
            path: d.path.trim(),
            bus: d.bus,
            cache: d.cache,
            aio: d.aio,
            discard_unmap: d.discard_unmap,
            iothread: d.iothread,
          }
    );
    return {
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
      mac_address: macAddress.trim() || null,
      tap_ifname: tapIfname.trim() || null,
      tap_br: tapBr.trim() || null,
      hostfwd:
        netMode === "nat"
          ? fwd.map((f) => ({
              proto: f.proto,
              host_port: Number(f.host_port),
              guest_port: Number(f.guest_port),
            }))
          : [],
      ssh: {
        host: sshHost.trim() || DEFAULT_SSH.host,
        port: Number(sshPort) || DEFAULT_SSH.port,
        user: sshUser.trim() || DEFAULT_SSH.user,
      },
      boot_first: bootFirst,
      boot_menu: bootMenu,
      smp_sockets: num(smpSockets),
      smp_cores: num(smpCores),
      smp_threads: num(smpThreads),
      maxcpus: num(maxcpus),
      rng,
      usb_tablet: usbTablet,
      vga,
      vgamem_mb: num(vgamemMb),
      gl,
      mem_prealloc: memPrealloc,
      mem_path: memPath.trim() || null,
      rtc,
      audio,
      vnc_password_auth: vncPasswordAuth,
      disks: specDisks,
    };
  }

  /** Canonical stored-form disks for update (create sends DiskSpec[] instead). */
  function buildEntryDisks() {
    return disks.map((d, i) => ({
      id: d.origId ?? `d${i}`,
      path: d.mode === "new" ? "" : d.path.trim(),
      size_gb: d.origId ? Number(d.sizeGb) : 0,
      owned: d.origId ? d.owned : false,
      opts: {
        bus: d.bus,
        cache: d.cache,
        aio: d.aio,
        discard_unmap: d.discard_unmap,
        iothread: d.iothread,
      },
    }));
  }

  async function submit() {
    busy = true;
    try {
      if (isEdit) {
        const merged = {
          ...existing!,
          ...buildDraft(),
          disks: buildEntryDisks(),
          id: existing!.id,
          vnc_display: existing!.vnc_display,
        };
        await api.updateVm(merged as unknown as Vm);
        toast(`VM "${name}" updated`, "success");
      } else {
        await api.createVm(buildDraft());
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

  function onkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && canSubmit) {
      e.preventDefault();
      void submit();
    }
  }

  const AUDIOS = audioBackends();
</script>

<svelte:window {onkeydown} />

<Modal
  title={isEdit ? `Edit VM — ${existing!.name}` : "Create a virtual machine"}
  width="700px"
  {onclose}
>
  <div class="tabs" role="tablist">
    {#each TABS as t}
      <button
        role="tab"
        aria-selected={tab === t.id}
        class="tab"
        class:active={tab === t.id}
        onclick={() => (tab = t.id)}
      >
        {t.label}
        {#if tabInvalid[t.id]}<span class="err-dot"></span>{/if}
      </button>
    {/each}
  </div>

  <div class="body pane-host">
    {#if tab === "general"}
      <div class="pane">
        <div class="row2">
          <label class="field"><span>Name</span>
            <input bind:this={nameEl} type="text" bind:value={name} placeholder="my-vm" />
          </label>
          <div class="pair">
            <label class="field"><span>Memory, MB</span>
              <input type="number" min="16" step="64" bind:value={memMb} />
            </label>
            <label class="field"><span>vCPUs</span>
              <input type="number" min="1" max="128" bind:value={cpus} />
            </label>
          </div>
        </div>
        <div class="chips">
          {#each MEM_PRESETS as m}
            <button type="button" class="chip" class:active={memMb === m} onclick={() => (memMb = m)}>
              {m >= 1024 ? `${m / 1024} GB` : `${m} MB`}
            </button>
          {/each}
        </div>

        <div class="row3">
          <label class="field"><span>Machine</span>
            <select bind:value={machine}>
              <option value="q35">q35 (modern)</option>
              <option value="i440fx">i440fx (legacy)</option>
              <option value="microvm">microvm</option>
              <option value="auto">auto (default)</option>
            </select>
          </label>
          <label class="field"><span>Firmware</span>
            <select bind:value={firmware}>
              <option value="bios">BIOS (SeaBIOS)</option>
              <option value="uefi">UEFI (OVMF)</option>
            </select>
          </label>
          <label class="field"><span>CPU model</span>
            <select bind:value={cpuModel}>
              {#each ["auto", "max", "host"] as c}<option value={c}>{c}</option>{/each}
            </select>
          </label>
        </div>

        <div class="row3">
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
          <div class="field"><span>ISO</span>
            <div style="display:flex; gap:6px">
              <input type="text" bind:value={iso} placeholder=".iso path" style="flex:1" />
              {#if iso}
                <button title="Clear ISO" onclick={() => (iso = "")}>✕</button>
              {:else}
                <button onclick={pickIso}>…</button>
              {/if}
            </div>
          </div>
        </div>
        {#if display !== "none"}
          <p class="hint">
            {#if display === "vnc"}Console opens through noVNC in a separate window.{/if}
            {#if machine === "microvm"}microvm is headless-only.{/if}
          </p>
        {/if}
      </div>
    {:else if tab === "disks"}
      <div class="pane">
        <div class="pane-head">
          <p class="hint" style="margin:0">
            {#if isEdit}Detaching keeps files on disk; new images are not created here.{:else}The first disk is the system disk.{/if}
          </p>
          <button class="ghost" disabled={disks.length >= 8} onclick={addDisk}>+ Add disk</button>
        </div>

        {#each disks as d, i}
          <div class="disk-card">
            <div class="disk-head">
              <b>d{i}</b>
              {#if isEdit}
                <span class="mono dim path-label">{d.path}</span>
                {#if d.owned}<span class="own-badge">managed</span>{:else}<span class="ext-badge">external</span>{/if}
              {:else}
                <button type="button" class="chip" class:active={d.mode === "new"} onclick={() => (d.mode = "new")}>create new</button>
                <button type="button" class="chip" class:active={d.mode === "existing"} onclick={() => (d.mode = "existing")}>use existing</button>
              {/if}
              <span style="flex:1"></span>
              {#if disks.length > 1 || isEdit}
                <button class="ghost danger-text" title="Detach this disk (files are never deleted here)" onclick={() => disks.splice(i, 1)}>✕</button>
              {/if}
            </div>

            {#if !isEdit && d.mode === "new"}
              <div class="row2">
                <label class="field"><span>Size — {d.sizeGb} GB</span>
                  <input type="range" min="4" max="512" step="4" bind:value={d.sizeGb} />
                </label>
                <div class="field"><span>Folder</span>
                  <div style="display:flex; gap:6px">
                    <input type="text" bind:value={d.folder} placeholder="default storage dir" style="flex:1" />
                    <button onclick={() => pickFolder(i)}>…</button>
                  </div>
                </div>
              </div>
            {:else if (!isEdit && d.mode === "existing") || (isEdit && !d.owned)}
              <div class="field">
                <div style="display:flex; gap:6px">
                  <input type="text" bind:value={d.path} placeholder="path to .qcow2" style="flex:1" />
                  <button onclick={() => pickQcow2(i)}>…</button>
                </div>
              </div>
            {/if}

            <div class="opts-row">
              <label class="mini"><span>Bus</span>
                <select bind:value={d.bus}>
                  <option value="virtio-blk">virtio (fast)</option>
                  <option value="nvme">nvme (fast)</option>
                  <option value="sata">sata/ahci (q35)</option>
                  <option value="ide">ide (compat)</option>
                </select>
              </label>
              <label class="mini"><span>Cache</span>
                <select bind:value={d.cache}>
                  <option value="auto">auto</option>
                  <option value="none">none</option>
                  <option value="writeback">writeback</option>
                  <option value="writethrough">writethrough</option>
                  <option value="unsafe">unsafe</option>
                </select>
              </label>
              <label class="mini"><span>AIO</span>
                <select bind:value={d.aio}>
                  <option value="auto">auto</option>
                  <option value="threads">threads</option>
                  <option value="native">native</option>
                  <option value="io-uring">io_uring (linux)</option>
                </select>
              </label>
              <label class="check"><input type="checkbox" bind:checked={d.discard_unmap} /> discard/TRIM</label>
              <label class="check"><input type="checkbox" bind:checked={d.iothread} /> iothread</label>
            </div>
          </div>
        {/each}
        <p class="hint">Windows guests without virtio drivers: pick nvme or sata for the install disk.</p>
      </div>
    {:else if tab === "perf"}
      <div class="pane">
        <div class="row4">
          <label class="field"><span>SMP sockets</span>
            <input type="number" min="1" bind:value={smpSockets} placeholder="—" />
          </label>
          <label class="field"><span>Cores/socket</span>
            <input type="number" min="1" bind:value={smpCores} placeholder="—" />
          </label>
          <label class="field"><span>Threads/core</span>
            <input type="number" min="1" bind:value={smpThreads} placeholder="—" />
          </label>
          <label class="field"><span>maxcpus</span>
            <input type="number" min="1" bind:value={maxcpus} placeholder="—" />
          </label>
        </div>
        <p class="hint">Empty fields fall back to QEMU defaults ({cpus} vCPU flat topology).</p>

        <div class="checks">
          <label class="check" title="Paravirtual RNG improves guest entropy (boot speed)">
            <input type="checkbox" bind:checked={rng} /> virtio-rng entropy device
          </label>
          <label class="check" title="Pointer integration for VNC/GTK windows">
            <input type="checkbox" bind:checked={usbTablet} /> USB tablet pointer
          </label>
          <label class="check" title="Preallocate all guest memory at start (KVM/huge pages)">
            <input type="checkbox" bind:checked={memPrealloc} /> mem-prealloc
          </label>
          <label class="check" title="VirGL 3D acceleration — Linux hosts, GTK/SDL displays">
            <input type="checkbox" bind:checked={gl} disabled={machine === "microvm"} /> 3D accel (virgl)
          </label>
        </div>

        <div class="field">
          <span>Huge pages / memory file (optional)</span>
          <input type="text" bind:value={memPath} placeholder="/dev/hugepages" />
        </div>

        <div class="sep"></div>

        <div class="row3">
          <label class="field"><span>Video</span>
            <select bind:value={vga}>
              <option value="auto">auto</option>
              <option value="std">std vga</option>
              <option value="virtio">virtio-gpu</option>
              <option value="qxl">qxl</option>
              <option value="ramfb">ramfb (uefi fb)</option>
              <option value="none">off (headless)</option>
            </select>
          </label>
          <label class="field"><span>VGA memory MB</span>
            <select bind:value={vgamemMb} disabled={vga !== "std" && vga !== "qxl"}>
              <option value="">default</option>
              {#each [16, 32, 64, 128, 256] as m}<option value={m}>{m}</option>{/each}
            </select>
          </label>
          <label class="field"><span>Audio</span>
            <select bind:value={audio}>
              {#each AUDIOS as a}<option value={a.value}>{a.label}</option>{/each}
            </select>
          </label>
        </div>
      </div>
    {:else if tab === "net"}
      <div class="pane">
        <div class="row3">
          <label class="field"><span>Network mode</span>
            <select bind:value={netMode}>
              <option value="nat">NAT (user)</option>
              <option value="bridged">Bridged (tap)</option>
              <option value="none">None</option>
            </select>
          </label>
          <label class="field"><span>NIC model</span>
            <select bind:value={netModel} disabled={netMode === "none"}>
              {#each [["auto", "auto (e1000)"], ["virtio", "virtio-net"], ["e1000", "e1000"], ["rtl8139", "rtl8139"]] as [v, l]}
                <option value={v}>{l}</option>
              {/each}
            </select>
          </label>
          <label class="field"><span>MAC address</span>
            <input
              type="text"
              bind:value={macAddress}
              placeholder="(stable auto)"
              class:invalid={!macValid}
            />
          </label>
        </div>

        {#if netMode === "bridged"}
          <div class="row2">
            <label class="field"><span>TAP interface</span>
              <input type="text" bind:value={tapIfname} placeholder="tap0" />
            </label>
            <label class="field"><span>Bridge</span>
              <input type="text" bind:value={tapBr} placeholder="br0" />
            </label>
          </div>
        {/if}

        {#if netMode === "nat"}
          <div class="pane-head" style="margin-top:4px">
            <b class="sec">Port forwarding (hostfwd)</b>
            <button class="ghost" onclick={addFwd}>+ Add rule</button>
          </div>
          {#if fwd.length === 0}
            <p class="hint">No rules. Example: tcp 2222 → guest 22 gives SSH access.</p>
          {/if}
          {#each fwd as f, i}
            <div class="fwd-row">
              <select bind:value={f.proto} style="width:76px">
                <option value="tcp">tcp</option>
                <option value="udp">udp</option>
              </select>
              <input
                type="number" min="1" max="65535" placeholder="host port"
                class:invalid={fwdInvalid[i]}
                bind:value={f.host_port}
                style="width:110px"
              />
              <span style="color:var(--fg3)">→ guest :</span>
              <input
                type="number" min="1" max="65535" placeholder="guest port"
                class:invalid={fwdInvalid[i]}
                bind:value={f.guest_port}
                style="width:110px"
              />
              <button class="ghost danger-text" title="Remove" onclick={() => removeFwd(i)}>✕</button>
              {#if validPort(f.guest_port)}
                <button
                  type="button"
                  class="chip"
                  title="Use this rule's host port for SSH"
                  onclick={() => (sshPort = f.host_port)}
                >ssh ↵</button>
              {/if}
            </div>
          {/each}
          {#if fwdInvalid.some(Boolean)}
            <p class="err-note">Ports must be integers between 1 and 65535.</p>
          {:else if fwdDuplicate}
            <p class="err-note">Duplicate host port for the same protocol.</p>
          {/if}
        {/if}
        {#if !macValid}
          <p class="err-note">MAC must look like 52:54:00:12:34:56.</p>
        {/if}
      </div>
    {:else}
      <div class="pane">
        <div class="row3">
          <label class="field"><span>First boot device</span>
            <select bind:value={bootFirst}>
              <option value="disk">disk</option>
              <option value="cd">cdrom</option>
              <option value="network">network (pxe)</option>
            </select>
          </label>
          <label class="field"><span>RTC clock</span>
            <select bind:value={rtc}>
              <option value="utc">utc (linux default)</option>
              <option value="localtime">localtime (windows)</option>
            </select>
          </label>
          <div class="checks" style="align-self:end; padding-bottom:4px">
            <label class="check"><input type="checkbox" bind:checked={bootMenu} /> boot menu (F12)</label>
            <label class="check" title="VNC requires a password set after start">
              <input type="checkbox" bind:checked={vncPasswordAuth} /> VNC password auth
            </label>
          </div>
        </div>
        {#if bootFirst === "disk" && !isEdit}
          <p class="hint">With an ISO attached, the CD boots once — then the disk takes over automatically.</p>
        {/if}

        <div class="sep"></div>

        <div class="pane-head">
          <b class="sec">SSH target</b>
          <button type="button" class="chip" onclick={resetSsh} title="Restore defaults (root@127.0.0.1:22)">
            reset
          </button>
        </div>
        <div class="row3">
          <label class="field"><span>Host</span>
            <input type="text" bind:value={sshHost} placeholder="127.0.0.1" />
          </label>
          <label class="field"><span>Port</span>
            <input type="number" min="1" max="65535" bind:value={sshPort} />
          </label>
          <label class="field"><span>User</span>
            <input type="text" bind:value={sshUser} placeholder="root" />
          </label>
        </div>
        {#if fwdSshRules.length > 0}
          <div class="chips">
            {#each fwdSshRules as r}
              <button
                type="button"
                class="chip"
                class:active={Number(sshPort) === r.host_port}
                title="Fill SSH port from this hostfwd rule"
                onclick={() => (sshPort = r.host_port)}
              >
                {r.proto} :{r.host_port} → :{r.guest_port}
              </button>
            {/each}
          </div>
          <p class="hint">With NAT, use the host port of a forwarding rule as the SSH port.</p>
        {/if}
      </div>
    {/if}
  </div>

  <footer>
    <span class="foot-hint">
      {#if firstInvalidLabel}
        <span class="err-note" style="margin:0">
          Fix “{firstInvalidLabel}” to continue
        </span>
      {:else}
        <span class="dim">Ctrl+Enter — save</span>
      {/if}
    </span>
    <span style="flex:1"></span>
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={submit} disabled={!canSubmit}>
      {isEdit ? "Save changes" : "Create VM"}
    </button>
  </footer>
</Modal>

<style>
  /* ---- tabs ---- */
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    position: relative;
    padding: 10px 13px 9px;
    border: none;
    background: transparent;
    border-radius: 0;
    color: var(--fg3);
    font-size: 12.5px;
    letter-spacing: 0.02em;
  }
  .tab:hover:not(.active) { color: var(--fg); background: transparent; }
  .tab.active {
    color: var(--fg);
    font-weight: 600;
    background: transparent;
  }
  .tab.active::after {
    content: "";
    position: absolute;
    left: 10px; right: 10px; bottom: -1px;
    height: 2px;
    background: var(--accent);
  }
  .err-dot {
    display: inline-block;
    width: 6px; height: 6px;
    margin-left: 6px;
    border-radius: 50%;
    background: var(--err);
    vertical-align: 1px;
  }

  .pane-host { flex: 1; min-height: 0; display: flex; }
  .pane { flex: 1; display: flex; flex-direction: column; gap: 12px; align-content: start; }
  .pane-head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
  }
  .sec { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--fg3); }

  .row2 { display: grid; grid-template-columns: 1fr auto; gap: 10px; align-items: end; }
  .row2 .pair { display: grid; grid-template-columns: auto auto; gap: 10px; }
  .row3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .row4 { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }

  .chips { display: flex; gap: 6px; margin-top: -6px; flex-wrap: wrap; }
  .checks { display: flex; flex-wrap: wrap; gap: 10px 18px; }
  .check {
    display: flex; gap: 7px; align-items: center;
    font-size: 12.5px; color: var(--fg2); cursor: pointer; user-select: none;
  }
  .sep { height: 1px; background: var(--line); margin: 0; }
  .hint { color: var(--fg3); font-size: 12px; margin: 0; }
  .dim { color: var(--fg3); }
  input.invalid { border-color: var(--err); }
  .err-note { color: var(--err); font-size: 12px; margin: 0; }

  /* ---- disks ---- */
  .disk-card {
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .disk-head { display: flex; gap: 8px; align-items: center; }
  .path-label { font-size: 11.5px; word-break: break-all; }
  .own-badge, .ext-badge {
    font-size: 10px; text-transform: uppercase; letter-spacing: .06em;
    padding: 2px 7px; border-radius: 999px; border: 1px solid var(--line2);
  }
  .ext-badge { color: var(--warn); border-color: color-mix(in srgb, var(--warn) 40%, transparent); }
  .opts-row { display: flex; gap: 10px; flex-wrap: wrap; align-items: end; }
  .mini { display: flex; flex-direction: column; gap: 3px; }
  .mini span { font-size: 10.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--fg3); }

  /* ---- network ---- */
  .fwd-row { display: flex; gap: 6px; margin-bottom: 0; align-items: center; }

  /* ---- footer ---- */
  footer { justify-content: flex-start; }
  .foot-hint { font-size: 12px; }
</style>
