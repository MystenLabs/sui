# TransactionPool: Pull-Based Consensus Submission

`transaction_pool.rs` is an alternative to the current submission pipeline
(`admission_queue.rs` + the submission half of `consensus_adapter.rs`), selected by
node config (`transaction_pool_enabled`, default off; the
`SUI_TRANSACTION_POOL_ENABLED` env var flips the default for test runs). It keeps
the same entry points, external contracts, and design principles as
`consensus_submission_pipeline.md`, while deleting most of the moving parts. Read
that doc first for the current pipeline and the principles referenced here. Keep
this doc in sync with behavior changes in `transaction_pool.rs`.

## The core inversion

Today's pipeline is **push-based**: the admission queue actor drains entries into
`ConsensusAdapter::submit_and_get_positions`, which spawns a long-lived task per
submission; the task pushes bytes into consensus's `TransactionClient` channel,
races a `processed_notify`, waits on block status and per-position statuses, and
frees an inflight slot on drop, which wakes the queue actor to drain more.

The pool is **pull-based**: transactions sit in one prioritized in-memory pool, and
consensus *takes* from it when proposing a block — the same moment today's
`TransactionConsumer::next()` drains its channel (`proposer.rs` `try_new_block`).
Settlement is **push-based callbacks** from the components that already learn the
outcomes (the commit handler, the checkpoint executor), instead of per-transaction
tasks awaiting notifications.

Everything between "validated transaction" and "bytes in a proposed block" becomes
a state machine over three maps behind one mutex. Deleted outright:

- The per-submission `submit_and_wait` task, its `within_alive_epoch` wrapper, the
  `select!` race against `processed_notify`, and the status-expiry settle path.
- `InflightDropGuard`, the `num_inflight_transactions` atomic, and the shared
  `inflight_slot_freed_notify` feedback `Notify`. Accounting becomes structural:
  an entry occupies capacity exactly while it is in the maps.
- The admission queue actor, its command mpsc, insert-ack oneshots, the drain loop,
  and the missed-wakeup registration dance.
- The `Bypass` / `Queue` / `Disabled` routing trichotomy and failover detection.
  There is no actor that can get stuck independently of consensus, so there is
  nothing to fail over from; every submission takes the same path.
- `submit_semaphore` / `max_pending_local_submissions`. The pull model self-limits:
  consensus takes exactly what fits in blocks, bounded by the inflight budget.
