# Object Funds Checking (In-Execution)

This document covers the **in-execution** sufficiency check for object-owned virtual balance
withdrawals: the `check_sufficient_object_funds` native consulted by
`funds_accumulator::withdraw_from_object` while the transaction is running inside the Move VM.
It is gated by the `check_object_funds_withdraw_in_execution` protocol flag and supersedes the
post-execution checker described in [`object_funds_checking.md`](./object_funds_checking.md)
(now `ObjectFundsCheckerDEPRECATED`); the plan is to enable the flag everywhere and then delete
the post-execution path.

For the on-chain data layout, see [`data_model.md`](./data_model.md). For how withdrawals are
declared, executed, and settled, see [`write_path.md`](./write_path.md). For the version and
retry machinery this check consumes (how the accumulator root gets a deterministic version on
every execution path), see `implicitly_read_system_objects.md`.

## 1. Why move the check into execution?

Object withdrawal amounts are still only known at runtime — that has not changed. What changes
is *when the decision is made*. The post-execution checker had to execute the whole transaction,
inspect the resulting running-max withdrawals, and on insufficiency either wait for settlement or
re-execute the transaction with an injected failure. In-execution, each withdrawal is checked at
the moment it happens:

- Insufficiency becomes an ordinary, deterministic Move abort at the offending command — a single
  execution, no pending-wait/re-execute machinery, and a legible failure
  (`InsufficientObjectFundsForWithdraw`) in the effects.
- The check runs on **every executing node** (fullnodes included), not just validators, because it
  affects execution results. This is why the unsettled-withdrawal store lives unconditionally on
  `AuthorityState` rather than behind validator-only init.

## 2. The check, step by step

```
  withdraw_from_object<T>(obj, amount)                     (Move, funds_accumulator.move)
        │
        ▼
  check_sufficient_object_funds native
        │
        ▼
  ObjectRuntime::check_object_funds_sufficiency            (per-(owner, type) running balance)
        │  in-transaction balance covers it? ── yes ──> Sufficient, no store read
        ▼ no (first time only)
  TemporaryStore::object_available_balance
        │
        ├─ accumulator root not yet at its required version on this node
        │      → SYSTEM_OBJECT_NOT_AVAILABLE_LOCALLY unwind: effects discarded,
        │        authority waits for the version and re-enqueues (node-local, never committed)
        │
        └─ settled balance at the required accumulator version
             minus unsettled withdrawals from earlier transactions in this commit
        │
        ▼
  Sufficient → continue          Insufficient → Move abort
                                   → ExecutionErrorKind::InsufficientObjectFundsForWithdraw
```

Key properties:

- **In-transaction netting.** The `ObjectRuntime` tracks a running available balance per
  `(owner, type)`: deposits made earlier in the same transaction cover later withdrawals without
  any store read (and therefore without any availability gate). The settled balance is folded in
  at most once per account, the first time the in-transaction balance falls short.
- **The settled read is version-exact.** `object_available_balance` reads the account balance at
  the transaction's *required accumulator version* (from `system_object_versions`), after gating
  on the root having reached that version locally. Settlement bumps the root only after all
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
  (a private helper on `AuthorityState`) — net, not running max, because that is what settlement will
  actually deduct. The in-execution check is only enabled together with
  `record_net_unsettled_object_withdraws`, so nets are the only amounts recorded; the running max
  survives as a debug assertion (net can never exceed the checked peak). Failed transactions
  settle nothing and record nothing.
- **Reading.** The Move VM reads the store through the `UnsettledObjectFundsRead` trait, threaded
  into the temporary store as `unsettled_object_funds`.
- **GC.** Entries are dropped at checkpoint commit for versions the committed effects settled
  (`UnsettledObjectWithdrawals::commit_effects`) — at commit rather than at barrier execution,
  because the barrier can execute concurrently with transactions that still read those entries.
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
| Checkpoint execution / crash recovery | Recorded `ReadOnlyRoot` in effects, plus the settlement-derived back-fill. See `implicitly_read_system_objects.md`. |
| Dev-inspect / dry-run | Pinned at the latest committed version from the object store; the availability gate cannot trigger. The old post-execution simulate check is bypassed when the flag is on (and can be deleted after rollout — simulation never re-executes historical transactions). |
| `sui-replay-2` | Superset harvest from expected effects. **Caveat:** unsettled in-commit withdrawals are *not* reconstructed in isolated replay (`unsettled = 0`), which can diverge from the original execution — see the TODO in `temporary_store.rs`. Mainnet enablement is blocked on this. |

## 5. Failure and error semantics

- Insufficiency aborts the transaction: a real, committed failed execution with normal gas
  charging (unlike the old path's cancellation-style outcome, which never entered execution).
- The native's raw abort is converted by the adapter (`convert_vm_error`) into the dedicated
  `ExecutionErrorKind::InsufficientObjectFundsForWithdraw`, rather than surfacing as an opaque
  `MoveAbort(funds_accumulator, 2)`. The proto and Rust-SDK conversions currently surface it as
  the pre-existing `InsufficientFundsForWithdraw` until dedicated values exist there.
- The address-funds scheduler path is unchanged and still produces
  `InsufficientFundsForWithdraw` itself (pre-execution, cancellation-style).
- The availability unwind (`SystemObjectNotAvailableLocally`) is node-local and is never
  committed to effects.

## 6. Rollout

`check_object_funds_withdraw_in_execution` is enabled at protocol version 130 on devnet and
testnet, off on mainnet pending the replay reconstruction caveat above. It requires
`record_net_unsettled_object_withdraws`. Once enabled everywhere, `ObjectFundsCheckerDEPRECATED`,
its dry-run/simulate counterpart in `authority.rs`, and the flag-off branches can be deleted.
