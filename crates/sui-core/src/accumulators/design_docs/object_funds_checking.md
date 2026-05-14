# Object Funds Checking (Post-Execution)

This document covers the **post-execution** sufficiency check for object-owned virtual balance
withdrawals — the work done by `ObjectFundsChecker` (in
`crates/sui-core/src/accumulators/object_funds_checker/`) after PTB execution finishes a
transaction that withdrew from object-owned accumulator accounts.

For the on-chain data layout, see [`data_model.md`](./data_model.md). For how withdrawals are
declared, executed, and settled, see [`write_path.md`](./write_path.md). For the
**pre-execution** check used for address-owned accumulator accounts, see
[`address_funds_scheduling.md`](./address_funds_scheduling.md). The two checks share machinery
and concepts but are distinct subsystems with different correctness arguments.

## 1. Why post-execution?

Address funds withdrawals have a key advantage: the maximum withdrawal amount is declared in
the transaction data, so it can be checked before the transaction is run. Object funds withdrawals
have no such advantage. An object-owned accumulator account is controlled by a Move program,
and the withdrawal amount is computed at runtime. There is no way to know how much will be
withdrawn until execution finishes.

This means the system must **execute first, then check**. If the check fails, the transaction
must be re-executed in a way that produces a deterministic failure.

```
  ┌───────────────┐     ┌───────────────────┐     ┌────────────────────┐
  │ Execute       │     │ ObjectFundsChecker │     │ Commit Effects     │
  │ adapter       │ ──> │ (post-execution    │ ──> │                    │
  │               │     │  sufficiency check)│     │                    │
  └───────────────┘     └────────┬──────────┘     └────────────────────┘
                                 │
                    ┌────────────┼────────────┐
                    │            │            │
               Sufficient   Pending       Pending
               (commit      (version      (version settled,
                effects)     unsettled,    insufficient →
                             wait then     re-execute with
                             re-execute)   Insufficient flag)
```

### Computing the withdrawal amount: the running max

After the adapter finishes executing a transaction, the system examines all accumulator events
produced during execution. These events are either **Split** operations (withdrawals — taking
funds out of an account) or **Merge** operations (deposits — putting funds into an account).
See [`write_path.md`](./write_path.md) §3 for the event types.

The system computes a value called `accumulator_running_max_withdraws` (stored on
`InnerTemporaryStore`), which represents the **peak net withdrawal** for each account at any
point during the transaction. The idea is to track the high-water mark of how much an account
is "in the red" at any moment during execution.

The algorithm walks through each event in order and maintains a running signed counter per
account:

- **Split(amount)** — adds `amount` to the running net withdrawal for that account.
- **Merge(amount)** — subtracts `amount` from the running net withdrawal for that account.

Whenever the running net withdrawal for an account becomes positive and exceeds the previous
peak, the peak is updated. The final output is the peak value for each account.

Consider a transaction that produces the following events for Account X:

| Event       | Running Net Withdrawal | Peak |
|-------------|------------------------|------|
| Split(100)  | 100                    | 100  |
| Merge(100)  | 0                      | 100  |
| Split(100)  | 100                    | 100  |

The running max for Account X is **100**, not 200. Although 200 worth of splits occurred, 100
was re-deposited in between, so the account was never more than 100 "in the red" at any one
time. This is the amount the checker needs to validate against the account's balance.

Another example:

| Event       | Running Net Withdrawal | Peak |
|-------------|------------------------|------|
| Split(300)  | 300                    | 300  |
| Split(200)  | 500                    | 500  |
| Merge(100)  | 400                    | 500  |

Here the running max is **500** — that's the moment when the account was furthest in the red.

### Separating address vs. object withdrawals

A single transaction can have both address and object withdrawals. After execution, the
checker needs to identify which withdrawals are which. It does this by comparing the
accumulator events against the transaction's pre-declared address funds reservations (from
[`address_funds_scheduling.md`](./address_funds_scheduling.md) §1). Any accumulator account
that appears in the running max withdrawals but was **not** pre-declared as an address
withdrawal is treated as an object withdrawal that needs post-execution checking.

