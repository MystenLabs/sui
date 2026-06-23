---
name: sui-and-move-tools
description: >
  Use to get bytecode for a deployed Sui package and produce a disassembled working view.
  One GraphQL call fetches every module's raw bytecode bytes; `sui move disassemble`
  (already on the system, running `sui prompt`) produces `.asm` files for analysis.
  Trigger on "fetch this package's bytecode", "get me the .mv for package X",
  "disassemble this package", or "I need to read a deployed Sui package".
---

# Sui and Move Tools

Get a deployed Sui package's bytecode and produce `.mv` and `.asm` (disassembly) files.
One Sui GraphQL call returns raw bytes for every module; `sui move disassemble` produces
the working view module-by-module.

For what the disassembly conveys (and what's lost in compilation), see
`move-bytecode-comprehension`. The end-to-end procedure is in `fetch-and-disassemble.md`.
