# Easy QEMU

Cross-platform GUI manager for QEMU virtual machines (Windows-first, Linux supported).

Tauri 2 + Svelte 5 · Rust core (`crates/core`) · VM console via embedded noVNC.

## Features

- Create / edit / delete VMs (qcow2, ISO, BIOS/UEFI-OVMF, q35/i440fx/microvm)
- Headless / VNC / GTK / SDL display modes, automatic accelerator detection (WHPX/KVM/TCG)
- Control via QMP: pause, resume, reset, ACPI shutdown, force stop
- Console in a separate window through noVNC (WebSocket proxy on 127.0.0.1)
- Port forwarding for user-NET networking (hostfwd tcp/udp)
- qcow2 snapshots (qemu-img snapshot)
- Per-VM QEMU log viewer; safe deletion of only manager-owned disks

## Layout

```
crates/core   UI-free core: store/config/qemu/qmp/proxy/snapshots/manager
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

## License

GPL-3.0-only — see [LICENSE](LICENSE).