If there are no object withdrawals, the checker returns immediately and effects are committed.

## 2. The check: settled vs. unsettled

The checker needs to determine whether the account has enough funds to cover the computed
running max withdrawal. This depends on whether the transaction's accumulator version has
already been settled. The checker tracks `last_settled_version` via a `tokio::sync::watch`
channel; that channel is bumped from inside `process_certificate` whenever a **barrier
transaction** finishes executing (see [`write_path.md`](./write_path.md) §4.3). This is
convenient given the two settlement paths described in
[`write_path.md`](./write_path.md) §5: whether the barrier ran via the validator's settlement
scheduler or via the checkpoint executor, the same hook fires, so the watch channel stays
accurate without any path-specific code.

```
  Object withdrawal computed after execution
       │
       ▼
  Is the accumulator version already settled?
       │
    ┌──┴──┐
   YES    NO
    │      │
    │      ▼
    │   WAIT: spawn watcher task
    │   (blocks until version settles,
    │    then re-enqueue transaction
    │    with MaybeSufficient status)
    │
    ▼
  Read balance at version.
  Subtract unsettled withdrawals (see §2.1).
  Is the effective balance ≥ running max withdrawal?
       │
    ┌──┴──┐
   YES    NO
    │      │
    │      ▼
    │   FAIL: re-enqueue with Insufficient flag
    │   (next execution will produce early error)
    │
    ▼
  SUCCESS: record this withdrawal as unsettled,
  commit effects normally
```

When the version is **not yet settled**, the checker cannot know the final balance — a
deposit in the same or earlier commit might change it. So it spawns a background task that
watches the `watch` channel for the settled version to advance. When settlement arrives, the
task re-enqueues the transaction for re-execution. On the second pass, the version will be
settled and the checker can make a definitive decision.

When the version **is settled** but the balance is insufficient, the checker immediately sends
an `Insufficient` status. The transaction is re-enqueued with
`FundsWithdrawStatus::Insufficient` set in its `ExecutionEnv`, so the next execution
short-circuits with an `InsufficientFundsForWithdraw` error without executing the transaction.

One subtlety worth noting: a balance read at a historical version is only meaningful if that
version is still readable. This is guaranteed by the pipeline, not by a separate retention
mechanism. Versions ahead of the checkpoint executor are still held in memory; versions at or
behind the executor are persisted and only become prune-eligible once the executor advances past
them. A transaction whose `ObjectFundsChecker` is still running has, by definition, not yet been
executed — so the checkpoint containing it has not been finalized, and the executor sits at or
before it. Every version the checker can be asked about is therefore either in memory or still on
disk, and safe to read.

### 2.1 Why object funds cannot just "skip" like address funds

It is tempting to ask whether object funds could use the same shortcut as the address-funds
scheduler: if storage has already advanced past version V, why not simply declare the request
stale and move on?

The answer is that object-funds checking runs at a different point in the pipeline and therefore
has a different responsibility. By the time `ObjectFundsChecker` runs:

- the transaction has already executed once,
- it has already produced concrete accumulator events and a running-max withdrawal,
- and there is no separate pre-execution scheduler left to "hand off" the transaction to.

Address funds have that handoff point. When `schedule_withdraws` returns `SkipSchedule`, it is
deliberately saying "some other execution path already advanced the authoritative state for this
version, so the dedicated address-funds gate should stop here." The transaction can continue
through the normal execution pipeline, which is the component that owns the next step.

Object funds do not have an equivalent escape hatch. The checker itself is the component that
owns the post-execution decision: commit these effects, retry later, or force a deterministic
insufficient-funds failure. If it merely observed that latest storage had advanced and then
"skipped", it would lose the only point where the already-executed transaction's object-withdraw
effects are reconciled against the version they were actually assigned.

This is why the checker waits for settlement visibility and then re-checks against the
transaction's assigned accumulator version, rather than trying to treat "latest storage is
newer" as a sufficient answer.

### 2.2 Preventing double-spending: unsettled withdrawals tracking

