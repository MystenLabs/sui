# Address Balances: Write Path

This document covers everything that *changes* an accumulator balance:

1. How a transaction declares an address-funds withdraw (`FundsWithdrawalArg`,
   `CallArg::FundsWithdrawal`).
2. How that declaration is rewritten and resolved during signing/execution
   (`accumulators/transaction_rewriting.rs`, `accumulators/coin_reservations.rs`).
3. How the Move VM emits accumulator events (`AccumulatorEvent`) for both Split (withdraw) and
   Merge (deposit) operations.
4. How those events are aggregated into the on-chain settlement: the placeholder
   `AccumulatorSettlement` key, the N parallel settlement transactions, and the barrier
   transaction that bumps the accumulator version.
5. The two paths by which settlement reaches a node: directly through the validator's own
   settlement scheduling, or indirectly via the checkpoint executor when catching up from
   peers. The schedulers handle both. This is referenced from every other doc in this
   directory.

For the on-chain data layout these operations target, see [`data_model.md`](./data_model.md).
For pre-execution withdraw scheduling, see
[`address_funds_scheduling.md`](./address_funds_scheduling.md). For post-execution sufficiency
checking, see [`object_funds_checking.md`](./object_funds_checking.md).

## 1. Declaring a withdraw in a transaction

Address-funds withdraws are explicit transaction inputs. They appear as one of two surface
forms:

- **`CallArg::FundsWithdrawal(FundsWithdrawalArg)`** — the canonical form. The transaction
  data carries an inline `FundsWithdrawalArg` describing what to withdraw and how much.
- **Coin reservations encoded as masked `ObjectRef`s.** A backward-compat form for old SDKs
  that only know how to construct object-ref-shaped inputs. These are detected and rewritten
  into `CallArg::FundsWithdrawal` before execution, after which the rest of the write path is
  identical. The encoding, validation, and rewriting pipeline are documented separately in
  [`coin_reservations.md`](./coin_reservations.md).

`FundsWithdrawalArg` is defined in `sui-types::transaction`:

```rust
pub struct FundsWithdrawalArg {
    pub reservation: Reservation,           // currently: MaxAmountU64(u64)
    pub type_arg: WithdrawalTypeArg,        // currently: Balance(TypeTag)
    pub withdraw_from: WithdrawFrom,        // Sender | Sponsor
}
```

A few things worth knowing:

- The `MaxAmountU64` is a **maximum** the transaction may withdraw, not the exact amount.
  Actual amounts are decided by the Move code at execution time and may be lower (even zero).
  This is what makes pre-execution scheduling safe but conservative — see
  [`address_funds_scheduling.md`](./address_funds_scheduling.md) §3.
- `withdraw_from = Sponsor` is what powers gas-from-balance. The implicit gas withdrawal is
  conceptually a `FundsWithdrawalArg::balance_from_sponsor(gas_budget, GAS::type_tag())` — see
  `TransactionData::get_funds_withdrawal_for_gas_payment`. It is added to the
  withdraw map for execution alongside any explicit user withdraws.
- Only `Balance<T>` is supported as the type today. The various derivation/lookup helpers in
  `accumulator_root.rs` enforce this at runtime.

### From declaration to per-account reservation map

At execution time, all of a transaction's address-fund withdraws are aggregated into a per-
account reservation map by `TransactionData::process_funds_withdrawals_for_execution`:

```rust
fn process_funds_withdrawals_for_execution(...)
    -> BTreeMap<AccumulatorObjId, u64>
```

The function:

1. Walks every `CallArg::FundsWithdrawal` in the PTB inputs.
2. Adds the implicit gas withdraw if gas is paid from address balance.
3. For each withdraw, derives the `AccumulatorObjId` from
   `(owner_for_withdrawal(tx), type_arg.to_type_tag())` (see [`data_model.md`](./data_model.md)
   §3) and accumulates the declared maximum into the per-account total.
4. For coin reservations that haven't been rewritten yet (e.g. validating a signed but
   not-yet-executed cert), it parses the masked object refs and adds them in the same shape.
   See [`coin_reservations.md`](./coin_reservations.md).

The output is exactly the input shape consumed by `WithdrawReservations` in the address-funds
scheduler (see [`address_funds_scheduling.md`](./address_funds_scheduling.md) §1).

There are sibling entrypoints used at signing/voting time
(`process_funds_withdrawals_for_signing`) and for gas estimation
(`process_funds_withdrawals_for_estimation`); they are non-deterministic helpers (uncoordinated
reads) and do **not** drive scheduling. Only `..._for_execution` is part of the deterministic
path. Related read-side validation also lives on `AccountFundsRead`: for gasless withdrawals,
`check_remaining_amounts_after_withdrawal` enforces that the leftover balance is either zero or
at least the token's configured minimum transfer amount.

