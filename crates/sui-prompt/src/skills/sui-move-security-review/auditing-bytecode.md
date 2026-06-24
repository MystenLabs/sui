# Auditing bytecode — disassembly signals for the SM-* catalog

The `SM-*` rules are written in Move semantics. For deployed on-chain packages, the
working representation is disassembly (`.asm`) produced from compiled bytecode. **This
file is the disassembly-side bridge for the catalog**: each per-rule signal below names
the concrete opcode pattern you'll see when reading `.asm` output from
`sui move disassemble`.

**If the target is a source repository**, skip this file — the Invariant/Detect/Exploit
statements in the other catalog files apply directly to `.move` text. **If the target is
a deployed package**, use this file alongside the rule files. Disassembly is 1:1 with the
executed bytecode; findings derived from it are byte-for-byte faithful.

## Disassembly-side workflow

1. **Stand up tools** — invoke `sui-and-move-tools` to fetch the package's `.mv` modules
   and disassemble every module to `.asm`.
2. **Read the `.asm` files** as the working view. Read surgically — `grep` / `sed` for the
   specific function or basic block rather than dumping the full file into context.
3. **Comprehend** — consult `move-bytecode-comprehension` for what survives compilation.
   Headline: abilities, visibility, `entry`, and signatures survive exactly; constant /
   error **names**, local names, comments, and macro sugar do not; empty structs (OTWs)
   show synthetic `dummy_field: bool`.
4. **Apply `SM-*` rules to the disassembly** using the per-rule signals below. Cite
   findings as `<SM-ID> · <module>.asm:B<block>@i<index>` — the basic-block label plus
   instruction index within the block.

## Reasoning idioms in disassembly

Many `SM-*` rules ask you to "trace", "walk", or "verify absence". In disassembly
these are operational steps, not visual scans of source-shaped prose. Three idioms
recur — the per-rule table further below pairs each rule with the idiom it needs:

1. **Verifying an absence-check across paths.** Find the privileged `Call` site
   (mint, transfer, borrow_mut, …). Walk backward through `BrFalse`/`Branch`
   predecessors to enumerate every basic block that *reaches* this Call. On each
   reaching path the guard — the disassembly form of an `assert!(cond, code)` /
   `if (!cond) abort code` — must be present. A guard is a 3-part sequence: (a)
   push the value being checked onto the stack, either via
   `[Imm/Mut]BorrowField(<state>)` (read a field) or via `Call <getter>` (call a
   function that returns the value); (b) compare it with `Eq` / `Lt` / `Gt` /
   `Neq`; (c) `BrFalse(<abort_block>)` where the abort block holds `LdConst[i]`
   + `Abort`. All three parts together are *the* guard — none of them alone is.
   A guard present on some paths does not clear paths missing it; a guard
   elsewhere in the module (different function, different branch that doesn't
   reach this Call) does not count.

