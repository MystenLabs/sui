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

Several rules below ask you to "trace", "walk", or "verify absence". In disassembly
these are operational steps, not visual scans of source-shaped prose. Three idioms
recur:

1. **Verifying an absence-check across paths.** Find the privileged `Call` site
   (mint, transfer, borrow_mut, …). Walk backward through `BrFalse`/`Branch`
   predecessors to enumerate every basic block that *reaches* this Call. On each
   reaching path the guard must be present: `[Imm/Mut]BorrowField(<state>)` or
   `Call <getter>` + `Eq`/`Lt`/`Gt` + `BrFalse(<abort_block>)` where the abort
   block holds `LdConst[i]` + `Abort`. A guard present on some paths does not
   clear paths missing it; a guard elsewhere in the module (different function,
   different branch that doesn't reach this Call) does not count.

2. **Tracing a value backward through the stack machine.** From a use site (the
   operand of a `Call`, `WriteRef`, `Pack`, or `Ret`), identify what produced the
   operand: `MoveLoc[k]` / `CopyLoc[k]` ← the most recent `StLoc[k]` defined
   `loc<k>`; `MutBorrowField(T.f)` ← read from the field of some `T` instance
   (continue tracing `T` itself back); `Call <fn>(...): RetType` ← the return value
   of `<fn>` (continue tracing inside `<fn>` if the question crosses the boundary).
   Walk until you reach an `Arg<k>` (function parameter — the question becomes
   "what does the caller pass") or a `Pack[i]` (locally constructed — the question
   becomes "what fields did this module fill in").

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

## Per-rule disassembly signals

Each entry below names the concrete assembly pattern to look for. Pair with the rule
definition (severity, exploit) in the category reference file. Reminder: `assert!(cond, code)`
compiles to a `BrTrue`/`BrFalse` past an `LdConst[i]` + `Abort` pair (or equivalent);
constant *names* are renamed to `C0/C1/...` with values preserved; locals are renamed
(`loc0`, `Arg0`); empty structs gain a synthetic `dummy_field: bool`.

### Structural — read off struct/function headers and abilities
- **SM-A1 cap ability hygiene.** Struct header carries `copy` or `drop`.
- **SM-B1 object shape.** `struct T has key` with no `id: UID`, OR value/authority type with `drop`.
- **SM-B2 broken soulbound via `store`.** A type marked `store` can be passed to any of
  the four `0x2::transfer` `public_*` variants from any module: `public_transfer` (move),
  `public_share_object` (share globally), `public_freeze_object` (freeze immutably),
  `public_receive` (accept from `Receiving<T>`). The bare variants (`transfer::transfer`,
  `share_object`, `freeze_object`, `receive`) are module-internal and don't require `store`.
  Disassembly check: any `Call transfer::public_X<T>` from a foreign module confirms `T`
  has `store` — verify that was intentional. Soulbound (intended-non-transferable) is the
  most common motivation, but `store` similarly opens shared / frozen / receive misuse.
- **SM-G1 mint/treasury custody.** Mint/burn sites: `Call coin::mint<T>(&mut TreasuryCap<T>, u64, ...)`
  / `Call coin::burn<T>(&mut TreasuryCap<T>, Coin<T>)`.
  **Trace the cap backward.** At the Call, the `&mut TreasuryCap<T>` argument came from one
  of: `MoveLoc[Arg<k>]` / `MutBorrowLoc[Arg<k>]` (the cap was passed in by the entry's caller);
  `MutBorrowField(<SharedState>.cap)` (read from a shared object's field); or
  `Call dynamic_field::borrow_mut` (read from a DF on shared state). For each source: if the
  cap is a parameter of the *enclosing* entry function, the custody question is "who can hold
  this cap" — trace to its `init` and any `Call transfer::*<TreasuryCap<T>>` sites. If it comes
  from a shared object, the question is whether the entry itself is gated (`&*Cap` parameter
  or `Call tx_context::sender(...)` + abort guard). Custody is reachability: walk every path
  that reaches the mint Call.
- **SM-H1 OTW well-formedness.** Struct named `MODULE_NAME` (all caps) with `has drop` and a
  single synthetic `dummy_field: bool` (the "no fields" signal). Bug if it has other abilities or
  more fields; the synthetic field itself is **not** a finding.
- **SM-J1 hot-potato weakening.** Receipt/ticket struct header carries any of `copy/drop/store`.

### Authorization — dataflow over the function body
- **SM-A2 missing authorization.** A `public`/`entry` function header that mutates shared state
  but has no `&*Cap`/`&mut *Cap` parameter in its signature AND no `Call tx_context::sender(...)`
  followed by `Eq` + `Abort` (or equivalent gate) earlier in the body.
- **SM-A3 cap not bound to its resource.** The cap struct's field list does NOT include an
  `ID`/`address`; OR it does, but the privileged function body never `[Imm/Mut]BorrowField`s that
  field and `Eq`-compares it to the target object's id before the privileged `Call`.
- **SM-A4 one-step admin handoff.** `Call transfer::transfer<*Cap>` / `public_transfer<*Cap>` to
  a caller-supplied `address` argument with no preceding `Eq` against a proposed-new-admin /
  acceptance step.
- **SM-A5 forgeable witness.** A witness-typed parameter (`W: drop`) whose declaring module makes
  it constructible — i.e. the struct header is publicly accessible (no module-private fields, no
  OTW invariant). Same struct + ability check as B1/H1 but applied to the auth-type.
- **SM-A6 state-guard before privileged release/mutate.** Identify the privileged `Call` site
  (e.g. `Call transfer::transfer<T>` / `Call balance::join` / `Call coin::take` / an internal
  state mutator that releases an asset). In the basic blocks reaching it, look for a
  `[Imm/Mut]BorrowField(Self.<state>)` (the controlling flag/deadline/amount on the object) +
  `Eq`/`Lt`/`Gt` + `BrFalse`/`Abort` guard. If the field is never read, or read but never
  compared+branched, that's the bug. (Cross-check: if no such field even exists on the type, the
  invariant cannot be enforced — the privileged path is unconditional, which is the high-severity
  shape of this bug.)
- **SM-C3 ungated by-value shared-object deletion.** Function signature `entry public <fn>(...,
  obj: T, ...)` where `T` is a shared object type (the module creates `T` via `Call
  transfer::share_object<T>` somewhere) and the parameter is **by value** — no `&` / `&mut`
  prefix on the type. The body reaches `Call object::delete(UID)` (typically following an
  `Unpack[i](T)` that releases the `UID` field). If the function lacks `&*Cap` parameters and
  lacks a `Call tx_context::sender(...)` + `Eq` + abort gate before the destruction, the rule
  fires.

### Type identity — generic / shared-object instantiation
- **SM-B4 type confusion / fake-object.** A function takes `&T` / `&mut T` / `Coin<T>` etc. for a
  shared protocol type or unconstrained generic, and the body never calls `Call object::id<T>` and
  `Eq`-compares it to an allow-listed/registry address before the function trusts its contents
  (e.g., reads reserves and `Call`s a transfer).

### Collections / dynamic fields
- **SM-E4 missing existence check before DF/collection access.** Any `Call dynamic_field::borrow*`
  / `borrow_mut*` / `remove*` / `Call bag::borrow*` / `Call table::borrow*` / `Call object_bag::*`
  / `Call object_table::*` access with no reaching guard on every path to the access. The guard
  is usually `Call dynamic_field::exists*` (resp. `bag::contains`, `table::contains`, etc.) plus a
  conditional branch/abort shape that prevents the access when the key is absent. The aborting key
  access is the on-chain DoS / branch-bypass primitive.
- **SM-C1 orphaned dynamic fields on destruction.** Sites: `Call object::delete(UID)` (typically
  preceded by `Unpack[i](T)` that releases the `UID` field), or any `Unpack[i](T)` of a type
  that previously owned dynamic fields. **DF-cleanup check.** Look for the type's DF
  add/borrow/remove surface across the module: `Call dynamic_field::add` / `dynamic_object_field::add`
  / `Call bag::add` / `Call table::add` indicate the type carried DFs. In the basic blocks
  reaching the `object::delete` site, look for the matching `Call dynamic_field::remove*` /
  `Call bag::destroy_empty` / `Call table::destroy_empty` (or destroy/drain calls for the
  specific collection used). If no removal/destroy call precedes the delete on the same path,
  the DFs orphan — funds/objects under them become permanently unreachable.

### Arithmetic & control flow
- **SM-F1 silent truncation.** Any `CastU8` / `CastU16` / `CastU32` / `CastU64` / `CastU128` /
  `CastU256` instruction on a value in the amount/price/index/supply dataflow.
- **SM-F2 rounding / zero amounts.** `Div` followed by `Mul` (rounding loss) over a fee/share
  computation; missing zero-value guard at the entry of mint/deposit/swap. In disassembly this is
  an equivalent literal-zero comparison (`LdU*`/`LdConst` + `Eq`/`Neq`/`Gt`, etc.) plus a
  branch/abort shape that rejects zero before the amount reaches the sensitive call.
- **SM-D1 trusted invariants.** A caller-supplied `min_out`/price/amount used (read into stack)
  and consumed by a `Call` to a transfer/swap **without** a preceding `Lt`/`Gt`/`Eq` + `Abort` bound
  check against the on-chain state.

### Composition & PTB-surface (require reasoning over the function header set)
- **SM-J2 internal transfer / leaky `_mut`.** A `public` (non-`entry`) function whose body calls
  `transfer::*` on a value it produced (so the caller cannot route it); or a function whose return
  signature is `&mut T` exposing fields the module asserts invariants over.
- **SM-K1 attacker-orchestrated PTB.** Pattern over the *set* of `public`/`entry` headers: a flow
  that depends on a fixed call order without an on-chain enforcer (no ability-less receipt being
  consumed). Inferred from API surface, not a single instruction.

### Init, OTW, upgrades
- **SM-H2 unsafe `init` capability routing.** The module's `init(Arg0: <OTW>, Arg1: &mut TxContext)`
  body is where authority caps are constructed via `Pack[i](*Cap)`. After packing, each cap must
  be routed to a trusted holder. Patterns to verify per cap:
  `Call transfer::transfer<*Cap>(cap, Call tx_context::sender(ctx))` (publisher-only) or
  `Call transfer::share_object<*>` *only* when subsequent mutators of the shared wrapper are
  gated. Bugs: cap value falls off the stack (dropped — usually only possible if the cap has
  `drop`, which is itself SM-A1); cap is `Call transfer::transfer<*Cap>` to a hardcoded foreign
  address constant (not `tx_context::sender`); cap is wrapped in a shared object whose admin
  fns are not gated.
- **SM-I1 `UpgradeCap` custody / policy.** For governance / upgrade-wrapper modules: a function
  taking `&mut UpgradeCap` that calls `Call package::authorize_upgrade(...)` must, in the same
  call (or directly from its caller via a hot-potato `UpgradeReceipt` parameter), call
  `Call package::commit_upgrade(UpgradeReceipt, ...)`. A wrapper that authorizes upgrade and
  `Pack`s the resulting `UpgradeReceipt` into a struct field (via `MoveLoc` + `Pack` followed by
  `WriteRef` to a stored slot) instead of immediately calling `commit_upgrade` is the bug.
  (The receipt is a hot potato so the tx would normally abort — flag any storage path or
  wrapper that mishandles it.)
- **SM-I2 versioning / migration gap.** For every shared-object type `T` that carries a
  `version: u64` field (visible in the struct definition), every entry function with `&mut T`
  in its signature must, before reaching any state-mutating Call, contain
  `ImmBorrowField(T.version)` + `Eq` against `LdU64(<CURRENT_VERSION>)` + `BrFalse`/`Abort`
  guard. If absent, the entry remains reachable from the old package version after upgrade —
  old-vs-new logic disagree on layout/invariants. New shared singletons introduced by an
  upgrade also need their own one-shot initializer entry function (the framework does NOT
  re-run `init` on upgrade).

### Time, randomness, deny lists
- **SM-L1 imprecise time.** `Call tx_context::epoch_timestamp_ms` in a deadline-bearing function
  (vs `Call clock::timestamp_ms` on `Clock`).
- **SM-L2 randomness test-and-abort.** `Call random::new_generator` / `Call random::generate_*`
  inside a function header that is NOT `entry` is compiler-rejected (so usually absent in
  bytecode). The realistic finding: an `entry` consumer whose random-derived value reaches a
  `Ret` / `Pack` / `Call transfer::*<reward>` before the random effect is finalized in a way the
  caller can observe and abort.
  **Dataflow tracing.** The random consumer is often a private helper (no `entry`/`public`
  prefix on the function header) called from a higher-level `entry` function — trace the value
  across the call boundary. From `Call random::generate_*`, follow the `StLoc[k]` destination
  through subsequent `MoveLoc[k]` / `CopyLoc[k]` uses; if `loc<k>` (or any function-return value
  derived from it) eventually reaches a `Ret`, a `Pack[i](RewardStruct)` then `Ret`, or a `Call
  transfer::public_transfer<*>` whose recipient comes from a caller-supplied `address` argument,
  the bug is present. Safe shape: the value flows into `WriteRef` on a shared-object field or
  into `Call transfer::public_transfer<*>(coin, Call tx_context::sender(ctx))` and the function
  has no return value depending on it.
- **SM-G2 deny list not enforced.** Module has a `Table<address,bool>`/`VecSet<address>` field
  (visible on a struct definition) but the transfer/action functions never `Call ...contains(...)`
  on it before performing the action.

### On-chain test-helper leakage
- **SM-M1 test helper leakage.** `#[test_only]` is gone in bytecode. Symptom: a `public`/`entry`
  function reachable without authorization whose body contains framework calls that should be
  gated (e.g. `Call coin::mint`, mint-equivalent helpers, capability synthesis). Often overlaps
  SM-A2/G1 — flag as both when applicable.

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