## 2. From transaction inputs to Move VM behavior

Once a `FundsWithdrawalArg` is in a `CallArg`, the Move VM treats it like any other input
during PTB execution. Withdrawing produces a `Balance<T>` value that downstream Move calls can
consume; depositing into another address's `Balance<T>` produces the symmetric effect. From the
VM's point of view there is no special-case flow — the deposit/withdraw show up as ordinary
Move runtime events.

What's special is what the VM *emits* when `Balance<T>` ends up moving in or out of an
accumulator account. These show up as **accumulator events** in the inner temporary store and
ultimately on `TransactionEffects`.

## 3. Accumulator events

`AccumulatorEvent` (`sui-types/src/accumulator_event.rs:21`):

```rust
pub struct AccumulatorEvent {
    pub accumulator_obj: AccumulatorObjId,
    pub write: AccumulatorWriteV1,
}
```

`AccumulatorWriteV1` (`sui-types/src/effects/object_change.rs:112`):

```rust
pub struct AccumulatorWriteV1 {
    pub address: AccumulatorAddress,        // (SuiAddress, TypeTag)
    pub operation: AccumulatorOperation,    // Merge | Split
    pub value: AccumulatorValue,            // Integer(u64) | IntegerTuple(...) | EventDigest(...)
}
```

For balances, `value` is always `Integer(u64)`. The only operations that matter for this doc
are:

- **Merge(amount)** — a deposit of `amount` units into the account.
- **Split(amount)** — a withdrawal of `amount` units from the account.

`AccumulatorEvent::from_balance_change(addr, balance_type, net_change_i64)` is the canonical
way to manufacture one given a signed delta (see `accumulator_event.rs:50`).

The full ordered list of events for a transaction lives on
`InnerTemporaryStore::accumulator_events` and is what feeds settlement. A derived summary,
`InnerTemporaryStore::accumulator_running_max_withdraws: BTreeMap<AccumulatorObjId, u128>`, is
also computed during execution; that one is the input to object-funds checking — see
[`object_funds_checking.md`](./object_funds_checking.md) §1 for what "running max" means.

### Where events surface in effects

After execution, accumulator events end up on `TransactionEffects`. The
`accumulator_events()` API on `TransactionEffectsAPI` reconstructs them by walking object
changes; on validators we usually still have the original vector in the writeback cache, so
`AccumulatorSettlementTxBuilder::new` prefers `cache.take_accumulator_events(tx)` to avoid the
linear scan (see `accumulators/mod.rs:246`).

## 4. Settlement: from placeholder to real on-chain transactions

The most subtle part of the write path is what happens at the *end* of a consensus commit.
What looks like "the settlement transaction" from a high level turns out to be a small batch of
real on-chain transactions, and the way they reach a node depends on whether the node executed
the source transactions itself or downloaded the checkpoint from a peer.

### 4.1 The placeholder key

What consensus emits at the end of each commit is **not** a complete settlement transaction. It
is just a placeholder — a `TransactionKey::AccumulatorSettlement(epoch, checkpoint_height)`
that reserves a slot for "whatever transactions will end up settling this commit." The
placeholder has no body yet because the body depends on what the regular transactions in the
commit actually do.

The placeholder is plumbed through the system as a `Schedulable::AccumulatorSettlement(...)`
in `SettlementScheduler::enqueue` (see
`crates/sui-core/src/execution_scheduler/settlement_scheduler.rs`). On a validator, the
settlement scheduler waits for the regular transactions to finish, then constructs and enqueues
the real settlement transactions itself — this is the "early settlement" path. The checkpoint
builder also constructs settlement transactions when a checkpoint is built; the two paths
coordinate via `wait_for_settlement_transactions` / `wait_for_barrier_transaction` on the epoch
store.

### 4.2 Building the real transactions

`AccumulatorSettlementTxBuilder` (in `accumulators/mod.rs:219`) does the construction. The
flow is:

1. **Aggregate events.** Walk the effects of every transaction included in the (early)
   settlement scope. For each `AccumulatorEvent`, accumulate Merge and Split totals per
   account, plus per-account input/output SUI flows (used for the balanced-flow assertions in
   the settlement Move code).

2. **Compute `funds_changes`.**
   `AccumulatorSettlementTxBuilder::collect_funds_changes(&self) -> BTreeMap<AccumulatorObjId, i128>`
   nets each account's Merge total minus its Split total. This is the same map that the
   address-funds scheduler eventually receives as `FundsSettlement::funds_changes`.

