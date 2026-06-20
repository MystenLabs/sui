---
name: bytecode
description: Reading, decompiling, and comprehending Move bytecode (typically in `.mv` files).
skills:
  - move-bytecode-comprehension
  - sui-and-move-tools
---

# Understanding Move bytecode

**Run `sui prompt category bytecode --all` to load the full catalog.** Content
cross-references heavily, and guidance in one file often informs reasoning
about another. Partial loads create blind spots you can't predict in advance.

`sui prompt category bytecode --list` reports skill sizes in case you have to
make adjustments due to your context window or token budget.

Compiled Move packages — including everything on chain — are bytecode, not source.
Reading them well requires a mental model of what survives compilation and what doesn't,
plus the tools to turn `.mv` files into something a human can read.

## Decompilation vs disassembly — a single rule

Use decompiled `.move` as the working view. It is compact and readable, while still
preserving the bytecode properties most analyses rely on. Use disassembly as the
verification view only for a specific question: it is 1:1 with executed bytecode, and it
wins when a checked decompiled excerpt is ambiguous, visibly broken, or inconsistent with
the bytecode.

## External references

- [move-book.com](https://move-book.com) — Move language reference.
- [docs.sui.io](https://docs.sui.io) — Sui framework + on-chain conventions.
- [`move-binary-format` source](https://github.com/MystenLabs/sui/tree/main/external-crates/move/crates/move-binary-format) — the canonical definition of the `.mv` table layout and instruction set.
