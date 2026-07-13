# J/K — Composability, hot potato & PTB attack surface

Move has no dynamic dispatch (so no EVM-style reentrancy), but composition risk reappears at the
PTB layer: the attacker chains your `public` functions in any order, supplies any owned/shared
input, and takes any returned value. Hot-potato types are the on-chain mechanism that forces a
flow to complete; weakening them removes the guarantee.

### SM-J1 — Weakened hot-potato / receipt   [Critical]
Invariant: a struct used to enforce "must be resolved in the same transaction" (flash-loan
receipt, borrow receipt, oracle-update ticket) has ZERO abilities. Adding `drop`, `store`, or
`copy` destroys the enforcement. **This also applies to the framework `sui::borrow::Referent<T>`
+ `Borrow` pattern** — the `Borrow` receipt MUST remain ability-less. Flag any module that
extends or wraps it with `drop`/`store`/`copy`.
Detect: a `Receipt`/ticket/potato-style type that has gained any ability; a flash-loan/borrow API
whose returned obligation type is not ability-less; a wrapper around `sui::borrow::Borrow` that
introduces abilities.
Exploit: take the loan/borrowed asset and drop (or stash) the receipt instead of repaying →
the repayment/return check is never forced.
Source: `MystenLabs/skills → object-model/patterns.md` (hot-potato + `sui::borrow::Referent` /
`Borrow` patterns), `MystenLabs/skills → naming-conventions/SKILL.md`.

### SM-J2 — Internal transfer or leaky `_mut` getter   [High]
Invariant: composable `public` functions return assets so the caller decides their destination
(don't `transfer` internally); accessor functions returning `&mut` into invariant-bearing
internals are dangerous and must be `_mut`-named, package-scoped, or absent.
Detect: `public fun` that creates/produces an asset and calls `transfer::*` on it internally
(coupling + redirecting control); `public fun \w+_mut(...): &mut` exposing fields that other code
relies on as invariant (balances, supply, config).
Exploit: callers mutate internal state directly through the leaked `&mut`, bypassing the checks
the module assumed; or a forced internal transfer routes an asset away from the intended owner.
Source: `MystenLabs/skills → composable-move-functions/SKILL.md`, `MystenLabs/skills → naming-conventions/SKILL.md`.

### SM-K1 — Logic unsafe under attacker-orchestrated PTB   [High]
Invariant: correctness does not depend on a fixed call order or on "the frontend calls these in
sequence". Multi-step flows are enforced on-chain via hot potatoes or recorded state, not by
convention. Assume: arbitrary command ordering, arbitrary owned/shared inputs of the right type,
and that every returned object/value is captured by the attacker.
Detect: a `public` function that leaves the system in an exploitable intermediate state if the
expected follow-up call is skipped or reordered; functions that return a powerful value (cap,
balance, `&mut`-derived object) with no obligation forcing safe use.
Exploit: build a PTB that calls step 1, then a different function, then step 3 — reaching a state
the author assumed unreachable (e.g. withdraw before the solvency-restoring step).
Source: `MystenLabs/skills → ptbs/fundamentals.md`, `MystenLabs/skills → ptbs/commands.md`.

### SM-K2 — Gas-coin / sponsored-transaction misuse   [Medium]
Invariant: `GasCoin` is passed by value only to `TransferObjects` (split first otherwise); in
sponsored flows both parties sign the entire `TransactionData` (incl. `GasData`) and the sponsor
validates the PTB doesn't divert the gas coin to app logic.
Detect (mostly off-chain/integration): app passing `tx.gas` by value to arbitrary calls; sponsor
signing partial data; sponsor not inspecting the PTB before signing.
Exploit: an untrusted sender drains the sponsor's gas coin via split+transfer, or gas data is
substituted mid-signing.
Also (off-chain PTB construction we review): `SplitCoins` with an empty `amounts` array and
`MergeCoins` with an empty `to_merge` array **fail pre-execution** — relevant for sponsor /
backend code that builds PTBs from user input without preflighting non-emptiness; treat as input
validation, not a Move-code finding.
Source: `MystenLabs/skills → ptbs/building.md`, `MystenLabs/skills → ptbs/fundamentals.md`,
`MystenLabs/skills → ptbs/commands.md` (non-empty array preflights).
