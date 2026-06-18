---
name: sui-and-move-tools
description: >
  Use to get bytecode for a deployed Sui package and produce a decompiled-source working
  view. One GraphQL call fetches every module's raw bytecode bytes; `sui move decompile`
  (already on the system, running `sui prompt`) produces readable `.move` files.
  Disassembly is fetched per-module only for specific verification questions. Trigger on
  "fetch this package's bytecode", "get me the .mv for package X", "decompile this", or
  "I need to read a deployed Sui package".
---

# Sui and Move Tools

Get a deployed Sui package's bytecode and produce `.mv` and decompiled `.move` files. One
Sui GraphQL call returns raw bytes for every module; `sui move decompile` produces the
readable working view. Disassembly is fetched per-module only when a specific question
needs it.

For what each output is for, see `move-bytecode-comprehension`. The end-to-end procedure
is in `fetch-and-decompile.md`.
