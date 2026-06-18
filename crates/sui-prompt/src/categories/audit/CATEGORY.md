---
name: audit
description: Security-review entry point for deployed Sui Move packages; choose for audits, vulnerabilities, suspicious behavior, suspected bugs, or "is this package safe?".
skills:
  - sui-and-move-tools
  - move-bytecode-comprehension
  - sui-move-security-review
---

# Auditing Move packages on Sui

**Run `sui prompt category audit --all` to load the full catalog.** Content
cross-references heavily, and guidance in one file often informs reasoning
about another. Partial loads create blind spots you can't predict in advance.

`sui prompt category audit --list` reports skill sizes in case you have to
make adjustments due to your context window or token budget.

Auditing here means finding security vulnerabilities in deployed (compiled) Move packages
on Sui: applying a catalog of invariant-violation rules against bytecode that's already
on chain, with a disciplined evidence chain from bytecode-derived evidence to finding.

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
- `sui --version` (the binary that ran `sui prompt`).

## External references

- [MystenLabs/skills](https://github.com/MystenLabs/skills) — the constructive Sui / Move
  skills the `SM-*` rules are derived from. Useful when you need to understand the
  well-formed pattern an `SM-*` rule describes the violation of.
- [docs.sui.io](https://docs.sui.io) — Sui framework documentation.
- [move-book.com](https://move-book.com) — Move language reference.
