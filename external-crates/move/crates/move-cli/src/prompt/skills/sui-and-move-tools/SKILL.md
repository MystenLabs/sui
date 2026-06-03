---
name: sui-and-move-tools
description: >
  Use to get bytecode for a deployed Sui package and produce both a disassembly (analysis
  substrate) and a decompiled-source view (human-explanation layer). One GraphQL call
  fetches and disassembles every module; `move decompile` (already on the system, running
  `move prompt`) handles the optional decompiled view. Trigger on "fetch this package's
  bytecode", "get me the .mv for package X", "disassemble this", or "I need to read a
  deployed Sui package".
---

# Sui and Move Tools

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary.
> Read the procedure with `move prompt skill sui-and-move-tools --file fetch-and-decompile`.
> See `move prompt categories` for which categories use this skill.

Get a deployed Sui package's bytecode and produce `.mv`, `.asm`, and optional `.move`
files. One Sui GraphQL call returns bytes + disassembly for every module; `move decompile`
(the binary that runs `move prompt`) handles the optional decompiled view. No `sui` CLI
install needed.

For what each output is for, see `move-bytecode-comprehension`. The end-to-end procedure
is in `fetch-and-decompile.md`. For audits, also see the Reproducibility section in
`sui-move-security-review/auditing-bytecode.md`.
