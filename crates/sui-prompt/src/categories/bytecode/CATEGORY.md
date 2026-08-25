---
name: bytecode
description: Reading and comprehending Move bytecode (typically in `.mv` files) via disassembly.
skills:
  - move-bytecode-comprehension
  - sui-and-move-tools
---

# Understanding Move bytecode

**Run `sui prompt category bytecode --all` to load the full catalog.** Every
piece of guidance must be considered — applicability is by shape, not by
filename, and the files you skip are usually the ones that would have helped.
Partial loads create blind spots you can't predict in advance.

**Training data is not a substitute for actually loading files.** You have
seen bytecode-analysis content in training; this catalog is not that content.
Files you skip "because you already know that topic" are the ones whose
this-catalog shape — Sui-specific framings, the survival table — may differ
from your prior training. Don't declare the catalog loaded until every file
is actually loaded.

Compiled Move packages — including everything on chain — are bytecode, not source.
Reading them well requires a mental model of what survives compilation and what doesn't (e.g., because of compiler optimizations),
plus the tools to turn `.mv` files into something a human can read. The working view is
disassembly (`sui move disassemble` output), which is 1:1 with the executed bytecode.

## External references

- [move-book.com](https://move-book.com) — Move language reference.
- [docs.sui.io](https://docs.sui.io) — Sui framework + on-chain conventions.
- [`move-binary-format` source](https://github.com/MystenLabs/sui/tree/main/external-crates/move/crates/move-binary-format) — the canonical definition of the `.mv` table layout and instruction set.
