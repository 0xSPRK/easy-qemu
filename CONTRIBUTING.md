# Contributing to Easy QEMU

Thanks for taking the time. This document covers how to build, test and
propose a change.

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | stable | MSVC toolchain on Windows |
| Node.js | 20+ | ships the Tauri CLI and the frontend toolchain |
| QEMU | 6.0+ | older builds work except live snapshots and saved state |
| WebView2 | - | preinstalled on Windows 10/11; on Linux install `webkit2gtk-4.1` |

Linux build dependencies for the Tauri shell:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  libxdo-dev libssl-dev patchelf
```

## Setup

```bash
npm install                # Tauri CLI (root)
npm --prefix ui install    # frontend dependencies
```

## Gates

Run all of them before opening a pull request. CI runs the same checks on
Windows and Linux.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
npm run check              # svelte-check
npm --prefix ui run build
```

`cargo fmt` is not optional: CI runs the format check on the whole workspace
and the clippy step treats warnings as errors.

## Branching and commits

* One logical change per pull request. Split unrelated fixes.
* Branch names: `feat/…`, `fix/…`, `docs/…`, `chore/…`, `ci/…`.
* Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`,
  `chore:`, `ci:`, `refactor:`, `test:`) with a short imperative subject and
  a body that explains the *why*.
* Never commit secrets, local paths, `config.toml` exports or VM records.

## Pull requests

1. Describe the user-visible change and the reason for it.
2. Note the gates you ran and the platforms you tested on.
3. Attach a screenshot or a short recording for UI changes.
4. If you touched `config.toml`, `vms/*.json` or `running.json`, say
   explicitly whether the on-disk format changed. A format change needs a
   schema version and a migration that keeps existing installations
   loadable — see `AGENTS.md`.
5. If you added a dependency, justify it: size, license, GPL-3.0
   compatibility.

## Code style

* Rust: edition 2021, `rustfmt` defaults, `clippy -D warnings`.
* Comments and user-facing messages in English.
* Errors are human-readable English strings; the core uses `anyhow` with
  context.
* Frontend: Svelte 5 runes (`$state`, `$derived`), no new component
  libraries.
* The core crate (`crates/core`) must not import Tauri or frontend code.

## Reporting bugs

Open an issue with the bug report template. Include:

* Easy QEMU version, OS and QEMU version,
* what you expected and what happened,
* the QEMU log of the affected VM (it lives next to the VM record as
  `<id>.log`),
* steps to reproduce.

Security issues should go through `SECURITY.md` instead of the public
issue tracker.
