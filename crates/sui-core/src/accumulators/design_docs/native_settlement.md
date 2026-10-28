# Native (Rust) Accumulator Settlement

Status: type definitions under review.

## 1. Goal

Settlement transactions today are `ProgrammableSystemTransaction`s that call
`0x2::accumulator_settlement::{settlement_prologue, settle_u128, settle_events}` and
`0x2::accumulator_metadata::record_accumulator_object_changes`. This design replaces them with a
new `TransactionKind` that the executor applies in Rust.

The purpose is to remove abort cases from settlement. The only abort that remains is a split
that would take an accumulator below zero, because allowing it would mint currency. Overflow is
not checked. Each transaction deposits at most `u64::MAX` into one accumulator, so a `u128`
overflow needs about `2^64` deposits.

The Rust executor must produce the same on-chain objects as the Move code:
- A merge into an accumulator that does not exist creates it.
- An accumulator whose value drops to exactly zero is deleted.
- Event streams append to the `EventStreamHead` MMR.
- The barrier records the net count of accumulator objects created and deleted.

## 2. Transaction format

The payload types live in `crates/sui-types/src/accumulator_settlement.rs`. They are added to
`TransactionKind` as two variants:

```rust
AccumulatorSettlementV1(AccumulatorSettlementV1),
AccumulatorBarrierV1(AccumulatorBarrierV1),
```

- Updates are grouped by type, so each `TypeTag` appears once per settlement.
- Within a group, an update is `{ owner, operation, amount }`.
- Event streams have their own group variant.
- The executor derives the dynamic-field ID from `(owner, type)`.

The barrier is a separate variant. It carries the object-count changes.

## 3. Execution semantics

The root is an input at its assigned version. Child reads are bounded by the root version, as
they are today.

For each `AccumulatorUpdate { id, op, amount }`:

| existing field | op    | result                                                               |
|----------------|-------|----------------------------------------------------------------------|
| none           | Merge | create `Field<Key<T>, U128>` with value `amount` (needs T, see Q1)   |
| none           | Split | abort (underflow)                                                    |
| v              | Merge | write `v + amount`                                                   |
| v              | Split | abort if `amount > v`; delete if `amount == v`; else write `v - amount` |

The builder nets merges against splits and emits no update when the net is zero, as it does
today. The executor should reject `amount == 0` as an invariant violation.

Other behavior the executor must keep:
- **SUI conservation:** record `input_sui` / `output_sui` in `ExecutionResultsV2` exactly as the
  `record_settlement_sui_conservation` native does. Reset them in `drop_writes`. This also fixes
  an existing gap, because today an aborted settlement keeps the totals.
- **Objects:**
  - Created fields have owner `ObjectOwner(root)`, `has_public_transfer = false`,
    storage rebate 0, and the lamport version.
  - Created fields go into `created_object_ids`.
  - Mutated and deleted fields must be registered in `loaded_runtime_objects` so that effects
    record their input state.
- **Root:** the root uses `NonExclusiveWrite` and must stay unchanged, so it appears in effects
  as `ReadOnlyRoot`. The barrier bumps the root and updates the `AccumulatorObjectCountKey`
  field, creating it if it is absent.
- **Gas:** gas is unmetered. Deleted fields have rebate 0, so
  `conserve_unmetered_storage_rebate` stays at 0.
- **Batching:** `execution_batches` (parallel Move batches) is not used for the new kind.

### Abort behavior

An underflow returns an execution error. `reset` then drops all writes, and the effects carry
a failure status. The checkpoint builder already treats a failed settlement as fatal
(`checkpoints/mod.rs` `resolve_settlement_effects`). That is unchanged.

### New TemporaryStore primitives

`TemporaryStore` has no native helpers for creating a child object or deleting a loaded child.
This work adds:
- `read_child_for_settlement` (bounded by the root version)
- `create_child_object`
- `mutate_child_object`, which already exists for safe mode
- `delete_child_object`

## 4. Audit: everything the settlement path does today

### sui-core
- **`AccumulatorSettlementTxBuilder`:**
  - Aggregates events per `AccumulatorObjId` (in BTreeMap order).
  - Chunks updates by `max_updates_per_settlement_txn`.
  - Builds `funds_changes`.
  - Builds event Merkle roots from `(checkpoint_seq, tx_index + tx_index_offset, event_idx)`.
- **`build_accumulator_barrier_tx`:** `idx = num_settlements`, SUI 0/0, and object counts from
  `count_accumulator_object_changes`. The count does not filter to accumulator objects.
