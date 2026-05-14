# Address Funds Withdraw Scheduling

This document covers the **pre-execution** sufficiency check for address-owned virtual balance
withdrawals — the work done by `FundsWithdrawScheduler` (under
`crates/sui-core/src/execution_scheduler/funds_withdraw_scheduler/address_funds/`) before a
withdrawing transaction is allowed to execute.

For the on-chain data layout, see [`data_model.md`](./data_model.md). For how a withdraw is
declared and how settlement transactions are constructed, see
[`write_path.md`](./write_path.md). For the post-execution sufficiency check used for
**object-owned** accumulator accounts, see
[`object_funds_checking.md`](./object_funds_checking.md).

## 1. Inputs and outputs

### What "address funds" means here

Address-funds withdrawals are withdrawals from address-owned accumulator accounts where the
*maximum* withdrawal amount is known **before execution**. The transaction data declares an
upper bound (`Reservation::MaxAmountU64`) on how much each account may withdraw — the actual
amount withdrawn during execution can be smaller (or even zero), but never larger. This upper
bound is enough to check sufficiency before transaction execution starts: if the account doesn't have
enough to cover the declared maximum, the transaction is guaranteed to fail, so we skip
execution entirely and return an early error.

Object-funds withdrawals (where the amount is known only after the transaction is executed) are out of
scope here — see [`object_funds_checking.md`](./object_funds_checking.md).

### Core data structures

A `TxFundsWithdraw` captures the maximum withdrawal a single transaction may perform from each
account:

```rust
pub struct TxFundsWithdraw {
    pub tx_digest: TransactionDigest,
    pub reservations: BTreeMap<AccumulatorObjId, u64>,
}
```

Each entry in `reservations` is an upper bound. The map is the same shape as
`process_funds_withdrawals_for_execution`'s output (see [`write_path.md`](./write_path.md) §1).

A `WithdrawReservations` groups all the withdrawals from a single consensus commit (that share
the same accumulator version) into one batch:

```rust
pub struct WithdrawReservations {
    pub accumulator_version: SequenceNumber,
    pub withdraws: Vec<TxFundsWithdraw>,
}
```

The scheduler returns a `ScheduleResult` for each transaction, which is one of three outcomes:

- **`SufficientFunds`** — the account has enough to cover the declared maximum withdrawal. The
  transaction can proceed to execution (where it may withdraw up to that amount).
- **`InsufficientFunds`** — the account cannot cover the declared maximum. The transaction
  will be executed with an `Insufficient` flag, causing it to fail immediately with an
  `InsufficientFundsForWithdraw` error without executing the transaction.
- **`SkipSchedule`** — the accumulator version for this transaction has already been settled.
  This happens either when the scheduler's own in-memory version has advanced past it (the
  validator path), or — more commonly for a node that is catching up — when the on-chain
  version of an account object has been moved past the requested version by settlement txns
  coming from the checkpoint executor (see [`write_path.md`](./write_path.md) §5). Either way,
  no scheduling action is needed; the transaction can be released.

A result can be **immediate** (known right away) or **pending** (will be resolved later when
settlement provides more information). Pending results are delivered through oneshot channels.

### Where it fits in the pipeline

When a batch of transactions arrives from consensus, `ExecutionScheduler::enqueue` classifies
each one:

1. **Ordinary transactions** (no fund withdrawals) go straight to the execution queue.
2. **Transactions with withdrawals** are routed through the funds withdraw scheduler first.
   They cannot execute until the scheduler determines whether their withdrawals are valid.
3. **Settlement and barrier transactions** are handled by `SettlementScheduler` (see
   [`write_path.md`](./write_path.md) §4 and §5). When their effects materialize,
   `settle_address_funds` is called to update the scheduler's in-memory state.

```
  Consensus          Withdraw            Execution
   Output    ──────> Scheduler  ──────>   Queue
              gate:                     (ready to
              "does this tx             execute)
               have enough
               funds?"

                         │
              Settlement │ called after settlement +
              (validator │ barrier txns execute (early
                  path)  │ settlement / checkpoint builder)
                         ▼
              Scheduler updates its
              internal balance state
```