3. **Chunk and build.** `build_tx` chunks the per-account updates into one or more
   programmable system transactions, sized by
   `protocol_config.max_updates_per_settlement_txn`. Each chunk produces one `TransactionKind`:

   - Acquires the accumulator root with `SharedObjectMutability::NonExclusiveWrite`. This is
     the key trick — it lets multiple settlement txns run concurrently, since none of them
     conflict with each other on the root, while still blocking ordinary withdraw/deposit
     transactions that need a determinate version.
   - Calls `accumulator_settlement::settlement_prologue(root, epoch, checkpoint_height, idx,
     total_input_sui, total_output_sui)`.
   - Calls `accumulator_settlement::settle_u128(root, address, merge_amount, split_amount, ...)`
     for each account in the chunk.

4. **Build the barrier transaction.** `accumulators::build_accumulator_barrier_tx` produces
   the final transaction in the settlement set. It:

   - Acquires the accumulator root with `SharedObjectMutability::Mutable`. This is what
     forces it to serialize after all settlement txns in the same commit — every settlement
     txn took `NonExclusiveWrite`, and `Mutable` is exclusive against `NonExclusiveWrite`.
   - Calls the same `settlement_prologue` (with `total_input_sui = 0`, `total_output_sui = 0`)
     so the barrier participates in the same protocol checks.
   - Calls `accumulator_metadata::record_accumulator_object_changes(root, created, deleted)`,
     where `created` and `deleted` are computed by scanning the settlement txns' effects
     (`accumulators::count_accumulator_object_changes`).
   - **The barrier mutates the root**, which is what bumps the accumulator version from V to
     V+1.

So while the algorithmic story in the rest of these docs talks about "the settlement at
version V→V+1" as if it were one event, the on-chain reality is: many parallel settlement
txns followed by one barrier that gates the version bump.

```
  ┌────────────────────────────────────────────────────────────────────┐
  │                       Consensus Commit                             │
  │                                                                    │
  │   TX1, TX2, ... (regular transactions, all reading accumulator     │
  │   version V; some withdraw, some deposit, some both)               │
  │                                                                    │
  │   AccumulatorSettlement placeholder key (no body yet)              │
  └────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
            Regular transactions execute (parallel, in any order
            consistent with consensus / object dependencies)
                                  │
                                  ▼
            Their accumulator events are aggregated, per-account,
            via AccumulatorSettlementTxBuilder.
                                  │
                                  ▼
       ┌─────────────────────────────────────────────────────┐
       │  Settlement TX 0 (chunk 0)  │  ... in parallel ...  │
       │     NonExclusiveWrite       │   NonExclusiveWrite   │
       │   settlement_prologue       │  settlement_prologue  │
       │   settle_u128 × M           │  settle_u128 × M      │
       └────────────────┬─────────────────────┬──────────────┘
                        ▼                     ▼
                       (effects)            (effects)
                              │           │
                              ▼           ▼
                  ┌───────────────────────────────────┐
                  │   Barrier TX  (Mutable on root)   │
                  │   settlement_prologue             │
                  │   record_accumulator_object_      │
                  │     changes(created, deleted)     │
                  │   ► bumps accumulator V → V+1     │
                  └────────────────┬──────────────────┘
                                   │
                                   ▼
            Address funds:       Object funds:
            settle_address_funds last_settled_version
            with FundsSettlement watch channel bumped
            { V+1, funds_changes }   to V+1 (when barrier
                                     executes)
```

### 4.3 What the schedulers see

After all settlement txns and the barrier finish, the validator-side settlement code calls:

```rust
ExecutionScheduler::settle_address_funds(FundsSettlement {
    next_accumulator_version: V+1,
    funds_changes: <aggregated map from collect_funds_changes>,
});
```

(See `settlement_scheduler.rs:399` for the early-settlement path and
`checkpoints/mod.rs:1729` for the checkpoint-builder path.) The address-funds scheduler
ingests this via its settlement channel and updates per-account state — see
[`address_funds_scheduling.md`](./address_funds_scheduling.md) §4.

The object-funds checker uses a different hook: it tracks `last_settled_version` via a
`tokio::sync::watch` channel that is bumped from inside
`AuthorityState::process_certificate` whenever a **barrier transaction** finishes executing
(gated by `TransactionKind::is_accumulator_barrier_settle_tx`). This is a deliberate design
choice — see §5 below.

## 5. Two paths to settlement

A second subtlety pervades these docs and is worth pinning down here:

> **Settlement transactions can execute on a node independently of the original
> balance-changing transactions that produced their inputs.**

In particular, a node that is catching up from peers via the **checkpoint executor** may
execute the settlement txns and the barrier directly out of a downloaded checkpoint, without
ever having scheduled or executed the withdraw/deposit transactions that fed them. From the
scheduler's point of view, the on-chain accumulator version simply advances "out of band."

There are therefore two paths by which a settlement reaches a node:

1. **Checkpoint-builder path.** The node executed the regular transactions, so it
   has the events available. After settlement txns + barrier execute, the system explicitly
   calls `ExecutionScheduler::settle_address_funds(...)` (from `SettlementScheduler` for early
   settlement, or from `CheckpointBuilder` later). The address-funds scheduler's in-memory
   state is updated through this call.
2. **Checkpoint-executor path.** The node downloaded a checkpoint and executes whatever it
   contains, including the settlement txns and the barrier. The on-chain version of the
   accumulator root and individual accumulator account objects advance as a side effect of
   executing those transactions, but **`settle_address_funds` is never called on this path** —
   the node never had the source events. The scheduler's in-memory `accumulator_version` may
   therefore lag behind storage.

Each subsystem keeps the two paths consistent without path-specific code:

- The **address-funds scheduler** uses storage-version checks at scheduling time plus
  idempotent in-memory settlement (see
  [`address_funds_scheduling.md`](./address_funds_scheduling.md) §3.1 and §4). On the
  checkpoint-executor path it never builds up state for an already-settled version in the
  first place, so there's nothing to reconcile.
- The **object-funds checker** drives `last_settled_version` from the barrier-tx execution
  hook in `process_certificate`, which fires regardless of which path produced the barrier.
  See [`object_funds_checking.md`](./object_funds_checking.md) §3.
- **Garbage collection** for unsettled withdraws is tied to effects commitment
  (`ObjectFundsChecker::commit_effects`), which is also path-agnostic.

In short: the validator path uses richer in-process notifications; the checkpoint-executor
path uses just on-chain effects. The schedulers are designed so that "I observed the on-chain
state moved" is sufficient — the in-process notifications are an optimization, not a
correctness requirement.

One subtle consequence is that "observed the on-chain state moved" is sufficient only for the
subsystem whose job is to gate *pre-execution* work. That is the address-funds scheduler: once
some other path has advanced storage past version V, the scheduler can safely stop and return
`SkipSchedule`.

The object-funds checker is different because it runs *post-execution*. At that point there is no
separate scheduler left to take over, and the checker must still reconcile an already-executed
transaction's object-withdraw effects against the accumulator version assigned to that
transaction. That is why it uses a barrier-driven settled-version signal plus MVCC-bounded reads,
rather than a latest-state skip.

There is a second boundary to keep in mind for those MVCC-bounded reads: checkpoint finalization
and pruning. Historical object versions are not assumed to live forever. The cache layer's
historical-read logic is deliberately written to read within a root-version stability window,
because once checkpoint progress advances enough to make a version prune-eligible, blindly
reaching back for "whatever version V used to say" is no longer safe. This is another reason the
code distinguishes carefully between latest-state detection (`SkipSchedule`) and true
historical-version validation.

## 6. Summary of artifacts and where they live

| Artifact | Type | Defined in | Consumed by |
|----------|------|-----------|-------------|
| `FundsWithdrawalArg` | tx input | `sui-types/src/transaction.rs` | signing; execution; scheduler |
| `process_funds_withdrawals_for_execution` | fn | `sui-types/src/transaction.rs` | scheduler input construction |
| `rewrite_transaction_for_coin_reservations` | fn | `accumulators/transaction_rewriting.rs` | tx normalization before execution |
| `AccumulatorEvent` | execution output | `sui-types/src/accumulator_event.rs` | settlement builder; object funds checker |
| `accumulator_running_max_withdraws` | execution output | `sui-types/src/inner_temporary_store.rs` | object funds checker |
| `AccumulatorSettlementTxBuilder` | settlement constructor | `accumulators/mod.rs` | early-settlement scheduler; checkpoint builder |
| `build_accumulator_barrier_tx` | settlement constructor | `accumulators/mod.rs` | early-settlement scheduler; checkpoint builder |
| `FundsSettlement { next_accumulator_version, funds_changes }` | scheduler input | `funds_withdraw_scheduler/address_funds/mod.rs` | address-funds scheduler |
| `settle_accumulator_version` | watch-channel update | `accumulators/object_funds_checker/mod.rs` | invoked from `process_certificate` on barrier execution |
