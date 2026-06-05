---
name: audit
description: Security-review entry point for deployed Sui Move packages; choose for audits, vulnerabilities, suspicious behavior, suspected bugs, or "is this package safe?".
skills:
  - sui-move-security-review
  - sui-and-move-tools
  - move-bytecode-comprehension
---

# Auditing Move packages on Sui

Auditing here means finding security vulnerabilities in deployed (compiled) Move packages
on Sui: applying a catalog of invariant-violation rules against bytecode that's already
on chain, with a disciplined evidence chain from bytecode-derived evidence to finding.

## Skills

Read each in order. Each skill bundle is **two-tier** — a SKILL.md that routes / summarizes,
and a set of reference files where the actual content lives. Enumerate (`--list`) and read
every reference file in each bundle before walking its rules.

1. **`sui-and-move-tools`** — get the package decompiled: one Sui GraphQL call returns
   every module's bytes, and `move decompile` (already on the system, running
   `move prompt`) produces readable `.move` files (the working view for the catalog
   walk). Disassembly is fetched per-module on demand only for specific verification.

   ```sh
   move prompt skill sui-and-move-tools
   move prompt skill sui-and-move-tools --list
   move prompt skill sui-and-move-tools --file <ref>
   ```

2. **`move-bytecode-comprehension`** — what survives compilation. Abilities, visibility,
   `entry`, and signatures survive exactly; constant / local names, comments, and macro
   sugar do not. This is the mental model for reading the decompiled output soundly —
   recognizing artifacts (renamed constants, invented locals, expanded macros, synthetic
   `dummy_field` on OTWs) and reasoning through them rather than dropping to disassembly.

   ```sh
   move prompt skill move-bytecode-comprehension
   move prompt skill move-bytecode-comprehension --list
   move prompt skill move-bytecode-comprehension --file <ref>
   ```

3. **`sui-move-security-review`** — the `SM-*` rule catalog. The SKILL.md is a routing
   table; the per-category reference files (`access-control.md`, `abilities-and-types.md`,
   `object-lifecycle.md`, …) contain the detection heuristics, severity ratings, and
   exploit sketches; pair with `auditing-bytecode.md` for the structured per-rule
   decompiled-source signals.

   ```sh
   move prompt skill sui-move-security-review
   move prompt skill sui-move-security-review --list
   move prompt skill sui-move-security-review --file <ref>
   ```

   For the audit workflow itself (apply SM-* rules to the decompiled view; reach for
   disassembly only for specific verification cases), start with `move prompt skill
   sui-move-security-review --file auditing-bytecode`.

## Triage discipline

- Quote evidence. Cite `<module>.move:<line>` for decompiled-derived claims (the default)
  or `<module>.asm:B<block>@i<index>` for the rare disassembly-verified claim.
- Never assert a vulnerability without bytecode-derived evidence backing it.
- Distinguish *exploitable* from *defense-in-depth*.
- The decompiled view is the working substrate. Disassembly is the tiebreaker when both
  views are inspected for the same site and disagree.

## Reproducibility

Record with every audit report:

- Target package id and network.
- GraphQL endpoint used (e.g. `https://graphql.mainnet.sui.io/graphql`).
- `move --version` (the binary that ran `move prompt`).

## External references

- [MystenLabs/skills](https://github.com/MystenLabs/skills) — the constructive Sui / Move
  skills the `SM-*` rules are derived from. Useful when you need to understand the
  well-formed pattern an `SM-*` rule describes the violation of.
- [docs.sui.io](https://docs.sui.io) — Sui framework documentation.
- [move-book.com](https://move-book.com) — Move language reference.
