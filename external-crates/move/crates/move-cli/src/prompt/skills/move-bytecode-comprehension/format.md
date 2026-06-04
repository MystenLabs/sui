# The Move binary format

A `.mv` file is a serialized `CompiledModule`. You rarely read the raw bytes — you read
disassembly/decompilation — but knowing the structure explains *why* certain things survive and
others don't.

Reference (public):
`external-crates/move/crates/move-binary-format/src/file_format.rs`. Key definitions (line numbers
approximate; they drift between refs — grep the enum name):
`Visibility` ~L492 · `Ability` ~L651 · `AbilitySet` ~L802 · `SignatureToken` ~L1019 ·
`Bytecode` ~L1309.

## Tables in a CompiledModule

A module is a set of pools/tables referenced by index:

- **Identifier pool** — every name: module, struct, field, function, type parameter labels. This
  is why on-chain names survive. (User-intent names that aren't needed at runtime — constants,
  locals — are *not* here as such.)
- **Address identifiers** — account addresses used by the module.
- **Module handles** — (address, name) of this module and its dependencies → the `use` lines.
- **Struct handles / definitions** — declared types: name, **`AbilitySet`**, type parameters
  (with their ability constraints + `is_phantom`), and the field list (names + `SignatureToken`s).
- **Function handles / definitions** — name, signature (param + return `Signature`),
  **`Visibility`**, **`is_entry`**, type parameters, and the **code unit** (locals signature +
  `Vec<Bytecode>`).
- **Signatures** — sequences of `SignatureToken`s (types) referenced by index.
- **Constant pool** — typed, BCS-encoded constant *values*. No names: a constant is just
  `(type, bytes)`, addressed by index (`LdConst[i]`). This is why error-constant names and
  `#[error]` strings vanish.
- **Friend declarations** — modules allowed to call `public(friend)`/package-visible functions.

## Abilities

`Ability` = `Copy | Drop | Store | Key`. An `AbilitySet` is a bitset (`struct AbilitySet(u8)`).
Both struct declarations and each type parameter carry an `AbilitySet`, fully preserved. So
"`AdminCap has key, store`", "no `copy`/`drop`", phantom-ness, and ability *constraints* on
generics are all recoverable — the backbone of the capability/soulbound/hot-potato patterns.

## Visibility & entry

`Visibility` = `Private | Public | Friend`. Separately, a function definition has an `is_entry`
bool. So the four observable forms are: `public`, `public(friend)`/package, `entry` (private
entry), and private. Both fields survive — visibility-based rules are reliable on bytecode.

## Signature tokens (types)

`SignatureToken` encodes types: `Bool, U8..U256, Address, Signer, Vector(_), Struct(handle),
StructInstantiation(handle, [args]), Reference(_), MutableReference(_), TypeParameter(idx)`. So
references vs values, mutability (`&` vs `&mut`), generic instantiations, and `vector<_>` are all
explicit — important for `_mut` getter and type-confusion reasoning even without local names.

## Bytecode (instructions)

The `Bytecode` enum is a stack machine (see `disassembly.md`). Categories:
- **Stack/locals:** `LdU8..LdU256, LdConst, LdTrue/False, CopyLoc, MoveLoc, StLoc, Pop`.
- **References:** `[Mut]BorrowLoc, [Mut]BorrowField, [Mut]BorrowGlobal*` (note: global storage ops
  are largely unused in Sui's object model), `ReadRef, WriteRef, FreezeRef`.
- **Calls/structs:** `Call, CallGeneric, Pack, Unpack` (and generic variants).
- **Arithmetic/logic:** `Add, Sub, Mul, Div, Mod, BitOr/And/Xor, Shl/Shr, Or, And, Not, Eq, Neq,
  Lt, Gt, Le, Ge`.
- **Casts:** `CastU8 .. CastU256` ← silent-truncation sites.
- **Control flow:** `Branch, BrTrue, BrFalse, Ret, Abort` (`Abort` pops the abort *code*).
- **Vectors:** `VecPack, VecLen, VecImmBorrow, VecMutBorrow, VecPushBack, VecPopBack, VecUnpack,
  VecSwap`.

What you will NOT find here: `MoveTo/MoveFrom/Exists` global-storage ops — Sui uses the
object model + framework calls (`object::new`, `transfer::*`, `dynamic_field::*`) instead, which
appear as ordinary `Call`s.
