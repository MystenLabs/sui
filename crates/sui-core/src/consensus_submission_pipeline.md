# Transaction Submission Pipeline: Admission Queue and Consensus Adapter

This documents the dataflow from a `SubmitTransaction` RPC arriving at a validator,
through the gas-price admission queue (`admission_queue.rs`) and the consensus adapter
(`consensus_adapter.rs`), to the point where the RPC request completes and the
background submission task settles.

**Keep this doc in sync**: when changing behavior in `admission_queue.rs`,
`consensus_adapter.rs`, or the submit path in `authority_server.rs`, update this file.

## Overview

```
gRPC SubmitTx
     │
ValidatorService::handle_submit_transaction        (authority_server.rs)
     │  per-tx validation, dedup, voting
     │
classify_submit_mode()
     │
     ├─ Bypass ───────────────────────────────┐
     ├─ Disabled (per-tx overload reject) ────┤
     │                                        │
     └─ Queue                                 │
         │ AdmissionQueueHandle::try_insert   │
         ▼                                    │
   AdmissionQueueEventLoop (per-epoch actor)  │
         │ drain_batch (highest gas price)    │
         ▼                                    ▼
   ConsensusAdapter::submit_and_get_positions
         │ spawn submit_and_wait task
         ├──► RPC caller gets Vec<ConsensusPosition>  ← request "finishes" here
         │
   background task continues:
         │ block status → per-position terminal status
         ▼
   InflightDropGuard::drop → inflight_slot_freed_notify → queue drains more
```

## Design principles

The pipeline assumes a well-behaved client submits a transaction **once** and then
waits on its status. Occasionally the same transaction is submitted again —
typically when the client lost its connection to this validator and chose to retry
here. Submitting many copies of the same transaction in a short interval is outside
this contract: the dedup and spam-accounting layers exist to defend against that
pattern, not to optimize for it.

1. **Inflight accounting is leak-free; admission is deliberately weak.**
   `num_inflight_transactions` is the single signal behind every backpressure
   decision — the bypass threshold, the queue's drain capacity, and the
   Disabled-mode reject — so it must stay accurate under high load. It is
   maintained exclusively by RAII (`InflightDropGuard`): every exit path —
   settled, skipped as already-processed, cancelled at epoch end, dropped
   mid-race — releases its slots. The asymmetry is intentional: admission checks
   are weak (checked, not atomically reserved; a transient overshoot
   self-corrects as tasks settle), but a leaked slot is permanent capacity loss
   for the epoch, so release correctness is the hard invariant.

2. **Answer early when the outcome is already known elsewhere.** A transaction
   observed as processed — via another validator's submission appearing in
   consensus output, or via checkpoint execution/sync — is answered without
   being (re)submitted, with a retriable `TransactionProcessing` or a concrete
   terminal error. This applies both before submission (the RPC handler's
   `is_consensus_message_processed` suppression, the adapter's
   `check_processed_via_consensus_or_checkpoint`) and during it
   (`processed_notify` racing the submit loop). No transaction is forced through
   this validator's own submission to get its answer.

3. **Deduplicate at every layer, with checks local to that layer.** Repeated
   digests within a request, concurrent in-flight submissions
   (`InflightTransactionsGuard`), recent submissions (TTL cache), already-queued
   keys (`queued_keys`), and already-processed keys are each caught where the
   information is cheaply available. Duplicates that slip through are still
   *accounted* — tallied as spam weight for traffic control — even when they are
   admitted.

4. **Dedup and early-response are optimizations, not correctness requirements.**
   Both may be imperfect where precision would cost real complexity: the
   admission queue flags duplicates but admits them rather than coalescing
   waiters across entries; the recently-submitted cache is a TTL approximation;
   an expired status-cache entry ends the task instead of reconstructing the
   outcome. This is safe because downstream consensus handling is idempotent and
   clients own transaction-level retries — a missed dedup costs load, never
   correctness.