- The `submit_inner` retry loop with exponential backoff. There is no submission
  RPC to fail; reconfig-window submissions simply wait in the pool (see
  [Epoch rotation](#epoch-rotation)).
- On the consensus side (once the pool is the only path):
  `TransactionClient`, `TransactionConsumer`, `TransactionsGuard`, the 2,000-entry
  submission channel, and the `block_status_subscribers` oneshot registry.

## Overview

```
gRPC SubmitTx                          system messages (checkpoint sigs, EoP, DKG, …)
     │                                                    │
ValidatorService::handle_submit_transaction               │
     │  per-tx validation, dedup, voting (unchanged)      │
     ▼                                                    ▼
SuiTransactionPool::submit ◄──────────────────────────────┘
     │  insert into pending (priority: system > gas price desc, FIFO within price)
     │  full → gas auction: evict lowest or reject with outbid error
     │
     │            consensus proposer (core thread)
     │                 │ take(max_count, max_bytes)   ── pull, replaces consumer.next()
     ├────────────────►│
     │                 │ block created & flushed
     │                 │ ack(block_ref)
     │◄────────────────┘
     │  entries → inflight[round]; positions → waiters   ← RPC finishes here
     │
     │            commit handler / checkpoint executor (push callbacks)
     │◄── note_statuses(position, Finalized|Rejected|Dropped)
     │◄── note_processed(consensus tx keys)               ── any validator's block
     │◄── note_executed_in_checkpoint(digests)
     │◄── notify_committed(own blocks, gc_round)          ── GC → requeue system entries
     ▼
   entry removed from maps (capacity freed structurally)
```

## Data structures

One `Mutex<PoolInner>`; every transition is a short synchronous critical section.
The lock is never held across an await and no callback under the lock calls back
out of the pool.

```rust
struct PoolInner {
    /// Transactions waiting to be proposed, in take order.
    pending: BTreeMap<PoolKey, Arc<PoolEntry>>,
    /// Transactions in our own proposed blocks, awaiting terminal outcome.
    /// BlockRef orders round-first, so GC is a front-pop while
    /// `key.round <= gc_round` — the same pattern as today's
    /// TransactionConsumer::block_status_subscribers. Each bucket records
    /// whether the block was sequenced and each entry's first block index,
    /// for matching position-keyed status updates.
    inflight: BTreeMap<BlockRef, ProposedBlock>, // { sequenced, Vec<(first_index, Arc<PoolEntry>)> }
    /// Ack/dedup index over both maps, mapping each key to its entry and the
    /// transaction's index within the entry. For user transactions the key is
    /// ConsensusTransactionKey::Certificate(digest), so this is the
    /// digest-keyed acknowledgment map; keying by ConsensusTransactionKey
    /// additionally covers system messages (which have no digest).
    /// First-writer-wins: an entry admitted while its key is already mapped
    /// (partial bundle overlap) settles via its own position statuses instead.
    by_key: HashMap<ConsensusTransactionKey, (Arc<PoolEntry>, usize)>,
    next_seq: u64,
    /// Per-transaction occupancy counters (an entry contributes its bundle
    /// size): `pending` for the capacity/auction bound, staged+proposed for
    /// the inflight take budget.
    pending_user_txs: usize,
    inflight_user_txs: usize,
    shutdown: bool,
}
```

**`PoolKey`** ranks entries; `BTreeMap` ascending order *is* take order:

```rust
struct PoolKey {
    class: PriorityClass, // System < User — system messages always first
    price: Reverse<u64>,  // user gas price, descending; system entries use 0
    seq: u64,             // insertion counter: FIFO within a price, unique keys
}
```

Extensible by construction: future criteria (fee-per-byte, age boost, client
class) slot in as comparison fields. The eviction victim is the *oldest entry at
the lowest price* — the last price band's smallest `seq`, one range query —
matching today's queue. System entries are never evicted and don't count against
capacity (small, bounded population).

**`PoolEntry`** is one submission group — a single transaction, or a
soft bundle that must land in one block (atomicity is preserved because `take`
moves whole entries, exactly like today's all-or-nothing `TransactionsGuard`):

```rust
struct PoolEntry {
    transactions: Vec<ConsensusTransaction>,
    serialized: Vec<Vec<u8>>,    // BCS cached at insert; take() never serializes
    keys: Vec<ConsensusTransactionKey>,
    gas_price: u64,              // min across the bundle; 0 for gasless
    kind: EntryKind,             // User | System | BestEffort{deadline} | Ping
    tx_type: &'static str,       // metrics label
    pool_key: PoolKey,           // fixed at insert; retained across GC requeue
    submit_time: Instant,
    mutable: Mutex<EntryMut>,    // locked only under the pool mutex
}

struct EntryMut {
    state: EntryState,
    /// All coalesced waiters receive the same positions (or error).
    waiters: Vec<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
    /// Per-transaction terminal-signal flags + countdown.
    unsettled: Vec<bool>,
    unsettled_count: usize,
    /// One element per (coalesced) submission, drained for DoS accounting at
    /// take time.
    unrecorded_addrs: Vec<Option<IpAddr>>,
    dos_recorded: bool,
}

enum EntryState {
    Pending,
    Staged,                       // taken, awaiting ack(block_ref)
    Proposed { block_ref: BlockRef, first_index: TransactionIndex },
    Settled,
}

// Settle reasons (waiter errors, metric labels, logs): Processed | Status(_)
// | CheckpointExecuted | GarbageCollected | Evicted | Shutdown
// | BestEffortExpired | Sweep.
```

Entries are `Arc`-shared across the maps; their mutable state sits behind a
per-entry mutex locked only while holding the pool mutex (a fixed lock order, so
no inversion is possible). State transitions are idempotent (first terminal
transition wins), so signals arriving in any order — status before GC
notification, processed-key before ack — are safe.

## Consensus integration

The `TransactionPool` trait lives in `consensus-core`, mirroring
`TransactionVerifier` exactly: defined next to it in `transaction.rs`,
`Arc<dyn _>` through `ConsensusAuthority::start` → `AuthorityNode::start`, with
sui-core's `SuiTransactionPool` implementing it the way `SuiTxValidator`
implements `TransactionVerifier`. It deals only in consensus types — bytes, `BlockRef`,
`Round` — keeping consensus-core Sui-agnostic:

```rust
pub trait TransactionPool: Send + Sync + 'static {
    /// Called by the proposer while building a block. Returns serialized
    /// transactions in block order, an ack invoked with the created block's
    /// ref after the block is durably created, and which limit stopped the
    /// take — the same contract as today's consumer `next()`. Dropping the ack
    /// without calling it returns the staged entries to the pool — any
    /// proposer error path is safe by construction.
    fn take(
        &self,
        max_count: usize,       // protocol max_num_transactions_in_block
        max_bytes: usize,       // protocol max_transactions_in_block_bytes
    ) -> (Vec<Transaction>, Box<dyn FnOnce(BlockRef) + Send>, LimitReached);

    /// Called from Core::try_commit with own committed block refs and the
    /// current GC round — the same signal that today feeds
    /// TransactionConsumer::notify_own_blocks_status.
    fn notify_committed(&self, own_committed: Vec<BlockRef>, gc_round: Round);
}
```

Proposer changes are minimal: `try_new_block` calls `take` where it called
`transaction_consumer.next()` and invokes the ack in the same place; `try_commit`
routes its existing own-blocks/GC notification to `notify_committed`. During
migration a shim implements the trait on top of the existing
`TransactionConsumer`, so the proposer code is unconditional and the legacy
pipeline keeps working unmodified.

No proposal wake-up is needed: block proposal is driven by round advance and
leader timeouts, never by transaction arrival — a newly inserted entry is picked
up at the next `try_new_block`, exactly like a transaction sitting in today's
consumer channel.

### Lifetimes and wiring

Two layers, mirroring `AdmissionQueueContext` over per-epoch actors:

- **`SuiTransactionPool`** is per-epoch. Created in
  `start_epoch_specific_validator_components` alongside `SuiTxValidator` and
  passed into `ConsensusManager::start` → `ConsensusAuthority::start` as the
  `Arc<dyn TransactionPool>` trait object. The per-epoch `ConsensusCommitHandler` (via
  `ConsensusHandlerInitializer`) and the checkpoint executor hold an `Arc` for the
  settle callbacks.
- **`TransactionPoolContext`** is long-lived (like `ConsensusAdapter` /
  `AdmissionQueueContext`): an `ArcSwap<Arc<SuiTransactionPool>>` rotated at
  reconfig. It carries the sui-core-facing entry points, including the
  `SubmitToConsensus` impl held by `RandomnessManager`,
  `SubmitCheckpointToConsensus`, etc. Calls verify the caller's epoch against the
  current pool's epoch and reject mismatches with `ValidatorHaltedAtEpochEnd`.

Why per-epoch instances rather than one pool with an internal `clear()`: a fresh
instance makes "no state survives the epoch" structural instead of a `clear()`
that must stay exhaustive, and it makes boundary races benign: late callbacks
from the old core thread or commit handler land on the old, inert pool object —
whose `inflight` keys and `gc_round` arithmetic are only coherent within one
consensus instance — rather than mutating new-epoch state. It also lets the pool
hold its `Arc<AuthorityPerEpochStore>` as a plain field (released when the pool
drops), with a single epoch check at the context boundary — versus the
cross-epoch `ConsensusAdapter`, which pays with an `epoch_store` parameter on
every call and `within_alive_epoch` on every task. The admission queue's
per-epoch actor exists for the same reason. Carrying pending user entries across
the boundary would be possible (ranking by absolute gas price stays valid under
an RGP change) but is deliberately not done: dropping everything keeps rotation
trivial, and clients already own retries.

## Lifecycle

### Insert

`TransactionPoolContext::submit_and_get_positions(transactions, gas_price,
epoch_store, submitter_client_addr)` — same contract and callers as the adapter's
method (the RPC layer passes the group's minimum gas price, as it did for the
queue); the queue's `try_insert` collapses into it.

1. **Already-processed short-circuit** (unchanged semantics, RPC thread): the
   adapter's `check_processed_via_consensus_or_checkpoint` — consensus-processed
   keys, checkpoint-executed digests, synced-checkpoint sequence for signature
   messages. Everything processed → retriable `TransactionProcessing`, nothing
   inserted. The same check guards **system** submissions
   (`submit_to_consensus` / `submit_best_effort`), where it is a correctness
   requirement, not just an optimization: system messages get no per-position
   statuses and a processed key is only recorded once, so an already-processed
   system message admitted to the pool would never settle.
2. **Dedup by coalescing**: when the submission's key set exactly matches an
   existing entry's, the caller's waiter (and client addr) attaches to that entry
   instead of admitting a copy — both waiters get the same positions, and if the
   entry is already `Proposed` the known positions are returned immediately. A
   *partial* key overlap (a bundle sharing only some keys with a resident entry)
   is admitted as a separate entry, like today's queue duplicates. Either way the
   insert reports `newly_inserted = false`, which the RPC layer converts to spam
   weight exactly as `duplicate_at_admission` does today. Coalescing is
   *stronger* than the queue (which flags duplicates but still admits them
   toward consensus) while preserving the accounting.
3. **Capacity / gas auction** (user entries only, unchanged semantics): while the
   pool is full, evict the oldest entry at the lowest price if that price is
   strictly below the newcomer's, failing its waiters with
   `TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price }`;
   if eviction cannot free enough capacity, reject the newcomer with the same
   error carrying the current min. Gas price is the minimum across the bundle, 0
   for gasless — gasless remain the designated first victims.
4. Insert into `pending` + `by_key`; BCS bytes cached now so `take` does no
   serialization on the core thread. The caller awaits its oneshot.
5. `shutdown` set → `ValidatorHaltedAtEpochEnd` (the RPC handler's existing outer
   retry waits for the next epoch).

System entries skip 3 (never evicted, never counted against capacity) and attach
no waiter. Pings insert a payload-less `Ping` entry at system priority.

### Take and ack

`take(max_count, max_bytes)` runs on the consensus core thread; it must stay
allocation-light and do **no DB reads or serialization**:

1. Effective count budget = `min(max_count, max_pending_transactions −
   inflight_transaction_count)`, where the inflight count is per *transaction*
   (an entry contributes its bundle size; staged and proposed entries both
   count), maintained as a counter under the pool lock. This is the same
   backpressure the queue's drain applies today: when settlement lags, blocks
   stop filling with user transactions. System entries are exempt from the
   budget.
2. Walk `pending` in `PoolKey` order, moving whole entries (bundle atomicity)
   until the next entry would exceed either budget — stop rather than skip ahead,
   preserving strict price order, exactly like today's consumer parking a
   non-fitting guard.
   - `BestEffort` entries past their deadline are settled (`BestEffortExpired`)
     and skipped. (Checkpoint-signature redundancy against synced checkpoints is
     handled at insert time — see Insert step 1 — keeping `take` free of store
     reads.)
3. Taken entries → `Staged`; DoS amplification is recorded per user transaction
   (`submitted_transaction_cache.record_submitted_tx`, in-memory LRU) at this
   point — the same "just before actual submission" point as today, so evicted
   entries never consume DoS allowance.
4. The ack closure captures the staged entries. On `ack(block_ref)`: entries →
   `inflight[block_ref]` as `Proposed`, indices assigned by block order,
   and every waiter receives its `Vec<ConsensusPosition>` — the RPC request is
   done here, as today. Pings get `ConsensusPosition::ping(epoch, block_ref)`. If
   the ack is dropped un-called (proposer error path, shutdown), a drop guard
   returns staged entries to `pending`.

### Settlement

Settlement callbacks are synchronous batched pushes from the code that already
computes the outcomes. Signal coverage (see `consensus_handler.rs::
filter_consensus_txns`) dictates that **no single signal suffices** — in
particular, vote-rejected transactions are *never* marked
consensus-message-processed, and system messages never get per-position statuses:

| Signal | Callback (called from) | Keyed by | Settles |
|---|---|---|---|
| Per-position status `Finalized` / `Rejected` / `Dropped` | `note_statuses` — commit handler, same site as `set_consensus_tx_statuses` | `ConsensusPosition`: direct lookup `inflight[position.block]` + index | User entries in our own blocks. **Sole signal for vote-rejected and for certs-closed / post-EoP drops.** |
| Consensus key processed | `note_processed` — commit handler, same site as `record_consensus_message_processed` / `process_notifications` | `ConsensusTransactionKey` → `by_key` | Accepted user txs from **any validator's block** (settles even `Pending` entries — answer early), and **the sole signal for system messages**. |
| Executed in checkpoint | `note_executed_in_checkpoint` — checkpoint executor's `insert_finalized_transactions` | digest → `Certificate` key → `by_key` | Entries whose transaction executed via a locally built or state-synced checkpoint. |
| Own block committed / GC round advanced | `notify_committed` — `Core::try_commit` (the trait method) | `BlockRef` keys (round-first order) | GC: for un-sequenced blocks with `round <= gc_round`, system entries requeue to `pending`; user entries settle (below). |
| Epoch end | `shutdown()` — reconfig | everything | All waiters fail with `ValidatorHaltedAtEpochEnd`; maps drained. |

Rules on settle:

- A bundle entry settles when **all** its keys/positions have a terminal signal
  (`unsettled` countdown) — mirrors today's per-key `FuturesUnordered` wait.
- A `Pending`/`Staged` entry settled by `note_processed` or checkpoint execution
  whose waiters never got positions receives the retriable
  `TransactionProcessing` error — identical to today's `processed_notify` win.
- A status for a *different* position of the same digest (someone else's block)
  does **not** settle our entry: our copy may still be voted differently. Only the
  digest-level "processed" signal (which only fires for accepted transactions) or
  our own position's status settles.
- Settle removes the entry from all maps — capacity frees structurally; there is
  no counter to leak (principle 1 satisfied by construction, with the release
  paths reduced from "every exit of a spawned task" to "one `settle` function").

Because outcomes are pushed at commit-processing time rather than pulled from the
status cache, the **status-expiry settle path disappears**: the 400-round
`ConsensusTxStatusCache` retention still serves `WaitForEffects` clients, but the
pool cannot "miss" a status. A defensive sweep (`debug_fatal!` + settle) reaps any
entry still unsettled under a *sequenced* block far below the GC round, so a
coverage bug degrades to a metric, not a capacity leak.

### Garbage collection and resubmission

`notify_committed(own_committed, gc_round)`:

- Committed own blocks are marked sequenced (direct `inflight` lookup); their
  entries settle via the commit handler's status/processed pushes for that same
  commit (either ordering of the two signals is fine — settle is idempotent).
- Un-sequenced blocks with `round <= gc_round` (a front-pop, since `BlockRef`
  orders round-first) will never commit. **System entries requeue to `pending`**
  with their original `seq` — the protocol depends on them landing; this
  replaces the adapter's "GC → sleep 1s → resubmit" loop. **User and
  `BestEffort` entries settle** (`GarbageCollected`): their positions were
  already delivered, and a GC'd position never receives a status, so the client
  observes `Expired` via `WaitForEffects` (or hits its own deadline) and
  resubmits.
- Delta vs today: the adapter internally resubmits GC'd *user* transactions
  until they land; the pool does not — principle 5 taken at face value (clients
  own user-transaction retries), which also keeps the user-entry lifecycle
  linear (`Pending → Staged → Proposed → Settled`, never backwards). Own-block
  GC is rare — the proposer's block must miss the DAG past the GC horizon — so
  the cost is an occasional client-driven resubmit.

### Epoch rotation

Consensus is torn down and rebuilt per epoch; the pool follows:

1. Reconfig calls `TransactionPoolContext::rotate_for_epoch(new_epoch_store)`:
   the old pool's `shutdown()` drains all maps and fails every waiter with
   `ValidatorHaltedAtEpochEnd` (explicit, actionable — an upgrade over today's
   dropped-oneshot → `TooManyTransactionsPendingConsensus` mapping), and the
   context swaps in a fresh pool bound to the new epoch.
2. The new pool is handed to `ConsensusManager::start`. Submissions arriving
   before the new consensus instance begins taking simply wait in `pending` —
   replacing today's reconfig-window backoff-retry loop inside `submit_inner`.
3. Old consensus's core thread stops before rotation, so no stale `take`/
   `notify_committed` can reach the new pool. Un-settled system messages are
   regenerated by their existing recovery paths (`recover_end_of_publish`,
   checkpoint-signature resubmission), as with today's cancelled tasks.

## Entry-point parity

| Today | Pool |
|---|---|
| `ConsensusAdapter::submit_and_get_positions(txns, epoch_store, addr)` | `TransactionPoolContext::submit_and_get_positions` — same contract and callers, with the group's min gas price passed explicitly |
| `AdmissionQueueHandle::try_insert(gas_price, txns, addr)` → `(rx, newly_inserted)` | Folded into the same submit path; gas price computed internally; `newly_inserted` preserved for spam weight |
| `classify_submit_mode` → Bypass / Queue / Disabled | Deleted — one path. Node config selects pool vs. legacy pipeline during rollout |
| `SubmitToConsensus::submit_to_consensus` (checkpoint sigs, DKG) | Implemented by `TransactionPoolContext`: insert system entry; retained until processed (GC-requeue) — same "retried until sequenced or epoch end" guarantee |
| `SubmitToConsensus::submit_best_effort` (execution-time observations) | Insert `BestEffort{deadline}` entry: no GC-requeue, lazily dropped past deadline; returns immediately as today |
| `ConsensusAdapter::submit` (EndOfPublish, capabilities, JWK) | Same messages via context system-submit |
| `recover_end_of_publish` | Unchanged, routed to the pool |
| `check_consensus_overload` (`TooManyTransactionsPendingConsensus`) | `TransactionPoolContext::check_overload()` over structural counts |
| Ping requests | `Ping` entry; position `ConsensusPosition::ping` at next ack |

`authority_server.rs` upstream is unchanged: the validation gauntlet,
`InflightTransactionsGuard` + TTL dedup, grouping, `SubmitTxResult` mapping, and
the `ValidatorHaltedAtEpochEnd` outer retry all stay. (TODO: revisit the RPC
layer's `InflightTransactionsGuard` + TTL cache after rollout — the pool's
`by_key` coalescing likely subsumes them for the submit path.) `WaitForEffects`
is untouched — the status cache and reject-reason cache continue to be populated
by the commit handler for clients regardless of the submission pipeline.

## Design principles, restated for the pool

1. **Leak-free accounting** — structural instead of RAII: capacity *is* map
   membership under one lock; a single `settle` function is the only release
   path. Admission remains deliberately weak (drain-budget overshoot for bundles
   self-corrects, as today).
2. **Answer early** — insert-time processed check, plus push-settlement that
   reaches even `Pending` entries the moment another validator's commit or a
   checkpoint covers them; they are never pointlessly proposed.
3. **Dedup at every layer** — upstream layers unchanged; the pool layer upgrades
   flag-and-admit to coalescing, with the duplicate still tallied as spam weight.
4. **Dedup as optimization, not correctness** — unchanged; downstream consensus
   handling stays idempotent and clients own transaction-level retries.
5. **Only system transactions are owed retries** — system entries are never
   evicted and are GC-requeued until processed; user entries get no internal
   retries at all: every outcome, including GC, settles the entry and the
   client resubmits.
6. **Every submission ends in one explicit outcome** — same error taxonomy;
   eviction keeps its outbid error; epoch-end becomes an explicit
   `ValidatorHaltedAtEpochEnd` instead of a mapped channel error.
7. **Prefer higher gas price** — one ordering (`PoolKey`) now drives admission,
   eviction, *and* block inclusion, instead of a queue order plus a drain order.
8. **System messages never blocked behind user traffic** — top priority class,
   capacity-exempt, budget-exempt; and with no semaphore there is no permit to
   wait on at all.

## Backpressure and limits

- `max_pending_transactions` (default 20,000) keeps both roles, as two
  **independent** bounds: pool `pending` capacity (the auction bound) and the
  inflight take budget. Both are counted per *transaction* — an entry
  contributes its bundle size — via counters maintained under the pool lock
  (today's analogues split this: the queue counts per entry, the inflight guard
  per transaction). They are deliberately not one joint bound: settle lag
  filling `inflight` must not shrink the auction space — holding the overflow
  *is* `pending`'s job — it just stops `take` from moving entries onward. With
  Bypass gone the pending capacity absorbs the full stream, so the default is
  the full value rather than the queue's 0.5×; total resident transactions are
  bounded by 2× the knob, comparable to today's queue + inflight.
- Deleted knobs: `admission_queue_bypass_fraction`,
  `admission_queue_capacity_fraction`, `admission_queue_failover_timeout`,
  `max_pending_local_submissions`, and the actor channel size. New knob:
  `transaction_pool_enabled` (mutually exclusive with `admission_queue_enabled`).
- Saturation semantics are unchanged from Queue mode: capacity converts to a
  gas-price auction; outright rejection only when outbid.

## Observability

### Metrics

The pool **reuses `ConsensusAdapterMetrics`** (the same registered instance, shared
with the adapter) wherever the semantics carry over, so existing dashboards and
alerts keep working across the migration:

| Reused adapter metric | Pool semantics |
|---|---|
| `sequencing_certificate_attempt{tx_type}` | incremented at insert |
| `sequencing_certificate_inflight{tx_type}` | entries resident between insert and settle |
| `sequencing_certificate_success` / `_failures{tx_type}` | settle outcome: done vs. evicted/shutdown/expired |
| `sequencing_certificate_status{tx_type, status}` | own-block `Sequenced` / `GarbageCollected` from `notify_committed` |
| `sequencing_certificate_settled_status{tx_type, status}` | terminal `Finalized` / `Rejected` / `Dropped` from `note_statuses` |
| `sequencing_certificate_latency{submitted, tx_type, processed_method}` | insert → settle; `processed_method` = settle reason |
| `sequencing_acknowledge_latency{retry, tx_type}` | insert → position ack (block inclusion); `retry` always `"false"` |
| `sequencing_certificate_processed{source}` | settle via consensus vs. checkpoint, as today |
| `sequencing_best_effort_timeout{tx_type}` | best-effort entry expired before being taken |
| `consensus_latency` | `submit_and_get_positions` → position return, as today |

Metrics with no pool analogue are left to the adapter and retire with it:
`sequencing_in_flight_submissions`, `sequencing_in_flight_semaphore_wait`, and the
`admission_queue_*` family. Pool-specific state gets new metrics:
`transaction_pool_pending` / `transaction_pool_inflight` (per-transaction gauges),
`transaction_pool_wait_latency` (insert → take), `transaction_pool_evictions` /
`_rejections` / `_coalesced_inserts` (auction and dedup outcomes), and
`transaction_pool_gc_requeues` (system entries re-queued after own-block GC).

### Logging

Every major lifecycle event is logged with the entry's transaction keys so one
transaction can be traced through the pool. Per-transaction events on the hot path
log at `debug!`; client-visible or abnormal events at `info!`/`warn!`:

- **Entrance**: `debug!` per insert — keys, kind, gas price, and whether it
  coalesced onto an existing entry.
- **Eviction / outbid rejection**: `info!` — the evicted (or rejected) keys and
  both gas prices; these are client-visible auction outcomes.
- **Submitted to consensus**: `debug!` at ack, one line per proposed block — block
  ref, entry count, and index range (`take`/ack run on the core thread, so no
  per-entry lines there).
- **Exit**: `debug!` per settle — keys, settle reason (processed / status /
  checkpoint-executed / GC-drop), and pool residence time. `warn!` when the
  defensive sweep settles an entry the status callbacks missed.
- **Epoch rotation / shutdown**: `info!` summary with counts of entries drained.

## Behavior deltas (intentional)

- **Duplicates coalesce** instead of being admitted alongside the original:
  strictly less consensus load; a duplicate of a `Proposed` entry now gets the
  real position instead of a second submission or an error.
- **GC'd user transactions are dropped, not resubmitted.** The adapter retries
  them internally after garbage collection; the pool settles them and the
  client resubmits (the position surfaces as `Expired` through
  `WaitForEffects`). Only system entries requeue.
- **No status-expiry outcome for the pool** (push vs. pull); `Expired` remains a
  client-visible `WaitForEffects` outcome only.
- **Reconfig-window submissions wait in the pool** rather than erroring or
  backoff-retrying against a cleared client.
- **The adapter's per-settle `EndOfPublish` epilogue is not carried over.**
  Timestamp-based epoch close is now the default, so the pool submits no
  `EndOfPublish` of its own. The remaining triggers (`close_epoch`,
  `recover_end_of_publish`, the consensus handler's
  `send_end_of_publish_if_needed`) are unchanged and route through the pool as
  ordinary system entries.
- **Bypass mode's zero-queue hop is gone**: every transaction pays one mutex'd
  map insert. This replaces a bounded-channel send + actor hop; below-threshold
  latency is expected to be equal or better (one fewer task handoff), to be
  confirmed by benchmark.

## Risks

- **Core-thread coupling**: `take` and `ack` run on the consensus core thread.
  Mitigations: cached serialization, no DB access in `take`, short critical
  sections. Watch proposer latency metrics; the mutex can be split (pending vs.
  settle-index) if insert contention shows up.
- **Settle-coverage completeness** is the correctness crux: every user
  transaction in a committed block receives exactly one terminal status in
  `filter_consensus_txns`, and every surviving or explicitly-dropped key is
  recorded processed — the callback sites piggyback on those exact points, and
  the sequenced-bucket sweep catches any future gap. New drop paths added to the
  commit handler must keep the pool callbacks in sync (extend the
  keep-in-sync mandate of `consensus_submission_pipeline.md` to this file).
- **Lock discipline**: commit handler and checkpoint executor call into the pool;
  the pool must never synchronously call back into them (it doesn't — its only
  outbound effects are oneshot sends and metric bumps, performed after the lock
  is released where practical).
- **Crash/amnesia**: after restart the pool is empty; pre-crash proposed blocks
  may still commit — harmless (no waiters; commit-handler processing is
  idempotent), identical to today.

## Rollout

1. Land the `TransactionPool` trait in consensus-core with a shim impl over the
   existing `TransactionConsumer`; proposer/core call sites switch to the trait
   unconditionally. No behavior change.
2. Land `transaction_pool.rs` + `TransactionPoolContext` behind
   `transaction_pool_enabled` (default off). `ValidatorService` and the system
   submitters route by config to pool or legacy queue+adapter.
3. Parity validation: e2e + simtests for both configurations; benchmark
   submission latency (p50/p99 position latency below and above saturation) and
   congestion behavior (auction fairness, eviction rates).
4. Default on; delete the admission queue, the adapter's submission half,
   `TransactionClient`/`TransactionConsumer`, and the legacy knobs.
