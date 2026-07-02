# Move bytecode disassembly

Disassembly is 1:1 with the executed bytecode. For compiled Move packages — including
everything deployed on chain — it is **the view**: there is no readable source available,
only the `.mv` binary and the disassembly produced from it via `sui move disassemble`.
Disassembly is faithful but low-level: a stack machine with numbered locals and basic
blocks.

## How to read it

- **Header:** `// Move bytecode vN` (version) and `module <addr>.<name> { ... }`. `use` lines are
  dependencies, fully address-qualified.
- **Structs:** name + `has <abilities>` + fields. A fields-less source struct (e.g. an OTW
  named `MODULE_NAME`) is represented with a synthetic `dummy_field: bool` — expected, not
  a bug; it's how a zero-field struct is encoded.
- **Functions:** signature line shows `name(Arg0: T, Arg1: U): Ret`, with `public` / `entry`
  prefixes when present. The module initializer `init(..., &mut TxContext)` has no
  visibility/`entry` prefix — it's the special initializer the framework calls once at
  publish. Parameters are `Arg0, Arg1, ...`; declared locals are `loc0, loc1, ...` with
  `Ln:` slot labels — **these names are invented**, not from source.
- **Basic blocks:** `B0:`, `B1:` … each a straight-line run; control flow between them is via
  `Branch/BrTrue/BrFalse` to a block, ending with `Ret`/`Abort`.
- **Instructions:** numbered, operate on the operand stack.
- **Abort sites:** an `Abort` instruction pops the abort code from the top of the stack.
  In simple cases the value is loaded immediately before the abort (`LdConst`, `LdU64`,
  etc.); otherwise follow the local stack/dataflow into the abort site. Source names
  (e.g. `EUnauthorized`) don't survive — reason from the integer value and the guarded
  condition.

## What's faithful (trust it)

- **Struct names, field names, field types, and abilities** (`has key, store`). Capability /
  hot-potato / non-transferability reasoning works directly from the struct header.
- **Function visibility and `entry`**, parameter and return types (incl. `&`/`&mut`,
  generics, phantom). Type-confusion reasoning works from signatures.
- **Control and data flow** — branches, calls, field reads/writes, the order of operations.

## What's lost (don't be fooled)

- **Constants are renamed `C0, C1, …`** with raw values. Source names (`ENotAuthorized`,
  `MAX_FEE`) and `#[error]` messages are gone. To decode a `vector<u8>` constant, read it
  as ASCII (e.g. `[110, 97, 109, 101]` = `"name"`). Abort sites: key off the numeric
  abort code and the guarded condition, not a name.
- **Local variable names are invented** (`loc0`, `Arg0`, …). Never base a conclusion on a
  local's name; follow the dataflow.
- **Macros are expanded.** `assert!(c, e)` appears as `BrTrue` past an `LdConst[i]` + `Abort`
  pair (or equivalent); loop macros (`n.do!(...)`) appear as explicit branch-based loops.
  A `vector[..]` literal may appear as a built-up vector via `VecPack` / `VecPushBack`.
  Recognize the shape, not the sugar.

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
| `Abort` | pop abort code from the top of the stack and abort |
| `Ret` | return |
