---
name: move-bytecode-comprehension
description: >
  Use when reading or reasoning about compiled Move bytecode, `move disassemble`
  output, or decompiled Move source (from `move decompile`). Mental model for the binary
  format, what survives compilation (and what's lost), and how to read each output
  soundly. Trigger on "what does this package do?", "read this .mv module", "interpret
  this disassembly", "is the decompiled source trustworthy?", or whenever an analysis
  needs to interpret bytecode faithfully.
---

# Move Bytecode Comprehension

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary. **Before
> relying on the routing in this SKILL.md, enumerate the reference files (`move prompt skill
> move-bytecode-comprehension --list`) and read each (`--file <ref>`).** They cover the
> bytecode format and the practice of reading disassembly / decompiled output; this SKILL.md
> is the mental-model wrapper. This skill belongs to one or more categories — run
> `move prompt categories` to see them and `move prompt category <name>` to read the
> category's workflow. No filesystem install is required — the binary is self-contained.

On-chain Sui packages are compiled **Move bytecode** (`.mv`, magic `0xA11CEB0B`). You analyze it
two ways: **disassembly** (stack-machine assembly, via `move disassemble`) and
**decompilation** (reconstructed Move source, via `move decompile`). This skill is the mental model
for both: the binary format, what survives, and how to read each output.

The single most important fact for an auditor: **abilities, visibility, the `entry` flag,
signatures (incl. generics/phantom), and all on-chain identifiers — module/struct/field/function
names — are preserved in bytecode.** What is *lost* is human intent metadata: constant/error
names, `#[error]` messages, local variable names, comments, and macro structure. Tune detection
accordingly.

## What survives compilation — the survival table

| Source construct | In bytecode? | Disassembly | Decompiled | Audit implication |
|---|---|---|---|---|
| Module address + name | ✓ | `module <addr>.<name>` | `module <addr>::<name>;` | Identity is exact |
| Struct names | ✓ | `struct Name ...` | `public struct Name ...` | Type-name rules hold |
| Field names + types | ✓ | listed | listed | Field-level reasoning holds |
| **Abilities** (key/store/copy/drop) | ✓ | `has store, key` | `has store, key` | **SM-A1/B1/B2/J1 reliable** |
| **Visibility** (private/public/friend) | ✓ | keyword on fn | keyword on fn | **SM-A2/J2 reliable** |
| **`entry` flag** | ✓ | `entry` shown | `entry` shown | **SM-L2/K1 reliable** |
| Function signatures | ✓ | full | full | Param/return reasoning holds |
| Generics + phantom type params | ✓ | `<T>` / phantom | `<T>` | SM-B4 type reasoning holds |
| Constant **values** | ✓ | `LdConst[i](..)` | `const C0: ... = ...;` | Values readable... |
| Constant / error **names** | ✗ | `LdConst[i]` index | `C0, C1, ...` | **names gone → use abort *codes*/positions** |
| `#[error]` messages | ✗ | — | — | Error intent lost; reason from the abort site |
| Local variable names | ✗ | `loc0, Arg1` | invented | Don't trust names; trust dataflow |
| Comments / doc | ✗ | — | — | No author intent text |
| Macros (`assert!`, `1u8.do!`) | expanded | inlined ops | inlined / loops | A macro looks like its expansion |
| Empty struct (e.g. OTW) | ✓ (as 1 field) | `dummy_field: bool` | `dummy_field: bool` | **OTW detect via name+`drop`+synthetic field** |

## How to read each output

| File | Use for |
|------|---------|
| `format.md` | The binary format: tables, abilities, visibility, signatures, opcode categories. Read once. |
| `disassembly.md` | Reading the disassembly view (stack machine, locals, the opcodes you'll actually see). |
| `decompilation.md` | Reading the decompiled view (what's faithful, what's a decompiler artifact). |

## Practical stance

- **Decompilation is the working view.** Apply analyses (including SM-* audit rules) to
  the decompiled `.move` files produced by `sui-and-move-tools/fetch-and-decompile.md`.
  Abilities, visibility, the `entry` flag, function signatures, control flow, struct /
  field shapes, and call patterns are byte-for-byte faithful — see `decompilation.md`'s
  "What is faithful" list. Decompiler artifacts (renamed constants `C0/C1...`, invented
  locals, expanded macros, synthetic `dummy_field` on empty structs) are recognized and
  reasoned through, not treated as reasons to drop to disassembly.
- **Drop to disassembly only when:**
  - A determination needs the **numeric value** of an abort code (decompiled shows `C7`,
    not the integer).
  - Decompilation **visibly failed** for a specific module (broken output, parse errors).
  - A specific question is **ambiguous** in decompiled and verification is required.
- **Critical: never load both views for the same module simultaneously.** The default
  workflow produces only `.move` files. When you need disassembly for a single module,
  fetch it on demand per `sui-and-move-tools/fetch-and-decompile.md` ("Fetching
  disassembly on demand") and read it surgically (`grep` / `sed` for the specific
  function or basic block) rather than dumping the full `.asm` into context.
- **Citation.** Decompiled-derived findings cite `<module>.move:<line>`. Disassembly-
  derived findings (the rare verification cases) cite `<module>.asm:B<block>@i<index>`.
  Pick whichever view supports the determination.
- **Disagreement.** When both views are inspected for the same site and disagree, the
  disassembly wins — it reflects the executed bytecode. Record the discrepancy as
  decompiler imprecision, not as a re-opening of the finding.
- Bytecode version is shown as `// Move bytecode vN`; note it — opcode availability
  varies by version.