When a checkpoint is being applied via the **checkpoint executor**
([`write_path.md`](./write_path.md) §5), the settlement and barrier transactions run through
the normal execution pipeline like any other system transaction, and the on-chain state
advances. But because the node didn't construct those transactions, `settle_address_funds` is
**not** invoked. The scheduler is designed to handle this gracefully — see
[§3 below](#3-the-eager-scheduler) for the specific in-memory checks that keep the scheduler
consistent with storage.

### After scheduling

Once the scheduler returns a result for a transaction:

- **SufficientFunds** — the transaction is re-enqueued into the normal execution pipeline. It
  will execute the transaction normally, and the withdrawal will happen as part of execution.
- **InsufficientFunds** — the transaction is re-enqueued with
  `FundsWithdrawStatus::Insufficient` set in its `ExecutionEnv`. When the execution pipeline
  picks it up, it sees this flag and immediately returns
  `ExecutionErrorKind::InsufficientFundsForWithdraw` — the transaction never runs. The transaction
  still produces effects (a failed execution) so it can be included in checkpoints.
- **SkipSchedule** — the transaction is dropped from the scheduler's perspective. This
  typically means a checkpoint executor has already executed the corresponding settlement (and
  barrier) on this node, so the on-chain state is already past the requested version.

### The channel-based wrapper

The public `FundsWithdrawScheduler` struct wraps the actual scheduling logic behind two
unbounded channels. One processes withdrawal requests; the other processes settlement
notifications. Two background tokio tasks drain these channels sequentially.

Why channels? The inner scheduler state (tracked balances, pending reservations) is mutable and
must be updated in a consistent order. By funneling all mutations through single-consumer
channels, we guarantee that withdrawals are processed one batch at a time and settlements are
applied one at a time, without needing complex locking protocols.

## 2. The simplest mental model: wait-then-check

Before explaining the eager scheduler (the production implementation), it helps to understand
the simplest possible approach. This mental model is implemented as the
`NaiveFundsWithdrawScheduler` in the codebase, used only for correctness testing — it will
never run in production. But it captures the essential logic that any withdraw scheduler must
get right.

The idea is straightforward: **wait until the accumulator version is fully settled, then check
balances from storage.**

When a batch of withdrawals arrives at version V, the scheduler looks at the current settled
version:

- If the current version **equals V**, the state is settled and we have exact balances. Read
  each account's balance from storage, then process transactions one by one in consensus
  order. For each transaction, check whether every account has enough to cover its withdrawal.
  Successful withdrawals deduct from a local balance copy (so later transactions in the same
  batch see the reduced balance). If any account is short, the transaction gets
  `InsufficientFunds`.
- If the current version is **ahead of V**, these withdrawals are stale — already settled by
  another path (e.g., the checkpoint executor advanced storage; see
  [`write_path.md`](./write_path.md) §5). All transactions get `SkipSchedule`.
- If the current version is **behind V**, settlement hasn't happened yet. Block until it does,
  then check as above.

### Example: why order matters

Consider Account A with balance 1000. Three transactions arrive in the same consensus commit,
all reading version 5:

| Transaction | Max Withdrawal |
|-------------|----------------|
| TX1 | up to 400 from A |
| TX2 | up to 300 from A |
| TX3 | up to 500 from A |

The scheduler reads A's balance (1000) and processes them in consensus order, deducting each
transaction's declared maximum:

1. **TX1**: max 400. Balance is 1000 — sufficient. Deduct max to 600. Result: **SufficientFunds**.
2. **TX2**: max 300. Balance is 600 — sufficient. Deduct max to 300. Result: **SufficientFunds**.
3. **TX3**: max 500. Balance is 300 — insufficient. Result: **InsufficientFunds**.

The deduction uses the declared maximum, not the actual amount (which isn't known until
execution). This is conservative: it guarantees that if a transaction is approved, it will
have enough funds no matter how much it actually withdraws (up to its declared max).

If TX3 had arrived before TX2 in the consensus ordering, TX3 would have succeeded (600 ≥ 500)
and TX2 would have failed (100 < 300). This is why all validators must process withdrawals in
the same order — the ordering comes from consensus, and the scheduling result is deterministic.

### Why this isn't good enough

The wait-then-check approach adds unnecessary latency: every withdrawal blocks until its declared
version is settled on chain, even when the scheduler already has all the information needed to
decide it.

The key observation is that the scheduler itself sees every withdrawal reservation in the order
consensus assigned them. By the time a new request arrives, the scheduler already knows what
every prior reservation against that account declared as its maximum withdrawal — that is enough
to compute the post-reservation balance without waiting for any of those earlier transactions to
finish executing. Settlement adjusts the in-memory state to match on-chain reality, but it is
not required to make a sound sufficient/insufficient decision for the next request. The naive
scheduler ignores this and blocks anyway.

The eager scheduler exploits this directly.

## 3. The eager scheduler

The eager scheduler is the production implementation. Its key innovation is maintaining
in-memory balance state so it can often approve withdrawals **immediately, without waiting for
settlement**.

### 3.1 Skipping already-settled versions (both paths)

Before getting into per-account bookkeeping, it is worth pinning down exactly when the eager
scheduler can short-circuit a batch. There are two version-based gates in
`schedule_withdraws`, each catching a different way that storage can have already moved past a
request:

- **`reservations.accumulator_version < cur_accumulator_version`** — the scheduler's own
  in-memory version is already past the request. This happens when `settle_address_funds` has
  already been called on this node for that version (validator path). All withdraws in the
  batch return `SkipSchedule`.
- **The initial balance read for an untracked account returns a version > the request's
  version** — the on-chain account object has been moved past the request even though the
  scheduler's in-memory `accumulator_version` may not have. This is precisely the
  checkpoint-executor case from [`write_path.md`](./write_path.md) §5: the settlement and
  barrier transactions ran out of a downloaded checkpoint, advancing the per-account version,
  but `settle_address_funds` was never called. The whole batch returns `SkipSchedule`.

Together these two checks guarantee that the eager scheduler never schedules withdraws against
a version that some component (in-memory or on-disk) has already moved past, regardless of
which path delivered the settlement.

### 3.1.1 Why a latest-state read is sufficient here

This is a place where the address-funds scheduler is intentionally asking a weaker question than
the object-funds checker asks later in the pipeline.

For an untracked account, `get_latest_account_amount` returns both the latest visible balance and
the version that balance came from. The scheduler uses that pair for two purposes only:

1. **Staleness detection.** If storage already says "this account is at version > V", then the
   scheduler knows the request for version V is stale and returns `SkipSchedule`.
2. **Conservative reservation against the current lower bound.** If storage is at version `<= V`,
   the scheduler may reserve against that balance, but only using the declared maximum withdrawal
   and only inside the scheduler's own per-account reservation discipline.

What the scheduler does **not** need here is an exact historical snapshot for version V. If the
version has already passed, some other part of the system has already carried the transaction
family forward: either this node processed settlement in-process and updated the scheduler via
`settle_address_funds`, or storage was advanced by the checkpoint executor and the scheduler can
stand down. In either case, the correct action is "do not schedule this batch again", not "replay
the old balance check at version V".

This is why latest-state plus version monotonicity is enough for address funds, while object
funds need MVCC-bounded reads.

### 3.2 The core insight

Suppose Account A has a balance of 10,000. A transaction arrives at a not-yet-settled version
declaring a max withdrawal of 100. The simple approach would wait. But the eager scheduler
reasons differently: "I know the current balance. I know how much I've already reserved (at
the declared maximum) for other pending withdrawals. If there's still room for this new max
withdrawal, I can approve it now — no matter what the settlement brings, the funds are there."