2. **Tracing a value through the stack machine.** Walk dataflow either
   *backward* from a use site (the operand of a `Call`, `WriteRef`, `Pack`,
   or `Ret`) to the opcode that put the operand on the stack, or *forward*
   from a known producer to where the value ends up. The producer cases
   below enumerate the patterns you may encounter — they're a menu, not a
   checklist; apply only the one matching the opcode you actually see at
   each step. Different rules exercise different subsets and directions: a
   reference-tracing question (e.g. SM-G1's `&mut TreasuryCap<T>`, backward
   from each mint/burn site) sees the `BorrowLoc` / `BorrowField` cases; a
   forward value trace (e.g. SM-L2's random `u64` from each
   `Call random::generate_*`; SM-H2's authority caps from each
   `Pack[i](*Cap)` in `init`) sees `StLoc`-to-`MoveLoc` chains and `Call`
   returns.

   - `MoveLoc[k]` / `CopyLoc[k]` — pushed the value of local `loc<k>`.
     Continue from the most recent `StLoc[k]` that *defined* `loc<k>`; resume
     tracing from whatever that `StLoc[k]` consumed off the stack.
   - `[Imm/Mut]BorrowLoc[k]` — pushed a reference to local `loc<k>`. Same
     trace step as `MoveLoc[k]` above: find the most recent `StLoc[k]`
     defining `loc<k>`.
   - `[Imm/Mut]BorrowField(T.f)` — pushed a reference to field `f` of some
     `T` instance. Continue by tracing the `T` value back (the field belongs
     to whichever `T` was on the stack when the BorrowField executed).
   - `ReadRef` — pushed the dereferenced *value* of a reference that was on
     the stack just before. Continue by tracing that reference back through
     its producer (typically one of the `BorrowField` / `BorrowLoc` / `Call`
     cases above or below).
   - `Call <fn>(...): RetType` — pushed the return value of `<fn>`. Continue
     by tracing inside `<fn>` if the question crosses the function boundary.

   Backward walk terminates at one of these cases:

   - `Arg<k>` — a function parameter. The question becomes *"what does the
     caller pass?"*; if audit scope includes callers, you may need to trace
     forward into their call sites.
   - `Pack[i](T)` — a struct constructed locally. The question becomes *"what
     values did this module put into each field at construction?"*.

   *Forward walk* inverts each producer rule above — from a `StLoc[k]` follow
   subsequent `MoveLoc[k]` / `CopyLoc[k]` / `[Imm/Mut]BorrowLoc[k]` of that
   local; from an unstored `Pack` / `Call` result on the stack, the next
   consuming opcode is the next step. Forward walk terminates at a
   final-effect opcode: `Ret`, `WriteRef` to a stored slot, a transfer /
   destroy `Call`, or a `Pack` consuming the value into a struct whose own
   trace continues.

3. **Type tracking across opcodes.** The function signature line names each
   `Arg<k>`'s type verbatim, including generics: `Arg0: &mut Pool<USDC>`,
   `Arg1: Coin<Ty0>`. `Pack[i]` / `Unpack[i]` reference a specific declared type.
   Generic instantiations appear in `Call` ops as `Call <fn><Ty0>(...)`. When a
   generic-`Ty0` value flows into the same operation as a concrete-typed value
   (e.g., an `Add` of a `Call coin::value<Ty0>` result over a
   `MutBorrowField(Pool<USDC>.balance) + ReadRef`), the type mismatch *is* the
   signal — Move's type system would normally reject this in source, so its
   presence in disassembly means the function is generic over `Ty0` and never
   constrained it.

## Per-rule disassembly procedure

Each `SM-*` rule's disassembly-side procedure is one of the three reasoning idioms
above (or a signature-shape check that needs no idiom). The table pairs each rule
with its procedure. The grep target — the framework `Call` that opens the
procedure — is named in the rule file (`access-control.md`,
`arithmetic-and-coins.md`, etc.); restating it here would just duplicate the
catalog. The rules whose search target is a disassembly-only opcode shape (no
source-level analog) are flagged **(shape below)** and detailed in the following
section. Macro / sugar expansion (`assert!(c, e)` → `BrFalse` past `LdConst[i]` +
`Abort`; `vector[..]` → `VecPack` / `VecPushBack`; `n.do!(...)` → branch-based
loop) is covered in `move-bytecode-comprehension/disassembly.md`.

**Rules may broaden the search beyond literal names.** Many `SM-*` rules note
that caps, protocol types, and controlling fields are identified by *role*,
not by literal symbol — e.g., SM-A3 ("the authority's name doesn't matter")
and SM-B4 ("the type's name doesn't matter"). Consult the rule file before
clearing a rule on the basis of a literal-name search alone.

| Rule | Procedure |
|---|---|
| **SM-A2** | #1 — caller-identity guard on the privileged path |
| **SM-A3** | #1 — cap-binding guard on the privileged path |
| **SM-A4** | #1 — acceptance guard on the cap-handoff path |
| **SM-A6** | #1 — object-state guard on the privileged path |
| **SM-B2** | signature-shape — `Call transfer::public_*<T>` from a foreign module confirms `T` has `store` |
| **SM-C1** | #1 — DF/collection cleanup on the path to the destruction primitive **(shape below)** |
| **SM-C3** | #1 — auth gate on the path to the destruction primitive **(shape below)** |
| **SM-E4** | #1 — existence-check guard on the access path |
| **SM-G1** | #2 — trace `&mut TreasuryCap<T>` from each mint/burn site to its source |
| **SM-H2** | #2 — trace each authority cap from its `Pack` to its disposition **(shape below)** |
| **SM-I1** | #2 — trace `UpgradeReceipt` between `authorize_upgrade` and its consumer **(shape below)** |
| **SM-I2** | #1 — version guard on the entry path **(shape below)** |
| **SM-L1** | signature-shape — `Call tx_context::epoch_timestamp_ms` (bug) vs `Call clock::timestamp_ms` (good) |
| **SM-L2** | #2 — trace random-derived value from `random::generate_*` to its observable use site |

