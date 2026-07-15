# Owned-object locking: `owned_object_locked_transactions` and `live_owned_object_markers`

Survey of how these two tables are used today (as of `main`, 2026-07-10), written as
groundwork for removing them and replacing them with in-memory owned-object lock tracking
that uses the `objects` table as the source of truth. Line numbers are as of this date and
will drift; function names are the stable reference. Untracked / scratch.

> **Status update (2026-07-10):** the `live_owned_object_markers` removal described in
> §10 is implemented in draft PR #27243 (`andll/remove-live-owned-object-markers`) —
> markers deleted outright; `check_owned_objects_are_live` and `get_lock` reimplemented on
> `get_object_impl` (objects table, dirty-aware). Sections describing the marker table
> document the pre-removal state. `owned_object_locked_transactions` is untouched.
>
> **Status update (2026-07-15):** Part 3 (below) is the replacement design for
> `owned_object_locked_transactions`: quarantine map + deferred-locks map + objects-table
> version verdict, accelerated by a vote-warmed `LiveObjectCache`. Feasibility: yes, with
> three amendments (monotone version rule instead of liveness, deferred/same-digest gap
> fillers, strict immutable-claims voting + immutable backstop). No protocol flag needed
> (§3.6a); two PRs: reads swapped with debug double-read, then table deletion.
> Implementation in progress on `andll/owned-object-locks-in-memory`.

---

## Part 1 — TL;DR (for humans)