The critical invariant is: **the known balance can only decrease by at most the sum of all
reserved maximums, plus whatever settlement deltas arrive.** By tracking reserved maximums, the
eager scheduler can determine that new withdrawals fit within what's left — or that they don't
and must wait. Because reservations are conservative (based on the declared max, not the
actual withdrawal), the scheduler may sometimes over-reserve, but this is corrected when
settlement arrives with the actual (smaller) delta.

The race to keep in mind is that settlement may be applied by a different execution path than the
one currently looking at the transaction. The eager scheduler is safe because its decisions are
only:

- approve now based on a conservative reservation, or
- wait for more settlement information, or
- detect that some other path already moved past this version and stop scheduling it.

It never needs to reconstruct the exact historical balance after the version has already passed.

### 3.3 Per-account state

The eager scheduler tracks an `AccountState` for every account with pending or reserved
withdrawals. To approve withdrawals without waiting for settlement, the scheduler must
repeatedly answer one question: *given everything I have already committed to for this account,
does the next request still fit?* Each field of `AccountState` exists to make that question
answerable.

- **`last_updated_balance`** — the anchor that sufficiency checks subtract from. It is the most
  recent balance the scheduler knows to be correct, read from storage the first time the
  account is seen and kept current as settlement deltas arrive.

- **`last_updated_version`** — the version that `last_updated_balance` corresponds to. The
  version is tracked for two reasons:
  - *Deciding when a balance is final.* Settlement advances one version at a time. If an
    incoming request's version equals `last_updated_version`, the balance held is final for
    that version and any insufficiency is a deterministic failure (outcome 2 in §3.4). If the
    request's version is ahead, a later settlement could still deposit funds, so the request
    must wait (outcome 3). Without the version, the scheduler cannot tell these two cases
    apart.
  - *Idempotency of settlement.* Settlement deltas can be re-delivered (for example after
    restart). `settle_funds` ignores deltas at versions less than or equal to
    `last_updated_version`, so each delta applies exactly once.

