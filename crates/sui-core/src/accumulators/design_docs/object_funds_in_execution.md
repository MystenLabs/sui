# Object Funds Checking (In-Execution)

This document covers the **in-execution** sufficiency check for object-owned virtual balance
withdrawals: the `reserve_object_funds_for_withdrawal` native consulted by
`funds_accumulator::withdraw_from_object` while the transaction is running inside the executor.
It is gated by the `check_object_funds_withdraw_in_execution` protocol flag and supersedes the
post-execution checker described in [`object_funds_checking.md`](./object_funds_checking.md)
(now `ObjectFundsCheckerDEPRECATED`); the plan is to enable the flag everywhere.

For the on-chain data layout, see [`data_model.md`](./data_model.md). For how withdrawals are
declared, executed, and settled, see [`write_path.md`](./write_path.md).

## 1. Why move the check into execution?

Object withdrawal amounts are still only known at runtime — that has not changed. What changes
is *when the decision is made*. The post-execution checker had to execute the whole transaction,
inspect the resulting running-max withdrawals, and on insufficiency either wait for settlement or
re-execute the transaction with an injected failure. This created a few issues:

- It is error prone to let arbitrary object funds withdraw to succeed. This has led to a few overflow bugs in the past.
- It is inefficient since we may have to execute the same transaction multiple times.
- It makes dryrun/simulate difficult to implement properly.

In-execution, each withdrawal is checked at the moment it happens:

- Insufficiency becomes an ordinary, deterministic Move abort at the offending command — a single
  execution, no pending-wait/re-execute machinery, and a legible failure
  (`MoveAbort(funds_accumulator, E_OBJECT_FUNDS_INSUFFICIENT)`) in the effects.
- The check runs on **every executing node** (fullnodes included), not just validators, because it
  affects execution results. This is why the unsettled-withdrawal store lives unconditionally on
  `AuthorityState` rather than behind validator-only init.

## 2. The check, step by step

```
  withdraw_from_object<T>(obj, amount)                     (Move, funds_accumulator.move)
        │
        ▼
  reserve_object_funds_for_withdrawal native
        │
        ▼
  ObjectRuntime::check_object_funds_sufficiency            (per-(owner, type) running balance)
        │  in-transaction balance covers it? ── yes ──> Sufficient, no store read
        ▼ no (first time only)
  TemporaryStore::object_available_balance
        │
        ├─ accumulator root not yet at its required version on this node
        │      → execution blocks inside `load_implicitly_read_system_object` until the
        │        version is committed locally (node-local wait, invisible in effects)
        │
        └─ settled balance at the required accumulator version
             minus unsettled withdrawals from earlier transactions in this commit
        │
        ▼
  Sufficient → continue          Insufficient → Move abort
                                   → MoveAbort(funds_accumulator, E_OBJECT_FUNDS_INSUFFICIENT)
```

Key properties:

- **In-transaction netting.** The `ObjectRuntime` tracks a running available balance per
  `(owner, type)`: deposits made earlier in the same transaction cover later withdrawals without
  any store read (and therefore without any availability wait). The settled balance is folded in
  at most once per account, the first time the in-transaction balance falls short.
- **The settled read is version-exact.** `object_available_balance` reads the account balance at
  the transaction's *required accumulator version* (from `system_object_versions`), blocking
  until the root has reached that version locally. Settlement bumps the root only after all
  per-account fields are written, so root-at-version implies every account is settled to at least
  that version.
- **Unsettled discounting.** Balances only change at settlement, so withdrawals by earlier
  transactions in the same consensus commit are invisible in the settled read. They are
  subtracted via `UnsettledObjectWithdrawals` (see §3).

## 3. Unsettled-withdrawal tracking

`UnsettledObjectWithdrawals` (in `accumulators/unsettled_object_withdrawals.rs`) is the
bookkeeping shared with the deprecated checker: per-account, per-accumulator-version totals of
executed-but-unsettled withdrawals.

- **Recording.** After a transaction executes *successfully* under the in-execution check, the
  authority records its per-account **net** withdrawal amounts from the effects
  through `UnsettledObjectWithdrawals::record_object_funds_withdraws` — net, not running max,
  because that is what settlement will actually deduct. The in-execution check is only enabled together with
  `record_net_unsettled_object_withdraws`, so nets are the only amounts recorded; the running max
  survives as a debug assertion (net can never exceed the checked peak). Failed transactions
  settle nothing and record nothing.
- **Reading.** The executor reads the store through the `UnsettledObjectFundsRead` trait, threaded
  into the temporary store as `unsettled_object_funds`.
