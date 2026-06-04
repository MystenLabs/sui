# Move bytecode disassembly

Disassembly is 1:1 with the executed bytecode — no heuristic reconstruction. In this
skill's workflow it's the **verification view**: open it on demand for a specific question
that the decompiled view (the working substrate; see `decompilation.md`) can't answer —
looking up an abort code's numeric value, confirming an ambiguous instruction sequence,
or when decompilation broke for a particular module.

Disassembly is faithful but low-level: a stack machine with numbered locals and basic
blocks. `.asm` files are fetched per-module via the "Fetching disassembly on demand"
section of `sui-and-move-tools/fetch-and-decompile.md`.

## How to read it

- **Header:** `// Move bytecode vN` (version) and `module <addr>.<name> { ... }`. `use` lines are
  dependencies, fully address-qualified.
- **Structs:** name + `has <abilities>` + fields. A fields-less source struct (e.g. an OTW
  named `MODULE_NAME`) is represented with a synthetic `dummy_field: bool` — expected, not
  a bug.
- **Functions:** signature line shows `name(Arg0: T, Arg1: U): Ret`, with `public` / `entry`
  prefixes when present (this `init` is the special initializer, shown without them). Parameters
  are `Arg0, Arg1, ...`; declared locals are `loc0, loc1, ...` with `Ln:` slot labels — **these
  names are invented**, not from source.
- **Basic blocks:** `B0:`, `B1:` … each a straight-line run; control flow between them is via
  `Branch/BrTrue/BrFalse` to a block, ending with `Ret`/`Abort`.
- **Instructions:** numbered, operate on the operand stack.
- **Abort sites:** an `Abort` instruction pops an abort code from the stack; the code's
  value is whatever was just pushed by the immediately preceding `LdConst`/`LdU64`.
  Source names (e.g. `EUnauthorized`) don't survive — reason from the integer + the
  condition that branched here.

## The opcodes you'll actually read

| Instruction | Meaning |
|---|---|
| `LdConst[i](T: "abc..)` | push constant pool entry `i` (value shown, no name) |
| `LdU64(n)` / `LdTrue` | push literal |
| `CopyLoc[k]` / `MoveLoc[k]` | push a copy of / move out local/arg `k` |
| `StLoc[k]` | pop into local `k` |
| `ImmBorrowLoc[k]` / `MutBorrowLoc[k]` | push `&`/`&mut` to local `k` |
| `ImmBorrowField` / `MutBorrowField(Type.field)` | push `&`/`&mut` to a struct field |
| `ReadRef` / `WriteRef` / `FreezeRef` | deref read / write / `&mut`→`&` |
| `Call mod::fn<T>(args): ret` | call (framework + cross-module ops appear here) |
| `Pack[i](Type)` / `Unpack[i](Type)` | build / destructure a struct (consumes/produces fields) |
| `CastU8..CastU256` | integer width cast — narrowing casts truncate silently |
| `Add/Sub/Mul/Div/Mod`, `Eq/Lt/...` | arithmetic / comparison (over/underflow aborts natively) |
| `VecPack(tok, n)` / `VecPushBack` / `VecMutBorrow` | vector build / mutate |
| `Branch/BrTrue/BrFalse b` | jump to block `b` |
| `Abort` | pop abort code and abort (the code is the constant just loaded) |
| `Ret` | return |

