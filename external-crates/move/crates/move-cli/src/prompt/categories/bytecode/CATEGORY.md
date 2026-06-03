---
name: bytecode
description: Understanding what a compiled Move package actually does — its disassembly, decompiled source, and what survives compilation. Reach for this on "what does this package do?", "read / decompile this", "show me what's in this .mv", or any analysis of deployed code that doesn't need a security checklist.
skills:
  - move-bytecode-comprehension
  - sui-and-move-tools
---

# Understanding Move bytecode

Compiled Move packages — including everything on chain — are bytecode, not source.
Reading them well requires a mental model of what survives compilation and what doesn't,
plus the tools to turn `.mv` files into something a human can read.

## Skills

1. **`move-bytecode-comprehension`** — the entry point. Covers the `.mv` format (magic
   `0xA11CEB0B`, table-based layout), the two views (disassembly, decompiled source) and
   their respective fidelity to the executed bytecode, and per-construct notes on what
   survives compilation.

   ```sh
   move prompt skill move-bytecode-comprehension
   move prompt skill move-bytecode-comprehension --list
   move prompt skill move-bytecode-comprehension --file <ref>
   ```

2. **`sui-and-move-tools`** — once you have a target in mind, this skill produces the
   bytecode views: one Sui GraphQL call returns every module's bytes + disassembly (the
   analysis source of truth) and `move decompile` (already on the system, running
   `move prompt`) produces the readable `.move` view (the human-explanation layer).
   The same workflow serves any bytecode-reading task.

   ```sh
   move prompt skill sui-and-move-tools
   move prompt skill sui-and-move-tools --list
   move prompt skill sui-and-move-tools --file <ref>
   ```

## Disassembly vs decompilation — a single rule

Disassembly is 1:1 with executed bytecode. Decompiled `.move` is a heuristic
reconstruction that can mis-render on edge cases. Reason from disassembly; render
decompiled source only as a presentation layer for an already-confirmed observation.

## External references

- [move-book.com](https://move-book.com) — Move language reference.
- [docs.sui.io](https://docs.sui.io) — Sui framework + on-chain conventions.
- [`move-binary-format` source](https://github.com/MystenLabs/sui/tree/main/external-crates/move/crates/move-binary-format) — the canonical definition of the `.mv` table layout and instruction set.
