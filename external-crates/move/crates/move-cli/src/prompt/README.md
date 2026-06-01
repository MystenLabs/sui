# `move prompt`

`move prompt` is the agent-agnostic audit-information surface of `move-cli`. It exposes
embedded skill markdown (audit playbooks) consumable by any AI agent that can shell out.

This README is the developer-facing entry point for the subcommand. The text printed at
runtime by `move prompt` (no args) lives in `prompt-output.md` next to this file and is
embedded into the binary at build time via `include_str!`.

## What this subcommand is

A self-contained, read-only CLI exposing audit information for compiled Sui Move packages.

- **Contract:** in = compiled `.mv` files (one or more, or a `build/<Pkg>/` directory); out =
  markdown (skills).
- **Read-only.** Never builds, fetches, or writes artifacts.
- **Self-contained.** All skill markdown is embedded in the binary at build time. No
  external install needed. Works for any AI agent that can shell out.

## Install

Build from source:

```sh
cd /path/to/sui-move-prompt/external-crates/move
cargo build --release -p move-cli --bin move
./target/release/move prompt   # see the overview
```

Or distribute the built binary; no additional install steps are needed (skills are baked in).

## Commands

### Example 1 — discoverability overview

Call with no subcommand to print the overview an AI agent should read first:

```sh
move prompt
```

Output (excerpt):

```
# move prompt — audit-information CLI for compiled Sui Move bytecode

## What this is
A self-contained, read-only CLI that exposes a set of embedded audit skills
for compiled Sui Move packages. ...

## How to perform an audit
1. Read the orientation skill — the SM-* rule catalog ...
2. Read the bytecode-comprehension skill — what survives compilation ...
3. Read the toolchain skill — how to obtain bytecode ...
4. Apply the SM-* rules to the target package's disassembly ...
```

The full text is rendered from `src/prompt/prompt-output.md`. This is the bootstrap an agent
uses to learn everything else about `move prompt`.

### Example 2 — list bundled skills

```sh
move prompt skills
```

Output:

```
Embedded skill bundles (3):
  move-bytecode-comprehension  (4 files)
  sui-audit-toolchain  (3 files)
  sui-move-security-review  (12 files)

Commands:
  move prompt skill <bundle>            — read SKILL.md
  move prompt skill <bundle> --list     — list reference files
  move prompt skill <bundle> --file <r> — read a specific reference file
```

### Example 3 — read a skill bundle's SKILL.md

```sh
move prompt skill sui-move-security-review
```

Prints the SKILL.md of the named bundle verbatim. SKILL.md is the agent's entry point into a
skill — it lays out the routing table to per-category reference files and the overall
workflow.

### Example 4 — list reference files in a bundle

```sh
move prompt skill sui-move-security-review --list
```

Output:

```
Files in skill bundle 'sui-move-security-review' (12):
  LINEAGE
  SKILL
  abilities-and-types
  access-control
  arithmetic-and-coins
  auditing-bytecode
  composability-and-ptb
  dynamic-fields
  init-otw-upgrades
  object-lifecycle
  test-and-offchain
  time-and-randomness
```

(The `.md` extension is dropped from the printed list so the names directly feed `--file`.)

### Example 5 — read a specific reference file

```sh
move prompt skill sui-move-security-review --file access-control
```

Prints the named reference file verbatim. Use this to drill into a category after reading
`SKILL.md`.

## Worked agent flow

The realistic use case — point any AI agent at the binary with a prompt like *"audit Sui
mainnet package `0x<id>` for security vulnerabilities; use the `move prompt` binary on PATH
to discover skills necessary to perform the audit"*:

1. Agent calls `move prompt` (no args) — learns the surface.
2. Agent calls `move prompt skill sui-move-security-review` — reads the rule catalog
   structure.
3. Agent calls `move prompt skill sui-move-security-review --file auditing-bytecode` —
   reads the bytecode-audit workflow (analyze on disassembly; explain with decompiled).
4. Agent calls `move prompt skill sui-audit-toolchain` — learns how to obtain the
   package's bytecode and stand up the `sui` / `move decompile` tools.
5. Agent follows that procedure: fetch `.mv` modules, `sui move disassemble` them, run
   `move decompile` to get the human-explanation layer.
6. Agent calls `move prompt skill sui-move-security-review --file <category>` for each
   relevant SM-* category (access-control, abilities-and-types, etc.) and walks the rules
   against the disassembly.
7. Agent produces findings in the format from `auditing-bytecode.md`:
   `SM-ID · module.asm:B<block>@i<index>` with paired disassembly evidence + decompiled
   "Human view" excerpt.

## Reproducibility

Record with every audit report:

- The target package id and network.
- The `SUI_REF` used to build the toolchain (see `sui-audit-toolchain` skill).
- The `move` binary version (built from `external-crates/move` at the sui-move-prompt
  commit hash) — embedded skill content is pinned to that build.

## Discipline (applies to all findings)

- Quote evidence. Cite `<module>.asm:B<block>@i<index>` for every claim.
- Never assert a vulnerability without disassembly evidence backing it.
- Distinguish *exploitable* from *defense-in-depth*.
- The decompiled `.move` view is for *human explanation* of confirmed findings, never the
  source of truth for analysis. Disassembly is the substrate.
