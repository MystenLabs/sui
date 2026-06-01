# C/D — Object lifecycle, ownership & shared-object concurrency

Sui's object model creates failure modes that don't exist in account-based chains: permanent
orphaning, irreversible sharing, by-value deletion of shared state, and consensus-level
contention. Shared objects are also the front-running surface.

### SM-C1 — Orphaned dynamic fields / leaked UID on destruction   [Critical]
Invariant: before `object::delete` (or unpacking a struct by value), remove every dynamic field /
dynamic object field and `destroy_empty` every collection it held; the `UID` must be consumed by
`object::delete`.
Detect: `object::delete` / by-value unpack of a parent that owned `dynamic_field` /
`dynamic_object_field` / `Table` / `Bag` entries without prior removal; destruction paths that
don't reach `id.delete()`.
Exploit: child objects/funds become permanently inaccessible (no recovery) — asset loss or locked
collateral. Deleting a *shared* parent makes its dynamic fields permanently unreachable.
Source: `MystenLabs/skills → object-model/dynamic-fields-and-collections.md`, `MystenLabs/skills → sui-move/move.md`.

### SM-C2 — Accidental / irreversible sharing   [High]
Invariant: `share_object` / `public_share_object` is intentional; owner-only or sensitive state
is not shared. Sharing cannot be undone.
Detect: `transfer::share_object` / `public_share_object` on objects that hold owner-private data
or that grant write access to state meant to be single-owner.
Exploit: state becomes world-mutable forever (anyone can pass it `&mut` to any public fn that
accepts it), or private data becomes permanently readable.
Source: `MystenLabs/skills → object-model/ownership.md`.

### SM-C3 — Ungated by-value shared-object deletion   [High]
Invariant: a function that takes a shared object **by value** (the only form that can delete it)
must be capability- or sender-gated.
Detect: `public`/`entry fn(..., obj: SharedT, ...)` (by value, not `&`/`&mut`) that reaches
`object::delete`/unpack, with no authorization check.
Exploit: any caller destroys core protocol state and orphans its dynamic fields (DoS + asset
lock).
Source: `MystenLabs/skills → sui-move/move.md`.

### SM-C4 — Unvalidated `Receiving<T>` acceptance   [Medium]
Invariant: `transfer::receive` / `public_receive` validates the received object's type and/or
sender before accepting it into the parent.
Detect: blind `receive` loops or accepting `Receiving<T>` without checking provenance.
Exploit: spam an account/object with crafted objects → inventory pollution, type-handling
confusion, or storage griefing.
Source: `MystenLabs/skills → object-model/transfers.md`.

### SM-D1 — Invariants trusted from the caller instead of enforced on-chain   [Critical]
Invariant: slippage / minimum-output / deadline / price / amount bounds are asserted **in Move**,
not assumed correct because the SDK set them. Anything in a PTB is attacker-controlled.
Detect: swap/trade/withdraw fns that use a caller-supplied `min_out`, `price`, or `amount`
without `assert!` bounds; reliance on off-chain validation "before the call".
Exploit: submit a PTB with `min_out = 0` (or crafted price) and sandwich/drain the pool;
front-run a shared-object write.
Source: `MystenLabs/skills → object-model/`, `MystenLabs/skills → ptbs/fundamentals.md` (PTBs are adversarial).

### SM-D2 — Shared-object contention / equivocation DoS   [Medium]
Invariant: hot shared objects on a common write path are sharded or contention-aware; the design
tolerates competing transactions on the same version.
Detect: a single global shared object mutated by every user action.
Exploit: flood concurrent txns on the same shared-object version; split validator reservations →
the object is unavailable until the next epoch (liveness DoS).
Source: `MystenLabs/skills → ptbs/troubleshooting.md`.
