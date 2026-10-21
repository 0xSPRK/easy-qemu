# AGENTS.md

Working notes for humans and AI agents changing this repository.
Read this before editing code; it encodes the invariants that are easy to break.

## What this is

Easy QEMU is a cross-platform desktop manager for QEMU virtual machines.
Tauri 2 shell, Svelte 5 frontend, UI-free Rust core, GPL-3.0-only.

## Layout

| Path | Contents |
|------|----------|
| `crates/core/` | UI-free core: domain model, store, config, QEMU launcher, QMP client, websockify proxy, snapshots, stats, orchestrator |
| `src-tauri/` | Tauri glue: 26 IPC commands, status event polling, console windows |
| `ui/` | Frontend (Vite + Svelte 5 + TypeScript), noVNC bundled through npm |

`crates/core` must stay free of Tauri and frontend dependencies — that is what
makes it testable without a window. The core is the only place allowed to
touch the filesystem or spawn processes; `src-tauri` only forwards IPC calls,
and `ui` only renders.

## Commands

```bash
# Rust
cargo fmt --all -- --check                                  # formatting gate
cargo clippy --workspace --all-targets -- -D warnings       # lint gate
cargo test --workspace --no-fail-fast                        # tests
cargo test -p easy-qemu-core                                # core tests only (fast)

# Frontend
npm run check                    # svelte-check type check (root proxy to ui/)
npm --prefix ui run build        # production bundle

# Full app
npm run dev                      # tauri dev
npm run build                    # tauri build -> NSIS installer
```

All five gates must pass before a commit. `cargo fmt` is not optional: CI
runs `cargo fmt --all -- --check` and the app clippy step no longer tolerates
warnings.

## Core modules

| Module | Responsibility |
|--------|----------------|
| `vm.rs` | Domain model: `Vm`, `VmDraft`, `DiskEntry`, enums, validation, legacy migration |
| `manager.rs` | Orchestrator: create/update/delete, start/stop, status refresh, snapshots, stats, console lifecycle |
| `qemu.rs` | Builds the QEMU command line, resolves binaries, starts the process, manages pidfile and log |
| `qmp.rs` | Async QMP client (JSON line protocol over TCP) |
| `proxy.rs` | axum websockify proxy for the noVNC console |
| `store.rs` | Persistence: `vms/*.json`, `running.json`, disk deletion |
| `config.rs` | `config.toml` |
| `snapshots.rs` | Offline snapshots through `qemu-img` |
| `stats.rs` | Host CPU% of the QEMU process, block stats, balloon |
| `process.rs` | PID liveness and forced termination |
| `ssh.rs` | External terminal launcher for `ssh` |

## Invariants

1. **On-disk formats are stable.** `config.toml`, `vms/*.json` and
   `running.json` are read by existing installations. Never change their
   shape without a schema version and a migration that keeps v0.2 records
   loadable. `Vm::normalize()` migrates the legacy single `disk_path` field
   to `disks[]`; new migrations follow the same pattern.
2. **No shell.** QEMU is started with `Command::new(bin).args(&args)` — an
   argument vector, never a command string. Keep it that way; a shell
   re-introduces injection.
3. **No destructive action outside the manager's own files.** A disk is
   deleted only when the manager created it. `owned` is a flag in a JSON
   file the user can edit, so it is not a trust boundary on its own.
4. **Networking stays on loopback** unless the user explicitly opts out:
   console proxy on `127.0.0.1` with an ephemeral port, VNC on `127.0.0.1`.
5. **Tauri capabilities stay minimal.** `core:default` and `dialog:default`
   only. Do not add `fs`, `shell` or `http` without a threat-model note in
   the pull request.
6. **Errors are human-readable English strings** returned to the UI
   (`anyhow` with context). The core uses `anyhow`; there is no typed error
   enum yet.
7. **The core never imports Tauri or frontend code.** `crates/core` compiles
   standalone.

## Conventions

* Rust: edition 2021, `rustfmt` defaults, `clippy -D warnings`.
* Comments and user-facing messages in English.
* New dependencies need a note on size, license and GPL-3.0 compatibility.
* UI: Svelte 5 runes (`$state`, `$derived`), no new component libraries.
* Prefer small, focused commits; one logical change per commit.

## Known gaps

`docs/AUDIT.md` holds the Phase 0 audit of the v0.2 code base: 44 findings
(`F-01`..`F-44`), the verification of the security hypotheses A-J and the
plan for the v0.3 cycle. Read it before touching path handling, the console
proxy, the QMP transport or the persistence layer — that is where the open
findings are.
