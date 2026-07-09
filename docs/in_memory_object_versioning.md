# In-memory object versioning — replacing the lock/marker tables with the objects table

Proposal: eliminate `live_owned_object_markers` (perpetual DB) and
`owned_object_locked_transactions` (epoch DB), and — as a follow-up — the
`next_shared_object_versions_v2` table, replacing all three with in-memory state whose
single durable source of truth is the perpetual `objects` table. Untracked / scratch.

Companion docs: `docs/consensus-handler-optimization.md` (the post-consensus hot path this
touches), `docs/tidehunter-pt-load-investigation.md` (why table count and key count matter
for tidehunter).

**Implementation status (2026-07-08, perf-eval branch):** all three tables are removed
outright, with the new checks unconditional — the flag-gated migration path described in
§10 is deliberately skipped for performance evaluation. Phase 1+2: liveness and
post-consensus conflict detection (consumed-check + executed-in-epoch exemption) run
against the objects table; lock state is quarantine-only; both lock tables deleted, with
deferred transactions' locks retained past flush and rebuilt from the deferral table at
construction (I1's deferral carve-out). Phase 3: `next_shared_object_versions_v2`
deleted; the next-version map is epoch-lifetime in-memory (quarantine), seeded lazily
from the objects table with **effects-aware seeding** (§6.3) during replay, plus a
debug-build assertion that recomputed assignments for executed transactions match their
effects. One durable exception survives (simtest-driven, see I1b): a ≤3-row
`system_object_next_versions` table holding the watermark values of the singleton system
objects (Clock, randomness state, accumulator root) — seeded at epoch-store creation,
updated per flush. These are the only keys whose mutators (prologues, settlement
barriers) are not digest-resolvable from a node whose checkpoint executor ran ahead of
its consensus, and which assignments read in batches containing none of their mutators;
object-chain walk-back was tried and rejected (pruning cuts the chain, round ties are
ambiguous). Not yet implemented: overlay eviction for the next-version map (§4.3),
cleanup of pre-existing marker CFs on old DBs, and the §11 crash-matrix simtests.

**Post-review direction (2026-07-09, IMPLEMENTED):** see **§13** — the review findings
concentrate entirely in the two epoch-table removals, and three moves close all of them:
revert Component C (the next-version table is the one irreducibly durable piece; §13.2),
fix the C5 exemption via the pre-publish executed mark instead of Design A (§13.3), and
enforce immutable-claim completeness at vote (§13.4). Components A and B stand. All
three moves are now implemented on the branch; **Phase 3 above no longer describes the
branch** — `next_shared_object_versions_v2` is durable again (init pins + flush writes,
main's model), and §4.3/§6.3's reconstruction machinery is deleted. §4.3, §6.3, and
§7.4 are retained as the record of why reconstruction was abandoned.

---

## 1. Executive summary

All three tables are **derived state**. Each one is a materialization of information that
is either (a) already present in the `objects` table for everything that has durably
executed, or (b) deterministically re-derivable by consensus replay for everything that
has not. The system already half-acknowledges this:

- Pre-consensus vote validation (`validate_owned_object_versions`,
  `execution_cache/object_locks.rs:92`) checks owned-input liveness against the **objects
  cache/table**, not the marker table.
- The writeback cache answers `check_owned_objects_are_live` from the **object-by-id
  cache** on a hit; the marker table is only a cold-miss fallback
  (`writeback_cache.rs:1951-1980`).
- Formal snapshot restore **rebuilds** the marker table from the live object set
  (`sui-snapshot/src/reader.rs:552` → `bulk_insert_live_objects`) — direct evidence it is
  a pure function of `objects`.
- `next_shared_object_versions_v2` is already **lazily initialized from the objects
  table** at first touch per epoch (`get_or_init_next_object_versions`,
  `authority_per_epoch_store.rs:2078-2161`).
- The unflushed portion of the lock and next-version tables already lives **in memory**
  in `ConsensusOutputQuarantine` (`consensus_quarantine.rs:495,506`); the DB tables exist
  only as the crash-recovery image of the flushed portion.

The core insight that makes full removal possible is an ordering invariant that already
holds today (§3, I1): **consensus output is flushed to the epoch DB only after the
corresponding transactions' execution outputs are durably committed to the perpetual
DB.** Therefore everything the flushed lock/version rows say is *also* said by the
objects table, and the unflushed remainder is exactly what consensus replay
reconstructs. The tables carry zero information that survives where the objects table
does not.

The proposed shape (matching the "memory-bound cache warmed from the objects table"
intuition, but with a sharpened correctness boundary):

| Piece | Nature | Bound | Lossy? |
|---|---|---|---|
| **Latest-ref cache** (`object_by_id_cache`, exists today) | performance cache over `objects` | configured capacity (moka) | yes — miss falls back to a reverse seek on `objects` |
| **Lock overlay** (quarantine `owned_object_locks`, exists today) | authoritative delta: locks of finalized txs whose commits are not yet flushed | quarantine window | **no** — entries removed only at flush, when the objects table has subsumed them |
| **Next-version overlay** (quarantine `shared_object_next_versions` + new init pins) | authoritative delta: shared-version state not yet subsumed by durable execution | quarantine window + pinned inits | **no** — same rule |

Nothing new is persisted. Crash recovery = consensus replay (existing mechanism) + one
new rule for transactions that were already executed before the crash (§5.3, §6.3).

Expected wins (§9): removal of the single largest-key-count keyspace in the perpetual
tidehunter store (one 80-byte key per live owned object; in-memory index, 16× mutex
multiplier, bloom filter), removal of two epoch-DB keyspaces, two fewer writes per owned
object per transaction in the checkpoint-execution commit batch, ~half the write volume
for owned objects during formal snapshot restore, and a simpler post-consensus handler.

---

## 2. Current state

### 2.1 `live_owned_object_markers` (perpetual, on-disk CF `owned_object_transaction_locks`)

`DBMap<ObjectRef, Option<LockDetailsWrapperDeprecated>>`, `authority_store_tables.rs:81`.
The value is **always `None`** since locks moved to the epoch table; the table is purely
the *set* of live address-owned object refs.

Writers:
- `write_one_transaction_outputs` (`authority_store.rs:836,840`): per executed
  transaction, insert markers for `new_locks_to_init` (= written objects with
  `Owner::AddressOwner`, `transaction_outputs.rs:164-173`) and delete markers for
  `locks_to_delete` (= address-owned mutable inputs + actually-received objects,
  `transaction_outputs.rs:156-162`). These land in the checkpoint-executor batch, i.e.
  the table always reflects exactly the checkpointed state.
- Genesis (`insert_object_direct`, `bulk_insert_genesis_objects`) and formal snapshot
  restore (`insert_objects_chunk`, `authority_store.rs:675-682`) — note these use a
  *different* predicate (`!is_child_object()`, so shared/immutable objects also get
  markers at genesis). The invariant is not even clean today.
- Never pruned; not referenced by `AuthorityStorePruner`.

Readers (all behind the writeback cache; the table is only hit on a by-id cache miss):
- `check_owned_objects_are_live` fallback (`authority_store.rs:927-945`) — used by the
  pre-execution guard `check_owned_locks` (`authority.rs:1666-1669`, called from
  `execute_certificate:1969`). Trait comment already says it "could potentially be
  deleted or changed to a debug_assert".
- `get_lock` / `get_latest_live_version_for_object_id` (`authority_store.rs:861-921`) —
  **dead in production**: the only caller of `ObjectCacheRead::get_lock` is unit tests;
  no RPC, orchestrator, or quorum-driver path reads `ObjectLockStatus`.

Tidehunter cost: `KeyIndexing::key_reduction` over 80-byte keys, `mutexes * 16` (the
highest multiplier of any keyspace), bloom filter (0.001, 32k), one index entry per live
owned object — this is a large in-memory index that exists only to answer a question the
`objects` keyspace can answer.

### 2.2 `owned_object_locked_transactions` (epoch DB)

`DBMap<ObjectRef, LockDetailsWrapper>` (`LockDetails = TransactionDigest`),
`authority_per_epoch_store.rs:430`.

Lifecycle:
- **Written only** by `ConsensusCommitOutput::write_to_batch`
  (`consensus_quarantine.rs:309-316`) when the quarantine flushes a commit. Never
  deleted within an epoch; the whole epoch directory is dropped by
  `AuthorityPerEpochStorePruner` after reconfig.
- Locks are **acquired post-consensus** in `filter_consensus_txns`
  (`consensus_handler.rs:2686-2740`): for each finalized `UserTransactionV2`,
  `owned_object_refs_to_lock` (`consensus_handler.rs:3206`) = all
  `ImmOrOwnedMoveObject` inputs not claimed immutable — **including gas**, excluding
  receiving refs, shared/consensus objects, packages.
  `try_acquire_owned_object_locks_post_consensus`
  (`authority_per_epoch_store.rs:1825-1861`) drops the tx (deterministically, on every
  validator) iff a claimed ref is locked to a *different* digest by (1) an earlier tx in
  the same commit or (2) `existing_locks` — the union of quarantine overlay and DB table,
  prefetched once per commit (`consensus_handler.rs:2476-2495`).

Readers:
- The per-commit prefetch above (the consensus-critical read).
- Submit-path pre-screen (`authority_server.rs:989-1028`): converts a would-be-retriable
  submit into a terminal `ObjectLockConflict{pending_transaction}` for equivocation
  losers. Advisory (read-only); a follow-up `handle_vote_transaction` catches
  already-executed winners via the objects table.
- The dead `get_lock` path.

### 2.3 `next_shared_object_versions_v2` (epoch DB)

`DBMap<ConsensusObjectSequenceKey, SequenceNumber>` (`(ObjectID, start_version) → next
input version`), `authority_per_epoch_store.rs:447`.

- **Init**: lazily per key per epoch inside `get_or_init_next_object_versions`
  (`authority_per_epoch_store.rs:2078-2161`), under `version_assignment_mutex_table`.
  Seed = current object version from `cache_reader.get_object` (with
  `owner().start_version()` matching for reshare/party transitions), else the
  certificate's initial version. **The init rows are written to the table immediately**
  (`:2156-2158`), bypassing the quarantine — this pins the pre-mutation seed so that a
  later re-read cannot be contaminated by interleaved execution (see the comment in
  `assign_versions_from_effects`, `shared_object_version_manager.rs:317-323`).
- **Advance**: `assign_versions_from_consensus` advances a working map per commit;
  the result goes into the quarantine (`set_next_shared_object_versions`,
  `consensus_quarantine.rs:191`) and flushes with the commit.
- Readers: `get_next_shared_object_versions` (overlay → DB fallback,
  `consensus_quarantine.rs:774-795`); `assign_shared_object_versions_idempotent`
  (read-only assignment for the end-of-epoch tx, `authority.rs:6166`);
  `assign_versions_from_effects` (checkpoint-executor path — assignments come from
  effects, the init call exists only to pin seeds).

### 2.4 The buffering architecture both sides share

Two watermark-gated buffers, flushed in a fixed order by the checkpoint executor
(`checkpoint_executor/mod.rs:436-457`):

1. `commit_transaction_outputs` — execution outputs (objects, effects, markers) become
   durable in the perpetual DB.
2. `handle_finalized_checkpoint` → `quarantine.update_highest_executed_checkpoint`
   (`authority_per_epoch_store.rs:1980`) — consensus outputs (locks, next-versions,
   `last_consensus_stats_v2`, `consensus_message_processed`) become durable in the epoch
   DB, but only for commits at or below the last drained checkpoint height
   (`consensus_quarantine.rs:636-663`).

On restart, consensus replays every commit above the flushed `last_consensus_stats`
watermark through the full handler (`consensus_manager/mod.rs:302-318`,
`consensus_handler.rs:3159-3168`), re-deriving all quarantined state. At epoch boundary
there is no revert machinery anymore: the writeback dirty set and quarantine are simply
dropped (`writeback_cache.rs:1315-1356`), and the epoch DB dies with its directory.

---

## 3. Invariants the proposal rests on

**I1 — Flush ordering (holds today, with one exception).** A consensus commit's output
(locks, next-versions) is flushed to the epoch DB only after every transaction in that
commit has been executed and its outputs durably committed to the perpetual DB. Proof:
flush is gated on `highest_executed_checkpoint`, which advances only in
`handle_finalized_checkpoint`, which the checkpoint executor calls *after*
`commit_transaction_outputs` (`checkpoint_executor/mod.rs:442` before `:456`); every
finalized tx of a commit is in that commit's checkpoints — **except deferred
transactions** (randomness/congestion deferral): they are finalized and locked in their
commit but scheduled (and checkpointed) only later. Their locks must therefore outlive
the flush and survive restarts explicitly: they are retained in the overlay at flush and
rebuilt from the durable deferral table at epoch-store construction. With that carve-out,
the corollary holds: **flushed lock ⇒ the locked refs are consumed in the durable objects
table** (via I2). Found the hard way — a simtest randomness workload double-spent a
deferred transaction's gas after its lock evaporated at flush.

The retention rule is therefore "**drop a lock at flush iff its holder is executed in the
current epoch**" (`transactions_executed_in_cur_epoch`), *not* "iff its holder is in this
commit's deferred set." The distinction is load-bearing: lock acquisition
(`filter_consensus_txns`) runs *before* deduplication, so a cross-commit duplicate of a
deferred transaction re-records that still-unexecuted transaction's locks into a later
commit whose `deferred_txns` does not list it. Keying removal on the output-local deferred
set would drop those locks at the later commit's flush while the holder is still finalized-
but-unexecuted, letting a conflicting transaction pass the lock maps and the consumed-check
(the ref is still live) and double-spend it. Keying on execution state closes this: an
unexecuted holder's locks are retained no matter which commit recorded them.

**I1b — Checkpoint execution can run ahead of consensus replay.** After a restart, the
checkpoint executor may execute state-synced certified checkpoints containing
transactions from commits that consensus has not yet re-processed. Any in-memory state
consumed by consensus-order processing (the next-version map) must therefore never be
seeded from the checkpoint-execution path: a synced batch can sit at a "future" point of
a version chain relative to the commit replay is about to re-process. This is why
`assign_versions_from_effects` does not touch the next-version map at all, and why the
accumulator-root key (read by every assignment but touched only by settlements) has a
dedicated recovery: the first settlement past the flushed height watermark either has
executed (its recorded input version is the watermark value) or it has not (in which
case no post-watermark settlement has, and the objects-table seed is clean). Found the
hard way too — the §6.3 recomputed-vs-effects assertion caught replayed prologues
assigned Clock versions one ahead of their effects.

**I2 — Every locked ref is consumed by execution.** The post-consensus lock set (all
non-immutable `ImmOrOwnedMoveObject` inputs, §2.2) equals the exclusive-mutable input set
(`InputObjects::exclusive_mutable_inputs`, `transaction.rs:4996` — every non-immutable
owned input, regardless of `&`/`&mut`/by-value usage in the PTB). Execution bumps every
exclusive-mutable input's version even on failure, cancellation, or the
insufficient-funds short-circuit (`ensure_active_inputs_mutated`,
`temporary_store.rs:331`; explicitly "so locks advance" in `execution_engine.rs:418-419`).
So once a finalized tx's outputs are durable, each of its locked refs `(id, v, d)` has a
successor version `> v` in the objects table — **for mutations**. For **deletions and
wraps** the only evidence is a tombstone, and object retention guarantees only the *latest*
version of a live object or a tombstone *until the pruner removes it* — tombstones are not
durably retained. So I2's "consumed ⇒ observable in the objects table" holds unconditionally
for mutated refs (the retained latest live version is strictly `> v`) but **fails for a
deleted/wrapped ref once its tombstone is pruned**: `latest_ref(id)` degrades from
`Tombstone(v')` to `NeverExisted`, which the consumed-check (§4.2, arm at "creator pending")
maps to ACCEPT. This is a known correctness gap — see the note below the consumed-check and
§5.2.

**I3 — Post-consensus decisions are deterministic functions of consensus history.** The
accept/drop decision at a given consensus position must be identical on every honest
validator regardless of its local execution/flush progress. Today this is achieved by
making the decision depend only on the epoch's cumulative lock map. Any replacement must
preserve this (§5.2).

**I4 — Epoch-boundary quiescence.** Reconfiguration completes only after every finalized
tx of the old epoch is checkpointed and its outputs durably committed. So at the first
commit of a new epoch, the durable objects table on every validator reflects *all*
consumptions of all previous epochs.

**I5 — Vote-quorum liveness.** A `UserTransactionV2` is finalized only if a quorum
accepted it, and honest validators vote accept only after
`validate_owned_object_versions` verified each owned input ref was the live version
(exact version + digest) at vote time within this epoch. Hence a finalized tx's owned
refs were live at some point in this epoch — refs consumed in a *prior* epoch, never-
existing refs, and digest-mismatched refs cannot reach the post-consensus check under
BFT assumptions.

---

## 4. Proposed design

### 4.1 Component A — liveness from the objects table (replaces `live_owned_object_markers`)

Define the single liveness primitive, implemented entirely by existing machinery:

```
latest_ref(id) -> LatestState        // Live(version, digest) | Tombstone(version) | NeverExisted
```

resolved through `object_by_id_cache` (monotonic, write-through from execution, moka-
bounded) with fallback to the `objects` reverse seek
(`get_latest_object_or_tombstone`, `authority_store_tables.rs:588`), computing the digest
via `compute_object_reference()`. This is exactly what `_get_live_objref` and the cache-
hit path of `check_owned_objects_are_live` already do.

Changes:
- `WritebackCache::check_owned_objects_are_live` DB fallback: replace the marker
  `multi_get` with `latest_ref` comparisons. Same answers, better errors (can distinguish
  digest mismatch from stale version).
- Delete `get_lock` / `ObjectLockStatus` / `SuiLockResult` /
  `get_latest_live_version_for_object_id` (dead in production; migrate the handful of
  unit tests).
- Remove marker writes from `write_one_transaction_outputs`, genesis, and
  `bulk_insert_live_objects` (`locks_to_delete` / `new_locks_to_init` fields go away
  entirely — `TransactionOutputs` shrinks).
- Drop the CF/keyspace (`drop_cf` exists for both backends; tidehunter table deletion is
  supported).

This component is **node-local** — no consensus-visible behavior change, independently
shippable, and the lowest-risk highest-value piece (it is the big perpetual keyspace).

### 4.2 Component B — lock overlay + conflict rule v2 (replaces `owned_object_locked_transactions`)

Keep the quarantine `owned_object_locks` map exactly as it is (insert on commit
processing, remove at flush). Delete the DB table and replace the "flushed locks" half
of the lookup with a consumed-check against the objects view. The post-consensus
per-ref decision becomes:

```
check_owned_ref(R = (id, v, d), tx_digest, current_commit_locks, overlay):
    // 0. Replay/out-of-band-execution exemption — see §5.3
    //    (evaluated once per tx, not per ref, before any ref checks)

    // 1. same commit
    if let Some(other) = current_commit_locks.get(R), other != tx_digest:
        DROP(ObjectLockConflict{other})

    // 2. unflushed commits this epoch (quarantine overlay — in-memory only now)
    if let Some(other) = overlay.get(R), other != tx_digest:
        DROP(ObjectLockConflict{other})

    // 3. flushed history (was: DB table) — replaced by the consumed-check
    match latest_ref(id):
        Live(v', d')  if v' >  v            => DROP(consumed)          // I2: some earlier finalized tx took it
        Live(v', d')  if v' == v && d' != d => DROP(digest mismatch)   // unreachable under I5; debug_fatal
        Tombstone(v') if v' >= v            => DROP(consumed)
        _                                   => ACCEPT
        // NB: Live(v') with v' < v, or NeverExisted, means the *creator* of R has not
        // executed locally yet (chained owned-object txs). This MUST accept — the
        // scheduler waits on input availability, exactly as today.
```

The `< v` / `NeverExisted` → ACCEPT arm is load-bearing: post-consensus checks must never
depend on local execution progress in the "not yet caught up" direction (I3). The
consumed-check only fires in the "already caught up" direction, where §5.2 shows it is
exactly equivalent to the flushed-lock lookup — **except for deleted/wrapped refs**.

> **Deletion gap (I2).** The `Tombstone(v') if v' >= v => DROP(consumed)` arm relies on the
> tombstone being present. Object retention keeps only the latest live version or a tombstone
> *until the pruner removes it*, so once a deleted/wrapped ref's tombstone is pruned,
> `latest_ref(id)` returns `NeverExisted` and the loser of an equivocation is silently
> **accepted** instead of dropped. Two failure modes: (a) a node that crashes and replays a
> commit whose loser claims a since-deleted-and-pruned ref accepts it while non-crashed peers
> (overlay lock or unpruned tombstone) drop it → checkpoint fork; (b) if every validator has
> pruned, all accept and the loser waits forever on an input version that can never appear →
> deterministic scheduler stall. Mutations are unaffected (the retained latest live version is
> strictly `> v`).
>
> **Backend status.** On tidehunter the objects keyspace compactor retains the latest entry
> per `ObjectID` — for a deleted object that entry *is* the tombstone — so the gap does not
> reproduce there (`authority_store_tables.rs`, `objects_compactor`). It reproduces only on
> the RocksDB pruner, which point-deletes tombstone rows (`authority_store_pruner.rs`).
>
> **Fix (implemented for RocksDB): retain tombstones for the epoch of the deletion.** The
> consumed-check replaces a *flushed owned-object lock*, and those locks lived for the whole
> epoch (the removed `owned_object_locked_transactions` table was per-epoch, dropped at
> reconfig); the deletion evidence must have the same lifetime. A tighter "prune once the
> quarantine flush watermark passes the deleting commit" gate is **not sufficient**: consensus
> GC only bounds a *single block's* staleness (`gc_depth`, ~60 rounds), but garbage-collected
> transactions are resubmitted and transaction expiration is epoch-granular, so a validly-
> sequenced conflicting claimant can land in an unflushed commit an unbounded number of commits
> after the deleter — the drain watermark can prune evidence a later replayed commit still
> needs. The epoch boundary is the only watermark that provably clears the replay set (I4).
> Concretely, `prune_for_eligible_epochs` now tracks a `tombstone_safe_ceiling` (the highest
> checkpoint of an already-completed epoch) and only prunes tombstones at or below it;
> superseded live versions prune unchanged. Cost: deletion tombstones survive to epoch end on
> RocksDB validators — still a net reduction versus the removed lock+marker tables, which held
> an entry per owned-object-touching transaction, not just per deletion. A durable per-epoch
> deleted-ref set consulted before the `NeverExisted → ACCEPT` arm would be a tighter but
> more invasive alternative if the tombstone storage proves material.

Per-tx exemption (checked before per-ref checks): if the transaction already has durable
effects with `executed_epoch == current epoch`, accept unconditionally and re-insert its
locks into the working map. Rationale and determinism analysis in §5.3.

The submit-path pre-screen keeps calling `get_owned_object_locks_map`, now backed by
overlay-only. Winners that already executed are caught by the existing follow-up
`handle_vote_transaction` (terminal `ObjectVersionUnavailableForConsumption`). The only
client-visible delta: in the window after the winner executes, the loser's terminal error
names no `pending_transaction` digest. (If that digest matters for debugging, `latest_ref`
gives the consuming version and the effects/objects tables identify the consumer.)

### 4.3 Component C — next-version overlay (replaces `next_shared_object_versions_v2`)

> **REVERTED (2026-07-09, §13.2).** The table is durable again with main's init-pin +
> flush-write model. This section and §6.3 are kept as the record of the design that
> was tried and why it was abandoned (C6/C7: the watermark value is not reconstructible).

Unify the two in-memory pieces that already exist — the quarantine's refcounted
`shared_object_next_versions` and the (currently table-persisted) lazy init pins — into
one epoch-store map:

```
next_version_overlay: Map<ConsensusObjectSequenceKey, (SequenceNumber, refcount)>
```

- **Init (pin)**: on a miss in `get_or_init_next_object_versions`, seed as today from the
  objects view (same `start_version` matching logic), insert into the overlay. No DB
  write.
- **Advance**: as today, per commit, values updated via the quarantine mechanism;
  refcounts track which unflushed commits reference the key (existing
  `RefCountedHashMap` pattern).
- **Evict ("reduce when objects are written")**: when a commit flushes, any key with
  refcount 0 is removable — by I1 its assigned transactions' outputs are durable, so
  re-seeding from the objects table reproduces the overlay value exactly (the stored
  next-version *is* the durable current version of the object: a mutating assignment
  sets next = the tx's output version, which is what the objects table then holds;
  read-only assignments don't advance; cancelled assignments don't advance). Keys whose
  first use was init-only likewise re-seed identically. Memory therefore tracks the
  consensus-to-durable-execution lag, not the epoch working set.
- **Restart**: overlay starts empty; replay re-derives it. The one gap versus today —
  the durable init pin protected replay from seeds contaminated by pre-crash execution
  of replayed commits — is closed by the effects-aware seeding rule (§6.3).
- `assign_shared_object_versions_idempotent` (end-of-epoch tx) reads the overlay + objects
  view without advancing — unchanged semantics.

This is the same "authoritative delta over the objects table" pattern as Component B, so
the two share the correctness argument, the eviction trigger (quarantine flush), and the
recovery story (replay + effects-awareness).

### 4.4 Optimistic warming

The consensus-critical reads become `latest_ref` lookups, so keeping `object_by_id_cache`
warm for owned inputs is the perf story:

1. **Vote-time warmup (free, exists).** `validate_owned_object_versions` already does a
   by-id `multi_get_objects` for every owned input at vote time — populating the
   monotonic cache. A validator votes on nearly every tx it later sees in a commit, so
   in steady state the commit-handler check is a pure cache hit. This is the "warm up
   optimistically from consensus validation" idea — it already exists; we just start
   relying on it and should add hit-rate metrics for the new call sites.
2. **Commit-pipeline prefetch.** `filter_consensus_txns` already prefetches lock refs
   per commit before the serial loop; repoint that prefetch at `latest_ref` (batched,
   in the parallel pre-processing stage alongside deserialization) to hide cold misses
   (post-restart, txs not voted on locally). Important: the prefetch is a **cache warmer
   only** — the authoritative read happens inside the serial check, after the overlay
   lookup (ordering requirement, §7.1).
3. **Replay warmup.** During startup replay, the same pipeline prefetch warms the cache
   for the replay window before the serial checks run, bounding cold-restart latency.

Eviction is safe everywhere: every consumer falls back to the `objects` reverse seek.
The cache is sized by config (`object_by_id_cache_size`) — this proposal may justify a
larger default on validators, which is still strictly less memory than the tidehunter
in-memory index of the markers keyspace it replaces.

**Measured at 18K TPS (private-testnet, 2026-07).** The warm lands in the right form from
both call sites above, but the design's "pure cache hit" expectation did **not** hold: the
`latest_objref_or_tombstone` hit rate was only **~36.5%** (`execution_cache_hits` /
`_requests`), and the resulting `objects` reverse seeks (`db_op{op="next_entry"}`) were the
single largest DB cost, on the saturated single-threaded handler (`handle_consensus_commit`
≈ 0.98 core, `filter_consensus_txns` ≈ 0.68). Root cause is **eviction, not a missing
warm**: `object_by_id_cache` is a single count-bounded (default 100k) `MonotonicCache` shared
with — and churned by — every execution object write (~100k/s at 18K TPS) and every other
by-id read, so it fully turns over in ~1s, the same order as commit latency, and entries
warmed at vote/prefetch time are gone before the handler reads them. Vote-time warmup also
has a structural coverage gap: a node never votes on its **own** proposed blocks (`Core`
bypasses `verify_and_vote`), so ~1/N of committed txns are never vote-warmed (the pipeline
prefetch covers those). Levers, in order: **(1)** raise `object_by_id_cache_size` (shipped:
default is now 10× the object cache — testing whether the working set fits); **(2)** if the
shared cache still evicts, a **dedicated coherent owned-input cache** (ObjectID → latest-ref)
kept fresh by the same execution write-through but invalidated only for keys it holds, so it
is churned by owned-object writes only, not the whole object working set — much higher hit
rate per byte, at the cost of a write-path coherence hook. Note the point-lookup shortcut
("point-get the exact claimed `(id, version)`, reverse-scan only on miss") is **not** correct:
a present historical row does not prove the version is live (a higher consumed version may
still exist pre-pruning), so the reverse-scan-for-latest is required — which is why caching
(fewer misses), not cheaper reads, is the lever.

---

## 5. Correctness — determinism of the post-consensus decision

### 5.1 What must hold

For every consensus position, all honest validators must make the same accept/drop
decision (I3), regardless of local execution progress, flush progress, restarts, or
state-sync-ahead execution. Drop *reasons* may differ (they are client-facing only);
decisions may not.

### 5.2 Equivalence of the consumed-check with the flushed-lock lookup

Claim: at the moment validator V processes position P, for any claimed ref R of tx T':

> some earlier finalized tx T ≠ T' locked R **iff** (R→T in V's overlay) ∨ (R is
> consumed in V's objects view) — with the two sides never disagreeing across validators.

*(⇒)* T is at an earlier position, so V already processed T's commit. Either that commit
is unflushed — lock is in the overlay — or it flushed, which by I1 means T's outputs are
durable, and by I2 R is consumed in the durable objects table. Either way V drops T', as
does every other validator (each sees its own overlay-or-consumed state, or the lock
table on the old binary). **Caveat (deletion gap):** this step invokes I2's "consumed ⇒
observable," which holds for mutations but not for a deletion/wrap whose tombstone the pruner
has removed — see the note under §4.2. There, a flushed T that *deleted* R leaves no durable
evidence, so V (overlay lock already flushed away, tombstone pruned) can wrongly ACCEPT T'
while a peer still holding the overlay lock DROPs it. The equivalence therefore holds
unconditionally only while deletion evidence outlives the replay window; the fix candidates
under §4.2 restore that.

*(⇐)* Suppose V sees R consumed but no lock. Consumption means some executed tx bumped R.
Only finalized txs execute (consensus-scheduled or certified-checkpoint execution — both
imply deterministic acceptance at an earlier position). Its acceptance implies it locked
R (I2's set equality: whatever consumed R claimed R). Prior-epoch consumers are excluded
by I5+I4 (T' could not have been finalized this epoch claiming a ref dead since last
epoch). Receiving-based consumption is excluded because a receivable ref's owner is an
object address, so it cannot be a signed owned input of T' (vote-time owner check). So a
conflicting earlier finalized tx exists and every validator drops T'.

The interesting asymmetric case — validator A executed the winner T (sees consumption,
lock possibly flushed away), validator B has not (sees the lock in overlay) — lands on
DROP on both. The chained-creation case — R not yet created locally — lands on ACCEPT on
both (old binary: no lock; new binary: explicit `< v / NeverExisted` arm). There is no
reachable state where the two formulations diverge, which also makes a **mixed old/new
fleet safe in principle**; we still gate the switch (§10).

### 5.3 The executed-transaction exemption

Without it, the design breaks in exactly one family of cases: T' itself executed before
the check runs — (a) crash replay of a commit whose txs executed pre-crash, (b)
state-sync/checkpoint-executor running ahead of local consensus. The consumed-check would
then see T's *own* consumptions and drop a tx every peer accepted.

Rule: **if durable effects exist for T' with `effects.executed_epoch == current_epoch`,
accept and insert its locks.** Why this is deterministic and safe:

- Executed ⇒ was in a certified checkpoint or scheduled by consensus ⇒ was
  deterministically accepted at this position ⇒ every peer also accepts (peers that have
  not executed it reach ACCEPT through the normal path: any state that would make them
  drop would contradict T' having been accepted deterministically before).
- Epoch-scoping via `executed_epoch` (available inside the effects, whose durability is
  atomic with the objects writes — same `write_one_transaction_outputs` batch) avoids
  depending on `executed_effects` *presence* for old digests, which is pruning-dependent
  and therefore not deterministic across nodes. Prior-epoch effects never exempt.
- Cost: the effects lookup is needed only on the drop path and during replay bursts —
  never in the steady-state accept path (check order: run the cheap per-ref checks
  first; consult effects only when they would drop... or equivalently check the
  epoch-scoped executed set first during replay. Either ordering is correct because the
  exemption can only flip DROP→ACCEPT; implement whichever profiles better).
- Re-inserting the exempted tx's locks keeps the working map identical to peers'
  (needed so a same-commit loser T'' still sees the conflict via `current_commit_locks`).

### 5.4 Defensive posture for unreachable-under-BFT cases

Digest-mismatch-at-live-version and dead-since-a-prior-epoch refs cannot be finalized
under I5. Today's code would accept them post-consensus and then wedge or
`debug_fatal` at execution; the new rule drops them deterministically (all honest nodes
compute identical `latest_ref` for any consensus-agreed lineage). Keep `debug_fatal!` +
`assert_reachable!`-style instrumentation on these arms so a BFT-assumption violation is
loud rather than silent.

---

## 6. Restartability

### 6.1 Recovery model (unchanged mechanics, new content)

Restart state: epoch DB has consensus state ≤ watermark W (`last_consensus_stats_v2`);
perpetual DB has execution outputs for everything up to the last executed checkpoint —
which, by I1, is a **superset** of the transactions in commits ≤ W. Quarantine and all
overlays are empty. Consensus re-delivers commits > W through the full handler.

Walkthrough of the crash windows for **locks** (Component B):

| Crash point | Pre-crash state | Recovery |
|---|---|---|
| After processing commit C (> W), before execution | lock in overlay only (lost) | replay C → normal path re-acquires deterministically (refs still live: consumption never happened) |
| After execution of C's txs, outputs in dirty cache only | overlay lost, dirty lost | same as above — durable objects unaware of C, refs still live durably |
| After checkpoint executor committed C's outputs, before quarantine flush | objects durable, consumed; lock table unaware (this is the window that exists today too) | replay C → per-tx **exemption** fires (durable effects, current epoch) → accept + re-insert locks; same-commit losers re-drop via `current_commit_locks` |
| After quarantine flush of C | (old design: lock rows exist) — new design: nothing to recover | C ≤ new W, not replayed; consumed-check covers all its locks by I1+I2 |

For **next-versions** (Component C) the same table applies, with recovery = re-running
`assign_versions_from_consensus` per replayed commit, plus the seeding rule below.

### 6.2 What the old design persisted that we must re-derive

1. Flushed locks → consumed-check (I1+I2), §5.2. Done.
2. Flushed next-version advancements → objects table: for every key whose assignments
   are all durably executed, next-version ≡ durable current version (§4.3). Done.
3. **Durable init pins** (the `multi_insert` at `authority_per_epoch_store.rs:2156`) —
   the one genuinely lost piece, handled next.

### 6.3 Effects-aware seeding (replaces the durable init pin)

> **REVERTED (2026-07-09, §13.2).** The durable init pin is back; this mechanism is
> deleted. Kept for the record — §7.4 explains why it was unsound (C6/C7).

The pin exists to defeat this race: object S first touched this epoch by tx T in commit
C > W; T assigned S at seed v0; T executed (durable: S now at v5); crash; replay of C
re-seeds S. Seeding from the objects table yields v5 — but peers assigned T at v0.
Fork.

Rule: during assignment, when a key misses the overlay, seed it **from the effects of
the first assignable in consensus order that touches it, if that assignable has durable
current-epoch effects and was not cancelled** (its recorded input version for the key is
precisely the pre-crash seed); otherwise seed from the objects view as today.

Why the store-seed is safe exactly when the rule falls through to it: replay processes
commits in order, so when key k is first needed at position P, every earlier epoch-local
toucher of k is either (a) in a flushed commit — executed & durable (I1), so the store
reflects it, which is the correct seed; or (b) earlier in the replay window — already
processed, so k is in the overlay (no seeding happens); or (c) the current assignable
itself — covered by the effects branch when executed, and when *not* executed nothing
this epoch has mutated k in the durable store (per-object execution order: a later
toucher cannot execute before an earlier one), so the store seed is clean. Cancelled
executed assignables don't constrain the seed (they neither read nor advance the real
version) and are skipped in favor of the next qualifying toucher or the store.

Two assertions make this self-checking and give fork detection for free:
- For every assignable with durable current-epoch effects, the recomputed assignment
  must equal `assign_versions_from_effects` output for that tx (debug/simtest: always;
  prod: cheap sampled check).
- On overlay eviction, assert value == store-derived seed (debug only).

Ordering requirement embedded here: the executed-check for an assignable must happen
after its potential effects write is visible — trivially satisfied because both the
effects lookup and the store seed go through the same cache, and a tx cannot become
"executed" between its own effects-check and the store read for a key it mutates without
the effects-check... more simply: read the store seed first, then check executedness;
if not executed at that point, the earlier store read predates any mutation by this tx
(§7.2 spells out the general pattern).

### 6.4 Epoch boundary

Nothing new: overlays live in the epoch store and die with it; the new epoch's first
seeds come from a durable objects table that reflects all prior-epoch execution (I4).
`owned_object_locked_transactions` and `next_shared_object_versions_v2` already vanished
at every epoch boundary — the proposal merely makes mid-epoch state equally ephemeral.

### 6.5 Fullnodes / non-consensus nodes

Fullnodes never write the lock table today (no consensus handler) — markers are pure
write overhead for them and Component A is a straight win. Version assignment on the
synced-checkpoint path stays effects-driven (`assign_versions_from_effects`); its
init-pinning call becomes unnecessary (effects-aware seeding subsumes it on validators;
on pure fullnodes it was only feeding the table) but can be retained harmlessly during
transition.

---

## 7. Concurrency

### 7.1 The one ordering that matters (locks)

Overlay-entry removal must happen only after the corresponding execution outputs are
durable. This holds today (flush is triggered by `handle_finalized_checkpoint`, which
runs after `commit_transaction_outputs`). Given that, the serial check must read in this
order per ref:

```
read overlay  →  (miss)  →  read latest_ref
```

If the overlay read misses because a concurrent flush just removed the entry, the
subsequent `latest_ref` read is guaranteed to observe the consumption (durable write
happened-before overlay removal, and the monotonic by-id cache reflects dirty execution
writes even earlier). Reading `latest_ref` *before* the overlay (e.g. trusting a
pipeline-stage prefetch as authoritative) reintroduces a TOCTOU window: prefetch sees R
live → winner executes, checkpoint certifies via state sync, quarantine flushes (overlay
entry gone) → serial check sees neither. Hence §4.4's "prefetch is a warmer only".

Today's equivalent is race-free for a different reason (locks never leave
union(quarantine, DB) mid-epoch); the new design substitutes the happens-before edge
durable-objects-write → overlay-removal. This edge is the load-bearing concurrency
contract; encode it in comments and, in simtest builds, an assertion at flush time
(sampled: for each lock entry being dropped, `latest_ref(id).version > v`).

### 7.2 Version assignment

`version_assignment_mutex_table` continues to serialize per-object-id assignment between
the consensus handler and the checkpoint executor. Within the critical section the
effects-aware seed uses the pattern: store-read, then executed-check (a tx that is
not-executed at check time cannot have contaminated the earlier store read for keys it
mutates; later touchers cannot execute before earlier ones per-object). All overlay
mutations stay under the existing quarantine write lock.

> **This paragraph's premise is false** under checkpoint-execution-ahead-of-consensus-
> replay (I1b), and read-only shared inputs break "later touchers cannot execute before
> earlier ones." See §7.4.

### 7.3 Warmup writers

Vote-time and pipeline warmers insert through the existing `MonotonicCache` ticketed
protocol, which already guarantees a stale read can never clobber a newer concurrent
execution write — this is precisely why reusing `object_by_id_cache` is preferable to
inventing a new liveness cache.

### 7.4 Effects-visibility races (C5 / C6 / C7) — RESOLVED via §13

Status: **resolved (2026-07-09).** C6/C7 closed by reverting Component C (§13.2); C5
closed by the two-source exemption (§13.3) — smaller than Design A below, which was
never built. The analysis is kept because it explains *why* those are the right moves.

**Root cause.** `WritebackCache::write_transaction_outputs`
(`writeback_cache.rs`) publishes a transaction's OUTPUT OBJECT writes into the dirty cache
*before* it inserts `transaction_effects` and `executed_effects_digests` (the "this tx
executed" gate `get_executed_effects` reads). So a reader can observe a tx's object writes
without yet observing its effects/executed-mark — a non-atomic publish. Three consensus
bugs follow, all reachable only when the checkpoint executor runs ahead of consensus replay
(I1b, e.g. crash-recovery with deferral):

- **C5** — the post-consensus owned-lock exemption
  (`try_acquire_owned_object_locks_post_consensus_v2`) reads the consumed-check (objects)
  then `get_executed_effects`; a catching-up node sees its own certified tx's input
  consumed but not-yet-executed → drops it → checkpoint-fork `fatal!`.
- **C6** — the shared-object seeder (`compute_effects_version_seeds` +
  `get_or_init_next_object_versions` store-candidate) reads the store (sees an *in-batch*
  toucher's object write) but that toucher's effects aren't visible yet → seeds the
  next-version map from a contaminated store value.
- **C7** — the contaminating mutator is in a *later* commit than the batch being seeded, so
  it is never among the batch's assignables; the seeder cannot find its effects, and the
  store-candidate fallback is contaminated. This falsifies §7.2's premise.

**C5 / C6 — designed fix (safe, no table).** The naive reorder — publish
`executed_effects_digests` before the object writes — is **unsafe**: it inverts the
`executed-mark visible ⇒ output objects visible` relationship that real readers depend on
(the checkpoint executor's `load_checkpoint` reads output objects at pipeline stage 3,
before commit, and panics if absent; the accumulator settlement barrier reads written
fields that are not its declared inputs; the legacy pre-effects-v2 hasher). The safe change
publishes only `pending_transaction_writes` (which carries the effects inline) *before* the
object writes, adds a `get_executed_effects_including_pending(digest)` reader (pending map
first, else the committed path), and uses it in the exemption (C5) and the seeder (C6).
This gives `object write visible ⇒ effects visible` without ever moving
`executed_effects_digests`, so every `executed ⇒ objects` reader keeps its guarantee.

**C7 — needs a durable next-version-at-watermark; open decision.** On replay,
`get_or_init` must seed a key touched in the replay window with its next-version *at the
flushed watermark W*. A "minimal first-touch init-pin" (write the key's seed once, at first
touch) is **insufficient**: for a hot object mutated before W and re-touched during replay
(the common case), first-touch value ≠ W value. Example: X goes `v0→v1` in a flushed
commit, then `v1→v2` in a replay-window commit; the correct seed is `v1`, but a first-touch
pin holds `v0` and the executor-contaminated store holds `v2`. The only value that is
correct is the next-version at W — exactly what the removed `next_shared_object_versions_v2`
stored (written at flush). Options:
1. **Persist the next-version map at flush (revert Component C).** Reinstate a durable
   next-version table written at flush = the watermark value per key. Fully closes C6+C7,
   and also fixes the §8 unbounded-memory gap (evict cold map entries to the table, re-read
   on demand — the old memory model). Amortized at-flush write. Cost: reverts the largest of
   the three table removals; the owned-lock and marker removals stand.
2. **Execution-ordering constraint (no table).** Prevent the checkpoint executor from
   advancing a shared object's version ahead of consensus replay, so the store is never
   contaminated when the seeder reads it. Closes C7 with no durable table but serializes
   execution behind consensus replay for shared objects — hurts state-sync/catch-up
   parallelism, i.e. the throughput this whole project targets.
3. **Flag-gate C7 as a known limitation.** Ship C5/C6, rely on the debug-only
   `verify_assignment_matches_effects` to catch C7 in simtest/CI, and close it before any
   mainnet rollout. Not gap-free in release.

Related: option 1 subsumes the P2 seeder cost (the durable read replaces the guaranteed-miss
`get_executed_effects` probing, §9); options 2–3 leave §9's ≤2-probe reduction as the P2
fix.

---

## 8. Memory bounds

- **Lock overlay** (exists today; the DB fallback's removal makes its bound worth
  stating): `O(unflushed finalized txs × avg owned inputs)`. Steady state at 15k TPS
  with ~2s flush lag and ~3 owned refs/tx ≈ 90k entries ≈ ~20 MB. Worst case is a
  checkpoint-certification stall; the quarantine already grows unboundedly in that
  scenario today (locks are one more proportional term, and backpressure already exists
  at the checkpoint watermark). Add a gauge for entry count.
- **Next-version overlay**: keys touched since the last flush plus unflushed pins —
  thousands, not millions, in steady state; worst case again the stall scenario.
- **Latest-ref cache**: whatever `object_by_id_cache_size` is configured to; lossy.
- Compare against what is being deleted on the tidehunter side: the markers keyspace
  keeps an in-memory index entry per **live owned object on chain** (hundreds of
  millions of 80-byte keys, reduced-key indexed, 16× mutexes, blooms) — orders of
  magnitude more memory than every overlay above combined.

---

## 9. Performance impact

Wins:
- Perpetual DB: two writes per owned-object mutation removed from the checkpoint commit
  batch (marker delete + marker insert), one keyspace/CF gone (with its tidehunter index
  memory, mutexes, bloom); snapshot-restore write volume for owned objects ~halved;
  genesis simplification.
- Epoch DB: two keyspaces gone; per-commit flush batches shrink by the lock rows
  (N_owned_refs per finalized tx) and next-version rows.
- Consensus handler (see `docs/consensus-handler-optimization.md` — the serial section
  is the system bottleneck): the per-commit `existing_locks` prefetch (quarantine + DB
  multiget with blooms) becomes overlay lookups plus by-id cache hits. With vote-time
  warmup the serial-section cost should be flat or better; the prefetch stage stops
  touching the epoch DB entirely.

Risks / costs:
- Cold restarts: replay-window checks miss the cache → reverse seeks on `objects`.
  Bounded by replay window size; mitigated by pipeline prefetch (§4.4.3). Measure.
- `latest_ref` on a miss deserializes the object to compute the digest — marginally more
  CPU than a marker point-get. Only on cache misses.
- The drop-path effects lookup (§5.3) — off the hot path by construction.

**Measured at 18K TPS (private-testnet, 2026-07).** The single-threaded consensus handler
saturates (~0.98 core) and effective throughput falls below target. The two dominant DB
costs on that thread, from `db_op` (DBMap layer, both backends) and cache hit/miss counters:

1. **`objects` reverse seek (`op="next_entry"`) — #1 cost.** The per-owned-object
   `latest_objref_or_tombstone` consumed-check at ~36.5% hit rate (see §4.4). This is a
   *caching/eviction* problem; being addressed by the cache-size lever (§4.4).
2. **`executed_effects_digests get` — #2 cost (~186k misses/s, ~1.6% hit).** The
   effects-aware seeder (§6.3) probes `get_executed_effects` for every toucher of a
   first-touched key; in steady state no toucher has executed yet, so these are guaranteed
   misses. **Caching cannot help** (the entries legitimately do not exist yet, and a
   negative cache would go stale the instant they execute) — the fix is to stop issuing the
   lookups: probe at most the earliest non-cancelled toucher (and earliest writer, for the
   §6.3 reader-ordering case) per key rather than every toucher, and/or gate the seeder on
   replay/executor-ahead being possible. This is independent of the cache work.

The `owned_object_refs_to_lock` recomputation (computed once in the deserialize worker and
carried on the parsed transaction, rather than recomputed in the prefetch and filter loops)
and the same-digest lock-map short-circuit are smaller CPU wins on the same thread.

---

## 10. Migration & rollout

Three independently shippable phases, each with a dual-write safety net:

**Phase 1 — markers (node-local, no gating).**
1. Release A: stop *reading* markers (rewrite the `check_owned_objects_are_live`
   fallback; delete `get_lock` surface); keep writing them (rollback safety).
2. Release B: stop writing; drop the CF/keyspace at open. Remove
   `locks_to_delete`/`new_locks_to_init` plumbing.

**Phase 2 — lock table (consensus-visible; protocol-gated).**
1. Release A: implement conflict rule v2 + exemption behind a `ProtocolConfig` feature
   flag (flips at an epoch boundary, so the whole committee switches at once and the
   epoch starts with empty tables — no mixed-rule window within an epoch, even though
   §5.2 argues mixed is safe). While the flag is off: dual-run the new rule in shadow
   mode and `debug_fatal!`/metric on any decision divergence — free network-wide
   validation. Keep dual-writing the table while the flag is on (mid-epoch rollback
   safety).
2. Release B: stop writing the table; remove it from `AuthorityEpochTables` (per-epoch
   DBs make this trivial — it just isn't created next epoch).
   Note: any `ProtocolConfig` change goes through the `/protocol-config` process.

**Phase 3 — next-versions (consensus-visible; protocol-gated).** Same two-step shape:
flag-gated effects-aware seeding + overlay-only reads with shadow-mode comparison
against the table, then drop the table (and update `sui-tool db_dump`'s reference).
Phase 3 can also ship first among the epoch tables if the shadow-mode results for the
seeding rule are wanted early — it is independent of Phase 2.

Rollback rule for both epoch phases: never remove the dual-write before the read-switch
release has soaked for at least one release cycle, because an old binary restarted
mid-epoch would otherwise miss flushed rows that peers on the old binary still consult.

**Mid-epoch upgrade is fail-fast, not fork.** Deploying a binary that maintains
`system_object_next_versions` onto a node whose current epoch was run by a binary that did
not (a mid-epoch upgrade) has no safe seed: the durable objects run ahead of the flushed
consensus watermark under load, so seeding the system rows from them would diverge the
first replayed prologue/settlement. `AuthorityPerEpochStore::new` therefore `fatal!`s when
`system_object_next_versions` is empty but `last_consensus_stats_v2` is not (the epoch has
already flushed commits), refusing to start rather than fork silently. A fresh-genesis
deployment is unaffected (both tables start empty → normal seed).

**Network rollout uses a protocol-config flag; the fatal is only a backstop.** This whole
feature is consensus-visible (every validator must make the same lock/version decision), so
on a live network it must be gated behind a `ProtocolConfig` feature flag that flips at an
epoch boundary — the standard pattern, and the §10 phases above. The rollout is then:
1. Ship the binary with the feature (and this seeding) gated **off** by default, still
   writing the old tables (the dual-write above). Validators do a **normal rolling upgrade
   mid-epoch**; with the flag off the new seeding path never runs, so the fatal never fires.
2. At an epoch boundary the protocol version bump enables the flag **fleet-wide at once**.
   That epoch is fresh (both tables empty, `last_consensus_stats_v2` empty), so seeding from
   epoch-start object versions is exactly the safe case.
Under that gating the fatal can only fire on a mis-gated activation (the flag flipping
mid-epoch), which the per-epoch protocol-version invariant forbids. The current branch
removed the old tables outright for performance evaluation, so it is **not** yet gated —
a network rollout requires re-introducing the flag and the dual-write before relying on the
fatal as a backstop rather than a hard deployment constraint.

---

## 11. Testing

- **Simtest crash matrix**: kill at each row of the §6.1 table (existing
  `fail_point!("crash")` in `build_db_batch` plus new failpoints between
  `commit_transaction_outputs` and `handle_finalized_checkpoint`, and inside quarantine
  flush). Assert no forks and identical checkpoint building vs a control validator.
- **Equivocation suites**: existing `authority_tests` lock-conflict tests migrate from
  `insert/delete_object_locks_for_test` to overlay manipulation; add: loser-after-winner-
  executed, loser-after-winner-flushed, chained owned inputs (creator unexecuted at check
  time), same-commit conflicts under replay.
- **Shadow-mode divergence counter** (Phase 2/3, §10) in private-testnet under 15k load,
  including induced restarts mid-load and a checkpoint-stall scenario.
- **Invariant assertions**: I1 flush-ordering assertion (simtest); §6.3's
  effects-vs-recomputed assignment equality (simtest always, prod sampled);
  eviction-equality debug assert.
- **Restore paths**: formal snapshot restore + db reset flows without marker rebuilding;
  verify restore-then-validate parity.

## 12. Open questions

1. **Error-shape compatibility** for submit-path losers once the winner has executed
   (`ObjectVersionUnavailableForConsumption` without the winner's digest) — acceptable,
   or should the drop-path look up the consumer digest via effects for parity with
   today's `ObjectLockConflict{pending_transaction}`?
2. **Exemption source**: `executed_effects` + `effects.executed_epoch` (proposed,
   durability-atomic with objects) vs `executed_transactions_to_checkpoint`
   (watermark-coupled). Confirm pruner retention of current-epoch effects is a hard
   guarantee, not a tuning default. *Largely resolved by §13.3: use both — the
   epoch-store executed mark for the live window, current-epoch effects for post-crash.*
3. **`checkpoint_queue_drained` interaction**: flush batching means "flushed" can lag
   "checkpoint executed" by several commits; the proofs only use flushed ⇒ durable, which
   still holds, but the shadow mode should specifically cover long undrained stretches.
4. Whether Phase 1 should also delete the `ObjectLockAlreadyInitialized` error variant
   and `LockDetailsWrapperDeprecated` types outright or leave them one release for
   downstream deserializers (sui-tool dumps of old DBs).
5. Sizing: whether validators should get a larger `object_by_id_cache` default once it
   becomes the primary liveness structure (measure hit rate at 15k with vote-warmup
   first).

## 13. Simplification review (2026-07-09) — three moves close every open finding

A design-level re-examination of all findings from the max-effort review, looking for
structural simplifications rather than per-finding patches. Conclusion: **three moves
resolve every open correctness finding, delete the two most subtle mechanisms this
design introduced, and remove one of the two measured hot-path costs.** All three are
now implemented on the branch (2026-07-09): §13.2 as "reinstate durable
next_shared_object_versions_v2", §13.3 as the two-source exemption in
`try_acquire_owned_object_locks_post_consensus_v2`, §13.4 as the unconditional
`verify_immutable_object_claims` inside `handle_vote_transaction`.

### 13.1 The findings sort cleanly by component

| Component | Findings | Status |
|---|---|---|
| **A — markers** (perpetual table; the storage prize) | none | clean |
| **B — lock table** (epoch) | C1 ✓, C4 ✓ fixed; C5, C2, C9 open | all three have small local fixes (§13.3, §13.4) |
| **C — next-versions** (epoch) | C6, C7, C14, P2 — all open | root cause is irreducible; revert (§13.2) |

Component A — the reason this project exists (the largest perpetual keyspace, its
tidehunter in-memory index, two writes per owned object per transaction) — attracted
zero findings. Every open correctness finding attaches to the two *epoch-table*
removals, which are the smallest share of the win.

### 13.2 Move 1 — revert Component C (reinstate `next_shared_object_versions_v2`)

**Why C is different in kind from B.** The lock decision is *set membership* over
consumption history: it is re-derivable at any time from durable execution state plus
replay, and stays deterministic under checkpoint-execution-ahead because acceptance and
execution cross-imply — a winner that executed ahead is exempted by its own effects; a
loser never executed anywhere, so the consumed-check drops it (§5.2, §5.3). Version
assignment is *counter arithmetic*: replay of a commit needs each key's counter **at the
flushed watermark W**, and no bounded read of durable state reproduces that value —
the objects table is contaminated by executor-ahead writes (C7), a first-touch pin holds
the wrong value for any key already advanced by flushed commits, and an effects-chain
walk-back (latest version → `previous_transaction` → its effects → `dependencies` →
producer of the input version, recursively) is sound but unbounded: for a hot key behind
a large sync-ahead window the walk is as long as the window. The only value that works
is the one the deleted table stored, written at flush. This is not a wart of the
implementation; it is the shape of the problem.

**Therefore: restore the durable table and the immediate init-pin writes (main's
`get_or_init_next_object_versions`), and delete the reconstruction machinery** —
`compute_effects_version_seeds`, the `effects_seeder` parameter and its ordering
contract (§6.3, §7.2), and the `system_object_next_versions` exception table together
with its I1b carve-out story and the §10 mid-epoch `fatal!` (system keys become ordinary
rows again; a mid-epoch binary upgrade is back to being trivially safe). Phase 3 of the
migration plan disappears outright.

One move closes: **C7** and **C6** (seed is the durable watermark value; the seeder
never consults effects or a contaminable store during replay), **C14** (cold overlay
entries evict to the table again — the old bounded-memory model), **P2** (the
`executed_effects_digests` probing — the **#2 measured hot cost, ~186k guaranteed
misses/s at 18K TPS** — is deleted, not optimized). What it re-adds is the cheap kind of
DB traffic: bloom-filtered point-gets on overlay miss and a few rows per touched key per
flush, which never appeared in any handler profile. Net at the bottleneck, the revert is
perf-*positive*.

The recomputed-vs-effects assertion (`verify_assignment_matches_effects`) is worth
keeping as debug/simtest hardening even though the mechanism it was built to check is
gone.

**Pre-existing hazard worth one line of hardening while reinstating:** the init pin
lands in the epoch DB while execution outputs land in the perpetual DB — two WALs with
no cross-ordering. A machine crash (not process crash) can lose an unsynced pin while
keeping the outputs it was supposed to guard against, reproducing the contaminated-seed
fork in a very narrow window. This existed on main for years. The pin is written once
per key per epoch, so writing it with `sync = true` is free and closes the window.

### 13.3 Move 2 — C5 without touching the writeback cache

§7.4's Design A (publish `pending_transaction_writes` before object writes + a combined
reader) is unnecessary. The ordering it tries to create **already exists** one level up:
`commit_certificate` (`authority.rs:1790-1802`) inserts the epoch-store executed mark
(`insert_executed_in_epoch`) *before* calling `write_transaction_outputs`, and the
comment there documents the ordering as intentional. `write_transaction_outputs` has
exactly one production caller, so this covers checkpoint-executor-driven execution —
precisely the C5 trigger path.

Fix: the exemption in `try_acquire_owned_object_locks_post_consensus_v2` reads
`transactions_executed_in_cur_epoch(digest) || (durable effects with executed_epoch ==
current)` instead of `get_executed_effects` alone. Coverage is airtight by construction:

- **Live window:** an object write visible to the consumed-check implies the mark was
  already inserted (program order on the executing thread + lock release/acquire on the
  shared maps). Mark pruning is safe: `remove_executed_in_epoch` runs only after the
  digests are committed to `executed_transactions_to_checkpoint` — which is the same
  lookup's DB fallback.
- **Post-crash:** consumption durably visible ⇒ effects durably visible (objects and
  effects commit in one perpetual-DB batch), so the existing effects branch covers it.

Both reads stay on the reject path only. No publish-order change, no new reader, no
`load_checkpoint` / accumulator-barrier / legacy-hasher exposure.

### 13.4 Move 3 — C2 + C9 with one vote-path gate

`verify_immutable_object_claims` (`consensus_validator.rs:333`) already enforces exact
two-way set equality between claimed and actual immutable inputs — including the
completeness direction (`ImmutableObjectNotClaimed`). C9 exists only because the whole
check is gated on `!claimed_immutable_ids.is_empty()` (`consensus_validator.rs:312`): a
Byzantine submitter who strips the *entire* claim list skips verification. (Partial
stripping is already caught.)

Fix: enforce claim completeness unconditionally. The owned input objects are already
loaded at vote time by `validate_owned_object_versions`, so folding the immutability
check there costs zero extra reads. Under I5, a claim-less frozen input can then never
finalize, which closes both findings at once:

- **C9** becomes unreachable: every locked ref is genuinely exclusive-mutable, restoring
  I2 exactly — no lock can outlive its consumption evidence. Keep a `debug_fatal!` on
  the post-consensus path for the BFT-violation case.
- **C2**'s recovery over-lock becomes provably harmless — honest claimants skip refs
  they claimed immutable, claim-less claimants can't finalize, and the C1 retention rule
  drops the over-lock when the deferred holder executes. The deferral record needs **no
  schema change**, and the existing safety comment above the rebuild
  (`authority_per_epoch_store.rs:1080-1085`) becomes true as written.

### 13.5 What remains after the three moves

- **Finding 9 (production fork detector):** demoted from "critical multiplier" to
  ordinary hardening — the seeding-fork class it was most needed for dies with the
  revert. Land after the moves so it never crash-loops on a known race.
- **Perf #1 (consumed-check reverse seeks, 36.5% by-id hit rate):** unchanged by the
  moves; the 10× cache default is deployed and under measurement. Decision ladder if it
  fails: dedicated coherent owned-input latest-ref cache → as last resort revert
  Component B too (restores the epoch lock table; A and its storage win are untouched
  and carry no findings).
- **Cleanups:** P1 (owned-refs computed once, carried on the parsed tx), P3 (same-digest
  short-circuit), R1 (v1/v2 dedup), and the leftover artifacts — orthogonal, unaffected.

End state if all three moves land: the feature keeps its headline (both owned-object
lock structures gone, liveness and conflicts answered by the objects table), the one
table whose contents are genuinely irreducible stays, and every mechanism whose
correctness argument needed more than a paragraph — effects-aware seeding, the §7.2
ordering contract, the system-object exception table, Design A — is deleted rather than
defended.
