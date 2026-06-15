# E — Dynamic fields & collections

Dynamic fields (DF), dynamic object fields (DOF), and the collection wrappers (`Table`, `Bag`,
`ObjectTable`, `ObjectBag`, `VecMap`, `VecSet`) each have distinct addressability, size, and
cleanup properties. Bugs here cause overwrites, DoS, and orphaning.

### SM-E1 — Predictable / attacker-controlled field or derived keys   [High]
Invariant: DF key names (and `derived_object` keys) are not attacker-controlled or predictable in
a way that enables overwriting existing entries or squatting future addresses. Adding a DF whose
(name,type) already exists aborts; a predictable derived address can be pre-funded before the
object exists.
Detect: `add` / `borrow_mut` / `remove` using a user-supplied key without namespacing or
validation; `derived_object::derive_address(parent, key)` where `key` is user-influenced and
assets/objects can be sent to the result before creation.
Exploit: overwrite or block another user's slot; pre-create / pre-send to a derived address to
hijack initialization, or front-run object creation.
Source: `MystenLabs/skills → object-model/dynamic-fields-and-collections.md`, `MystenLabs/skills → object-model/patterns.md`.

### SM-E2 — Wrong DF vs DOF choice for visibility   [Medium]
Invariant: use `dynamic_object_field` when the child must keep its own ID and be independently
queryable/transferable; use `dynamic_field` when it should be wrapped/hidden. Mismatch leaks or
hides objects.
Detect: objects that must be addressable stored as plain `dynamic_field` (invisible by ID); or
objects meant to be encapsulated stored as DOF (independently exposed).
Exploit: assets become unreachable by tooling, or "hidden" children are independently accessible.
Source: `MystenLabs/skills → object-model/dynamic-fields-and-collections.md`.

### SM-E3 — Unbounded inline collections   [High]
Invariant: `VecMap` / `VecSet` / inline `vector` are bounded (≈100 entries before the ~256 KB
object-size cap; lookups are O(n)); use `Table`/`Bag` for unbounded growth. Collections lack
`drop` and must be `destroy_empty`'d (see SM-C1).
Detect: per-user or otherwise unbounded inserts into `VecMap`/`VecSet`/`vector`; missing
`destroy_empty` on a destruction path; O(n) scans inside hot functions.
Exploit: grow the collection until the owning object exceeds the size limit or gas cost →
the object can no longer be mutated (permanent DoS / locked funds).
Source: `MystenLabs/skills → object-model/dynamic-fields-and-collections.md`.

### SM-E4 — Missing existence check before dynamic_field / collection access   [Medium]
Invariant: `dynamic_field::borrow*<T>` / `borrow_mut*<T>` / `remove*<T>` — and the equivalent
`Bag`/`Table`/`ObjectBag`/`ObjectTable` accessors — **abort the transaction if the key is
absent**. When a field's presence depends on prior state that the caller (or any other code path)
can influence — an attacker can `remove`, an unrelated path may not have `add`-ed it — the access
path must first call `*::exists*` / `*::contains` and either branch to a safe path or abort with
a clear, named error code.
Detect: `dynamic_field::borrow*`/`remove*` / `bag::borrow*` / `table::borrow*` access with no
preceding existence check that gates the access. In decompiled output: the bug is the absence
of a matching `dynamic_field::exists*(...)` / `bag::contains(...)` / `table::contains(...)`
inside an `if (...)` guard or `assert!(...)` that reaches the specific access. The guard
must precede the access on the path — an `exists*` / `contains` elsewhere in the function
doesn't qualify. See `auditing-bytecode.md` SM-E4 for the structured per-rule signal.
_Absence rule:_ walk every `dynamic_field::borrow*`/`bag::*`/`table::*` access; an
`exists*`/`contains` *elsewhere* does not clear it — the guard must reach *this* access.
Exploit: an attacker triggers the missing-field path to abort honest users' transactions (DoS
against an entrypoint), or exploits the absence-induced abort to take a different code path the
author did not anticipate (e.g. a fallback branch that bypasses an accumulator that was supposed
to be initialized).
Source: `MystenLabs/skills → object-model/dynamic-fields-and-collections.md` ("Accessing a
nonexistent field aborts the transaction. Adding a field with a name that already exists … also
aborts.").
