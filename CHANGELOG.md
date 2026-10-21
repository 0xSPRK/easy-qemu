# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Repository audit and v0.3 plan in `docs/AUDIT.md` (44 findings, hypotheses
  A-J, five-PR remediation plan).
- `AGENTS.md` with the build gates, module map and the invariants that are
  easy to break.
- `CONTRIBUTING.md`, `SECURITY.md`, pull request and issue templates.
- `svelte-check` type checking (`npm run check`) and a matching CI job.
- CI matrix for Windows and Linux, workspace-wide format, clippy and test
  gates, a real `tauri build` step and a dependency audit workflow.
- Dependabot updates for cargo, npm and GitHub Actions.
- `scripts/bump-version.mjs` to keep the version in one place.

### Changed
- `.gitignore` covers `src-tauri/target`, `*.tmp`, `.env*`, editor
  directories and the machine-specific MCP configuration.
- `.mcp.json` and `.vscode/mcp.json` are no longer tracked: they contained an
  absolute local path.
- Both `package.json` files declare the `GPL-3.0-only` license.
- The core tests that create a disk need `qemu-img`: it is installed on the
  Linux runner, and on the Windows runner those tests skip themselves instead
  of failing.

### Removed
- Generated, machine-specific Repowise block in `.claude/CLAUDE.md`; the file
  now points at `AGENTS.md`.

## [0.2.0] - 2026-10-21

### Added
- Multiple disks per VM with per-disk bus (virtio-blk, NVMe, SATA-AHCI, IDE),
  I/O mode, cache mode and discard/unmap.
- Import of existing `qcow2`/`qcow` images; such disks are never deleted by
  the manager.
- Live snapshots of running VMs across all disks through QMP, next to the
  existing offline `qemu-img` snapshots.
- Live statistics: host CPU% of the QEMU process, per-disk block statistics
  and guest memory through the QMP balloon.
- Per-VM SSH target (host, port, user) with a terminal launcher and
  host-forward port autofill.
- QMP commands for key combinations, screenshots, CD tray control, memory
  ballooning, block statistics and migration.

### Changed
- VM form reworked into machine, CPU, firmware, network, audio and display
  sections with memory and disk presets.
- Details panel shows the live statistics and hosts the snapshot manager.

### Fixed
- Records written by earlier versions with a single `disk_path` are migrated
  to `disks[]` on load.

## [0.1.0] - 2026-08-24

### Added
- Initial core: VM store, `config.toml`, QEMU launcher, async QMP client,
  websockify proxy for the noVNC console, offline snapshots, accelerator
  probe.
- Tauri shell with 26 IPC commands, status event polling and separate
  console windows.
- Svelte 5 frontend: VM list, details, form, settings, snapshot manager,
  log viewer.
- Initial GitHub Actions workflow.

[Unreleased]: https://github.com/0xSPRK/easy-qemu/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/0xSPRK/easy-qemu/releases/tag/v0.2.0
[0.1.0]: https://github.com/0xSPRK/easy-qemu/releases/tag/v0.1.0
