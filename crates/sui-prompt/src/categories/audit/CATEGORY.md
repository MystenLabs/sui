---
name: audit
description: Security review of Move code on Sui (source or compiled bytecode); for audits, vulnerabilities, or suspected bugs.
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
loads create blind spots you can't predict in advance. Incremental loading is
possible but should be the last resort. Even then, do not skip any files or rules to avoid creating blind spots.

**Training data is not a substitute for actually loading files.** You have
seen Move security rules in training; this catalog is not those rules. Files
you skip "because you already know that category" are the ones whose
this-catalog shape — `[+domain]` rules, Sui-specific framings, absence-detection
disciplines — may differ from your prior training. Don't declare the catalog
loaded until every file is actually loaded.

Auditing here means finding security vulnerabilities in Move code on Sui by walking a
catalog of invariant-violation rules. Choose the representation from the target, not from
convenience:

- `.move` source files — for source repositories you are explicitly reviewing.
  Move packages organize sources under `sources/` with a `Move.toml` manifest at the
  root; see [docs.sui.io](https://docs.sui.io/develop/manage-packages/move-package-management)
  for the canonical layout.
- `.asm` disassembly of compiled bytecode — for deployed on-chain packages. Produce `.asm` files via
  `sui-and-move-tools/fetch-and-disassemble.md`.

The rule statements and examples are written in Move semantics. For deployed packages,
derive findings from disassembly and pair the rules with
`sui-move-security-review/auditing-bytecode.md` for per-rule opcode-pattern signals. For
source-repository audits, the rule files apply directly to `.move` text.

## Triage discipline

- Quote evidence. Cite from the file you derived the finding from:
  `<module>.move:<line>` for source-derived claims, `<module>.asm:B<block>@i<index>`
  for disassembly-derived claims.
- Never assert a vulnerability without code-derived evidence backing it.
- Distinguish *exploitable* from *defense-in-depth*.
- **A grep miss is NOT proof of absence.** Many `SM-*` rules detect the *absence*
  of a guard, check, or invariant assertion. For those, an empty grep often means
  *"the guard is missing everywhere"*, not *"the rule doesn't apply"*. Walk the
  candidate set explicitly — every fn touching the rule's subject (shared-state
  mutators for SM-A2, cap-gated fns for SM-A3, DF accessors for SM-E4,
  `object::delete` sites for SM-C1, etc.) — and verify each.

## Reproducibility

Record with every audit report the inputs needed to re-derive the same findings:

- **The Move code under audit:**
  - Source audit: source repo + commit / branch.
  - Bytecode audit: target package id and network; GraphQL endpoint used
    (e.g., `https://graphql.mainnet.sui.io/graphql`).
- `sui --version` (the binary that ran `sui prompt`; for bytecode audits, also the
  binary that ran `sui move disassemble`).

## External references

- [MystenLabs/skills](https://github.com/MystenLabs/skills) — the constructive Sui / Move
  skills the `SM-*` rules are derived from. Useful when you need to understand the
  well-formed pattern an `SM-*` rule describes the violation of.
- [docs.sui.io](https://docs.sui.io) — Sui framework documentation.
- [move-book.com](https://move-book.com) — Move language reference.