**SM-K1** has no per-opcode procedure — the rule fires on patterns over the *set*
of `public`/`entry` function headers (PTB-orchestration surface), inferred from
the API surface, not from any single instruction.

## Disassembly-only opcode shapes

A handful of search targets exist only in disassembly — opcode patterns with no
condensed source-level expression. These rows from the pairing table need the
shape *as* the search target; once you have a hit, apply the rule's procedure
from the table above.

- **SM-C1 / SM-C3 — destruction primitive: `Unpack[i](T)` + `Call object::delete(UID)`.**
  Destroying a `T` releases its `UID` field via `Unpack[i](T)` of the wrapping
  struct, then consumes the UID via `Call object::delete(UID)`. SM-C1 walks back
  from each such pair looking for matching DF/collection cleanup
  (`Call dynamic_field::remove*` / `bag::destroy_empty` /
  `table::destroy_empty`) on the path. SM-C3 walks back checking the enclosing
  function's signature takes `obj: T` by value (no `&` / `&mut`) and lacks an
  auth gate.

- **SM-H2 — `Pack[i](*Cap)` inside `init(Arg0: <OTW>, Arg1: &mut TxContext)`.**
  Authority caps are constructed by `Pack[i](<Cap>)` in the module initializer.
  Each cap value produced by `Pack` is the starting point for a forward dataflow
  trace (the inverse of idiom #2 — same producer table walked the other way),
  following each cap to a publisher-only routing
  (`Call transfer::transfer<*Cap>(cap, Call tx_context::sender(ctx))`), a gated
  share, or — the bug — a hardcoded address, a drop, or an un-gated share.

- **SM-I1 — receipt-storage misuse: `Pack[i]` of a struct containing
  `UpgradeReceipt` then `WriteRef` to a stored slot.** The hot-potato pattern
  would normally consume `UpgradeReceipt` immediately via
  `Call package::commit_upgrade`. A misuse stashes it instead: `MoveLoc` of the
  receipt + `Pack[i]` of a containing struct + `WriteRef` to a field on stored
  state. The `Pack` + `WriteRef` over a receipt-bearing struct is the
  disassembly-only signal.

- **SM-I2 — version guard: `ImmBorrowField(T.version)` + `Eq` against
  `LdU64(<CURRENT_VERSION>)` + `BrFalse(<abort_block>)`.** The source-level
  `assert!(obj.version == CURRENT_VERSION)` compiles to an idiom-#1 guard whose
  right-hand comparand is a `LdU64` literal — the `CURRENT_VERSION` constant
  *name* is gone, the value remains. Idiom #1 walks every entry function with
  `&mut T` checking this shape is present before any state-mutating Call on `T`.

## Reporting

For each finding, present the disassembly excerpt as evidence. Use this shape:

```
SM-A3 [Critical] · pool.asm:B3@i17

Evidence (disassembly):
    14: CopyLoc[1](Arg1: &Pool)
    15: ImmBorrowField[2](Pool.id)
    16: Call object::id<Pool>(&Pool): ID
    17: Call <withdraw>(...): ...        # <- privileged op WITHOUT prior cap.pool_id == id check

Why it's exploitable: ...
Exploit: ...
```

## Reproducibility

For every audit, record alongside the findings:

- Target package id + network (`mainnet` / `testnet` / `devnet`)
- GraphQL endpoint used (e.g. `https://graphql.mainnet.sui.io/graphql`)
- `sui --version` (the binary that ran `sui prompt` and `sui move disassemble`)

Recording the tool version matters because textual disassembly can change across `sui`
versions even when the underlying package bytecode is the same.
