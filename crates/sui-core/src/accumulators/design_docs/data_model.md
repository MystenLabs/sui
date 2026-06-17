# Address Balances: Data Model

This document covers **how virtual balances are laid out on-chain**: the accumulator root, the
per-account dynamic field, the value representation, and the version invariant. It is purely
about data structures — the algorithms that read and modify them live in
[`write_path.md`](./write_path.md), [`address_funds_scheduling.md`](./address_funds_scheduling.md),
and [`object_funds_checking.md`](./object_funds_checking.md).

## 1. The accumulator root

There is exactly one accumulator root object per network, at the well-known address
`SUI_ACCUMULATOR_ROOT_OBJECT_ID` (commonly written `0xACC`). It is a **shared object**, with its
initial shared version recorded in the epoch start configuration
(`accumulator_root_obj_initial_shared_version`).

Every virtual balance is a dynamic field hanging off this single root, so all balance reads and
writes touch the same object identity. From a consensus perspective, the root is therefore a
heavily-contended shared object — which is precisely why the system goes to such lengths to
**not** force every withdraw to wait on a strict version chain. See
[`write_path.md`](./write_path.md) §3 for how shared-object access modes (`NonExclusiveWrite` vs.
`Mutable`) make parallel settlement work.

The root carries a `SequenceNumber` like any other object. We call this the **accumulator
version**, and it is bumped exactly once per consensus commit, by the barrier transaction
(see §4 below and [`write_path.md`](./write_path.md) §3).

## 2. Per-account accumulator objects

Each address-owned virtual balance is a dynamic field under `0xACC`, keyed by
`accumulator::Key<Balance<T>>` and storing a Move value of type
`accumulator::U128 { value: u128 }`.

The relevant Rust types live in `sui-types/src/accumulator_root.rs`:

```rust
pub struct AccumulatorKey { pub owner: SuiAddress }

pub enum AccumulatorValue {
    U128(U128),
}

pub struct U128 { pub value: u128 }
```

A wrapper `AccumulatorObjId` newtypes the underlying `ObjectID` so the rest of the codebase can
make it impossible to confuse "any object id" with "an accumulator account id":

```rust
pub struct AccumulatorObjId(ObjectID);
```

Sui balances are nominally `u64`, but the accumulator stores them as `u128`. The wider storage
type is forward-looking: it leaves room to extend the system to `u128`-denominated balance
types in the future without an on-chain migration. Since storage types are effectively
permanent (changing them requires a migration of every accumulator account), it pays to be
conservative here.

## 3. ID derivation

`AccumulatorObjId` is derived deterministically from `(owner_address, balance_type)`. The
derivation goes through Sui's standard dynamic-field ID machinery, with the parent set to
`SUI_ACCUMULATOR_ROOT_OBJECT_ID` and the field name set to `Key<Balance<T>>`:

```rust
pub fn get_field_id(owner: SuiAddress, type_: &TypeTag) -> SuiResult<AccumulatorObjId> {
    if !Balance::is_balance_type(type_) {
        return Err(...); // only Balance<T> is supported today
    }
    let key = AccumulatorKey { owner };
    Ok(AccumulatorObjId(
        DynamicFieldKey(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            key,
            AccumulatorKey::get_type_tag(std::slice::from_ref(type_)),
        )
        .into_unbounded_id()?
        .as_object_id(),
    ))
}
```

(implemented by `AccumulatorValue::get_field_id` in `sui-types::accumulator_root`)

The implication is that **any caller can compute the `AccumulatorObjId` for `(addr, T)` without
reading state**. That is what makes the address-funds scheduler's pre-execution analysis
possible — given a `FundsWithdrawalArg` and the transaction's sender/sponsor, the scheduler
knows exactly which account ids the withdraw will touch (see
[`write_path.md`](./write_path.md) §1 and [`address_funds_scheduling.md`](./address_funds_scheduling.md) §1).

## 4. The accumulator version

The accumulator version (the `SequenceNumber` of `0xACC`) is the linchpin of the whole system.
Three things to internalize:

1. **It advances by exactly one per settlement batch.** Even though the on-chain settlement is
   spread across multiple transactions (N parallel settlement txns plus a barrier — see
   [`write_path.md`](./write_path.md) §4), only the barrier mutates the accumulator root. So
   the version goes from V to V+1 in a single step, atomically with the barrier transaction's
   effects. In the common case there is one such batch for the ordinary transactions derived from
   a consensus commit, but the code can also construct an additional accumulator settlement batch
   for the randomness lane. The invariant is therefore per settlement batch, not globally "once
   per consensus commit".

2. **All withdraws and deposits within a single settlement batch read the same accumulator version.**
   Shared-object version assignment pins every transaction in that batch that touches
   `0xACC` to one version V, and the barrier that closes the batch bumps the version to V+1.
   This means transactions within that batch see *identical* balances (in storage), which is
   what motivates the unsettled-withdraws tracking in
   [`object_funds_checking.md`](./object_funds_checking.md) §2.