5. **Only system transactions are owed retries.** System transactions
   (checkpoint signatures, `EndOfPublish`, DKG, capability notifications) have
   no client behind them and the protocol depends on them landing, so the
   adapter retries them until they are sequenced, observed processed, or the
   epoch ends — the deliberate exception being self-superseding messages on the
   `submit_best_effort` path, where the next periodic submission replaces a
   missed one. User transactions have a client that owns retries: the adapter
   still retries internally where it is cheap and unambiguous (resubmission
   after garbage collection, backoff on submission errors), but only as best
   effort — ambiguous or terminal conditions (expired status, processed
   elsewhere, epoch end) end the task rather than retry it, and the client is
   expected to resubmit.

6. **Every submission ends in exactly one explicit, actionable outcome.** No
   silent drops: an evicted queue entry receives a distinct outbid error (not a
   bare channel error), a processed-elsewhere transaction a retriable error, an
   owned-object conflict loser a terminal error. Errors are chosen to tell the
   client what to do next — retry, outbid, or stop.

7. **Prefer higher gas priced user transactions for submission.** When consensus
   capacity is scarce, it goes to the transactions bidding the most for it —
   admission, eviction, and drain order are all driven by gas price.

8. **System messages and reconfiguration are never blocked behind user
   traffic.** Checkpoint signatures, `EndOfPublish`, DKG, and capability
   notifications skip both the admission queue and the submit semaphore;
   `within_alive_epoch` cancels all pending submissions at epoch termination so
   reconfiguration cannot be held up by a stuck submission.

## 1. Entry and routing (`authority_server.rs`)

`handle_submit_transaction` runs a long per-transaction validation gauntlet before
anything touches the queue or adapter: deserialize + validity check,
duplicate-digest-in-request check, soft-bundle gas-price equality, system overload
check, gasless rate limiting, signature verification, already-executed
short-circuits, an `is_consensus_message_processed` suppression path (with
owned-lock conflict probing so conflict losers get a terminal error instead of a
retriable one), the `InflightTransactionsGuard` dedup, and `handle_vote_transaction`.
Survivors become `ConsensusTransaction::UserTransactionV2` messages.

**Dedup guard** (`InflightTransactionsGuard`): digests are atomically acquired into
an in-flight set for the duration of the handler; concurrent duplicates get
`TransactionSubmitted`. On drop, digests demote into a TTL `recently_submitted`
cache (window from node config) so immediate resubmissions are still suppressed.

**Grouping**: a soft bundle is one submission group; a batch request becomes one
group per transaction. Groups map back to `(result index, digest)` so per-group
outcomes land on individual per-tx results.

**Routing** — `classify_submit_mode`, evaluated once per request:

| Condition | Mode |
|---|---|
| Queue not configured (`admission_queue_enabled: false`, default true) | Disabled |
| Ping request | Bypass |
| `num_inflight_transactions < bypass_threshold` (default 0.9 × `max_pending_transactions`) | Bypass |
| Overloaded and `failover_tripped()` | Disabled |
| Otherwise (overloaded, queue healthy) | Queue |

Two behavioral notes:

- The per-tx `check_consensus_overload()` reject (`TooManyTransactionsPendingConsensus`)
  only runs in **Disabled** mode. Bypass mode skips it — inflight is below threshold
  by definition. Queue mode replaces rejection with queuing/eviction.
- The failover check is only consulted on the overloaded path, so the hot path never
  pays the `ArcSwap` load.

## 2. The queue path (`admission_queue.rs`)

**Handle → actor**: `AdmissionQueueHandle::try_insert` creates a `oneshot` for the
eventual consensus positions, packages a
`QueueEntry { gas_price, transactions, position_sender, client_addr, enqueue_time }`,
and sends an `InsertCommand` over a bounded mpsc to the per-epoch actor.
`send().await` applies backpressure when the command channel
(capacity = max(queue capacity, 1024)) is full; a closed channel or dropped ack maps
to `TooManyTransactionsPendingConsensus`. Gas price for an entry is the *minimum*
across the bundle, 0 for gasless — so gasless entries are always first to be evicted.

