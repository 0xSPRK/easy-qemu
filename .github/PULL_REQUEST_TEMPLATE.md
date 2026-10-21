## What does this change?

<!-- Describe the user-visible change and the reason for it. -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / internal cleanup
- [ ] Documentation
- [ ] Build, CI or dependency change

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --no-fail-fast`
- [ ] `npm run check` (svelte-check)
- [ ] `npm --prefix ui run build`
- [ ] Screenshots or a recording for UI changes
- [ ] No secrets, local paths or VM records in the diff

## On-disk format

- [ ] Not touched
- [ ] Touched — a schema version and a migration that keeps existing
      installations loadable are included (see `AGENTS.md`)

## Dependencies

<!-- New dependency? Explain size, license and GPL-3.0 compatibility. -->

## Notes for the reviewer

<!-- Anything that needs extra attention: risky areas, follow-ups, known gaps. -->
