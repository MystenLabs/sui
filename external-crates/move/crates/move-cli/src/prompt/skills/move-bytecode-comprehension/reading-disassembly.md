# Reading `move disassemble` output

> **This is the analysis substrate for every `SM-*` finding.** Disassembly is 1:1 with the
> executed bytecode — no heuristic reconstruction — so it is what you reason over. Decompiled
> `.move` is only used to render confirmed findings for humans (see `reading-decompiled.md`).

Disassembly is faithful but low-level: a stack machine with numbered locals and basic blocks.

```sh
move disassemble path/to/module.mv
```

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
| `CastU8..CastU256` | integer width cast — **silent truncation site (SM-F1)** |
| `Add/Sub/Mul/Div/Mod`, `Eq/Lt/...` | arithmetic / comparison (over/underflow aborts natively) |
| `VecPack(tok, n)` / `VecPushBack` / `VecMutBorrow` | vector build / mutate |
| `Branch/BrTrue/BrFalse b` | jump to block `b` |
| `Abort` | pop abort code and abort (the code is the constant just loaded) |
| `Ret` | return |

## Audit moves from disassembly

- **Transfer variant:** `Call transfer::transfer<T>` (module-internal, no `store` needed) vs
  `Call transfer::public_transfer<T>` (needs `store`, callable anywhere) — the exact symbol
  disambiguates SM-B2 / soulbound questions.
- **Abort codes:** find the `LdConst`/`LdU64` immediately before an `Abort`; that integer is the
  abort code (the source name is gone — reason from the code + the checked condition).
- **Casts:** any `CastU*` is a candidate SM-F1 truncation; check the source/target width and the
  value's origin.
- **Authorization:** look for `tx_context::sender` calls and `Eq`/`Abort` near privileged ops, or a
  capability type in the params, to judge SM-A2.