There are two tables with confusingly similar names and completely different jobs. Both are
leftovers of the pre-consensus locking design that was removed when all user transactions
started going through consensus (PR #24676 "Disable Owned Object Locking", Dec 2025, and
cleanup #25150, Feb 2026).

### `live_owned_object_markers` — perpetual DB, all node types

- `DBMap<ObjectRef, Option<LockDetailsWrapperDeprecated>>` in `AuthorityPerpetualTables`
  (`authority_store_tables.rs:81`). On-disk column-family name is still the old
  `owned_object_transaction_locks` (via `#[rename]`). The value is **always written as
  `None`** — it is a pure existence marker; the value type survives only for legacy DB
  compatibility.
- **Why it existed:** the `objects` table stores *all* versions of every object, so a
  point read there can only answer "did (X, v) ever exist" — not "is (X, v, digest) the
  *current live* version". Answering liveness from `objects` requires a latest-version
  lookup (a bounded reverse seek, or the in-memory object-by-id cache). The marker table
  was a **materialized index that made exact-ref liveness a single point get**: key
  present ⇔ this exact ref is the live version of an address-owned
  (`Owner::AddressOwner`) object.
- **Writes:** maintained atomically with `objects` in the per-checkpoint commit batch
  (`AuthorityStore::write_one_transaction_outputs`): insert a marker for each
  address-owned output, delete for each consumed address-owned input and received object.
  Also written by genesis init and formal-snapshot restore. Durability is
  checkpoint-gated, like all transaction outputs.
- **Reads — hot function, cold table (easy to get wrong):**
  - It was **not** the system's primary liveness guard. Vote-time validation
    (`validate_owned_object_versions`) — the thing that actually keeps stale-version
    transactions out of consensus — always read live objects, never markers (§4).
  - Its one hot consumer was `check_owned_objects_are_live`, a defense-in-depth assert
    run before executing **every** certificate (failure is `fatal!` — node crash). The
    *function* is hot, but it is cache-first: the in-memory object-by-id/dirty caches
    answer nearly every query, and the marker *table* was only touched on a full
    in-memory miss (cold objects).
  - The remaining reads: `get_latest_live_version_for_object_id`, a reverse scan used on
    failure paths to report "the live version is actually X", and `get_lock` /
    `ObjectLockStatus` — **dead in production**, only unit tests call it.
- **What replaces it (PR #27243, deletes the table):** the `objects` table becomes the
  sole source of truth for liveness. The assert and `get_lock` go through
  `get_object_impl` (object-by-id cache → dirty set → **latest-version reverse seek on
  `objects`**) and compare the full ref against the live object; error enrichment reports
  the live version from the same lookup; the writes simply stop. Semantics are unchanged
  on the dominant cache-hit path and strictly stricter on the miss path (the old fallback
  trusted marker existence, which stale genesis-era markers could fool). The cost trade:
  on a cold miss, a per-object reverse seek on `objects` replaces a batched point get on
  markers — that seek is the price of deleting the index — in exchange every transaction
  stops paying two marker writes per owned object plus a marker multi-get in the commit
  path, and the whole write-heavy keyspace goes away. Vote-time validation is untouched;
  it never used the table.

### `owned_object_locked_transactions` — per-epoch DB, validators only

- `DBMap<ObjectRef, LockDetailsWrapper>` in `AuthorityEpochTables`
  (`authority_per_epoch_store.rs:430`); `LockDetails = TransactionDigest`.
- Semantics: post-consensus equivocation protection. When the consensus commit handler
  processes a finalized `UserTransactionV2`, it "locks" every owned input ref (including
  gas) to the transaction digest. The first transaction to claim a ref in an epoch wins;
  any later transaction claiming the same ref is deterministically **dropped** by every
  validator (`ObjectLockConflict`). Locks are **never released during an epoch** — the
  whole epoch DB is deleted after reconfiguration, which is how locks are released.
- Writes are **not** immediate: locks accumulate in the in-memory
  `ConsensusOutputQuarantine` and are flushed to the epoch DB (atomically with
  `last_consensus_stats_v2`, the consensus replay watermark) only once the checkpoints
  covering the commit are certified and executed.
- Reads always go through the quarantine first, falling back to the DB
  (`AuthorityPerEpochStore::get_owned_object_locks`). Readers: the consensus handler's
  per-commit prefetch, and the transaction-submission path (to give clients a terminal
  conflict error instead of a retriable one).
- Fullnodes have the table but never write or read it.

### The two key nuances (restart & DB divergence)

**Recovery on restart.** The quarantine is volatile. On restart, consensus replays every
commit after the durable `last_consensus_stats_v2` watermark, and the commit handler
re-runs lock acquisition from scratch. This is safe because lock acquisition is a **pure
conflict check** — it never looks at object liveness — so it produces identical results
even though the perpetual DB may already contain the effects of the replayed transactions
(inputs consumed, objects gone). Because the watermark and the locks are written in one
batch, "the commit's locks are durable" ⇔ "the commit will not be replayed"; there is no
partially-recovered lock state.

**Epoch-DB vs perpetual-DB divergence.** The perpetual DB is always *ahead or equal*: the
checkpoint executor commits transaction outputs (perpetual) *before* it advances the
quarantine watermark that flushes locks (epoch). The reverse direction can't happen. The
tolerated divergence — outputs durable, locks not — is exactly what liveness-free replay
handles. Across epochs, the divergence is by design: markers persist, locks evaporate with
the old epoch DB, so an object locked-but-not-consumed in epoch N is lockable by a
different transaction in epoch N+1.

---

## Part 2 — Details (for a future AI working on the replacement)

### 1. Physical definitions

**`live_owned_object_markers`** — `crates/sui-core/src/authority/authority_store_tables.rs:76-81`

```rust
#[rename = "owned_object_transaction_locks"]
pub(crate) live_owned_object_markers: DBMap<ObjectRef, Option<LockDetailsWrapperDeprecated>>,
```

- `LockDetailsWrapperDeprecated::V1(LockDetailsV1Deprecated { epoch, tx_digest })` is
  defined at the bottom of `authority_store.rs` (~line 1673). New code always writes
  `None`. A `Some(...)` value can only exist in a DB written before the lock split
  (pre-2024 era; the split was gated by the now-deprecated
  `EpochFlag::_ObjectLockSplitTablesDeprecated`). The only code that looks at the value is
  the conflict guard in `initialize_live_object_markers` (errors with
  `ObjectLockAlreadyInitialized` if it finds `Some`); everything else checks key existence.
- RocksDB config: `owned_object_transaction_locks_table_config`
  (`authority_store_tables.rs:952`) — write-throughput-optimized, block cache from env
  `LOCKS_BLOCK_CACHE_MB` (default 1 GiB), range deletions respected.
- TideHunter config (`authority_store_tables.rs:257-287`): keyspace
  `owned_object_transaction_locks`, `KeyIndexing::key_reduction(80, 16..64)` — the index
  key keeps serialized-ObjectRef bytes [16..64), i.e. drops the first 16 bytes of the
  ObjectID and the tail of the digest. Consequence: the index preserves per-object grouping
  and version order only through the ObjectID's *last* 16 bytes (relies on their
  uniqueness), which is what makes `reversed_safe_iter_with_bounds` in
  `get_latest_live_version_for_object_id` still work. `mutexes * 16`, bloom filter, high
  dirty-key allowance — it is tuned as a write-heavy table.

**`owned_object_locked_transactions`** — `crates/sui-core/src/authority/authority_per_epoch_store.rs:428-430`

```rust
#[default_options_override_fn = "owned_object_transaction_locks_table_default_config"]
owned_object_locked_transactions: DBMap<ObjectRef, LockDetailsWrapper>,
```

- `LockDetailsWrapper::V1(TransactionDigest)`, `LockDetails = TransactionDigest`
  (`authority_per_epoch_store.rs:3410-3451`). Reads go through `.migrate().into_inner()`
  (migration is a no-op today; the wrapper exists for future versioning).
- RocksDB config (`authority_per_epoch_store.rs:544-553`): same
  `LOCKS_BLOCK_CACHE_MB`-driven config as the perpetual table.
- TideHunter config (`authority_per_epoch_store.rs:592-601`): `KeyIndexing::Hash` —
  **point lookups only, no ordered iteration is possible**, which is fine because nothing
  ever iterates this table. `mutexes * 8`, bloom filter.
- The epoch DB lives in `<epochs-dir>/epoch_<N>`; old directories are deleted by
  `AuthorityPerEpochStorePruner` (`authority_per_epoch_store_pruner.rs`). That deletion is
  the only "release" of locks.

### 2. Exactly which refs end up in each table

**Markers** (`TransactionOutputs::build_transaction_outputs`, `transaction_outputs.rs:156-173`):

- `new_locks_to_init` = every written output object with `owner.is_address_owned()`, i.e.
  **strictly `Owner::AddressOwner`**. Not shared, not immutable, not `ObjectOwner` (child),
  not `ConsensusAddressOwner`, not `Party`.
- `locks_to_delete` = mutable inputs whose owner was `AddressOwner`, **plus** received
  objects (Receiving args that were actually received — determined by membership in
  `effects.modified_at_versions()`). Receiving args that were *not* received keep their
  markers (comment at `authority_store.rs:838-839`).
- Deleted / wrapped objects: their input marker is deleted via `locks_to_delete` (they were
  address-owned mutable inputs); tombstones go to the `objects` table only. An object that
  is later unwrapped reappears in `written` and gets a fresh marker at its new version.
- **Genesis and snapshot-restore use a different filter**: `!object.is_child_object()`
  (`bulk_insert_genesis_objects` at `authority_store.rs:589-618`, snapshot restore's
  `insert_objects_chunk` at `authority_store.rs:657-703`, called from
  `sui-snapshot/src/reader.rs:552` with `is_force_reset = true`). This writes markers for
  **shared, immutable and package objects too**. Those extra markers are never deleted
  (those objects are never address-owned mutable inputs), so:
  - a genesis/restored node's marker table is a strict superset of an organically-grown
    one;
  - a shared object created at genesis keeps a marker at its genesis version forever while
    its real version advances — a permanently *stale* marker.
  This is benign today only because all production readers query refs that transactions
  claim as address-owned inputs. Do **not** treat the marker table as an exact index of
  live owned objects.
- `insert_genesis_object`/`insert_object_direct` (single-object variant, slightly different
  filter again: `get_single_owner().is_some() && !is_child_object()`) is test-only
  ("TODO: delete this method entirely").

**Locks** (`owned_object_refs_to_lock`, `consensus_handler.rs:3206-3229`):

- Key set = all `InputObjectKind::ImmOrOwnedMoveObject` refs of the transaction
  (**including gas coins**) minus the transaction's *claimed immutable* object IDs.
- The immutable-claims come from `UserTransactionV2` (`PlainTransactionWithClaims`); they
  are attested by the submitting validator and verified against real objects at vote time
  by `SuiTxValidator` (`consensus_validator.rs:~350-395`). This exists precisely so the
  commit handler never has to read the object store to distinguish immutable from owned.
  An unclaimed-but-actually-immutable input would be locked unnecessarily (harmless).
- The locked ref may **not** be address-owned by execution time (e.g. the object became
  `ConsensusAddressOwner` between signing and execution — see the re-audit comment in
  `execute_certificate`, `authority.rs:1955-1966`). Locks are conflict bookkeeping, not
  ownership statements; the key sets of the two tables are related but not identical.

### 3. Write paths

**Markers — execution/commit pipeline (all node types):**

1. Execution writes `TransactionOutputs` into the writeback cache **dirty set only**
   (`WritebackCache::write_transaction_outputs`). Nothing durable yet; the coherent
   `object_by_id_cache` is updated so readers see the new state.
2. The checkpoint executor, per checkpoint and in sequence-number order
   (`checkpoint_executor/mod.rs:415-453`): `build_db_batch(epoch, tx_digests)` →
   `AuthorityStore::write_one_transaction_outputs` per tx, which in **one `DBBatch`**
   writes effects, executed_effects, transactions, objects, per-epoch markers, events, and:
   - `initialize_live_object_markers_impl(batch, new_locks_to_init, is_force_reset=false)`
     (`authority_store.rs:836`)
   - `delete_live_object_markers(batch, locks_to_delete)` (`authority_store.rs:840`)
   Then `commit_transaction_outputs` writes the batch and moves entries dirty→cached.
3. Idempotency: re-committing the same checkpoint after a crash rewrites identical data.
   Initializing a marker over an existing `None` marker is fine (the
   `ObjectLockAlreadyInitialized` error fires only on legacy `Some` values); deletes are
   idempotent.
4. `is_force_reset=true` (snapshot restore) skips the legacy-value check entirely.

**Locks — consensus commit pipeline (validators only):**

1. `ConsensusCommitHandler::filter_consensus_txns` (`consensus_handler.rs:2452-2781`):
   - Prefetches the cross-commit lock state for **all** owned refs of all
     `UserTransactionV2`s in the commit in one batched read:
     `get_owned_object_locks_map(&prefetch_refs)` (`consensus_handler.rs:2476-2495`).
     NOTE: a DB read error here degrades to "treat everything as unlocked"
     (`.unwrap_or_default()`) — a deliberate leniency inherited from the old
     per-transaction read; it is a (theoretical) determinism/fork hazard to be aware of.
   - Per accepted user transaction, **after** all other drop-filters (reconfig state,
     end-of-publish, deprecated kinds):
     `try_acquire_owned_object_locks_post_consensus(refs, digest, current_commit_locks, existing_locks)`
     (`authority_per_epoch_store.rs:1825-1861`). This is a **pure function** over the two
     maps: conflict iff some ref is held by a *different* digest. Same-digest re-lock
     succeeds (idempotency for duplicate sequencing and replay). **No liveness check, no
     object reads.**
   - Winner: locks accumulate into the commit-local map, status `Finalized`.
   - Loser: dropped — `ConsensusTxStatus::Dropped`, reject reason recorded
     (`set_rejection_vote_reason`), key still marked consensus-processed (so waiters
     resolve and resubmission is suppressed), metric
     `consensus_handler_dropped_transactions{reason="lock_conflict"}`.
2. Lock acquisition happens **before deduplication** (`handle_consensus_commit`,
   `consensus_handler.rs:1165-1185`): a transaction sequenced in two commits re-locks
   identically in the second commit and is then deduped from scheduling.
3. Deferral interaction: congestion-control deferral happens *after* filtering, so
   **deferred transactions hold their locks from first appearance**; when re-loaded from
   the deferred-transactions table in a later commit they do *not* re-acquire.
4. The accumulated map goes into `ConsensusCommitOutput::set_owned_object_locks`
   (`consensus_quarantine.rs:265`) and the whole output is pushed into the quarantine
   (`consensus_handler.rs:1305` → `push_consensus_output`).
5. There is **no delete path** during the epoch (only a `#[cfg(test)]` helper,
   `delete_object_locks_for_test`). The table is append-only within an epoch.

### 4. Read paths (complete inventory)

**Markers:**

| Caller | Path | Notes |
| --- | --- | --- |
| `execute_certificate` → `check_owned_locks` (`authority.rs:1666-1669, 1968-1971`) | `ObjectCacheRead::check_owned_objects_are_live` | Runs for **every** certificate execution on every node type, right before the VM. Failure → `ExecutionOutput::Fatal` → `fatal!` (process abort, `execution_driver.rs:134-136`). |
| `WritebackCache::check_owned_objects_are_live` (`writeback_cache.rs:1951-1980`) | cache-first, DB fallback | Hits `object_by_id_cache` per ref; only cache **misses** reach `AuthorityStore::check_owned_objects_are_live` (`authority_store.rs:927-945`) which `multi_get`s the marker table. |
| `AuthorityStore::get_latest_live_version_for_object_id` (`authority_store.rs:893-921`) | reverse scan over markers bounded by `(id, MAX, MAX)` | Error-path enrichment only: distinguishes `ObjectVersionUnavailableForConsumption { current_version }` (marker exists at another version) from `ObjectNotFound` (no marker at all). Depends on ≤1 marker per object id — true organically, violated only by genesis/restore junk entries (§2). |
| `ObjectCacheRead::get_lock` (`writeback_cache.rs:1902-1939`, `authority_store.rs:861-890`) | markers + epoch lock table | **Unit-test-only.** `handle_object_info_request` now returns `lock_for_debugging: None` unconditionally (`authority.rs:3417-3419`). `ObjectLockStatus`, `LockDetailsDeprecated`, and `ObjectLocks::get_transaction_lock` exist only for this path. |

Note on the DB fallback (correction, verified 2026-07-10): `get_object_by_id_cache_only` →
`get_object_entry_by_id_cache_only` consults the moka-backed `object_by_id_cache` **and,
on miss, the `dirty.objects` set and the committed object cache**
(`with_locked_cache_entries`, `writeback_cache.rs:777-786`). So a `CacheResult::Miss` here
means the object has no in-memory state at all and the durable DB is authoritative — there
is no dirty-eviction hole. A replacement that answers liveness from "objects" gets this
for free by using `get_object_impl` (object-by-id cache → dirty → cached → DB), which is
the standard latest-object read path.

**Locks:**

| Caller | Path | Notes |
| --- | --- | --- |
| Consensus handler prefetch (`consensus_handler.rs:2476-2495`) | `get_owned_object_locks_map` → `get_owned_object_locks` (`authority_per_epoch_store.rs:1786-1812`) → quarantine `do_fallback_lookup` (`consensus_quarantine.rs:799-819`) | Quarantine in-memory map first, epoch DB fallback. "After crash recovery, quarantine is empty so we naturally fall back to DB." |
| Submit path (`authority_server.rs:963-1020`) | same quarantine-aware read + `try_acquire_owned_object_locks_post_consensus` with empty `current_commit_locks` | Runs only for digests already consensus-processed but without effects. Purpose: convert the generic retriable "TransactionProcessing" suppression into a terminal `ObjectLockConflict { pending_transaction }` so clients stop retrying a dropped conflict-loser **before the winner executes** (while the loser's inputs still validate as live). After the winner executes, the subsequent `handle_vote_transaction` revalidation produces the terminal stale-version error instead. |
| `ObjectLocks::get_transaction_lock` (`execution_cache/object_locks.rs:23-29`) | raw `tables.get_locked_transaction` — **bypasses the quarantine** | Only reachable from test-only `get_lock`. Inconsistency worth knowing about, not fixing. |
| Test helpers | `insert_object_locks_for_test` / `delete_object_locks_for_test`, direct map asserts in `consensus_handler.rs` tests | |

**What does *not* read locks or markers:** transaction signing/voting. Since
post-consensus locking, `handle_vote_transaction` (`authority.rs:1152-1210`; called from
`SuiTxValidator::vote_transaction` and the submit path) validates owned inputs with
`validate_owned_object_versions` (`execution_cache/object_locks.rs:89-105`), which reads
**live objects** via the object cache and compares version+digest. The objects table is
already the source of truth at vote time. Fullnode-side pre-validation
(`check_transaction_validity`) likewise uses live objects only.

**Why `validate_owned_object_versions` exists (the liveness/conflict split).** Post-consensus
lock acquisition checks *only conflicts, never liveness* — it would happily lock a version
that never existed, a wrong digest, or a version consumed in a previous epoch. Liveness
must therefore be verified somewhere, and it **cannot** be verified post-consensus:
liveness is time-dependent (an input can be consumed between vote and commit, and
crash-replay re-processes commits whose winners' inputs are already durably consumed), so
a liveness check in the commit handler would give different answers on replay → different
Finalized/Dropped sets → checkpoint fork. Votes are allowed to be time-dependent because
quorum certification is what makes the aggregate deterministic. Hence the split:
**liveness pre-consensus** (per-validator vote, objects-based, non-deterministic OK),
**conflicts post-consensus** (deterministic, append-only lock table). The two compose with
no gap because "live at vote time" can only become stale via consumption *within the same
epoch* (invariant 3: consumed-within-epoch ⇒ locked), which the conflict check covers.
It also keeps object reads out of the CPU-bound commit handler (same reason
immutable-object claims exist) and rejects invalid-version spam before it consumes
consensus bandwidth. This makes the execution-time `check_owned_objects_are_live` purely
a third-layer defense-in-depth assert, not a primary guard.

### 5. Durability model and the quarantine (the epoch-DB side of divergence)

`ConsensusOutputQuarantine` (`consensus_quarantine.rs:484-747`) holds each commit's
`ConsensusCommitOutput` in memory:

- On push, locks are mirrored into a flat `owned_object_locks: HashMap<ObjectRef,
  LockDetails>` used by reads (`insert_owned_object_locks`). On flush they are removed
  from the map (safe even with duplicate same-digest entries across commits, because the
  DB then serves the same value).
- **Flush rule** (`commit_with_batch`, `consensus_quarantine.rs:589-670`): when
  `handle_finalized_checkpoint` advances `highest_executed_checkpoint`, pop builder
  summaries with `seq ≤ highest_executed`, derive the highest committed checkpoint
  *height*, then flush the queue **prefix** up to the last output within that height whose
  `checkpoint_queue_drained` flag is true (a "drain point": after that commit's checkpoint
  flush there were no pending roots, so nothing built later can reference it). Outputs past
  the last drain point stay quarantined and are fully replayed on restart.
- Each flushed `ConsensusCommitOutput::write_to_batch` (`consensus_quarantine.rs:270-406`)
  writes, in **one `DBBatch`**: `consensus_message_processed`, `end_of_publish`,
  reconfig state, **`last_consensus_stats_v2`** (the replay watermark),
  `next_shared_object_versions_v2`, **`owned_object_locked_transactions`**, deferred-tx
  add/removes, randomness/DKG/JWK state, congestion debts. The same batch (built in
  `handle_finalized_checkpoint`, `authority_per_epoch_store.rs:1959-1991`) also deletes
  `signed_effects_digests` for the finalized digests.
- Therefore: **a commit's locks are durable ⇔ the commit is marked processed ⇔ the commit
  will not be replayed.** There is no partial per-commit lock durability.
- There is also an opportunistic flush inside `push_consensus_output` for the case where
  state sync / checkpoint execution ran ahead of local consensus.

**Ordering vs the perpetual DB.** In the checkpoint executor pipeline
(`checkpoint_executor/mod.rs:415-457`) the order per checkpoint is strictly:
`build_db_batch` → `commit_transaction_outputs` (perpetual DB durable: objects, effects,
**markers**) → `handle_finalized_checkpoint` (epoch DB durable: **locks**, watermark). The
flush gate additionally requires the commit's checkpoints to be *executed*, and the drain
rule requires all the commit's roots to be inside built checkpoints at or below that
height. Net invariant:

> **For every flushed commit, all of its finalized transactions' outputs are already
> durable in the perpetual DB.** The epoch DB's lock state never runs ahead of the
> perpetual DB; it only lags.

### 6. Restart recovery, step by step (validator)

1. `AuthorityPerEpochStore::new` reopens the epoch DB and builds an **empty** quarantine
   seeded with `highest_executed_checkpoint` from the checkpoint store
   (`authority_per_epoch_store.rs:1010-1019`). No lock state is loaded into memory —
   reads just fall through to the DB.
2. `ConsensusCommitHandler::new` recovers `last_consensus_stats` from the epoch DB
   (`consensus_handler.rs:859-867`) — the value last written by a quarantine flush.
3. `consensus_manager/mod.rs:302-309`: consensus is asked to replay from
   `last_processed_commit_index - consensus_num_requested_prior_commits_at_startup`.
   Commits `≤ last_processed_at_startup` go to `handle_prior_consensus_commit`
   (state-observation only, no lock effects); commits above it are re-processed by the
   full `handle_consensus_commit` path (`consensus_handler.rs:3156-3170`).
4. Replayed commits re-run `filter_consensus_txns`: the prefetch sees exactly the flushed
   lock state (quarantine empty → DB), and `try_acquire_owned_object_locks_post_consensus`
   re-derives identical Finalized/Dropped decisions because it is deterministic and
   liveness-free. Locks re-enter the quarantine and will re-flush.
5. Already-executed replayed transactions are not re-executed: scheduling/execution dedups
   on `executed_effects` (`is_tx_already_executed`), and effects-equivocation is prevented
   independently by `signed_effects_digests` (entries for un-finalized txns survive
   restart). Pending checkpoints are rebuilt by the same replay ("full-replay ... with
   correct root reconstruction", `consensus_quarantine.rs:636-640`).
6. Perpetual side: if the crash hit between `commit_transaction_outputs` and the
   checkpoint-store watermark bump, the checkpoint executor simply re-commits the same
   checkpoint — marker init/delete are idempotent (§3).

Fullnodes: nothing to recover — they never populate the lock table, and markers are
maintained purely by (re-)executing checkpoints.

### 7. Epoch boundary

At reconfiguration, with all execution paused under the execution write-lock:

- `clear_state_end_of_epoch_impl` (`writeback_cache.rs:1315-1357`) **discards** all dirty
  (executed-but-not-checkpointed) transaction outputs and invalidates their cache entries.
  Those transactions' marker/object writes never become durable — this replaced the old
  `revert_state_update` machinery; there is no on-disk revert anywhere.
  (`object_locks.clear()` is an explicit no-op; the "per-epoch marker tables" cleared next
  are `object_per_epoch_marker_table{,_v2}` — *different tables*, do not confuse them with
  the live markers.)
- The new `AuthorityPerEpochStore` opens a fresh `epoch_<N+1>` directory: the lock table
  starts empty. Equivocation protection deliberately resets: an object locked but never
  consumed in epoch N can be locked by a *different* transaction in epoch N+1.
- Old epoch directories (and all their locks) are removed later by
  `AuthorityPerEpochStorePruner`.
- The perpetual markers carry across epochs untouched. (The table comment "for old epochs
  it may also contain the transaction they are locked on" refers to pre-split legacy
  values, which nothing reads.)

### 8. Divergence matrix

| # | Scenario | State | How it is handled |
| --- | --- | --- | --- |
| D1 | Crash after `commit_transaction_outputs`, before quarantine flush | Perpetual has outputs (markers updated, inputs gone); epoch DB has no locks, watermark not advanced | Consensus replays the commit; lock acquisition is liveness-free and deterministic → same decisions; execution layer dedups already-executed txns. **This is the load-bearing divergence.** |
| D2 | Epoch DB ahead of perpetual for a commit | — | Cannot happen: flush is gated on the commit's checkpoints being built *and executed*, and outputs are committed earlier in the same pipeline (§5). |
| D3 | Crash between perpetual commit and checkpoint-store `highest_executed` bump | Outputs durable, watermark stale | Checkpoint re-executed/re-committed on restart; all writes idempotent. `highest_committed_checkpoint` (singleton in the same perpetual batch) exists for consumers that can't tolerate even this lag. |
| D4 | Locks exist for transactions that never execute (deferred at epoch end, conflict losers' *winners* deferred, cancelled) | Epoch DB has locks; perpetual never sees the tx | Benign within the epoch (that's the point: keep losers out); resolved by epoch-DB deletion at reconfig. Deferred txns themselves are durable in `deferred_transactions_with_aliases_v3` (flushed in the same batches) and re-loaded on restart via `ConsensusOutputCache::new`. |
| D5 | Executed-but-unfinalized txns at epoch end | Markers/objects updated only in the dirty cache | Dirty cache dropped at reconfig (§7); durable state = exactly the checkpointed prefix. |
| D6 | Marker table contains genesis/snapshot junk (shared/immutable/package refs, possibly stale versions) | Markers ⊃ live address-owned refs | Benign: production readers only query refs claimed as address-owned inputs (§2, §4). |
| D7 | Dirty-window liveness reads | Durable markers lag executed state | Bridged by the in-memory read path: object_by_id cache plus, on miss, the `dirty.objects` set (§4 correction). The DB fallback is only reached for objects with no in-memory state, for which durable state is authoritative. |
| D8 | Cross-epoch: locked-in-N, consumed-in-N+1 by another tx | Marker live throughout; N's lock gone | By design — per-epoch equivocation scope. Vote-time validation in N+1 checks liveness against objects, and N+1's lock table starts clean. |

### 9. Invariants the current design relies on (the replacement must preserve or consciously re-derive)

1. **Determinism across validators and across replay** — the Finalized/Dropped decision
   per transaction feeds checkpoint contents. Any nondeterminism here is a fork. This is
   node-local state (not protocol-visible), so a redesign needs no protocol gate *iff* its
   decisions are bit-identical in all reachable states — including edge behaviors like the
   lenient `unwrap_or_default()` on prefetch read errors.
2. **Liveness-freedom of post-consensus acquisition** — replay (D1) executes lock
   acquisition for transactions whose inputs are already consumed durably. Any replacement
   check of the form "input must be live in the objects table" breaks replay for
   already-executed winners unless it treats "consumed" as "locked" correctly.
3. **Consumed-within-epoch ⇒ locked** — within an epoch, every consumption of an owned ref
   was performed by a finalized tx that holds the lock; so conflict-checking against locks
   subsumes liveness-checking. Safety against *stale-version* claims (versions consumed in
   earlier epochs, or never-existing versions) comes from vote-time validation: acceptance
   requires a quorum of accept votes, hence honest validators validated version+digest
   against live objects at vote time.
4. **Locks are never released within an epoch** — a conflict loser must stay dropped even
   after the winner executes and the objects table has moved past the contested version.
   With locks removed, "input version < live version" (objects table) reproduces the
   conflict verdict for *executed* winners; **deferred** winners are the case where the
   lock is the only record (their inputs are still live) — recoverable from the deferred
   table.
5. **Same-digest idempotency** — duplicate sequencing across commits and crash-replay both
   re-acquire; `lock_holder == self` must succeed.
6. **Flushed ⇒ durable** — every flushed commit's finalized txns are durably executed
   (§5). This is what would let a replacement rebuild "consumed by whom is irrelevant, just
   consumed" for the non-replayed history from the objects table + deferred table alone.
7. **Atomicity pairings** — markers with objects (one perpetual batch per checkpoint);
   locks with the replay watermark (one epoch batch per commit). The second pairing is what
   makes recovery all-or-nothing per commit.
8. **Exact-ref liveness assert before execution** — `check_owned_objects_are_live` is
   fatal-on-failure. A replacement either reimplements it against the objects table
   (latest-version lookup must consult dirty state, §4) or removes it deliberately.

### 10. Practical considerations for the replacement project

- **Startup rebuild recipe** for an in-memory lock map: (a) un-flushed commits — rebuilt
  for free by consensus replay (this is exactly how the quarantine map is rebuilt today);
  (b) flushed commits — every finalized tx is either durably executed (its consumed inputs
  are absent/superseded in the objects table → conflicts derivable without knowing the
  digest) or sitting in the deferred-transactions table (its owned refs re-derivable from
  the stored transactions). Cancelled transactions execute (consuming gas), so they fall
  under (a)/(b) like everything else.
- **What you lose without the digest**: `ObjectLockConflict { pending_transaction }` names
  the winner in client-facing errors and reject reasons; the submit-path pre-check
  (`authority_server.rs:963-1020`) uses it to emit a terminal error for conflict losers
  *before* the winner executes. Post-execution, the existing `handle_vote_transaction`
  revalidation already produces an equivalent terminal error from live-object state, so
  the gap is only the window between "winner finalized" and "winner executed" (and error
  message quality).
- **Consensus-handler hot path**: today's cost is one batched `multi_get` on the lock
  table per commit plus pure in-memory checks. The handler is single-threaded and
  CPU-bound at high TPS (see `docs/consensus-handler-optimization.md`); an objects-table
  latest-version prefetch for all owned refs of a commit would be strictly more expensive
  than the lock-table point-gets unless served from the in-memory structures. Keeping the
  whole current-epoch lock set in memory is the cheap option but is unbounded
  (≈ owned-input refs × finalized TPS × epoch seconds: at 15k TPS with ~2 owned refs/tx
  and 24h epochs that is ~2.6B entries × ~100B ≈ hundreds of GB — too big), which is
  presumably why the plan is objects-table-as-source-of-truth for the executed portion
  plus a bounded in-memory map for the un-executed portion (quarantine-resident commits +
  deferred txns) — mirroring invariant 6.
- **`get_latest_live_version_for_object_id` replacement**: `objects`-table reverse scan per
  id already exists (`get_latest_object_ref_or_tombstone`); on TideHunter the objects
  keyspace uses fixed 40-byte index keys, so per-id ordered scans work.
- **Deleting the tables**: both are plain struct fields + ThConfig entries; TideHunter
  supports outright table deletion (no `cfg` retention dance needed). For RocksDB,
  removing a `DBMapUtils` field means the CF is no longer opened — verify whether
  typed-store drops unknown on-disk CFs automatically or whether a manual drop/migration
  is needed for existing deployments. The `#[rename]`/deprecated-type machinery
  (`LockDetailsWrapperDeprecated`, `LockDetailsDeprecated`, `ObjectLockStatus`,
  `ObjectLocks::get_transaction_lock`, `ObjectCacheRead::get_lock`) all goes with them.
- **Snapshot restore / genesis** must stop writing markers (`insert_objects_chunk`,
  `bulk_insert_genesis_objects`); nothing else consumes what they write, so this is
  removal-only.
- **Test surface**: `#[cfg(test)]` helpers `insert/delete_object_locks_for_test`
  (`authority_per_epoch_store.rs:1640-1660`); unit tests calling `get_lock`
  (`authority_tests.rs`, `transaction_tests.rs`); consensus-handler tests asserting
  `get_owned_object_locks_map` contents and the `lock_conflict` drop metric; equivocation
  e2e/simtests.
- **Observability**: `consensus_handler_dropped_transactions{reason="lock_conflict"}`,
  `consensus_quarantine_queue_size`, cache metrics labeled `"lock"`/`"object_is_live"`
  (`record_db_get`/`record_db_multi_get` in the writeback cache).

### 11. One-paragraph history (why the code looks like this)

Originally validators acquired owned-object locks at signing time
(`acquire_transaction_locks`) and the perpetual table stored the locking tx (hence the
on-disk name `owned_object_transaction_locks` and the `Option<LockDetails…>` value). The
lock *contents* were then split into the per-epoch DB (epoch-scoped equivocation,
`EpochFlag::_ObjectLockSplitTablesDeprecated`), leaving the perpetual table as a bare
existence marker (renamed in code to `live_owned_object_markers`). With Mysticeti fastpath
removal (all user transactions through consensus), pre-consensus locking was disabled
entirely (#24676, Dec 2025) and replaced by post-consensus lock acquisition in the commit
handler; the leftover flag plumbing was removed in #25150 (Feb 2026). What remains is the
machinery described above: markers as an execution-time safety assert plus error
enrichment, and the epoch lock table as the post-consensus conflict arbiter.

---

## Part 3 — Replacement design: `owned_object_locked_transactions` → in-memory maps + objects-table verdict + `LiveObjectCache` (2026-07-15)

Assessment of the proposal: *"a `LiveObjectCache` primitive warmed by `SuiTxValidator`
when a transaction is first seen; the consensus handler consults the cache (falling back
to the objects table), caching both positive and negative results, with a metric for
remaining lookups; and address restartability given that handler state is per-epoch while
objects are perpetual."*

**Verdict: feasible, with no new durable state.** The durable lock table can be deleted
outright. But the proposal needs three amendments to be fork-safe, each derived below:

1. The objects-table verdict must be the **monotone version rule** `conflict ⟺
   latest_version(id) > claimed_version`, *not* a liveness check ("is this exact ref
   live") — liveness has a false-drop fork for pipelined transactions (§3.2).
2. Two gap fillers where the objects table cannot answer: a **deferred-locks map**
   (finalized-but-never-executed transactions are invisible to the objects table, §3.4)
   and a **same-digest carve-out via `executed_effects`** (duplicate sequencing across a
   restart, §3.3 case D).
3. A reachability cleanup: **strict immutable-claims voting** (today's exact-match
   check is skipped when the claims list is empty — `consensus_validator.rs:312`), plus a
   narrow **immutable backstop** table read that makes the swap unconditionally
   equivalent to the old logic, so no protocol flag is needed (§3.6a).

Verified production usage as of today (branch `andll/deprecate-live-owned-object-marker-reads`):

| Site | What it does |
| --- | --- |
| `authority_per_epoch_store.rs:436` | table definition; ThConfig at `:600`; point get/multi-get at `:807`/`:814`; test helpers at `:1659-1670` |
| `consensus_handler.rs:2366-2385` | per-commit batched prefetch of cross-commit lock state (`get_owned_object_locks_map(...).unwrap_or_default()`) |
| `consensus_handler.rs:2604-2629` | per-tx `try_acquire_owned_object_locks_post_consensus` (pure function over the two maps, `authority_per_epoch_store.rs:1839`) |
| `consensus_quarantine.rs:311` | the only write — quarantine flush batch, atomic with `last_consensus_stats_v2` |
| `consensus_quarantine.rs:799-819` | quarantined-map-first read with DB fallback (`insert`/`remove_owned_object_locks` at `:736-746` — the map holds exactly the un-flushed window) |
| `authority_server.rs:1002-1003` | submit-path pre-check to give clients a terminal `ObjectLockConflict` before the winner executes |

### 3.1 The decision procedure

Per commit (same batched shape as today's prefetch): collect all `owned_object_refs_to_lock`
refs; then per transaction, per ref, in order:

1. **`current_commit_locks`** (unchanged) — refs locked earlier in this commit. Different
   digest ⇒ conflict; same ⇒ ok.
2. **Quarantine map** (`owned_object_locks`, unchanged, keyed by full `ObjectRef`) — locks
   of finalized txs in un-flushed commits. Different digest ⇒ conflict.
3. **Deferred-locks map** (new, §3.4) — owned refs of currently-deferred transactions.
   Different digest ⇒ conflict.
4. **Objects-table verdict** (new; through `LiveObjectCache`, §3.5, falling back to the
   real latest-object read): `latest_version(id) > claimed_v` ⇒ *tentative conflict*;
   otherwise (absent, `<`, or `==`) ⇒ no conflict.
5. **Same-digest carve-out** (only on tentative conflict from step 4): if
   `executed_effects` contains the claimant's own digest, the claimant already executed —
   treat as a successful same-digest re-acquire (§3.3 case D). Otherwise: conflict, drop.

Winners insert their refs into `current_commit_locks` and, at commit end, into the
quarantine output exactly as today. The flush batch simply stops writing the lock table;
`last_consensus_stats_v2` and everything else in the batch is unchanged, so the flush
gate (commit's checkpoints built *and executed*) keeps guaranteeing the invariant the
fallback relies on: **flushed ⇒ every finalized tx of the commit is durably executed**.

### 3.2 Why the monotone version rule, not liveness

A liveness check ("claimed ref is the live version") forks on pipelined submissions.
Client submits tx1, then tx2 consuming tx1's output `(id, v)`. tx2 can be finalized once
2f+1 validators have executed tx1 and vote accept. When the handler on a *lagging*
validator (tx1 finalized but not yet locally executed) processes tx2, `(id, v)` does not
exist in its objects table; a liveness rule would drop tx2 there and finalize it on
up-to-date validators — checkpoint fork. Under the version rule the lagging validator
sees `latest(id) < v` (or absent) ⇒ no conflict — same verdict as the up-to-date
validator's `latest == v`. Note the old design agrees: lock acquisition never looked at
objects at all, so tx2 simply acquired.

The version rule is safe to evaluate against a *moving* objects table because of a
freshness sandwich proven in §3.3: whenever the two sides of the comparison could differ
across validators, the ref is guaranteed to be covered by an earlier (memory) layer.

### 3.3 Determinism argument

Definitions: at a given consensus position, let S = the map {owned ref → digest} of all
finalized txs so far this epoch (the abstract state the old table materialized). The
procedure must compute "ref ∈ S with different digest" identically on every validator, in
every reachable state, including crash-replay and restarts.

Load-bearing invariants (all verified in code today):

- **(I1) Execution always bumps every locked ref.** `ensure_active_inputs_mutated`
  (`temporary_store.rs:424`) runs on *every* execution path — success, Move abort,
  out-of-gas (all three reset paths in `gas_charger.rs:395,510,517`), congestion/randomness
  cancellation, and the IFFW short-circuit (`execution_engine.rs:422`). Deleted/wrapped
  inputs get tombstones at the lamport version. So for an executed tx, every ref in its
  lock set satisfies `latest(id) > v` — *provided the lock set contains no immutable
  objects*, which is what strict claims voting guarantees (§3.6): immutable inputs are
  excluded from `exclusive_mutable_inputs` (`transaction.rs:5056`) and never bump.
- **(I2) Flushed ⇒ durably executed** (unchanged flush gate, §5 of Part 2).
- **(I3) Quorum-unreachability of stale claims.** A finalized `UserTransactionV2` needs
  2f+1 accept votes; ≥ f+1 honest voters ran `validate_owned_object_versions`
  (`object_locks.rs:92`) against live objects *in this epoch* (epoch N starts only after
  epoch N-1 is fully executed everywhere). So no finalized tx claims a ref consumed in a
  prior epoch, a never-existing version, or a wrong digest.
- **(I4) Post-restart coverage.** Consensus replays every commit after
  `last_consensus_stats_v2`; the quarantine map is rebuilt by that replay (this is already
  how it works). So the union quarantine-map ∪ deferred-map ∪ {flushed executed txs}
  covers all of S.

Case analysis for a ref `(id, v)` reaching step 4 (missed all memory layers), where some
finalized tx W ≠ claimant consumed `(id, v)` this epoch (i.e. the true verdict is
conflict): W is not in the memory layers ⇒ by I4, W's commit was flushed pre-restart ⇒ by
I2 W executed before the restart ⇒ by I1 `latest(id) > v` durably, on this validator, at
all times after restart ⇒ step 4 says conflict. ✓ Conversely if no such W exists: any
`latest(id) > v` observation would imply a this-epoch consumer (prior-epoch consumers are
quorum-unreachable by I3 — the claimant got finalized) — contradiction; so every validator
observes `latest(id) ≤ v` or absent at step 4 ⇒ no conflict. ✓ The objects table *does*
move underneath the handler (execution is async), but only via this-epoch finalized
consumers, and those are always in a memory layer on the validators where they haven't
yet reached the objects table — the comparison can never straddle the transition.

**Case D — duplicate sequencing across restart** (the one place the pure version rule
diverges from the old table): tx X finalized in commit A, commit A flushed, X executed,
crash, restart; X appears again in commit B (post-watermark, replayed). Old design:
same-digest re-lock succeeds ⇒ Finalized (then deduped from scheduling). New step 4:
`latest > v` ⇒ tentative conflict — wrong, and non-restarted validators (memory hit,
same digest) would say Finalized ⇒ fork. The carve-out repairs it: X's own
`executed_effects` are durable (I2 applied to commit A), so restarted validators also
conclude Finalized. The carve-out cannot mask a real conflict: if a *different* winner W
holds a ref and claimant X also has executed effects, X must itself have acquired that
ref earlier — impossible while W holds it. And it does not resurrect previously-*dropped*
duplicates: dropped txs never execute, so they have no effects and still fall through to
conflict (matching the old table, which still holds W's digest).

### 3.4 Deferred-locks map

Deferred transactions are the one class of finalized txs that hold locks but may not
execute for a long time (congestion deferral) — invisible to the objects table, and their
quarantine entries flush away. Today the durable table covers them; the replacement needs
an explicit in-memory map: owned refs → digest for every currently-deferred transaction.

- **Maintenance:** insert when a commit defers a tx; remove when the tx is re-loaded into
  a commit (at which point it either re-locks into that commit's output as a finalized tx,
  is re-deferred, or is cancelled — and cancelled txs execute, so I1 covers them after).
- **Startup:** seed from `deferred_transactions_with_aliases_v3`, which is already loaded
  wholesale at startup (`ConsensusOutputCache::new`, `consensus_quarantine.rs:431-438`);
  owned refs are derivable from the stored transactions via `owned_object_refs_to_lock`.
- **Bound:** proportional to the deferred backlog (small; already bounded by congestion
  control), not to epoch length.
- Epoch-end: deferred txs that never re-load are cancelled or dropped at reconfig along
  with all per-epoch state — nothing to clean up, same as today.

### 3.5 `LiveObjectCache` — semantics that make it correct with zero invalidation

The objects-table read in step 4 is the only remaining DB touch, and it sits on the
CPU-bound single-threaded handler (`docs/consensus-handler-optimization.md`). The cache's
job is to make it a hash lookup. The crucial design choice: the cache is a **monotone
lower bound on latest versions**, not a snapshot of liveness:

- Entry: `ObjectID → LowerBound` where `LowerBound ∈ {KnownAbsent, Version(v)}`;
  `KnownAbsent < Version(_)`, merges are max-merge (never decrease). No digests, no
  ownership, no per-ref keys.
- **Warm sources:** (a) the vote path — `validate_owned_object_versions` and
  `verify_immutable_object_claims` already hold the loaded objects (`object_locks.rs:97`,
  `consensus_validator.rs:357`); warm on *accept and reject alike* — a rejection for
  `ObjectNotFound` warms `KnownAbsent` (this is the "negative result" that saves the
  pipelined-tx lookup), a version-mismatch rejection warms the actual latest. (b) step-4
  fallback reads. (c) optionally, execution output commits (free monotone bumps).
- **Verdict from a hit (corrected 2026-07-15 during implementation):**
  - `cached > v` ⇒ conflict is *safe to conclude*: cached is a lower bound, so
    `true_latest > v`.
  - `cached ≤ v` ⇒ **not decisive** — an earlier draft argued this implies no conflict
    because any post-warm consumer would be in the quarantine map; that is wrong: the
    quarantine holds only *un-flushed* commits, so a consumer that finalized, executed,
    and had its commit flushed after the warm leaves no in-memory trace while the stale
    bound still reads `== v`. All potential-clears therefore take the authoritative
    latest-object read (object-by-id cache → dirty → DB — usually a memory hit), which
    is immune (flushed ⇒ executed ⇒ bumped). A clear-side shortcut becomes sound by
    bumping the cached bound of every flushed lock ref to `version+1` at the moment the
    quarantine drops it (cache bump happens-before quarantine removal ⇒ any resolution
    missing the quarantine sees the bump) — deliberately left out of PR 1 to keep its
    correctness argument minimal; candidate follow-up alongside PR 2.
  - Cache **miss** ⇒ authoritative read, same as above. `source=objects_db` in the
    resolution metric counts these authoritative reads.
- Consequences: no invalidation protocol, LRU/TTL eviction is always safe (eviction just
  converts hits into metered fallbacks), restart-cold is safe, and the cache can live at
  the `AuthorityState`/cache layer with **no epoch scoping** — version lower bounds are
  valid forever (perpetual semantics), which neatly sidesteps the per-epoch vs perpetual
  split for the cache itself. Clearing it at epoch boundaries is optional hygiene.
- Sizing: it needs to retain the vote→commit window. At 15k TPS × ~3 owned refs × ~60 s
  of in-flight window ≈ 2.7M entries ≈ low hundreds of MB with headroom; cap and let the
  metric confirm.
- Warm-coverage note: every validator votes on every block it verifies
  (`verify_and_vote_batch`), so vote-time warming covers essentially all refs the handler
  will query — the residual DB lookups are restarts (cold cache + replay), commits
  containing blocks the validator never voted on (catch-up paths), and evictions.

A leaner v0 alternative: skip the dedicated primitive and let step 4 call the existing
`multi_get_objects` path (`object_by_id_cache` → dirty → DB) with the metric attached.
That already serves hot objects from memory; what it lacks is negative caching (pipelined
inputs miss to a DB reverse-seek every time) and eviction isolation from general read
traffic. Ship the metric first; add the dedicated cache if the residual is material.

### 3.6 Behavior deltas, reachability cleanup, and rollout

The Finalized/Dropped decision feeds checkpoints, so old and new procedures must agree in
every *reachable* state, including mixed fleets. Deltas found:

1. **Unclaimed-immutable inputs — reachable today, must be closed first.**
   `verify_immutable_object_claims` is exact-match both directions
   (`ImmutableObjectNotClaimed`, `consensus_validator.rs:411-419`) but is only invoked
   when the claims list is non-empty (`:312`). A tx with immutable inputs and an *empty*
   claims list passes voting (immutable objects always match version/digest), gets its
   immutable refs locked, and — since immutable refs never bump (I1 fails for them) — the
   new fallback would say "no conflict" where the old table says "conflict", *and* the new
   design disagrees with itself across a restart. Two-part fix: (a) drop the `is_empty()`
   short-circuit so honest validators vote-reject unclaimed-immutable inputs, making such
   finalized txs quorum-unreachable — cheap (the objects are already loaded in the vote
   path), and byzantine-only exposure (claims are attached by the *submitting validator*;
   an honest submitter cannot under-claim, since owner changes bump versions and would
   fail vote-time version checks instead); (b) until (a) is universal, the **immutable
   backstop** (§3.6a) covers the case exactly.
2. **Garbage refs** (never-existing versions, wrong digests, prior-epoch-consumed):
   verdicts differ from the old table in principle but are quorum-unreachable (I3) —
   mixed fleets agree on all reachable inputs.
3. **Prefetch read-error leniency.** Today a lock-table read error degrades to
   "everything unlocked" (`consensus_handler.rs:2380-2384`) — already a theoretical fork
   hazard. The replacement must **fail-stop** on step-4 read errors instead
   (`expect`/`fatal!`): a validator that cannot answer deterministically must not answer.
4. **Conflict-error digest quality.** `ObjectLockConflict { pending_transaction }` names
   the winner. Memory layers still have digests; the step-4 path recovers the winner
   from the latest object's `previous_transaction` (multi-hop consumption names a
   downstream consumer; deletion tombstones store no digest → `ZERO`). Affects error
   text and the submit-path pre-check (`authority_server.rs`) — that path is best-effort
   client UX, not determinism-critical, and reuses the same layered read. One observable
   submit-path change (implemented, test updated): a consumed input now reports
   `ObjectLockConflict` naming the consumer even when the winner has already executed,
   where the old path fell through to revalidation's
   `ObjectVersionUnavailableForConsumption`. Both are terminal; the new error is more
   informative.

### 3.6a Cutover without a protocol flag (revised 2026-07-15)

An earlier draft gated the read-swap on a protocol feature flag. It is unnecessary: the
new procedure can be made **unconditionally bit-identical to the old one** with a single
narrow backstop, because a case analysis of the fallback shows only one state where a
flushed lock can hide from the memory layers + objects verdict. When the fallback reads
the latest object for claimed `(id, v)`:

- `latest > v` → conflict either way (I1).
- `latest == v`, owner **owned** → a flushed lock on `(id, v)` is impossible: flushed ⇒
  executed (I2) ⇒ would have bumped (I1). Deferred lockers (flushed-but-not-executed) are
  caught by the deferred map before the fallback.
- `latest < v` **or id absent** → a flushed lock is impossible by **prefix-flush
  ordering**: the locker's accept votes required `(id, v)` to exist at honest voters, so
  the producing tx was finalized in an earlier commit; the quarantine flushes commits
  strictly in order, so the locker cannot be flushed while the producer is not — and a
  flushed producer means the object exists locally at `≥ v`. (This is also why pipelined
  inputs never need a table read.)
- `latest == v`, owner **immutable** → the one undecidable case (an unclaimed-immutable
  lock, delta 1). **Backstop: point-read the lock table** (writes continue in PR 1, so it
  is complete) and conflict iff hit. Never fires for honest traffic — claimed immutables
  are excluded from lock sets before any lookup — so the hot path is untouched.

With the backstop, old and new verdicts agree in *all* states, not just quorum-reachable
ones, so PR 1 is a plain binary rollout: no protocol version, no epoch alignment, no
sequencing discipline. Two seams to implement carefully (the debug double-read
differentially tests both): deferred-map seeding must reproduce the exact lock set
(owned refs always; actually-immutable refs excluded — a byzantine under-claim that
slips through in the mixed era is covered by the backstop, and becomes impossible once
strict voting is universal); and a re-loaded deferred tx must re-acquire into the current
commit's locks when it leaves the deferred map, or there is a coverage gap between reload
and execution (`latest == v`, owned, no memory layer).

**PR 2 (deletion)** removes the backstop + table + writes + `LockDetailsWrapper` +
quarantine lock plumbing + debug compare. Its safety condition is the deploy discipline
already planned for the markers step 2: PR 1 everywhere (strict voting universal ⇒
unclaimed-immutable txs can no longer finalize) **plus one epoch boundary** (locks taken
on unclaimed-immutable refs during the mixed era die with their epoch DB). Deletion
itself is simpler than the markers case — per-epoch table, no data migration; remove the
field + ThConfig entry (tidehunter supports outright deletion; old RocksDB epoch dirs are
pruned wholesale).

### 3.7 Restartability summary (the per-epoch vs perpetual question)

No new durable state is introduced; the answer to "handler state is per-epoch but objects
are perpetual" is that the durable lock table was only ever needed *behind* the flush
horizon, and behind the flush horizon the perpetual objects table is guaranteed complete
(I2 + I1) — with the deferred table (per-epoch, already flushed atomically with the
watermark today) covering the one exception. In front of the flush horizon, consensus
replay rebuilds the quarantine map exactly as it does today. The watermark
(`last_consensus_stats_v2`) keeps its existing batch and gating; the epoch DB keeps being
"never ahead of perpetual"; nothing about D1-D8 (§8) changes except that D1's "epoch DB
has no locks" becomes universal and harmless by construction.

### 3.8 Metrics

- `consensus_handler_owned_ref_lock_checks{source, verdict}` — source ∈ {current_commit,
  quarantine, deferred, cache, objects_db, lock_table_backstop}, verdict ∈ {clear,
  conflict, same_digest}. The user-requested "remaining lookups" metric is
  `source="objects_db"`; expect ~0 in steady state, spikes on restart/catch-up;
  alertable. `lock_table_backstop` should be identically 0 for honest traffic.
- `live_object_cache_{entries,inserts{source=vote|fallback|execution},evictions}`.
- Keep `consensus_handler_dropped_transactions{reason="lock_conflict"}` for continuity;
  existing tests assert on it.

### 3.9 Alternatives considered

- **Whole-epoch in-memory lock map, no fallback:** unbounded (~hundreds of GB at 15k TPS
  × 24h epochs, §10) — rejected.
- **Keep the durable table but prune flushed-executed locks:** shrinks the table to
  ~deferred-backlog size but keeps every write on the flush path and every negative
  lookup in the handler — most of the cost, little of the win. The pruned residue is
  exactly what the deferred table already stores.
- **Move conflict detection to vote time:** impossible — votes are per-validator and
  time-dependent; the conflict decision must be post-consensus-deterministic.
- **Reuse `object_by_id_cache` instead of a new primitive:** viable v0 (see §3.5); lacks
  negative caching and retention control.

### 3.10 What this buys

At 15k TPS with ~2-3 owned refs/tx: eliminates 30-45k point writes/s into the largest
epoch-DB keyspace (WAL + compaction + tidehunter dirty-key pressure for the whole epoch),
eliminates the per-commit batched *negative* multi-get against that ever-growing table
from the CPU-bound handler (replaced by hash lookups on three small maps + a cache), and
collapses epoch-DB size/pruning cost. Costs: the deferred map (small), the cache (capped),
the residual metered objects-table reads, and a determinism argument that now spans three
subsystems — which is what §3.3 is for.