- **`reserved_funds`** — the running total of withdrawals the scheduler has *already approved*
  but that have not yet settled on chain. The scheduler approves before execution, so until
  each approved withdrawal actually deducts itself, that money has been promised and must be
  held back from the next sufficiency check. Reservations are stored as a deque of
  `(version, amount)`: when settlement crosses a version, that version's reservation is
  released and replaced by the *actual* deduction from the settlement delta (which may be
  smaller than the reserved maximum). The running total keeps sufficiency checks O(1).

- **`pending_reservations`** — a FIFO queue for withdrawals the scheduler cannot decide yet —
  requests whose version is not yet settled and whose balance is currently insufficient. A
  future settlement may bring deposits that make them feasible, so they wait here until the
  next settlement gives the scheduler more information.

The lifecycle of an account in the tracker:

```
  Account first seen    Reserve withdrawals    Settlement arrives    All resolved
  with withdrawal   ──> against known      ──> balance updated,  ──> account removed
  (read balance         balance                reservations          from tracker
   from storage)                               released, pending
                                               queue drained
```

### 3.4 Reserving funds: the three outcomes

When a new withdrawal arrives for an account, the scheduler pushes it onto the account's
pending queue and then tries to process it. There are three possible outcomes:

**1. Immediate Success (reserve against known balance).**
If `total_reserved + max_withdrawal ≤ last_updated_balance`, then there are clearly enough
funds to cover even the worst case. The max withdrawal amount is added to `reserved_funds`,
and the withdrawal is marked as satisfied for this account. If this was the last account the
transaction was waiting on, the transaction immediately gets `SufficientFunds`.

This is the fast path. It handles the common case where an account has a large balance
relative to the declared maximum, and the scheduler doesn't need to wait for anything.

**2. Deterministic Failure (version is settled, balance is insufficient).**
If the withdrawal's accumulator version equals the last settled version, the scheduler has the
*final* balance for that version — no future settlement can change it. If the balance minus
reserved funds is too low, the transaction is doomed. The scheduler immediately notifies
`InsufficientFunds`.

**3. Uncertain — Must Wait (version is not yet settled, balance is insufficient).**
If the withdrawal's version is *ahead* of the last settled version, the scheduler doesn't have
final balance information. A future settlement might deposit funds into this account, making
the withdrawal feasible. The withdrawal stays in the `pending_reservations` queue and will be
retried when settlement arrives.

```
  New withdrawal arrives for Account A
       │
       ▼
  total_reserved + amount ≤ balance?
       │
    ┌──┴──┐
   YES    NO
    │      │
    │      ▼
    │  Is this version already settled?
    │      │
    │   ┌──┴──┐
    │  YES    NO
    │   │      │
    │   │      ▼
    │   │   WAIT: stay in pending queue
    │   │   (settlement may bring deposits)
    │   │
    │   ▼
    │  FAIL: InsufficientFunds
    │  (final answer — balance can't grow)
    │
    ▼
  SUCCESS: reserve the amount
  (add to reserved_funds, notify if all accounts done)
```

### 3.5 The pending queue and head-of-line ordering

