---
name: bytecode
description: Bytecode-comprehension entry point for deployed code; choose to read, decompile, inspect `.mv`, or understand what a package does without security framing.
skills:
  - move-bytecode-comprehension
  - sui-and-move-tools
---

# Understanding Move bytecode

*Default to `sui prompt category bytecode --all` — load every skill bundle the
category names in one call. Step down to per-bundle (`sui prompt skill <bundle>
--all`) or per-file (`sui prompt skill <bundle> --file <ref>`) only when the
full load would risk your token budget or context window — never to skip rules
whose names don't obviously match your task, since rules apply by shape, not by
name. `sui prompt category bytecode --list` shows the inventory if you need to
make that capacity judgement.*

Compiled Move packages — including everything on chain — are bytecode, not source.
Reading them well requires a mental model of what survives compilation and what doesn't,
plus the tools to turn `.mv` files into something a human can read.

## Skills

1. **`move-bytecode-comprehension`** — the entry point. Covers the `.mv` format (magic
   `0xA11CEB0B`, table-based layout), the two views (disassembly, decompiled source) and
   their respective fidelity to the executed bytecode, and per-construct notes on what
   survives compilation.

   ```sh
   sui prompt skill move-bytecode-comprehension
   sui prompt skill move-bytecode-comprehension --list
   sui prompt skill move-bytecode-comprehension --file <ref>
   ```

2. **`sui-and-move-tools`** — once you have a target in mind, this skill produces the
   working view: one Sui GraphQL call returns every module's bytes, and `sui move decompile`
   (already on the system, running `sui prompt`) produces readable `.move` files.
   Disassembly is fetched per-module on demand only when a specific question can't be
   answered from decompiled.

   ```sh
   sui prompt skill sui-and-move-tools
   sui prompt skill sui-and-move-tools --list
   sui prompt skill sui-and-move-tools --file <ref>
   ```

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
