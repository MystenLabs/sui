# move prompt — audit-information CLI for compiled Sui Move bytecode

## What this is

A self-contained, read-only CLI that exposes a set of embedded audit skills for compiled
Sui Move packages. Every skill is plain markdown; nothing needs to be installed beyond this
binary.

## Agent-agnostic contract

Any AI agent that can shell out can use `move prompt`. The contract:

- **Out:** markdown (skills, overview text).
- **Read-only:** never builds, fetches, or writes artifacts.
- **Self-contained:** all skill markdown is embedded in the binary at build time. No
  external install needed — `move prompt skill <bundle>` returns the skill text directly
  from this binary.

## How to perform an audit

Each skill bundle has a **two-tier structure**: a SKILL.md that routes / summarizes, and a
set of per-category reference files where the actual content lives (detection heuristics,
exploit sketches, bytecode signals, procedures). **Reading SKILL.md alone is not enough** —
you must enumerate (`--list`) and read every reference file in the bundle before walking
its rules.

1. **Read the orientation skill — including every per-category reference file.** The SM-*
   rule bodies (detection heuristics, severity, exploit sketches, bytecode signals) live in
   the per-category files; the SKILL.md routing table is a summary only. Enumerate and read
   every reference file before walking the catalog:

   ```sh
   move prompt skill sui-move-security-review                       # SKILL.md (routing + workflow)
   move prompt skill sui-move-security-review --list                # enumerate reference files
   move prompt skill sui-move-security-review --file <ref>          # for every listed ref
   ```

2. **Read the bytecode-comprehension skill — including its reference files.** SKILL.md
   gives the mental model; the reference files cover the disassembly format and how to read
   disassembly / decompiled output in practice.

   ```sh
   move prompt skill move-bytecode-comprehension                    # SKILL.md
   move prompt skill move-bytecode-comprehension --list
   move prompt skill move-bytecode-comprehension --file <ref>       # for every listed ref
   ```

3. **Read the toolchain skill — including its reference files.** SKILL.md frames the two
   tools (`sui` / `move decompile`); the reference files contain the actual setup,
   fetch, and decompile procedures.

   ```sh
   move prompt skill sui-audit-toolchain                            # SKILL.md
   move prompt skill sui-audit-toolchain --list
   move prompt skill sui-audit-toolchain --file <ref>               # for every listed ref
   ```

4. **Apply the SM-* rules** to the target package's disassembly. Follow the toolchain skill's
   procedure to fetch + disassemble + decompile a package; walk the rule catalog **using the
   per-category reference files** (not SKILL.md, which is summary only); cite findings as
   `<module>.asm:B<block>@i<index>` per the security-review skill's reporting format.

## Universal commands

- `move prompt`                                          — this overview
- `move prompt skills`                                   — list bundled skill bundles
- `move prompt skill <bundle>`                           — read a skill bundle's `SKILL.md`
- `move prompt skill <bundle> --list`                    — list reference files in a bundle
- `move prompt skill <bundle> --file <ref>`              — read a specific reference file

## Triage discipline (applies to all findings)

- Quote evidence. Cite `<module>.asm:B<block>@i<index>` for every claim.
- Never assert a vulnerability without the disassembly evidence backing it.
- Distinguish *exploitable* from *defense-in-depth*.
- The decompiled `.move` view is for *human explanation* of confirmed findings, never the
  source of truth for analysis. Disassembly is the substrate.

## Reproducibility

Record with every audit report: the target package id, the network, the `SUI_REF` used to
build the toolchain (see `sui-audit-toolchain`), and the `move` binary version.