- **GC.** Entries are dropped at checkpoint commit for versions the committed effects settled
  (`UnsettledObjectWithdrawals::commit_accumulator_versions`) — at commit rather than at barrier
  execution, because the barrier can execute concurrently with transactions that still read those entries.
- **Determinism.** Two transactions withdrawing from the same account conflict on the owning
  object, so they never execute concurrently, and both live execution and checkpoint execution
  run a commit's transactions in the same dependency order — every node accumulates the same
  unsettled totals at each read. This is also why the store must be populated on fullnodes:
  a fullnode that skipped recording would compute a different available balance than the
  validators and fork on the next same-commit withdrawal.

## 4. Version anchoring per execution path

| Path | Where the accumulator root version comes from |
|------|-----------------------------------------------|
| Live consensus execution | Assigned versions (`AssignedVersions.system_object_versions`). |
| Checkpoint execution / crash recovery | Back-filled from the settlement barrier's input version, with recorded `ReadOnlyRoot` versions checked for consistency. See `CheckpointTransactionData::new` in the [checkpoint executor](../../checkpoints/checkpoint_executor/mod.rs). |
| Dev-inspect / dry-run | Reuses explicit input versions and selects other roots from the latest store state (`TrackingBackingStore::pin_system_objects`). The implicit registry is retained before execution; missing pinned data is an error. The old post-execution simulate check is bypassed when the flag is on. Implicit reads are tracked so the response includes the objects referenced by effects. **Caveat:** the unsettled-withdrawal view is empty, so simulation can succeed when committed execution would reject the withdrawal. |
| `sui-replay-2` | Reconstructed from expected effects (`SystemObjectVersions::from_effects`). **Caveat:** unsettled in-commit withdrawals are *not* reconstructed in isolated replay (`unsettled = 0`), which can diverge from the original execution — see the TODO in `crates/sui-replay-2/src/execution.rs`. Mainnet enablement is blocked on this. |
| Single-node benchmarks | Every transaction retains its entry from the consensus assignment map, including owned-only transactions executed in parallel. The in-memory path also uses these assignments when generating checkpoint effects; raw setup execution obtains assignments through the same version manager. |

Accumulator settlement batches and barriers exclude the forwarding registry from their implicit inputs.
Their outputs use the accumulator's independent clock; a registry Lamport input could make the barrier
skip versions and violate the address-funds schedulers' consecutive-version contract. Consensus
assignment, simulation, and Simulacrum preserve this exclusion.

Runtime loading first reuses a retained explicit input at the assigned version. Simulation and replay
can therefore keep using that root after its stored version is pruned. A retained input at a different
version is not a fallback for the assigned root.

For an exclusively implicit registry read, simulation materializes the chosen version in
`TrackingBackingStore` before entering execution. If that version has already been pruned, simulation
returns `ObjectNotFound`; otherwise the retained exact-version object survives subsequent pruning.
These retained reads are not added to declared inputs, so their storage-read gas treatment is unchanged.

Shared-input metrics classify the transaction's declared inputs, not its effects. Mandatory implicit
registry reads therefore do not turn an owned-only transaction into a shared-input transaction;
explicit registry inputs still count.

Forwarding resolution and user registry inputs activate at protocol 138 on devnet/Unknown only.
Both mutable and immutable registry arguments require the feature flag, preserving pre-activation
rejection even when the registry already exists. Protocol 137's frozen
framework contains registry creation but not registration or resolution, so its configuration and
bytecode snapshots must remain unchanged.

## 5. Failure and error semantics

- Insufficiency aborts the transaction: a real, committed failed execution with normal gas
  charging (unlike the old path's cancellation-style outcome, which never entered execution).
- The native's abort surfaces as an ordinary framework abort,
  `MoveAbort(funds_accumulator, E_OBJECT_FUNDS_INSUFFICIENT)` — deliberately *not* remapped to a
  dedicated `ExecutionErrorKind`, keeping the invariant that Move aborts surface uniformly as
  `MoveAbort`. Rust consumers match it via
  `sui_types::funds_accumulator::is_object_funds_insufficient_abort`.
- The address-funds scheduler path is unchanged and still produces
  `InsufficientFundsForWithdraw` itself (pre-execution, cancellation-style).
- During committed validator/fullnode execution, a root that has not caught up locally is awaited,
  so temporary unavailability is invisible in effects. Dry-run reads do not wait and can instead
  produce a load error if the captured root version is unavailable.
- An assigned forwarding registry must materialize before execution can produce effects, even when
  the native is not invoked. A missing required version is an invariant failure: omitting its
  read-only effect while retaining its Lamport contribution would make effects-based replay compute
  a different timestamp. Simulation checks availability before this boundary and returns
  `ObjectNotFound` when materialization is impossible.
