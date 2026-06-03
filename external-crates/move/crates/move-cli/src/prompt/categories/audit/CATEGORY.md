---
name: audit
description: Reviewing a deployed Move package on Sui for vulnerabilities, exploitable bugs, suspicious behavior, or anything suspected wrong on chain. Reach for this on "is this package safe?", "what's wrong with this?", "I suspect there's a bug in X", "find vulnerabilities", "security review", or a direct "audit this".
skills:
  - sui-move-security-review
  - sui-and-move-tools
  - move-bytecode-comprehension
---

# Auditing Move packages on Sui

Auditing here means finding security vulnerabilities in deployed (compiled) Move packages
on Sui: applying a catalog of invariant-violation rules against bytecode that's already
on chain, with a disciplined evidence chain from disassembly to finding.

## Skills

Read each in order. Each skill bundle is **two-tier** — a SKILL.md that routes / summarizes,
and a set of reference files where the actual content lives. Enumerate (`--list`) and read
every reference file in each bundle before walking its rules.

1. **`sui-and-move-tools`** — set up the toolchain: `suiup`-managed `sui` for fetching
   on-chain packages and disassembling them, plus a freshly-built `move decompile` binary
   for the human-explanation layer.

   ```sh
   move prompt skill sui-and-move-tools
   move prompt skill sui-and-move-tools --list
   move prompt skill sui-and-move-tools --file <ref>
   ```

2. **`move-bytecode-comprehension`** — what survives compilation. Abilities, visibility,
   `entry`, and signatures survive exactly; constant / local names, comments, and macro
   sugar do not. This is the mental model for reading disassembly accurately.

   ```sh
   move prompt skill move-bytecode-comprehension
   move prompt skill move-bytecode-comprehension --list
   move prompt skill move-bytecode-comprehension --file <ref>
   ```

3. **`sui-move-security-review`** — the `SM-*` rule catalog. The SKILL.md is a routing
   table; the per-category reference files (`access-control.md`, `abilities-and-types.md`,
   `object-lifecycle.md`, …) contain the detection heuristics, severity ratings, exploit
   sketches, and bytecode signals you actually apply.

   ```sh
   move prompt skill sui-move-security-review
   move prompt skill sui-move-security-review --list
   move prompt skill sui-move-security-review --file <ref>
   ```

   For the audit workflow itself (*analyze on disassembly; explain with decompiled
   source*), start with `move prompt skill sui-move-security-review --file
   auditing-bytecode`.

## Triage discipline

- Quote evidence. Cite `<module>.asm:B<block>@i<index>` for every claim.
- Never assert a vulnerability without disassembly evidence backing it.
- Distinguish *exploitable* from *defense-in-depth*.
- The decompiled `.move` view is for *human explanation* of confirmed findings, never the
  source of truth for analysis. Disassembly is the substrate.

## Reproducibility

Record with every audit report:

- The target package id and network.
- The `SUI_REF` used to build the toolchain (see `sui-and-move-tools`).
- The `move` binary version (built from `external-crates/move`) — embedded skill content
  is pinned to that build.

## External references

- [MystenLabs/skills](https://github.com/MystenLabs/skills) — the constructive Sui / Move
  skills the `SM-*` rules are derived from (per-rule citations live in
  `sui-move-security-review/LINEAGE.md`). Useful when you need to understand the
  well-formed pattern an `SM-*` rule describes the violation of.
- [docs.sui.io](https://docs.sui.io) — Sui framework documentation.
- [move-book.com](https://move-book.com) — Move language reference.
