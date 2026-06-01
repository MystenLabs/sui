---
name: sui-audit-toolchain
description: >
  Use at the START of an on-chain Sui Move audit session to stand up the toolchain: clone Sui at
  a pinned, confirmed tag, fetch the target package's bytecode, and **disassemble it as the
  analysis source of truth** (`sui move disassemble`). Also builds the `move decompile` binary so
  confirmed findings can be rendered as readable Move source for human explanation. Trigger on
  "set up the audit tools", "disassemble this package", "get the bytecode for <package id>", or
  before applying the `sui-move-security-review` rules to an on-chain (compiled) package.
  Self-contained and shareable — depends only on public sources.
---

# Sui Audit Toolchain

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary. **Before
> executing the workflow below, enumerate the reference files (`move prompt skill
> sui-audit-toolchain --list`) and read each (`--file <ref>`).** They contain the actual
> setup, fetch, and decompile procedures; this SKILL.md frames the tools but the steps live
> in the reference files. No filesystem install is required — the binary is self-contained.

On-chain Sui packages are **bytecode**, not source. This skill bootstraps the two tools needed to
read them, from public sources only, so any teammate can reproduce an audit. The two tools play
**different roles** — do not confuse them:

- **`sui move disassemble`** — ships with the **`suiup`-managed** `sui` CLI; bytecode → assembly
  text. **This is the primary analysis source** — faithful 1:1 with the executed bytecode. Every
  `SM-*` finding is derived from this output.
- **`move decompile`** — built from the Sui repo's `move-cli` (in `$AUDIT_WORK`); bytecode →
  readable Move source. **This is a human-explanation layer**, not an analysis source: a heuristic
  reconstructor that can mis-render on edge cases. Use it only to render *already-confirmed*
  findings in familiar Move so a reader can follow them. The `move-decompiler` crate is on public
  `github.com/MystenLabs/sui` `origin/main`.

Pair this with `move-bytecode-comprehension` (how to read the output) and
`sui-move-security-review` (what to look for).

> 🧭 **Analyze on disassembly; explain with decompiled source.** All `SM-*` reasoning derives from
> `sui move disassemble` output (the executed bytecode, faithfully). When a finding is confirmed,
> render the matching decompiled `.move` snippet *alongside* it as a "Human view" so a reader can
> see the construct in familiar Move. **Never** treat a pattern visible only in decompiled output —
> but absent or different in the disassembly — as a finding. The decompiler is a presentation tool,
> not a source of truth.

> 🔒 **Isolation — non-negotiable.** Do NOT search for, or use, any pre-existing local Sui source
> checkout (`~/sui`, `~/sui-*`, any directory containing `Cargo.toml` for the Sui workspace, or any
> `*/target/*` build dir), and do NOT use any `move`/`sui` binary that resides inside one or was
> put on `$PATH` by one. The reason: local checkouts may sit on working branches with unreleased or
> modified code, and binaries built inside them inherit that state — silently invalidating audit
> findings. Analysis tooling for this audit comes from exactly **two clean sources**:
> 1. the `move` decompiler **built fresh from the pinned clone in `$AUDIT_WORK`** during this
>    session (see `setup.md`);
> 2. the `sui` CLI **managed by `suiup`** (the official Sui version manager). Verify provenance
>    with `command -v sui` — it must resolve to a `suiup`-managed path, NOT under any `*/target/*`.

## Configuration (edit these two values)

```sh
# The Sui ref to clone & build the decompiler from. Pinned for reproducible, shareable audits.
# Bump deliberately — newer refs have decompiler fixes but change output for the same package.
SUI_REF="sui_v1.73.0"

# Disposable session work dir for the clone + build + decompiled output. Not committed.
AUDIT_WORK="./.audit-workspace"
```

> ⚠️ **CONFIRM BEFORE BUILDING.** Cloning + building is heavy and pulls a large dependency tree.
> ALWAYS ask the user to confirm `SUI_REF` (and that they want to proceed) **before** running any
> clone or `cargo build`. Do not auto-build. **Reuse `$MOVE_BIN` only if it was built in this
> `$AUDIT_WORK` from the currently-confirmed `SUI_REF`**; otherwise rebuild. Do not reuse a `move`
> binary from anywhere else (see Isolation, above).

## Workflow

1. **Setup** (`setup.md`) — verify prereqs (incl. `suiup`-managed `sui`), confirm `SUI_REF`, clone,
   build `move`, verify.
2. **Fetch & convert** (`fetch-and-decompile.md`) — get the target's `.mv` modules (by package id
   or supplied files), **disassemble every module (analysis substrate)**, then decompile every
   module (kept for finding-explanation later).
3. **Audit** — hand the `.asm` disassembly to `sui-move-security-review`; consult
   `move-bytecode-comprehension` for what survives compilation. Reach for `.move` decompiled output
   only when rendering a confirmed finding for human consumption.

## Routing table

| File | When |
|------|------|
| `setup.md` | First time this session / no cached `move` binary — clone & build the decompiler |
| `fetch-and-decompile.md` | Have the tools; need a package's `.mv` modules turned into `.asm` (analysis) and `.move` (explanation) |

## Prerequisites

- **`suiup`** — the official Sui version manager. If absent, `setup.md` asks the user before
  installing it via `curl -sSfL https://raw.githubusercontent.com/MystenLabs/suiup/main/install.sh | sh`.
- `sui` CLI installed and switched via `suiup` (`suiup install sui@<network>` +
  `suiup switch sui@<network>`) — used for fetching on-chain objects and for `disassemble`. The
  `suiup`-managed `sui` is the ONLY acceptable `sui` for this audit.
- Rust toolchain (`cargo --version`) — to build the `move` decompiler binary in `$AUDIT_WORK`.
- `git`, `jq`, `python3` — for cloning and extracting module bytecode from package JSON.
