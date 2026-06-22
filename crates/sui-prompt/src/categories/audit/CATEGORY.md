---
name: audit
description: Security review of deployed Sui Move packages; for audits, vulnerabilities, or suspected bugs.
skills:
  - sui-and-move-tools
  - move-bytecode-comprehension
  - sui-move-security-review
---

# Auditing Move packages on Sui

**Run `sui prompt category audit --all` to load the full catalog.** Every
`SM-*` rule must be considered — applicability is by shape, not by name, and
the rules you skip are usually the ones that would have fired. Absence-detection
rules silently miss vulnerabilities when their files weren't loaded. Partial
loads create blind spots you can't predict in advance.

**Training data is not a substitute for actually loading files.** You have
seen Move security rules in training; this catalog is not those rules. Files
you skip "because you already know that category" are the ones whose
this-catalog shape — `[+domain]` rules, Sui-specific framings, absence-detection
disciplines — may differ from your prior training. Don't declare the catalog
loaded until every file is actually loaded.

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
- **A grep miss is NOT proof of absence.** Many `SM-*` rules detect the *absence*
  of a guard, check, or invariant assertion. For those, an empty grep often means
  *"the guard is missing everywhere"*, not *"the rule doesn't apply"*. Walk the
  candidate set explicitly — every fn touching the rule's subject (shared-state
  mutators for SM-A2, cap-gated fns for SM-A3, DF accessors for SM-E4,
  `object::delete` sites for SM-C1, etc.) — and verify each.

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