Each account maintains its pending queue as a strict FIFO. If a withdrawal at the front of the
queue cannot be resolved (it's waiting for settlement), **all subsequent withdrawals for that
account are blocked too**, even if they individually would fit in the available balance.

Why? Because processing out-of-order could violate determinism. Consider: if TX1 needs 800
from an account with balance 1000, and TX2 needs 100, we might be tempted to approve TX2
immediately. But whether TX1 ultimately succeeds or fails depends on settlement. If TX1
succeeds, the remaining balance is 200 (enough for TX2). If TX1 fails, the remaining balance is
1000 (still enough). So TX2 is fine either way in this case — but in general, the math doesn't
always work out, and processing out of order would require complex speculation logic. The FIFO
approach is simple and guarantees that all validators make identical decisions.

## 4. Settlement in the eager scheduler

When a settlement arrives (version N → N+1) via `settle_address_funds`, the eager scheduler
does three things:

**Step 1: Identify affected accounts.** Three sets of accounts need updating:

- Accounts with reserved funds at version N (just settled — their reservations can be
  released).
- Accounts with pending withdrawals at version N+1 (can now be deterministically decided).
- Accounts that appear in the settlement's `funds_changes` map (their balance changed).

**Step 2: Update each affected account.**
For each account, the scheduler:

1. Applies the balance delta from the settlement (e.g., balance += deposit, balance -=
   withdrawal). Note that this delta reflects the *actual* amount withdrawn during execution,
   which may be less than the *maximum* that was reserved.
2. Releases reserved funds for the now-settled version (reducing `total_reserved` by the
   reserved maximum). The net effect of steps 1 and 2 can actually *increase* the effective
   available balance — for example, if we reserved 400 but only 300 was actually withdrawn,
   releasing the reservation adds back 400 while the settlement subtracts only 300, freeing
   up 100 more than what was previously available.
3. Drains the pending queue: repeatedly tries to reserve the front withdrawal, since the
   balance and/or reserved total may have changed enough to make previously-blocked
   withdrawals feasible.

**Step 3: Garbage-collect empty accounts.**
If an account has no more reserved funds and no pending withdrawals, it is removed from the
tracker. This keeps memory usage proportional to the number of in-flight withdrawals.

**Idempotency.** `settle_funds` short-circuits if its `next_accumulator_version` is less than
or equal to the scheduler's current `accumulator_version`, and `AccountState::settle_funds`
ignores deltas at versions less than or equal to the account's `last_updated_version`. This is
what makes the call safe in the face of mixed paths: even if a node briefly tracks an account
in memory and then the same version is later observed via storage (or vice versa), no
double-application occurs.

Note that on the **checkpoint-executor path** ([`write_path.md`](./write_path.md) §5),
`settle_address_funds` is never called — there is simply nothing to do in memory, because
§3.1's storage-version check already prevents the scheduler from ever building up reservations
or pending entries against a version that storage has moved past.

## 5. Worked examples

### 5.1 Basic eager scheduling (reservation vs. actual withdrawal)

**Setup**: two accounts, Alice (balance 1000) and Bob (balance 500), both at version 5.

**Consensus Commit 1** arrives with two transactions at version 5:

- TX1 declares a max withdrawal of 400 from Alice and 200 from Bob.
- TX2 declares a max withdrawal of 300 from Alice.

The scheduler processes TX1 first:

- Alice: total reserved is 0, and 0 + 400 ≤ 1000. Reserve succeeds. Reserved = {v5: 400}.
- Bob: total reserved is 0, and 0 + 200 ≤ 500. Reserve succeeds. Reserved = {v5: 200}.
- Both accounts are done → TX1 gets **SufficientFunds** immediately.

Then TX2:

- Alice: total reserved is 400, and 400 + 300 = 700 ≤ 1000. Reserve succeeds. Reserved = {v5: 700}.
- Only one account → TX2 gets **SufficientFunds** immediately.

Both transactions are released for execution without waiting for any settlement. Note that 700
is the total *maximum* reserved for Alice — the actual execution may withdraw less.

```
  After scheduling:
  ┌──────────────────────────────────────────┐
  │  Alice:  balance=1000  reserved={v5:700} │
  │  Bob:    balance=500   reserved={v5:200} │
  └──────────────────────────────────────────┘
```

**Consensus Commit 2** arrives with TX3 at version 6 (before version 5 settles):

- TX3 declares a max withdrawal of 400 from Alice.
- Alice: total reserved is 700, and 700 + 400 = 1100 > 1000. Cannot reserve.
- Version 6 ≠ last settled version (5), so this is **uncertain**. TX3 goes into Alice's
  pending queue and the scheduler returns **Pending**.

```
  After scheduling TX3:
  ┌────────────────────────────────────────────────────┐
  │  Alice:  balance=1000  reserved={v5:700}           │
  │          pending_queue=[TX3: needs 400 at v6]      │
  │  Bob:    balance=500   reserved={v5:200}           │
  └────────────────────────────────────────────────────┘
```

**Settlement v5→v6** arrives with changes: Alice −500, Bob −200.

Notice that Alice's settlement delta is −500, not −700. TX1 and TX2 reserved a combined maximum
of 700, but during actual execution they only withdrew 500 total (each withdrew less than its
declared maximum).

The scheduler processes Alice:

1. Apply delta: balance = 1000 + (−500) = 500. Version advances to 6.
2. Release reserved funds at v5: total reserved goes from 700 to 0.
   The net effect: the balance went from an *effective available* of 300 (1000 − 700 reserved)
   to 500 (actual balance after settlement, with no reservations). The gap between the
   reserved max (700) and the actual withdrawal (500) freed up 200 extra.
3. Drain pending queue: TX3 needs 400. Is 0 + 400 ≤ 500? Yes! Reserve succeeds.
   TX3 gets **SufficientFunds**.

The scheduler processes Bob:

1. Apply delta: balance = 500 + (−200) = 300. Version advances to 6.
2. Release reserved funds at v5: total reserved goes from 200 to 0.
3. No pending withdrawals.

Both accounts are now empty (no reserved funds, no pending queue) and are removed from the
tracker.

This example illustrates how the conservative reservation approach (reserving the max) can
temporarily over-reserve, but settlement corrects this. TX3 was initially blocked because the
scheduler assumed the worst case (700 withdrawn from Alice). When settlement revealed the
actual amount was only 500, the freed-up balance was enough for TX3 to proceed.

### 5.2 A deposit unblocks a pending withdrawal

**Setup**: Account A has balance 100 at version 5.

**Consensus Commit** at version 6 (before v5 settles):

- TX1 declares a max withdrawal of 200 from A.
- A: total reserved is 0, but 0 + 200 = 200 > 100. Cannot reserve.
- Version 6 ≠ last settled version (5) → **uncertain**. TX1 is queued as Pending.

The withdrawal is stuck. But then...

**Settlement v5→v6** arrives with changes: A +150 (someone deposited 150 into A's accumulator
during the v5 batch).

The scheduler processes A:

1. Apply delta: balance = 100 + 150 = 250. Version advances to 6.
2. No reserved funds to release (nothing was reserved at v5).
3. Drain pending queue: TX1 needs 200. Is 0 + 200 ≤ 250? Yes! Reserve succeeds.
   TX1 gets **SufficientFunds**.

```
  Timeline:

  Version 5           Settlement v5→v6         After settlement
  ─────────           ────────────────         ────────────────
  A: bal=100          A gets +150 deposit      A: bal=250
  TX1 wants 200       ───────────────────>     TX1 reserved: 200 ✓
  Can't reserve yet                            → SufficientFunds!
  (uncertain)
```

The key point: the eager scheduler didn't reject TX1 outright when it first saw insufficient
balance. Because the version hadn't settled yet, it held the withdrawal in the pending queue.
When settlement brought a deposit, the pending withdrawal was automatically retried and
approved.

### 5.3 Multi-account withdrawal

A single transaction can withdraw from multiple accounts. The `PendingWithdraw` object is
shared (via `Arc`) across all the accounts that a transaction touches. It maintains a set of
accounts that have **not yet** successfully reserved their portion.

**Setup**: Alice has balance 1000, Bob has balance 500, both at version 5.

TX1 declares a max withdrawal of 100 from Alice and 200 from Bob. The scheduler creates a
single `PendingWithdraw` for TX1, shared between Alice's account state and Bob's account state.

```
  PendingWithdraw for TX1:
  ┌─────────────────────────────────────┐
  │  pending accounts: {Alice, Bob}     │
  │  oneshot sender: [waiting to send]  │
  └─────────────────────────────────────┘
         ▲                    ▲
         │                    │
    Alice's account      Bob's account
    state holds a        state holds a
    reference (Arc)      reference (Arc)
```

Processing proceeds account by account:

1. **Alice's account** checks: 0 + 100 ≤ 1000. Success! It calls
   `pending_withdraw.remove_pending_account(Alice)`. The pending set becomes `{Bob}`. Not
   empty yet — don't notify.

2. **Bob's account** checks: 0 + 200 ≤ 500. Success! It calls
   `pending_withdraw.remove_pending_account(Bob)`. The pending set becomes `{}`. Empty —
   **send SufficientFunds** through the oneshot channel.

If Bob's account had failed (say Bob only had 50), then `notify_insufficient_funds()` would
fire immediately, sending **InsufficientFunds** — even though Alice's portion was fine. The
transaction fails as a whole if **any** account is short.

An important subtlety: even when a transaction fails due to one account being insufficient, the
other accounts that succeeded still have their balance "reserved" (deducted from the available
pool for subsequent transactions). This is intentional — it ensures that the scheduler's
accounting stays consistent regardless of which transactions ultimately succeed or fail.

### 5.4 Queued withdrawals across versions

This example shows how the pending queue interacts with multiple versions.

**Setup**: Account A has balance 500 at version 5. Last settled version is 5.

**Step 1**: TX1 at version 5 declares a max withdrawal of 400.

- 0 + 400 ≤ 500 → reserved immediately. Reserved = {v5: 400}. **SufficientFunds**.

**Step 2**: TX2 at version 6 declares a max withdrawal of 200.

- 400 + 200 = 600 > 500 → cannot reserve.
- Version 6 ≠ settled version 5 → **uncertain** → queued in pending.

**Step 3**: TX3 at version 6 declares a max withdrawal of 50.

- TX2 is already at the front of the pending queue and is unresolved.
- TX3 is pushed behind TX2. Even though 400 + 50 = 450 ≤ 500, TX3 cannot skip ahead.
- TX3 is also **Pending**.

```
  Account A state:
  ┌─────────────────────────────────────────────────┐
  │  balance: 500       reserved: {v5: 400}         │
  │  pending queue: [TX2(200@v6), TX3(50@v6)]       │
  └─────────────────────────────────────────────────┘
```

**Step 4**: Settlement v5→v6 arrives, with changes: A −400 (the TX1 withdrawal is settled).

1. Apply delta: balance = 500 + (−400) = 100. Version → 6.
2. Release reserved at v5: total reserved drops from 400 to 0.
3. Drain pending queue:
   - TX2 needs 200. Is 0 + 200 ≤ 100? No. Version 6 == settled version 6 →
     **InsufficientFunds**. TX2 is removed from the queue.
   - TX3 needs 50. Is 0 + 50 ≤ 100? Yes! Reserved. **SufficientFunds**.

Even though TX3 could have been approved earlier in isolation, the FIFO ordering ensured it
waited behind TX2. Once TX2 was resolved (and failed), TX3 was immediately processed and
approved.

## 6. Determinism

The withdraw scheduler must be **deterministic**: given the same inputs, all validators must
produce the same scheduling decisions. This is essential because the scheduling outcome
(sufficient vs. insufficient) determines whether a transaction executes normally or fails with
an early error, and validators must agree on this.

Determinism is achieved through several mechanisms:

1. **Consensus ordering** — all validators see the same transactions in the same order within
   each consensus commit, and process commits in the same sequence.
2. **Sequential processing** — the channel-based wrapper ensures withdrawals and settlements
   are processed one at a time, in arrival order.
3. **FIFO per-account queuing** — the pending queue prevents out-of-order decisions that
   could diverge across validators.
4. **Version-gated decisions** — a withdrawal is only decided at the "uncertain" vs.
   "deterministic" boundary based on whether its version has been settled, which all
   validators agree on.

The eager scheduler's stress tests verify determinism by running the same inputs multiple
times and asserting identical results.

## 7. Epoch boundaries

At each epoch boundary, the address-funds scheduler is fully replaced:

1. **`close_epoch()`** is called on the current `FundsWithdrawScheduler`, signaling it to stop.
2. The old scheduler is dropped, and a fresh one is constructed (initialized with the current
   accumulator version from the new epoch's state).
3. The old scheduler's channel senders are dropped, which causes its background tasks to
   gracefully exit when their receivers close.

This clean break means no in-flight in-memory state carries over across epochs. Any pending
withdrawals from the old epoch are effectively abandoned — they'll be handled by the
checkpoint executor during epoch transition, and the new epoch starts from the on-chain state
at the boundary.

The object-funds checker is replaced through the same mechanism — see
[`object_funds_checking.md`](./object_funds_checking.md) §5.
