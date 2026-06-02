---
name: move-bytecode-comprehension
description: >
  Use when reading or reasoning about compiled Move bytecode, `sui move disassemble` output, or
  decompiled Move source (from `move decompile`). Explains the Move binary format, exactly what
  information survives compilation (and what is lost), and how to read disassembly and decompiled
  output so conclusions about on-chain packages are sound. Trigger on "what does this package
  do?", "read this .mv module", "interpret this disassembly", "is the decompiled source
  trustworthy?", or whenever an analysis (audit or otherwise) needs to interpret bytecode
  faithfully. Pair with `sui-and-move-tools` (to produce the output) and, for audits,
  `sui-move-security-review` (to know what to look for).
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
two ways: **disassembly** (stack-machine assembly, via `sui move disassemble`) and
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
| `reading-disassembly.md` | Interpreting `sui move disassemble` (stack machine, locals, the opcodes you'll actually see). |
| `reading-decompiled.md` | Use to translate **already-confirmed** findings into readable Move for human explanation; understand the decompiler's artifacts so you don't misread them when presenting. |

## Practical stance

- **Analyze on disassembly** — it is faithful to the executed bytecode (1:1 with what the validator
  runs). Every `SM-*` finding's evidence must be an assembly excerpt; cite as
  `<module>.asm:B<block>@i<index>`.
- **Use decompiled source only to *explain* confirmed findings to humans**, never to derive them.
  The decompiler is a heuristic reconstructor: structurally faithful in the common case but with
  mis-renderings on edge cases. A pattern that appears in the decompiled `.move` but is absent or
  different in the assembly is NOT a finding.
- If decompiled view and assembly disagree at a confirmed site, the **assembly wins** — record the
  discrepancy as decompiler imprecision, not as a re-opening of the finding.
- Bytecode version is shown as `// Move bytecode vN`; note it — opcode availability varies by
  version.
