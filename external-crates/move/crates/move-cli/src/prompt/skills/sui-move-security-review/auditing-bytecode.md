# Auditing on-chain (bytecode) targets

The `SM-*` rules are written against source, but on-chain packages are compiled bytecode. **The
disassembly is the analysis substrate** — `sui move disassemble` output is faithful, 1:1 with the
executed bytecode, and that is what every `SM-*` finding must be derived from. Decompiled `.move`
is a *heuristic* reconstruction; reach for it only to render an *already-confirmed* finding for a
human reader. A pattern visible only in decompiled source — but absent or different in the
disassembly — is not a finding.

## Workflow

1. **Stand up tools** — invoke `sui-audit-toolchain` (confirm `SUI_REF` with the user, suiup-managed
   `sui`, clone, fetch the package's `.mv` modules). **Never reuse a local Sui checkout or any
   binary built inside one** — the audit uses only the freshly-built decompiler in `$AUDIT_WORK`
   and the `suiup`-managed `sui` CLI (the toolchain skill enforces this).
2. **Disassemble every module** — this is the analysis substrate. The toolchain runbook also
   decompiles every module, but those `.move` files are for *later* finding-explanation only; do
   not read them as analysis input.
3. **Comprehend** — consult `move-bytecode-comprehension` for what survives compilation. Headline:
   abilities, visibility, `entry`, and signatures survive exactly; constant/error **names**, local
   names, comments, and macro sugar do not; empty structs (OTWs) show `dummy_field: bool`.
4. **Apply `SM-*` rules to the disassembly** using the per-rule signals below. Cite findings as
   `<SM-ID> · <module>.asm:B<block>@i<index>` (basic block + instruction index). Once a finding is
   confirmed, render the matching decompiled snippet alongside as **Human view** (see Reporting).
5. **Decompiled disagreement is a tell, not a finding.** If the decompiled `.move` for a confirmed
   site reads differently from the assembly, the **assembly wins** — note the discrepancy as
   decompiler imprecision, do not re-open the finding.

## Per-rule disassembly signals

Each entry below names the concrete assembly pattern to look for. Pair with the rule definition
(severity, exploit) in the category reference file.

### Structural — read off struct/function headers and abilities
- **SM-A1 cap ability hygiene.** Struct header: `struct *Cap has ... { ... }` containing `copy`
  or `drop` is the bug. Disassembly prints `has <abilities>` verbatim.
- **SM-B1 object shape.** `struct T has ... key ...` with no `id: UID` field, OR an
  authority/value type with `drop`. Both visible on the struct header line.
- **SM-B2 broken soulbound via `store`.** `struct T has ... store ...` on a type intended to be
  non-transferable. **Confirm transfer variant** at the call site: `Call transfer::transfer<T>`
  (module-restricted) vs `Call transfer::public_transfer<T>` (anyone, requires `store`).
- **SM-G1 mint/treasury custody.** Function signatures taking `&mut TreasuryCap<T>` /
  `&mut DenyCap<T>`, and the `Call coin::mint<T>(...)` / `Call coin::burn<T>(...)` sites; trace
  where the cap argument was obtained (signature + initializer disassembly).
- **SM-H1 OTW well-formedness.** Struct named `MODULE_NAME` (all caps) with `has drop` and a
  single synthetic `dummy_field: bool` (the "no fields" signal). Bug if it has other abilities or
  more fields; the synthetic field itself is **not** a finding.
- **SM-J1 hot-potato weakening.** Receipt/ticket struct header carrying any of `copy/drop/store`
  is the bug — its strength is exactly the ability set.

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

### Type identity — generic / shared-object instantiation
- **SM-B4 type confusion / fake-object.** A function takes `&T` / `&mut T` / `Coin<T>` etc. for a
  shared protocol type or unconstrained generic, and the body never calls `Call object::id<T>` and
  `Eq`-compares it to an allow-listed/registry address before the function trusts its contents
  (e.g., reads reserves and `Call`s a transfer).

### Collections / dynamic fields
- **SM-E4 missing existence check before DF/collection access.** Any `Call dynamic_field::borrow*`
  / `borrow_mut*` / `remove*` / `Call bag::borrow*` / `Call table::borrow*` / `Call object_bag::*`
  / `Call object_table::*` access whose predecessor block contains no `Call
  dynamic_field::exists*` (resp. `bag::contains`, `table::contains`, etc.) + `BrFalse` to a guarded
  path. The aborting key access is the on-chain DoS / branch-bypass primitive.

### Arithmetic & control flow
- **SM-F1 silent truncation.** Any `CastU8` / `CastU16` / `CastU32` / `CastU64` / `CastU128` /
  `CastU256` instruction on a value in the amount/price/index/supply dataflow.
- **SM-F2 rounding / zero amounts.** `Div` followed by `Mul` (rounding loss) over a fee/share
  computation; missing `LdU64(0)` + `Eq` + `Abort` guard at the entry of mint/deposit/swap.
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

### Time, randomness, deny lists
- **SM-L1 imprecise time.** `Call tx_context::epoch_timestamp_ms` in a deadline-bearing function
  (vs `Call clock::timestamp_ms` on `Clock`).
- **SM-L2 randomness test-and-abort.** `Call random::new_generator` / `Call random::generate_*`
  inside a function header that is NOT `entry` is compiler-rejected (so usually absent in
  bytecode). The realistic finding: an `entry` consumer whose random-derived value reaches a
  `Ret` / `Pack` / `Call transfer::*<reward>` before the random effect is finalized in a way the
  caller can observe and abort.
- **SM-G2 deny list not enforced.** Module has a `Table<address,bool>`/`VecSet<address>` field
  (visible on a struct definition) but the transfer/action functions never `Call ...contains(...)`
  on it before performing the action.

### On-chain test-helper leakage
- **SM-M1 test helper leakage.** `#[test_only]` is gone in bytecode. Symptom: a `public`/`entry`
  function reachable without authorization whose body contains framework calls that should be
  gated (e.g. `Call coin::mint`, mint-equivalent helpers, capability synthesis). Often overlaps
  SM-A2/G1 — flag as both when applicable.

## Reporting

For each finding, present **both** views — disassembly as ground-truth evidence, decompiled snippet
as the human view. Use this shape:

```
SM-A3 [Critical] · pool.asm:B3@i17  (human view: pool.move:42)

Disassembly evidence (ground truth):
    14: CopyLoc[1](Arg1: &Pool)
    15: ImmBorrowField[2](Pool.id)
    16: Call object::id<Pool>(&Pool): ID
    17: Call <withdraw>(...): ...        # <- privileged op WITHOUT prior cap.pool_id == id check

Human view (decompiled):
    public fun withdraw(cap: &AdminCap, pool: &mut Pool, amount: u64, ctx: &mut TxContext) {
        // no assertion comparing cap.pool_id to object::id(pool)
        ...
    }

Why it's exploitable: ...
Exploit: ...
```

Always record `SUI_REF`, network, and package id with the report so the audit is reproducible.
