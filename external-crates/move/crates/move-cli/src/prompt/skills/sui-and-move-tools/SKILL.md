---
name: sui-and-move-tools
description: >
  Use to set up the toolchain for working with on-chain Move on Sui: the `suiup`-managed
  `sui` CLI (fetches packages, runs `sui move disassemble`) and the `move decompile` binary
  (renders bytecode as readable Move source for human explanation). Run at the START of any
  session that needs `.mv` files in hand — auditing a deployed package, reading a release,
  comparing versions. Trigger on "fetch this package's bytecode", "get me the .mv for
  package X", "disassemble this", or "set up the move decompiler".
---

# Sui and Move Tools

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary. **Before
> executing the workflow below, enumerate the reference files (`move prompt skill
> sui-and-move-tools --list`) and read each (`--file <ref>`).** They contain the actual
> setup, fetch, and decompile procedures; this SKILL.md frames the tools but the steps live
> in the reference files. This skill belongs to one or more categories — run
> `move prompt categories` to see them and `move prompt category <name>` to read the
> category's workflow. No filesystem install is required — the binary is self-contained.

On-chain Sui packages are **bytecode**, not source. This skill bootstraps the two tools needed
to read them, from public sources only, so any teammate can reproduce the work. The two tools
play **different roles** — do not confuse them:

- **`sui move disassemble`** — ships with the **`suiup`-managed** `sui` CLI; bytecode → assembly
  text. **This is the analysis source of truth** — faithful 1:1 with the executed bytecode. Any
  claim about the package should derive from this output.
- **`move decompile`** — built from the Sui repo's `move-cli` (in `$WORK_DIR`); bytecode →
  readable Move source. **This is a human-explanation layer**, not an analysis source: a heuristic
  reconstructor that can mis-render on edge cases. Use it to render *already-confirmed* results
  in familiar Move so a reader can follow them. The `move-decompiler` crate is on public
  `github.com/MystenLabs/sui` `origin/main`.

Pair this with `move-bytecode-comprehension` (how to read the output). For audits, also pair
with `sui-move-security-review` (what to look for).

> 🧭 **Analyze on disassembly; explain with decompiled source.** Any reasoning about the
> package's behavior derives from `sui move disassemble` output (the executed bytecode,
> faithfully). When a result is confirmed, render the matching decompiled `.move` snippet
> *alongside* it as a "Human view" so a reader can see the construct in familiar Move.
> **Never** treat a pattern visible only in decompiled output — but absent or different in
> the disassembly — as a finding. The decompiler is a presentation tool, not a source of truth.

> 🔒 **Isolation — non-negotiable.** Do NOT search for, or use, any pre-existing local Sui source
> checkout (`~/sui`, `~/sui-*`, any directory containing `Cargo.toml` for the Sui workspace, or any
> `*/target/*` build dir), and do NOT use any `move`/`sui` binary that resides inside one or was
> put on `$PATH` by one. The reason: local checkouts may sit on working branches with unreleased or
> modified code, and binaries built inside them inherit that state — silently invalidating any
> findings derived from them. Tooling for this session comes from exactly **two clean sources**:
> 1. the `move` decompiler **built fresh from the pinned clone in `$WORK_DIR`** during this
>    session (see `setup.md`);
> 2. the `sui` CLI **managed by `suiup`** (the official Sui version manager). Verify provenance
>    with `command -v sui` — it must resolve to a `suiup`-managed path, NOT under any `*/target/*`.

## Configuration (edit these two values)

```sh
# The Sui ref to clone & build the decompiler from. Pinned for reproducible, shareable work.
# Bump deliberately — newer refs have decompiler fixes but change output for the same package.
SUI_REF="sui_v1.73.0"

# Disposable session work dir for the clone + build + decompiled output. Not committed.
WORK_DIR="./.move-tools-workspace"
```

> ⚠️ **CONFIRM BEFORE BUILDING.** Cloning + building is heavy and pulls a large dependency tree.
> ALWAYS ask the user to confirm `SUI_REF` (and that they want to proceed) **before** running any
> clone or `cargo build`. Do not auto-build. **Reuse `$MOVE_BIN` only if it was built in this
> `$WORK_DIR` from the currently-confirmed `SUI_REF`**; otherwise rebuild. Do not reuse a `move`
> binary from anywhere else (see Isolation, above).

## Workflow

1. **Setup** (`setup.md`) — verify prereqs (incl. `suiup`-managed `sui`), confirm `SUI_REF`, clone,
   build `move`, verify.
2. **Fetch & convert** (`fetch-and-decompile.md`) — get the target's `.mv` modules (by package id
   or supplied files), **disassemble every module (analysis substrate)**, then decompile every
   module (kept for human-explanation later).
3. **Use the output.** Hand the `.asm` disassembly to whatever category is consuming it —
   `sui-move-security-review` for audit work; `move-bytecode-comprehension` for understanding
   what survives compilation. Reach for `.move` decompiled output only when rendering a
   confirmed result for human consumption.

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
  `suiup`-managed `sui` is the ONLY acceptable `sui` here (see Isolation).
- Rust toolchain (`cargo --version`) — to build the `move` decompiler binary in `$WORK_DIR`.
- `git`, `jq`, `python3` — for cloning and extracting module bytecode from package JSON.
