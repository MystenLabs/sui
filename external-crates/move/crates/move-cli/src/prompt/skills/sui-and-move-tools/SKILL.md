---
name: sui-and-move-tools
description: >
  Use to get bytecode for a deployed Sui package and produce a decompiled-source working
  view. One GraphQL call fetches every module's raw bytecode bytes; `move decompile`
  (already on the system, running `move prompt`) produces readable `.move` files.
  Disassembly is fetched per-module only for specific verification questions. Trigger on
  "fetch this package's bytecode", "get me the .mv for package X", "decompile this", or
  "I need to read a deployed Sui package".
---

# Sui and Move Tools

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary.
> Read the procedure with `move prompt skill sui-and-move-tools --file fetch-and-decompile`.
> See `move prompt categories` for which categories use this skill.

Get a deployed Sui package's bytecode and produce `.mv` and decompiled `.move` files. One
Sui GraphQL call returns raw bytes for every module; `move decompile` produces the
readable working view. Disassembly is fetched per-module only when a specific question
needs it.

For what each output is for, see `move-bytecode-comprehension`. The end-to-end procedure
is in `fetch-and-decompile.md`.
