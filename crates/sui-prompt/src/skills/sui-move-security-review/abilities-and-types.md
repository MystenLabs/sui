# B — Struct abilities & type safety

Abilities (`key`, `store`, `copy`, `drop`) are load-bearing security properties, not annotations.
Most ability bugs are *irreversible by design* (adding `store` can't be walked back), so they are
worth flagging even when the current code looks safe.

### SM-B1 — Wrong object shape   [High]
Invariant: a `key` struct has an `id: UID` field; resource/authority objects do NOT have `drop`
(absence of `drop` forces explicit handling and preserves accounting).
Detect: `has key` declarations missing `id: UID`; value-bearing or authority objects declared
with `drop`.
Exploit: a `drop`-able resource/authority object can be silently discarded, breaking supply or
access accounting (lost funds, lost admin).
Source: `MystenLabs/skills → sui-move/move.md`.

### SM-B2 — Irreversible `store` breaks soulbound / locked invariants   [Critical]
Invariant: objects intended to be non-transferable, locked, or under custom transfer rules must
NOT have `store`. The `store` ability globally enables `public_transfer`, `public_share_object`,
and `public_freeze_object` from *any* module — and cannot be removed later.
Detect: types described/used as "soulbound", bound, locked, or escrowed that carry `store`; or
such types with module-exposed transfer functions.
Exploit: anyone calls `public_transfer` to move/steal the object, or `public_share_object` to
force it into shared state, bypassing the intended binding.
Source: `MystenLabs/skills → object-model/ownership.md`, `MystenLabs/skills → object-model/transfers.md`.

### SM-B3 — Secret data in events or Display   [Medium]
Invariant: event structs have exactly `copy, drop`; no confidential field is emitted in an event
or interpolated into a `Display<T>` template (both are publicly indexed/rendered).
Detect: `event::emit` of structs containing secrets/keys/PII; `{field}` Display placeholders over
sensitive fields.
Exploit: read "private" on-chain data straight from an indexer or wallet UI — chain storage is
public regardless of access functions.
Source: `MystenLabs/skills → sui-move/events-coins.md`, `MystenLabs/skills → object-model/display.md`.

### SM-B4 — Type confusion / fake-object injection   [Critical] [+domain]
Invariant: a function that accepts an instance of a protocol-defined type
(`&Pool`, `&mut Vault`, `&Treasury`, etc.) for accounting or authorization must verify the
instance is the canonical one — e.g. it is *the* registry/shared object, is recorded in an
allow-list, or its `object::id` matches an expected value. Type alone is not identity: a
value of the same type can come from somewhere else (the protocol's own constructor, a
test-only helper that leaked into production, or attacker-supplied if the type is publicly
constructible).
Detect: protocol-type parameters used to read reserves/balances/permissions without an
identity/canonicity check; functions trusting the *contents* of a passed-in object that
the caller could have minted.
_Absence rule:_ walk every fn taking a protocol-type parameter (`&Pool`, `&mut Vault`, etc.)
whose body reads its state; an `object::id(...)`/allow-list check *elsewhere* does not
clear an unchecked fn.
Example:
```move
// Vulnerable: trusts the contents of `pool` without verifying its identity.
public fun get_quote(pool: &Pool, amount_in: u64): u64 {
    let reserve_in = pool.reserve_x;
    let reserve_out = pool.reserve_y;
    (amount_in * reserve_out) / (reserve_in + amount_in)
}

// Patched: registry-pinned identity check before reading state.
public fun get_quote(pool: &Pool, registry: &PoolRegistry, amount_in: u64): u64 {
    assert!(registry.contains(object::id(pool)), EUnknownPool);
    let reserve_in = pool.reserve_x;
    let reserve_out = pool.reserve_y;
    (amount_in * reserve_out) / (reserve_in + amount_in)
}
```
*This is one shape, not the only shape.* SM-B4 fires on any fn reading state
from a passed-in object without first verifying the instance is canonical.
The type's **name doesn't matter** (e.g., `Pool`, `Vault`, `Market`, or any
project-specific name). Other variants: identity check via registry or
allow-list membership, `object::id` match against a stored expected id,
identity stamped during a separate `*_init` call. The absence rule above is
authoritative; identify trust-bearing types by *role*, not by name.
Exploit: pass a self-created `Pool` with fake reserves to satisfy a price/solvency check, or to
redirect a withdrawal that reads state from the attacker's object.
Source: [+domain] (object/type-confusion; the constructive skills assume the canonical object).

### SM-B5 — Generic-type substitution / unconstrained witness   [Critical] [+domain]
Invariant: a function generic over a caller-chosen type parameter (`<T>`, `<T: drop>`,
`Coin<T>`, etc.) whose body relies on `T` being a specific expected type must either
constrain `T` in the signature (e.g., `T: SomeWitness`, or a concrete `T = USDC`) or
verify `T` at runtime (`std::type_name::get<T>() == expected`). A bare generic where the
caller picks `T` and the function trusts it implicitly is the bug.
Detect: generic fns over `T` whose body uses `T` in an identity-sensitive way — accepts a
`Coin<T>` and credits its value to a balance denominated in a different token; trusts a
witness parameter `w: T` (typically `T: drop`) as proof of authority without constraining
which `T` is valid; a generic transfer/store over caller-chosen `T`.
_Absence rule:_ walk every fn whose signature is generic over `T` (or takes `Coin<T>` /
similar) where the body uses `T` in an identity-sensitive way. A `T: <Trait>` constraint
at the signature, or a `type_name::get<T>() == expected` runtime check *elsewhere*, does
not clear an unchecked fn.
Example:
```move
// Vulnerable: credits a USDC-denominated balance with the value of any Coin<T>.
public fun deposit_premium<T>(pool: &mut Pool<USDC>, payment: Coin<T>) {
    let amount = coin::value(&payment);
    pool.balance = pool.balance + amount;   // pool.balance is USDC; T isn't checked
    coin::destroy_zero(payment);
}

// Patched: constrain T at the signature so only Coin<USDC> is accepted.
public fun deposit_premium(pool: &mut Pool<USDC>, payment: Coin<USDC>) {
    let amount = coin::value(&payment);
    pool.balance = pool.balance + amount;
    coin::destroy_zero(payment);
}
```
*This is one shape, not the only shape.* SM-B5 fires on any generic fn whose body relies
on `T` being a specific type without that being enforced. Variants: a forgeable witness
parameter (`T: drop` where the function trusts any `T` as proof of authority); `Coin<T>`
consumed for accounting in a different denomination; generic transfer/store over caller-
chosen `T`. The absence rule above is authoritative — verify the constraint or the runtime
check on *this* specific fn.
Exploit: pass `Coin<WorthlessToken>` to a function denominated in `USDC` and credit a real
balance with worthless value; or construct a witness of an attacker-chosen type to satisfy
a generic authorization gate.
Source: [+domain] (generic-type substitution / unconstrained-witness; not directly derived
from upstream `MystenLabs/skills` — verified empty in `modern-move-syntax/`,
`composable-move-functions/`, and `sui-move/move.md` at the pinned ref).
