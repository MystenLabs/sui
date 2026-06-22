# Auditing on-chain (bytecode) targets

The `SM-*` rules are written against source, but on-chain packages are compiled bytecode.
**The decompiled view (`sui move decompile` output) is the working substrate for SM-* rule
application** — abilities, visibility, the `entry` flag, signatures, struct shapes, control
flow, and call patterns are byte-for-byte faithful (see
`move-bytecode-comprehension/decompilation.md`). Disassembly is reserved for **post-finding
verification** when a determination needs an exact abort code value, when decompilation
visibly failed for a module, or when a specific question is ambiguous in decompiled output.

The per-rule signals below are written in decompiled-source terms — the patterns you'll see
when reading `.move` files.

## Workflow

1. **Stand up tools** — invoke `sui-and-move-tools` to fetch the package's `.mv` modules
   and decompile them. The default workflow writes only `.mv` + `.move` files; disassembly
   is not produced unless explicitly requested.
2. **Read the decompiled `.move` files** as the working view.
3. **Comprehend** — consult `move-bytecode-comprehension` for what survives compilation.
   Headline: abilities, visibility, `entry`, and signatures survive exactly; constant /
   error **names** become `C0/C1/...`, local names are invented, macros are expanded, empty
   structs (OTWs) show synthetic `dummy_field: bool`. These are decompiler **artifacts**
   recognized and reasoned through — they are not reasons to drop to disassembly.
4. **Apply `SM-*` rules to the decompiled view** using the per-rule signals below. Cite
   findings as `<SM-ID> · <module>.move:<line>` by default. For the rare verification
   cases (abort code value lookup, decompilation broke for the module, ambiguous decompiled
   output), fetch that module's disassembly per-module per `sui-and-move-tools/fetch-and-decompile.md`
   ("Fetching disassembly on demand") and cite as `<SM-ID> · <module>.asm:B<block>@i<index>`.
   Never load both views for the same module simultaneously.
5. **Disagreement.** If both views are inspected for the same site and disagree, the
   disassembly wins — record the discrepancy as decompiler imprecision, not as a re-opening
   of the finding.

## Per-rule signals (decompiled-source terms)

Each entry below names the concrete decompiled-source pattern to look for. Pair with the
rule definition (severity, exploit) in the category reference file. Reminder on common
decompiler renderings: `assert!(cond, code)` appears as `if (!cond) abort code;`; constant
*names* are renamed to `C0/C1/...` (values are preserved); locals are renamed (`v0`, `arg0`);
empty structs gain a synthetic `dummy_field: bool`.

### Structural — read off struct/function headers and abilities
- **SM-A1 cap ability hygiene.** Struct header carries `copy` or `drop`.
- **SM-B1 object shape.** `struct T has key` with no `id: UID`, OR value/authority type with `drop`.
- **SM-B2 broken soulbound via `store`.** A type marked `store` can be passed to any of
  the four `0x2::transfer` `public_*` variants from any module: `public_transfer` (move),
  `public_share_object` (share globally), `public_freeze_object` (freeze immutably),
  `public_receive` (accept from `Receiving<T>`). The bare variants (`transfer::transfer`,
  `share_object`, `freeze_object`, `receive`) are module-internal and don't require `store`.
  Detection: any `transfer::public_transfer<T>(...)` / `transfer::public_share_object<T>(...)` /
  `transfer::public_freeze_object<T>(...)` / `transfer::public_receive<T>(...)` call site from
  a foreign module confirms `T` has `store` — verify that was intentional. Soulbound
  (intended-non-transferable) is the most common motivation, but `store` similarly opens
  shared / frozen / receive misuse.
- **SM-G1 mint/treasury custody.** Function signatures taking `&mut TreasuryCap<T>` /
  `&mut DenyCap<T>`, and `coin::mint<T>(...)` / `coin::burn<T>(...)` call sites; trace
  where the cap argument was obtained (signature + initializer body).
- **SM-H1 OTW well-formedness.** Struct named `MODULE_NAME` (all caps) with `has drop` and a
  single synthetic `dummy_field: bool` (the "no fields" signal). Bug if it has other abilities or
  more fields; the synthetic field itself is **not** a finding.
- **SM-J1 hot-potato weakening.** Receipt/ticket struct header carries any of `copy/drop/store`.

### Authorization — dataflow over the function body
- **SM-A2 missing authorization.** A `public`/`entry` function header that mutates shared state
  but has no `&*Cap`/`&mut *Cap` parameter in its signature AND no `tx_context::sender(...)`
  followed by an `==` comparison guarded by `if (...) abort <code>` (or `assert!(...)`) earlier
  in the body.
- **SM-A3 cap not bound to its resource.** The cap struct's field list does NOT include an
  `ID`/`address`; OR it does, but the privileged function body never reads `cap.<id_field>`
  (or `&cap.<id_field>` / `&mut cap.<id_field>`) and `==`-compares it to `object::id(target)`
  before the privileged call.
- **SM-A4 one-step admin handoff.** `transfer::transfer<*Cap>(...)` /
  `transfer::public_transfer<*Cap>(...)` to a caller-supplied `address` argument with no
  preceding `==` against a proposed-new-admin / acceptance step.