**Priority queue** (`PriorityAdmissionQueue`): `BTreeMap<gas_price, VecDeque<QueueEntry>>`,
FIFO within a price level. Capacity defaults to 0.5 × `max_pending_transactions`.
Insert semantics when full:

- New price **strictly greater** than the current minimum → evict the lowest-priced
  (oldest at that price) entry. The evicted caller's `position_sender` gets
  `TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: evicter's price }`,
  so its `rx.await` returns a distinct outbid error rather than a `RecvError`.
- Otherwise → the *incoming* insert is rejected with the same error carrying the
  queue's current min price (equal price does not evict).

**Duplicates are admitted, not rejected**: a `queued_keys` refcount detects entries
whose `ConsensusTransactionKey` is already queued; the insert returns
`newly_inserted = false`, which the RPC layer records as `duplicate_at_admission`
and converts to spam weight 1 for traffic control (`request_spam_weight`). If the
adapter does not suppress the entry as already processed, the duplicate can still
flow to consensus; admission duplicate detection itself only affects spam weight.

**Event loop** (`AdmissionQueueEventLoop::run`): a single actor per epoch, spawned by
`AdmissionQueueManager::spawn` and rotated at reconfig. Each iteration:

1. Drain all pending inserts non-blockingly; publish queue depth to an atomic
   (read by `failover_tripped`).
2. If the queue is non-empty and consensus has capacity
   (`num_inflight_transactions < max_pending_transactions`), `drain_batch` and loop.
3. Empty queue → block on `recv()` (channel closed = actor shutdown).
4. Non-empty but consensus saturated → register the `slot_freed_notify.notified()`
   future **before** re-checking capacity (missed-wakeup avoidance), then
   `select! { biased; recv, slot_freed }` — new inserts win ties so eviction
   ordering stays current.

**Drain** (`drain_batch`): pops up to `max_pending − inflight` entries, highest gas
price first, observes `queue_wait_latency`, and spawns one `submit_queue_entry` task
per entry, which calls `consensus_adapter.submit_and_get_positions(...)` and
forwards the result (positions or error) into the entry's `position_sender`.
`last_drain` is stamped only when entries were actually popped, so a stuck drainer
isn't hidden from failover. Note the slot accounting is per-*entry* at drain time,
while the adapter's inflight count is per-*transaction* — a soft bundle occupies one
drain slot but N inflight slots, so drains can transiently overshoot `max_pending`
slightly.

**Failover** (`AdmissionQueueHandle::failover_tripped`): if the queue is non-empty
and no drain has happened for `admission_queue_failover_timeout` (default 30s),
`classify_submit_mode` treats the queue as stuck and routes around it in Disabled
mode (direct submit with per-tx saturation rejects) until the actor makes progress
again. An empty queue never trips failover.

**Epoch rotation** (`AdmissionQueueContext::rotate_for_epoch`, wired in
`sui-node/src/lib.rs`): `AdmissionQueueContext` holds an
`ArcSwap<AdmissionQueueHandle>`; reconfig spawns a fresh actor bound to the new
epoch store and swaps handles. The old actor keeps draining while consensus has
capacity (its drains reject inside the adapter with `ValidatorHaltedAtEpochEnd` once
user certs close), and shuts down when its closed channel is observed; any entries
still queued at that point are dropped, and their callers see `RecvError` →
`TooManyTransactionsPendingConsensus`.

**Waiting on the result**: back in `authority_server.rs`, the handler awaits all
groups' `position_rx` receivers *without short-circuiting*. Per-group outcomes:

- `Ok(positions)` → each tx gets `SubmitTxResult::Submitted { consensus_position }`.
- `Err(TransactionProcessing)` → retriable per-tx `Rejected` (the tx is already
  being processed elsewhere), not a request failure.
- Any other error (outbid, halted-at-epoch-end, etc.) fails the whole request;
  `ValidatorHaltedAtEpochEnd` is retried once by the outer loop after waiting
  (≤15s) for the next epoch.

## 3. Inside the consensus adapter (`consensus_adapter.rs`)

