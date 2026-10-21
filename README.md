# Easy QEMU

Cross-platform GUI manager for QEMU virtual machines (Windows-first, Linux supported).

Tauri 2 + Svelte 5 · Rust core (`crates/core`) · VM console via embedded noVNC.

Requires **QEMU ≥ 6.0** (live snapshots / saved state); most features work on older builds too.

## Features

### Machines
- Create / edit / delete VMs; multiple disks per VM (mixed buses)
- Disk bus per disk: virtio-blk / NVMe / SATA-AHCI (q35) / IDE
- BIOS (SeaBIOS) or UEFI (OVMF), incl. portable QEMU installs
- q35 / i440fx / microvm platforms; automatic accelerator detection (WHPX/KVM/TCG)

### Performance tuning
- MTTCG multi-threaded TCG for multi-vCPU guests
- Per-disk `cache`, `aio` (native/io_uring), `discard=unmap`, dedicated iothread
- CPU topology (`sockets/cores/threads/maxcpus`), host/max CPU models
- virtio-rng entropy device, USB tablet pointer integration
- Video model (std/virtio/qxl/ramfb) + VGA memory, VirGL 3D accel (Linux)
- Huge pages / memory backend file, `-mem-prealloc`
- Deterministic auto-generated MAC addresses

### Runtime control (QMP)
- Pause / resume / reset / ACPI shutdown / suspend & wake / NMI
- Insert/eject ISO without stopping the VM
- Send key combos to the guest, screenshots to PNG
- Memory ballooning with live stats (CPU %, per-disk I/O)
- VNC password authentication

### Snapshots & state
- Offline qcow2 snapshots per disk (qemu-img) — now with readable dates
- Live snapshots of running VMs across all disks (QEMU ≥ 6.0)
- Save state / resume later (migration to a file), like VirtualBox "saved state"

### Networking & misc
- NAT user-net with port forwarding (tcp/udp) and configurable bind address
- Bridged mode via TAP (interface/bridge options, no default scripts)
- Headless / VNC / GTK / SDL display modes
- Console in a separate window through noVNC (WebSocket proxy on 127.0.0.1)
- Per-VM SSH launcher (remembered target, terminal of your choice)
- Per-VM QEMU log viewer; safe deletion of only manager-owned disks

## Layout

```
crates/core   UI-free core: store/config/qemu/qmp/proxy/snapshots/stats/manager
src-tauri     Tauri glue: commands, status events, console windows
ui            frontend (Vite + Svelte 5 + TS); noVNC is bundled via npm
```

## Development

Requirements: Rust (MSVC), Node 20+, WebView2 Runtime (built into Win10/11).

```bash
npm install                       # installs ui/ dependencies
cargo test -p easy-qemu-core      # core tests
npm run dev                       # run in development mode (alias: npm run tauri dev)
```

## Build

```bash
npm run build                     # NSIS installer in target/release/bundle/
```

## Data locations

- Config: `%APPDATA%/easy-qemu/config.toml` (Linux: `~/.config/easy-qemu`)
- VM records: `<config>/vms/*.json`, runtime state: `running.json`
- Disks and logs: directory from settings (default `<config>/disks`)
- Manager-owned per-VM files live in the config dir: `<id>.pid`, `<id>-vars.fd`,
  `<id>.log`, `<id>.sav` (saved state), `screenshots/`

VM records created by v0.2 (single `disk_path`) are migrated automatically on load.

## License

GPL-3.0-only — see [LICENSE](LICENSE).