- **SM-A5 forgeable witness.** A witness-typed parameter (`W: drop`) whose declaring module makes
  it constructible — i.e. the struct header is publicly accessible (no module-private fields, no
  OTW invariant). Same struct + ability check as B1/H1 but applied to the auth-type.
- **SM-A6 state-guard before privileged release/mutate.** Identify the privileged call site
  (e.g. `transfer::transfer<T>(...)` / `balance::join(...)` / `coin::take(...)` / an internal
  state mutator that releases an asset). In the code reaching it, look for a read of
  `self.<state>` / `&self.<state>` / `&mut self.<state>` (the controlling flag/deadline/amount
  on the object) compared with `==` / `<` / `>` and guarded by `if (...) abort <code>` or
  `assert!(...)`. If the field is never read, or read but never compared+branched, that's the
  bug. (Cross-check: if no such field even exists on the type, the invariant cannot be
  enforced — the privileged path is unconditional, which is the high-severity shape of this bug.)

### Type identity — generic / shared-object instantiation
- **SM-B4 type confusion / fake-object.** A function takes `&T` / `&mut T` / `Coin<T>` etc. for a
  shared protocol type or unconstrained generic, and the body never calls `object::id<T>(...)` and
  `==`-compares it to an allow-listed/registry address before the function trusts its contents
  (e.g., reads reserves and calls a transfer).

### Collections / dynamic fields
- **SM-E4 missing existence check before DF/collection access.** Any
  `dynamic_field::borrow*(...)` / `borrow_mut*(...)` / `remove*(...)` / `bag::borrow*(...)` /
  `table::borrow*(...)` / `object_bag::*(...)` / `object_table::*(...)` access not guarded by
  an `if (...)` whose condition includes the matching `dynamic_field::exists*(...)`
  (resp. `bag::contains(...)`, `table::contains(...)`, etc.). The aborting key access is the
  on-chain DoS / branch-bypass primitive.

### Arithmetic & control flow
- **SM-F1 silent truncation.** Any `as u8` / `as u16` / `as u32` / `as u64` / `as u128` /
  `as u256` cast on a value in the amount/price/index/supply dataflow.
- **SM-F2 rounding / zero amounts.** A `/` followed by a `*` (rounding loss) over a fee/share
  computation; missing zero-check (`if (amount == 0) abort <code>` or `assert!(amount > 0, ...)`
  / `assert!(amount != 0, ...)`) at the entry of mint/deposit/swap.
- **SM-D1 trusted invariants.** A caller-supplied `min_out`/price/amount consumed by a
  transfer/swap call **without** a preceding `<` / `>` / `==` bound check guarded by
  `if (...) abort <code>` or `assert!(...)` against the on-chain state.

### Composition & PTB-surface (require reasoning over the function header set)
- **SM-J2 internal transfer / leaky `_mut`.** A `public` (non-`entry`) function whose body calls
  `transfer::*` on a value it produced (so the caller cannot route it); or a function whose return
  signature is `&mut T` exposing fields the module asserts invariants over.
- **SM-K1 attacker-orchestrated PTB.** Pattern over the *set* of `public`/`entry` headers: a flow
  that depends on a fixed call order without an on-chain enforcer (no ability-less receipt being
  consumed). Inferred from API surface, not a single line.

### Time, randomness, deny lists
- **SM-L1 imprecise time.** `tx_context::epoch_timestamp_ms(...)` in a deadline-bearing function
  (vs `clock::timestamp_ms(...)` on `Clock`).
- **SM-L2 randomness test-and-abort.** `random::new_generator(...)` / `random::generate_*(...)`
  inside a function header that is NOT `entry` is compiler-rejected (so usually absent in
  compiled output). The realistic finding: an `entry` consumer whose random-derived value
  reaches a return, a struct constructor (`Reward { ... }`), or `transfer::*<reward>(...)`
  before the random effect is finalized in a way the caller can observe and abort.
- **SM-G2 deny list not enforced.** Module has a `Table<address,bool>`/`VecSet<address>` field
  (visible on a struct definition) but the transfer/action functions never call
  `...contains(...)` on it before performing the action.

### On-chain test-helper leakage
- **SM-M1 test helper leakage.** `#[test_only]` is gone in compiled output. Symptom: a
  `public`/`entry` function reachable without authorization whose body contains framework
  calls that should be gated (e.g. `coin::mint(...)`, mint-equivalent helpers, capability
  synthesis). Often overlaps SM-A2/G1 — flag as both when applicable.

## Reporting

For each finding, present the **decompiled excerpt as evidence** (the default working view).
If the determination required disassembly verification, add the disassembly excerpt too;
otherwise omit it. Use this shape:

```
SM-A3 [Critical] · pool.move:42

Evidence (decompiled):
    public fun withdraw(cap: &AdminCap, pool: &mut Pool, amount: u64, ctx: &mut TxContext) {
        // no assertion comparing cap.pool_id to object::id(pool)
        ...
    }

Why it's exploitable: ...
Exploit: ...
```

## Reproducibility

For every audit, record alongside the findings:

- Target package id + network (`mainnet` / `testnet` / `devnet`)
- GraphQL endpoint used (e.g. `https://graphql.mainnet.sui.io/graphql`)
- `sui --version` (the binary that ran `sui prompt`)

Recording the tool version matters because textual disassembly and decompilation can
change across `move` versions even when the underlying package bytecode is the same.
