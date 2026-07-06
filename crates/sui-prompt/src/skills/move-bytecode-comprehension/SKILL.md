---
name: move-bytecode-comprehension
description: >
  Use when reading or reasoning about compiled Move bytecode or `sui move disassemble`
  output. Mental model for the binary format, what survives compilation (and what's lost),
  and how to read disassembly soundly. Trigger on "what does this package do?", "read this
  .mv module", "interpret this disassembly", or whenever an analysis needs to interpret
  bytecode faithfully.
---

# Move Bytecode Comprehension

> **Self-bootstrap:** before relying on the routing below, load every reference file —
> default to `sui prompt skill move-bytecode-comprehension --all`, or `--list` + `--file
> <ref>` when budget is tight. This SKILL.md is the mental-model wrapper; reference files
> cover the binary format and the practice of reading disassembly.

On-chain Sui packages are compiled **Move bytecode** (`.mv`, magic `0xA11CEB0B`). For
analysis you read it as **disassembly** (stack-machine, via `sui move disassemble`). This
skill is the mental model for the binary format and for reading disassembly soundly.

The single most important fact when reading Move bytecode: **abilities, visibility, the
`entry` flag, signatures (incl. generics/phantom), and all on-chain identifiers —
module/struct/field/function names — are preserved.** What is *lost* is human intent
metadata: constant/error names, `#[error]` messages, local variable names, comments, and
macro structure. Tune reasoning accordingly.

## What survives compilation — the survival table

| Source construct | In bytecode? | How it appears in disassembly | Implication |
|---|---|---|---|
| Module address + name | ✓ | `module <addr>.<name>` | Identity is exact |
| Struct names | ✓ | `struct Name ...` | Type-name reasoning is reliable |
| Field names + types | ✓ | listed on the struct | Field-level reasoning holds |
| **Abilities** (key/store/copy/drop) | ✓ | `has store, key` | **Capability / soulbound / hot-potato reasoning is reliable** |
| **Visibility** (private/public/friend) | ✓ | keyword on fn | **Call-surface reasoning is reliable** |
| **`entry` flag** | ✓ | `entry` keyword | **Tx-entry reasoning is reliable** |
| Function signatures | ✓ | full `(Arg0: T, ...): Ret` | Param/return reasoning holds |
| Generics + phantom type params | ✓ | `<T>` / phantom | Type-identity reasoning holds |
| Constant **values** | ✓ | `LdConst[i](T: ...)` | Values readable, names are not |
| Constant / error **names** | ✗ | `LdConst[i]` index only | **Use abort *codes*/positions, not names** |
| `#[error]` messages | ✗ | — | Error intent lost; reason from the abort site |
| Local variable names | ✗ | `loc0`, `Arg1`, … | Don't trust names; trust dataflow |
| Comments / doc | ✗ | — | No author intent text |
| Macros (`assert!`, `1u8.do!`) | expanded | inlined opcodes / branches | A macro looks like its expansion |
| Empty struct (e.g. OTW) | ✓ (as 1 field) | `dummy_field: bool` | **OTW detect via name+`drop`+synthetic field** |

## How to read

| File | Use for |
|------|---------|
| `format.md` | The binary format: tables, abilities, visibility, signatures, opcode categories. Read once. |
| `disassembly.md` | Reading the disassembly view: stack machine, basic blocks, locals, and the opcodes you'll actually see. |

## Practical stance

- **Disassembly is the view.** Apply analyses to the `.asm` files produced by
  `sui-and-move-tools/fetch-and-disassemble.md`. Abilities, visibility, the `entry` flag,
  signatures, struct/field shapes, control flow, and call patterns are all faithful.
- **Citation.** Findings cite `<module>.asm:B<block>@i<index>` — the basic-block label
  plus instruction index within the block.
- Bytecode version is shown as `// Move bytecode vN`; note it — opcode availability
  varies by version.