3. **The version is *not* an execution dependency for withdraws.** A transaction at version V+1
   does **not** have to wait for the V→V+1 settlement to finish executing before it can execute
   itself. The eager scheduler may approve it immediately based on in-memory balance bookkeeping
   (see [`address_funds_scheduling.md`](./address_funds_scheduling.md) §3). The version is a
   logical label that tells the scheduler which "snapshot" to reason about, not a barrier the
   transaction has to physically clear.

The per-account accumulator object itself is also versioned. Reads can be **version-bounded**:

```rust
AccumulatorValue::load(child_object_resolver, version_bound, owner, type_) -> Option<Self>
```

This is how `AccountFundsRead::get_account_amount_at_version` works. Version-bounded reads use
the dynamic-field MVCC machinery (`BoundedDynamicFieldID`); they let the system read "what was
the balance at the accumulator version this transaction was assigned" even when storage has
since moved past.

That does **not** mean arbitrary historical reads are always available. The contract on
`AccountFundsRead::get_account_amount_at_version` is narrower: callers must only use it when they
can guarantee the target version has not yet been pruned. In practice, old versions remain
available only while the system has not finalized enough checkpoint progress to make them
prune-eligible.

That precision is necessary whenever correctness depends on the answer to the historical
question "what balance did version V observe?" rather than the latest-state question "has the
system already advanced past V?" The two scheduler/checker subsystems intentionally ask
different questions:

- The **address-funds scheduler** mostly needs the latest-state question. It is deciding whether
  to reserve a transaction *before* execution, and if storage has already advanced past the
  requested version then the request is stale and can be handed off via `SkipSchedule` to the
  rest of execution, which will observe the already-applied settlement effects through normal
  state reads.
- The **object-funds checker** needs the historical question. It runs *after* execution, once a
  transaction has already produced accumulator events, and it must decide whether those effects
  are valid for the transaction's assigned accumulator version. At that point a latest-state read
  would be unsafe: storage may already include later settlements that this transaction was not
  allowed to rely on.

That is also why the implementation is careful about *when* it attempts such reads. If checkpoint
progress advances far enough while a historical read is in flight, the old version can become
prune-eligible. The code therefore "dances" around the root version and checkpoint progress to
make sure it is reading inside a window where the target historical version is still guaranteed to
exist.

The rest of this design directory is largely about keeping those two questions from being
accidentally conflated.

## 5. Object-owned virtual balances

The same machinery supports balances owned by *another object* rather than an address. The
`AccumulatorKey { owner }` field is a `SuiAddress`, but a Move object's UID encodes as an address
too, so the same dynamic-field structure works. The differences are operational, not structural:

- An object-owned balance can only be withdrawn from by a Move program with access to the owning
  object, and the withdrawal *amount* is determined by that program's logic at runtime — not
  declared up front in transaction data.
- Sufficiency must therefore be checked *after* execution. That is what
  [`object_funds_checking.md`](./object_funds_checking.md) is about.

The on-chain layout is unchanged: still a dynamic field under `0xACC`, still a `U128` value,
still derived deterministically from `(owner, type)`.

## 6. Lifecycle

A given accumulator account object goes through a small number of states during its lifetime:

```
      not-yet-created
           │
           │  first deposit settles
           ▼
      ┌─────────────────────┐
      │  exists, balance>0  │
      └─────────────────────┘
           │  ▲
   each    │  │  each
   settle  │  │  settle
           ▼  │
      ┌─────────────────────┐
      │  exists, balance=0  │
      └─────────────────────┘
           │
           │  garbage-collected when an
           │  empty account is left
           │  empty after settlement
           ▼
      destroyed (deleted as part of settlement)
```

The exact creation/deletion behavior is enforced by the settlement Move logic
(`accumulator_settlement::settle_u128`). The barrier transaction records counts of accounts
created and destroyed via `record_accumulator_object_changes` for metrics. From the Rust
scheduler's point of view, this lifecycle is mostly invisible — it sees deposits and withdrawals
as `funds_changes: BTreeMap<AccumulatorObjId, i128>` deltas — but it is reflected in the
storage-version reads:

- For an account that doesn't exist yet, `get_consistent_latest_account_amount_and_version` returns
  `(0, current_root_version)`.
- After deletion, the same call returns `(0, current_root_version)`.
- Between creation and deletion, it returns `(balance_at_or_before_root, current_root_version)`.
  If the account object has advanced ahead of the root, this may be older than the latest account
  object amount.

This shape — "balance plus the root version it is consistent with" — is what lets the scheduler detect when
storage has advanced past a request even without an in-process settlement notification (see
[`address_funds_scheduling.md`](./address_funds_scheduling.md) §3).

## 7. Off-topic data: event streams

For completeness: the accumulator infrastructure is also used to host *event streams* (the
`EventStreamHead` machinery in `accumulator_root.rs`). Those are **not** balances and are not
covered by the rest of this directory. They share the dynamic-field-under-`0xACC` storage shape
but live in a different module (`accumulator_settlement::EventStreamHead`) and are settled with a
different Move entrypoint (`settle_events`). If you are working on event streams, treat the rest
of these docs as informative context rather than authoritative.