There is a subtle problem. All transactions in the same consensus commit read the **same**
accumulator version, and balances in storage are only updated by settlement transactions. So
if TX1 and TX2 both read version 5 and both withdraw from the same object account, the balance
they each see in storage is identical. Without additional tracking, the checker would approve
both withdrawals against the full balance, potentially allowing more to be withdrawn than the
account holds.

The `ObjectFundsChecker` solves this with a structure called `unsettled_withdraws`:

```rust
unsettled_withdraws: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>
```

This tracks, for each account at each version, how much has been approved but not yet
settled. When the checker evaluates a new withdrawal, it reads the balance from storage and
**subtracts** the already-tracked unsettled amount for that account and version:

```
effective_balance = storage_balance_at_version - unsettled_withdrawals_at_version
```

If the effective balance is sufficient, the checker approves the withdrawal and adds the new
amount to the unsettled tracking. This ensures that each subsequent check sees a progressively
reduced available balance.

A companion structure, `unsettled_accounts`, tracks which accounts have unsettled entries at
each version. This is used for garbage collection (§4) and is not required for correctness.

### 2.3 Determinism

Object funds checking is deterministic, but through a different mechanism than the address-funds
scheduler. A transaction can only withdraw from an object by taking that object as a mutable
input. Sui's execution model already serializes any transactions that share a mutable input, so
for each object account, all withdrawals from it are linearized in a single consensus-determined
order — even though unrelated transactions across the system execute in parallel.

The checker leverages this directly: for any given object, it sees withdrawals one at a time, in
the same order on every validator. Combined with the unsettled-withdrawals tracking, this means
every validator deducts the same amounts in the same order from each object's balance, and so
arrives at the same sufficient/insufficient decisions. Determinism is per-object, which is
exactly the granularity object-funds checking requires.

## 3. Worked examples

### 3.1 Double-spend prevention within a consensus commit

**Setup**: Object account X has balance 1000 at version 5. Version 5 is already settled.

Three transactions in the same consensus commit all read version 5 and withdraw from X:

**TX1 executes**: running max for X is 300.

- Checker reads balance: 1000.
- Unsettled at (X, v5): 0.
- Effective balance: 1000 − 0 = 1000 ≥ 300 → **sufficient**.
- Update unsettled: (X, v5) = 300.
- Effects committed.

**TX2 executes**: running max for X is 400.

- Checker reads balance: 1000 (unchanged in storage — no settlement yet).
- Unsettled at (X, v5): 300.
- Effective balance: 1000 − 300 = 700 ≥ 400 → **sufficient**.
- Update unsettled: (X, v5) = 700.
- Effects committed.

**TX3 executes**: running max for X is 400.

- Checker reads balance: 1000.
- Unsettled at (X, v5): 700.
- Effective balance: 1000 − 700 = 300 < 400 → **insufficient**.
- TX3 is re-enqueued with `Insufficient` flag. Its next execution will fail with an early
  error.

```
  Storage balance: 1000 (constant — hasn't settled yet)

  TX1: needs 300   effective = 1000 - 0   = 1000 ≥ 300 ✓   unsettled → 300
  TX2: needs 400   effective = 1000 - 300 =  700 ≥ 400 ✓   unsettled → 700
  TX3: needs 400   effective = 1000 - 700 =  300 < 400 ✗   → Insufficient
```

Without the unsettled tracking, all three would see the full 1000 balance and all would be
approved, allowing 1100 to be withdrawn from an account with only 1000.

### 3.2 Pending check with re-execution

**Setup**: Object account Y has balance 500 at version 5. Last settled version is 5.

**Consensus Commit at version 6** contains TX1, which reads accumulator version 6.

**TX1's first execution**: the transaction is executed and produces a running max withdrawal of 200 from
Y.

- Checker sees accumulator version 6. Last settled version is 5.
- Version 6 > settled version 5 → **unsettled**, cannot check yet.
- Checker spawns a background watcher task and returns `Pending`.
- `should_commit_object_funds_withdraws()` returns `false`.
- Execution pipeline returns `RetryLater` — effects are **not** committed.

