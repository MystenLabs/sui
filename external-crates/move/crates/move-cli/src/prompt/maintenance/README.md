# `move prompt` maintenance content

This directory holds **all content related to maintaining `move prompt`** — for example,
agentic skills that help a maintainer (or a maintenance-bot agent) refresh or extend the
embedded bundles, and any artifacts those workflows need.

**Not embedded in the binary.** `move-cli/build.rs` walks `src/prompt/skills/` and
`src/prompt/categories/` only, so anything under `maintenance/` is invisible to runtime
agents using `move prompt`. The exclusion is by directory location — no skip-lists or
filters.

## What's currently here

- `sui-move-security-review/LINEAGE.md` — provenance of the `SM-*` catalog (pinned
  `MystenLabs/skills` ref, file set scanned at derivation, refresh protocol).

Update this section as content lands.