`submit_and_get_positions` is the entry for both Bypass/Disabled and drained queue
entries:

1. Under the **reconfig read lock**: if user certs are no longer accepted →
   `ValidatorHaltedAtEpochEnd`. Otherwise `submit_batch` → `submit_unchecked` spawns
   the long-lived `submit_and_wait` task and the lock is released.
2. The caller then awaits a `oneshot` carrying `SuiResult<Vec<ConsensusPosition>>`.
   A dropped sender (e.g. the task was cancelled at epoch end) surfaces as
   `FailedToSubmitToConsensus`.

The spawned task is wrapped in `epoch_store.within_alive_epoch(...)` — every pending
submission is cancelled at epoch termination, which is also what guarantees
reconfiguration can proceed and that an ex-validator stops submitting.

**`submit_and_wait_inner`** — the core lifecycle:

- **Ping** (empty tx list): one `submit_inner` call to get a position; returned
  immediately, block status ignored.
- **DoS accounting**: each user tx is recorded in `submitted_transaction_cache` with
  an amplification factor of `gas_price / reference_gas_price` before submission.
- **Inflight accounting**: `InflightDropGuard::acquire` bumps
  `num_inflight_transactions` by the bundle size. This is the number the queue's
  capacity checks and the bypass threshold read. It covers the *entire* task
  lifetime — until terminal status or cancellation, not just until the position is
  returned.
- **Already-processed short-circuit**
  (`check_processed_via_consensus_or_checkpoint`): a synchronous check against
  consensus-processed keys, checkpoint-executed digests, and (for checkpoint
  signatures) already-synced checkpoint sequence numbers. If everything is
  processed, submission is skipped and the position-waiting caller gets a retriable
  `TransactionProcessing` error instead of a meaningless position.
- **Submission concurrency**: non-system transactions must acquire
  `submit_semaphore` (`max_pending_local_submissions` permits); system messages
  (checkpoint sigs, EndOfPublish, DKG, capabilities) bypass it so they're never
  buffered behind user traffic.
- **Submit loop** raced against **`processed_notify`** via `select`:
  - `submit_inner` calls `consensus_client.submit` with unbounded retries and
    100ms→10s exponential backoff (errors here are expected during reconfig). On
    success it returns `(positions, block-status waiter)`.
  - The positions from the **first successful attempt** are sent to the caller
    immediately (`tx_consensus_positions.take()`); the RPC request is effectively
    done at this point. Internal retries never send a second position — clients
    handle retries themselves if their position doesn't produce results.
  - Block status outcomes:
    - **`Sequenced`** → for system messages, done; for user transactions,
      `wait_for_position_statuses` waits for every position to reach a terminal
      `ConsensusTxStatus` (Finalized / Rejected / Dropped) via the status cache,
      which settles the submission (`SequencingOutcome::Sequenced`). A status that
      expired from the cache before being read means the outcome existed and was
      missed — the task ends (`StatusExpired`) rather than resubmitting.
    - **`GarbageCollected`** → the block never committed; sleep 1s and resubmit.
    - **Waiter error** → sleep 1s and retry.
  - `processed_notify` wins the race when the tx becomes visible through another
    path first — consensus output from another validator's submission, execution in
    a checkpoint, or (for signature messages) a synced checkpoint. The submit future
    (including its retry loop) is dropped, and if the position was never sent, the
    caller gets the retriable `TransactionProcessing` error.
- **Epilogue**: after a user-transaction settle, if the epoch is closing (and not
  timestamp-based close), an `EndOfPublish` is submitted.

**How "processed" is observed**: the synchronous short-circuit and the
`processed_notify` race are backed by the same three notification paths, each a
sync table check paired with a notify-read:

| Path | Sync check | Async notification | Populated by |
|---|---|---|---|
| Consensus output | `is_consensus_message_processed` | `consensus_messages_processed_notify` | The consensus handler, when a commit containing the key is processed: the key is recorded in the commit output (in-memory quarantine, later flushed to the `consensus_message_processed` table) and `process_notifications` wakes waiters. Fires for commits originating from *any* validator's submission, and for system messages. |
| Checkpoint execution | `is_transaction_executed_in_checkpoint` | `transactions_executed_in_checkpoint_notify` | The checkpoint executor's `insert_finalized_transactions`, writing `executed_transactions_to_checkpoint` when a certified checkpoint — locally built or state-synced — is executed. Applies to cert-shaped keys (user transaction digests). |
| Synced checkpoint | `get_highest_synced_checkpoint_seq_number() >= seq` | `notify_read_synced_checkpoint` | State sync advancing the highest synced checkpoint in `CheckpointStore`. Applies only to checkpoint-signature keys: a synced checkpoint at or above the signature's sequence number makes the signature redundant. |

For soft bundles, each key is waited on individually (`FuturesUnordered`), so a
bundle completes even when its transactions are observed through different paths;
if any key resolves via a checkpoint path, `CheckpointExecuted` is reported as the
processed method.

**Completion and the feedback loop**: when the task ends by any path — settled
statuses, processed notification, status expiry, or epoch-end cancellation —
`InflightDropGuard::drop` decrements `num_inflight_transactions` and fires
`inflight_slot_freed_notify.notify_one()`. That single `Notify` (created in
`sui-node/src/lib.rs`, shared between `ConsensusAdapter` and
`AdmissionQueueManager`) is exactly what the admission queue actor selects on,
closing the loop: consensus capacity frees → the actor wakes → drains the next
highest-gas-price entries.

## 4. What "finished" means at each layer

| Actor | Finishes when |
|---|---|
| RPC caller | All groups return consensus positions or errors → `SubmitTxResult` per tx (Submitted / Executed / Rejected). Finality is *not* awaited here; clients follow up via wait-for-effects. |
| Queue entry | Popped and handed to the adapter (positions flow back through its oneshot), evicted (outbid error), or dropped at actor shutdown (`TooManyTransactionsPendingConsensus`). |
| Adapter task | All positions reach a terminal consensus status, or the tx is observed processed via another path, or the status expired, or the epoch ends. Only then does the inflight slot free. |

## 5. Per-transaction outcomes: errors and effects

### SubmitTx results

Each transaction in the request resolves to one `SubmitTxResult`:

| Result | Meaning |
|---|---|
| `Submitted { consensus_position }` | Accepted (directly or via the queue) and acknowledged by consensus; the position identifies the block/index. A queue-admitted duplicate can still get its own position if it is not suppressed by already-processed checks; admission duplicate detection only affects spam weight. |
| `Executed { effects_digest, details }` | The transaction already executed; effects are returned directly in the submit response. |
| `Rejected { error }` | Not submitted (or suppressed); the error tells the client what to do next. |

### Rejection errors, by provenance

**Generated internally** — from this validator's own admission state, nothing
consensus-derived consulted:

| Error | Trigger | Client action |
|---|---|---|
| `RepeatedTransactions` | Duplicate digest within one request (fails the whole request for soft bundles) | Fix the request |
| Overload errors (e.g. `ValidatorOverloadedRetryAfter`) | `check_system_overload` at signing; gasless rate limiting | Retry after backoff |
| `TooManyTransactionsPendingConsensus` | Per-tx consensus saturation check (Disabled mode only); also whole-request when the queue actor is gone | Retry after backoff |
| `TransactionSubmitted` | Concurrent in-flight duplicate or recently-submitted duplicate (dedup guard / TTL cache) | Wait; the earlier submission is in flight |
| `TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price }` | Queue full and gas price too low (immediate reject), or entry later evicted by a higher bid (delivered through the position channel); fails the whole request | Resubmit with gas price above `min_gas_price` |
| `ValidatorHaltedAtEpochEnd` | Reconfig closed user certs; the handler internally retries once after waiting (≤15s) for the new epoch before surfacing it | Retry (possibly on another validator) |
| `FailedToSubmitToConsensus` | Position channel dropped (e.g. submission task cancelled at epoch end) | Retry |

