# Easy QEMU

[![CI](https://github.com/0xSPRK/easy-qemu/actions/workflows/ci.yml/badge.svg)](https://github.com/0xSPRK/easy-qemu/actions/workflows/ci.yml)
[![Dependency audit](https://github.com/0xSPRK/easy-qemu/actions/workflows/audit.yml/badge.svg)](https://github.com/0xSPRK/easy-qemu/actions/workflows/audit.yml)
[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-blue.svg)](LICENSE)

Cross-platform desktop manager for QEMU virtual machines — Windows-first, Linux
supported.

Tauri 2 · Svelte 5 · Rust core · VM console through an embedded noVNC.

Requires **QEMU ≥ 6.0** (live snapshots and saved state need it; most other
features work on older builds too).

## Screenshots

Screenshots live in [`docs/img/`](docs/img). Until they are added, this
section stays empty — see the issue tracker for the current state.

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
- VNC password authentication (QEMU side; the noVNC console currently
  connects without credentials — see [`docs/AUDIT.md`](docs/AUDIT.md), F-05)

### Snapshots & state
- Offline qcow2 snapshots per disk (qemu-img) — with readable dates
- Live snapshots of running VMs across all disks (QEMU ≥ 6.0)
- Save state / resume later (migration to a file), like VirtualBox "saved state"

### Networking & misc
- NAT user-net with port forwarding (tcp/udp) and configurable bind address
- Bridged mode via TAP (interface/bridge options, no default scripts)
- Headless / VNC / GTK / SDL display modes
- Console in a separate window through noVNC (WebSocket proxy on 127.0.0.1)
- Per-VM SSH launcher (remembered target, terminal of your choice)
- Per-VM QEMU log viewer; safe deletion of only manager-owned disks

## Installation

### Windows

Download the NSIS installer from the releases page and run it. It needs the
[WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/),
which is preinstalled on current Windows 10/11. QEMU itself is **not**
bundled — install it separately (for example from
[qemu.weilnetz.de](https://qemu.weilnetz.de/w64/)) and point Easy QEMU at
`qemu-system-x86_64.exe` if it is not on `PATH`.

### Linux

There is no Linux package yet — build it from the sources:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev patchelf   # Tauri build dependencies
npm install && npm --prefix ui install
npm run build                       # -> target/release/bundle/
```

## Quick start

1. Start Easy QEMU; the *Detected environment* panel shows which QEMU build
   and accelerators were found.
2. **New VM** — pick a machine type, memory and CPU count, then add a disk
   (a new qcow2 image or an existing one).
3. **Start** — Easy QEMU builds the QEMU command line, starts the process
   detached and shows the live status.
4. **Console** — opens a separate window with the noVNC view of the guest.
5. **Snapshots** — create offline or live snapshots per disk from the details
   panel.

## Data locations

| What | Where |
|------|-------|
| Config | `%APPDATA%/easy-qemu/config.toml` (Linux: `~/.config/easy-qemu`) |
| VM records | `<config>/vms/*.json` |
| Runtime state | `<config>/running.json` |
| Disks and logs | directory from the settings (default `<config>/disks`) |
| Per-VM files | `<id>.pid`, `<id>-vars.fd`, `<id>.log`, `<id>.sav` (saved state), `screenshots/` |

VM records created by v0.2 (single `disk_path`) are migrated automatically on
load. The on-disk formats are stable across releases; see
[`AGENTS.md`](AGENTS.md) for the policy.

## Limitations

* The console proxy and the VNC port are reachable by other local processes;
  the proxy is not yet token-protected. Forwarded guest ports bind all
  interfaces by default — set a bind address in the settings if that is not
  what you want.
* There is no Linux package yet, and no automated release build.
* `swtpm` is not supported: TPM state has to be provided by the guest image.

## FAQ

**No acceleration on Windows.** Windows needs WHPX: enable *Windows
Hypervisor Platform* in the Windows features, or the VM falls back to slow
TCG emulation.

**No acceleration on Linux.** Enable KVM (`/dev/kvm` must be readable by the
user) and make sure the user is in the `kvm` group.

**UEFI VM does not boot.** Easy QEMU needs an OVMF build. Install `ovmf` or
point the firmware setting at your own `OVMF_CODE.fd` / `OVMF_VARS.fd`.

**The console is black.** The guest has no framebuffer yet (no display
driver installed) or the VM is paused. Check the QEMU log in the details
panel.

**Ports are already in use.** Change the host port of the forward or stop
the program that holds it.

## Development

Requirements: Rust (MSVC on Windows), Node 20+, WebView2 on Windows.

```bash
npm install                # Tauri CLI
npm --prefix ui install    # frontend dependencies
npm run dev                # run in development mode
```

The gates that must pass before a commit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
npm run check              # svelte-check
npm --prefix ui run build
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full workflow and
[`AGENTS.md`](AGENTS.md) for the code map.

## Documentation

| Document | Contents |
|----------|----------|
| [`AGENTS.md`](AGENTS.md) | module map, commands, invariants |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | setup, gates, commit and PR rules |
| [`SECURITY.md`](SECURITY.md) | threat model and reporting channel |
| [`CHANGELOG.md`](CHANGELOG.md) | release history |
| [`docs/AUDIT.md`](docs/AUDIT.md) | Phase 0 audit, open findings, v0.3 plan |

## License

GPL-3.0-only — see [LICENSE](LICENSE).
