<!--
Thanks for the PR! A few things that make review fast:
- Small, focused changes — one concern per PR.
- Tests for services / git ops / slug logic changes.
- Screenshot / loom for UI changes.
- `bun x tsc --noEmit` and `cd src-tauri && cargo test` both clean.

See CONTRIBUTING.md for the full guidelines.
-->

### What

<!-- One sentence. -->

### Why

<!-- What's the user-visible problem this solves, or the architectural reason for the change? -->

### How

<!-- Key implementation decisions. Tradeoffs you considered. -->

### Verification

<!-- How did you test this end-to-end? Link any related issues. -->

- [ ] `bun x tsc --noEmit` passes
- [ ] `cd src-tauri && cargo test` passes
- [ ] Manually smoke-tested the affected flow