**Read from consensus-derived state** — caches and tables populated by consensus
output or checkpoints, so they fire even if this validator never submitted the
transaction itself:

| Error | Read from | Client action |
|---|---|---|
| `TransactionAlreadyExecuted` | Transaction cache: digest executed in a previous epoch | Stop; terminal |
| Owned-object lock conflict | Epoch owned-lock table (populated post-consensus): the digest lost a conflict to another transaction holding its input locks | Stop; terminal |
| Revalidation errors (stale/unavailable input versions, etc.) | Live object state via `handle_vote_transaction`, surfacing terminal errors once a conflict winner executed | Stop; terminal |
| `TransactionProcessing { digest, status }` | Consensus-processed key table: the digest is known-processed but may still execute (e.g. deferred). Returned upfront by the RPC handler, or by the adapter when the already-processed check or the `processed_notify` race fires before a position was sent | Retriable: poll WaitForEffects or resubmit later |

Note: per-position consensus statuses (`Finalized` / `Rejected` / `Dropped`) never
surface through SubmitTx — the submit response completes at position
acknowledgment. They surface through WaitForEffects.

### Effects in responses

- **SubmitTx**: when the digest already has executed effects,
  `complete_executed_data` packages the effects together with events and
  input/output objects into `SubmitTxResult::Executed`. If that data can no longer
  be reconstructed (e.g. objects pruned), the handler falls through to the
  processed-suppression path and returns a retriable error instead.
- **WaitForEffects** (the expected follow-up to `Submitted`, keyed by digest plus
  optional consensus position):
  - `Executed { effects_digest, details? }` — resolves when executed effects appear
    in the transaction cache, whether execution came via this validator's
    submission, another validator's, or checkpoint sync. Details (events,
    input/output objects) only when requested.
  - `Rejected { error: Option<_> }` — the status cache reported `Rejected` or
    `Dropped` for the position. The error is **read from the
    `TransactionRejectReasonCache`**, populated when reject votes are cast or
    observed (`consensus_validator.rs`, `consensus_handler.rs`); it is `None` when
    no reason was recorded locally or it already expired.
  - `Expired { epoch, round }` — the position aged out of the status-cache
    retention window with no recorded status; the client should resubmit.
  - Request-level: a guard rejects positions too far ahead of the last committed
    round; the handler times out after 20s; `EpochEnded` if the epoch terminates
    mid-wait.

## 6. Backpressure and limits

Config lives in `AuthorityOverloadConfig` (`sui-config/src/node.rs`).

- `max_pending_transactions` — global inflight budget; caps queue drains and defines
  the two derived thresholds. Enforced weakly (checked, not atomically reserved).
- `admission_queue_bypass_fraction` (default 0.9) — below this inflight level the
  queue is skipped entirely.
- `admission_queue_capacity_fraction` (default 0.5) — queue size; overflow resolves
  by gas-price eviction/rejection.
- `admission_queue_failover_timeout` (default 30s) — stuck-queue detection window.
- `max_pending_local_submissions` — semaphore on concurrent
  `consensus_client.submit` calls for user txs.
- Actor mpsc channel — `max(queue capacity, 1024)`; senders await rather than fail
  when it's full.

A design consequence worth stating: in Queue mode, saturation no longer rejects
transactions outright — it converts consensus capacity into a gas-price auction,
with FIFO fairness within a price level and gasless transactions as designated first
victims. Rejection only reappears when the queue itself is full (outbid) or presumed
dead (failover → Disabled semantics).

## 7. Non-queue producers into the adapter

System messages never pass through the admission queue — they enter the adapter
directly and skip the submit semaphore:

- Checkpoint signatures via `SubmitToConsensus::submit_to_consensus`.
- `EndOfPublish` via `ReconfigurationInitiator::close_epoch` and
  `recover_end_of_publish` (crash recovery).
- Timeout-bounded `submit_best_effort` for self-superseding messages (e.g.
  execution-time observations): one `submit_inner` attempt bounded by a timeout, no
  status wait, no retry on GC. User transactions are rejected on this path.