**Settlement v5→v6** arrives. The watcher task is notified (via the watch channel). It sends
`MaybeSufficient` through its oneshot channel. The background task re-enqueues TX1 for
execution.

**TX1's second execution**: the transaction is executed again (same code, same inputs) and again produces
a running max withdrawal of 200 from Y.

- Checker sees accumulator version 6. Last settled version is now 6.
- Version 6 ≤ settled version 6 → can check.
- Storage balance at version 6: let's say 600 (after settlement applied deposits). Unsettled
  at (Y, v6): 0.
- Effective balance: 600 − 0 = 600 ≥ 200 → **sufficient**.
- Update unsettled: (Y, v6) = 200.
- Effects committed.

If the balance after settlement had been only 100, the checker would have immediately sent
`Insufficient`. TX1 would be re-enqueued with the insufficient flag, and its third execution
would produce an `InsufficientFundsForWithdraw` early error without executing the transaction.

## 4. Garbage collection

The unsettled withdrawals tracking must be cleaned up to prevent unbounded memory growth.
This happens via `ObjectFundsChecker::commit_effects`, called once a batch of effects has
been committed.

The checker scans the committed effects for transactions whose object changes touch the
accumulator root (i.e., settlement and barrier transactions). For each such version, it
removes all unsettled withdrawal entries at that version from both `unsettled_withdraws` and
`unsettled_accounts`.

This is safe because once effects are committed, storage balances reflect the actual state,
and no further checking against the old unsettled entries is needed. As with the watch
channel in §2, this hook is path-agnostic: it works the same whether the settlement was
produced by this validator or downloaded via the checkpoint executor (see
[`write_path.md`](./write_path.md) §5).

## 5. Assigned version requirement

`ObjectFundsChecker` validates a post-execution object withdrawal against the transaction's
assigned accumulator version. That version tells it which historical balance snapshot to read
and which `unsettled_withdraws` bucket to charge.

This is part of the production invariant for the checker: by the time object-funds checking
runs, the transaction has a concrete assigned accumulator version. Maintain changes to this
area with that requirement in mind; without a specific assigned version, the checker would have
no well-defined historical balance to validate against.

## 6. Epoch boundaries

The `ObjectFundsChecker` is replaced at epoch boundaries the same way the address-funds
scheduler is — see [`address_funds_scheduling.md`](./address_funds_scheduling.md) §7. The
`watch` channel sender on the old checker is dropped, ending any in-flight watcher tasks
(their `within_alive_epoch` guard breaks them out of waiting cleanly). Pending object funds
checks from the old epoch are effectively abandoned and will be handled by the checkpoint
executor during epoch transition.

## 7. Key differences from address funds scheduling

While both systems validate withdrawals against accumulator balances, they differ in
fundamental ways:

**When the amount is known.** Address fund withdrawal maximums are declared in the
transaction data and known before execution. Object fund withdrawal amounts are computed
during execution.

**When the check happens.** Address funds are checked *before* execution (pre-execution gate).
Object funds are checked *after* execution (post-execution validation).

**How insufficient funds are handled.** For address funds, the eager scheduler can
immediately fail the transaction. For object funds, the PTB has
already been run; the checker must re-enqueue the transaction for a second execution with an
`Insufficient` flag that triggers an early error.

**Reservation vs. exact amounts.** The address funds scheduler uses conservative reservations
based on declared maximums. The object funds checker uses the exact running max computed from
actual execution events.

**State tracking complexity.** The eager address funds scheduler maintains per-account state
with balance tracking, reserved funds, and pending queues. The object funds checker maintains
a simpler structure: just a map of unsettled withdrawal amounts per account per version.

**Settlement interaction.** The address-funds scheduler uses settlement to release
reservations and drain pending queues — it actively updates in-memory balance state, driven
by the `settle_address_funds(...)` call (validator path only). The object funds checker uses
settlement only as a trigger: once the version is settled, it re-executes the transaction and
checks the actual balance from storage. The trigger is the barrier transaction's execution
hook in `process_certificate`, which fires regardless of whether settlement reached the node
via the validator path or the checkpoint executor.