- **Callers:**
  - settlement scheduler (`construct_and_execute_settlement`)
  - checkpoint builder (`resolve_settlement_effects`, which rebuilds the transactions for their
    digests)
  - `settle_accumulator_for_testing` / `replay_settlement_for_testing`
  - `mock_checkpoint_builder` (simulacrum)
- **Placeholder:** consensus assigns one root version through the `Schedulable::AccumulatorSettlement`
  placeholder. `BarrierDependencyBuilder` orders the barrier after every `NonExclusiveWrite`
  transaction.
- **Recognition:** these helpers inspect PTB contents:
  - `is_accumulator_settle_tx`
  - `is_accumulator_barrier_settle_tx`
  - `accumulator_barrier_settlement_key`, which reads epoch and height from PTB inputs 1 and 2
  - `execution_driver` (index-less transaction check)
  - `authority.rs` (`notify_barrier_executed`, `settle_accumulator_version`)
  - `checkpoint_executor`, which finds the barrier by the root's input version in effects
- **SumU128U128 / IntegerTuple** cannot be reached and has a `todo!()`. The new format drops it.

### Execution
- `execution_batches` splits a settlement PTB into Move batches by type-argument addresses.
- The adapter checks `NonExclusiveWrite` roots for "unchanged" in `record_execution_results`.
- Conservation is checked only by the expensive check. It uses the settlement totals, plus SUI
  in the field objects, which `get_total_sui` reads for `Field<Key<Balance<SUI>>, U128>`.
- The historical execution versions v0–v3 need a match arm. It should panic or return an error.

### Consumers outside sui-core and sui-execution
- **New match arms are needed in:**
  - `sui-types` (`is_system_tx`, `shared_input_objects`, `input_objects`, `receiving_objects`,
    `validity_check`, `name`, `Display`, `is_consensus_commit_prologue`)
  - `rpc_proto_conversions`
  - JSON-RPC `SuiTransactionBlockKind`
  - GraphQL `TransactionKind` union
  - `sui-replay-2` example
- **Wire format:**
  - `sui_sdk_types_conversions` round-trips `TransactionKind` through BCS, so the variant is
    needed in sui-rust-sdk `sui-sdk-types`, along with the proto enum in `sui-rpc`.
  - The format snapshot gains `TransactionKind` variant 11.
- **Behavior changes that do not fail to compile:**
  - `sui-indexer-alt-consistent-store` `address_balances` handler only processes
    `ProgrammableSystemTransaction`.
  - `sui-benchmark` simtest `is_accumulator_settlement_tx` matches the same shape.
  - Rosetta.
  - The analytics `kind().name()` string.
- **Unaffected (they read effects, not the transaction body):**
  - `balance_change.rs`
  - inverted-index
  - light-client event streams
  - `funds_changes` consumers

## 5. Rollout (two-PR rule)

**PR 1:**
- Add the variant(s), the payload types and protocol flag `native_accumulator_settlement`.
  The flag is not enabled, or is enabled only under the devnet guard.
- Add the Rust executor, the builder branch on the flag, every match arm, and the recognition
  helpers for both forms. Historical checkpoints keep the PTB form forever.
- Tests:
  - parity tests that run the Move and Rust paths on the same inputs and compare written
    objects
  - flag-on e2e tests and simtests

**PR 2:**
- Enable the flag after PR 1 ships and sui-rust-sdk, the TypeScript SDK, the indexers and
  GraphQL have released with the new variant.

## 6. Resolved questions

- **Q1 (creation needs `T`):** the type is carried once per group, and owners are listed within
  the group.
- **Q2 (event streams):** the update group is an enum with `Balance` and `EventStream` variants.
- **Q3 (barrier):** the barrier is a separate `TransactionKind` variant.
- **Abort path:** any settlement abort is fatal for the validator. Bugs in the abort path
  therefore do not matter.
- **Effects:** effects must be identical to what the Move implementation produces.

## 7. Progress

- [x] Audit current settlement path
- [x] Resolve open questions
- [x] Payload type definitions
- [ ] sui-types: variant, payload, match arms, recognition helpers, format snapshot
- [ ] protocol flag
- [ ] TemporaryStore child create/mutate/delete primitives
- [ ] Rust executor (balance updates, event streams, barrier object count, conservation totals)
- [ ] Builder emits new kind under flag
- [ ] Downstream match arms (RPC, JSON-RPC, GraphQL, indexers, replay)
- [ ] Parity tests (Move vs Rust), e2e, simtests
- [ ] PR 1
