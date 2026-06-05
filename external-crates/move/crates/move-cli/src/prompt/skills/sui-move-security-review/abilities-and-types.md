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
Invariant: a function that accepts an object of a protocol type for accounting/authorization
(`&mut Pool`, `&Vault`, `Coin<T>` with unconstrained `T`, or a generic `<T>`) must verify the
instance is the canonical one — e.g. it is *the* registry/shared object, is recorded in an
allow-list, or its `object::id` matches an expected value. Type alone is not identity: any module
or even the attacker can create another value of the same type.
Detect: protocol-type parameters used to read reserves/balances/permissions without an
identity/canonicity check; generic transfer/store over caller-chosen `T`; functions trusting the
*contents* of a passed-in object that the caller could have minted.
_Absence rule:_ walk every fn taking a protocol-type parameter (`&T`/`&mut T`/`Coin<T>`/
`&Pool`/`&Vault`) whose body reads its state; an `object::id(...)`/allow-list check
*elsewhere* does not clear an unchecked fn.
Exploit: pass a self-created `Pool` with fake reserves to satisfy a price/solvency check, or to
redirect a withdrawal that reads state from the attacker's object.
Source: [+domain] (object/type-confusion; the constructive skills assume the canonical object).
