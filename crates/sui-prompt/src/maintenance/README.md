# `sui prompt` maintenance content

Refresh records and provenance for the embedded skill bundles. This directory
is itself **not embedded** in the binary — `build.rs` walks `src/skills/` and
`src/categories/` only, so anything under `maintenance/` is invisible to
runtime agents.

## Layout

- `UPSTREAMS.md` — cross-cutting pinned upstream references.
- `<skill>/LINEAGE.md` — per-skill derivation methodology and refresh
  protocol. Currently: `sui-move-security-review/`, `official-sui-skills/`.
