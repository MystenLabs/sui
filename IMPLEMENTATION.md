# Move `post_process_one_tx` Off the Critical Execution Path

## Goal
Move `post_process_one_tx` (JSON-RPC indexing and event emission) off the critical
transaction execution path by spawning the work on a blocking thread, while ensuring
the `CheckpointExecutor` does not advance its watermark past transactions whose
post-processing is still in flight.

---

## Correctness Argument

Transaction execution is only persisted to disk via `commit_transaction_outputs` in
`CheckpointExecutor`. We now wait for post-processing to complete **before**
`commit_transaction_outputs`, so transaction effects are never persisted unless
post-processing has finished. This means persistence implies post-processing completion.

On crash and restart, there are three scenarios:

1. **No transactions were persisted before the restart.** Since post-processing
   completes before persistence, and persistence did not occur, there is nothing to
   recover. All transactions will be re-executed, which will call `post_process_one_tx`
   again, spawning post-processing for all of them. `CheckpointExecutor` will wait for
   post-processing to finish before persisting.

2. **Transactions were persisted but the watermark was not yet advanced.** Since we
   wait for post-processing before persisting, post-processing is guaranteed complete.
   On restart, the checkpoint is re-processed from the watermark.
   `schedule_transaction_execution` detects that effects already exist (via
   `executed_fx_digests`) and skips re-execution. Post-processing is already done, so
   no work is lost.

3. **The watermark was advanced before the restart.** All work (execution,
   post-processing, persistence, watermark) completed successfully. Nothing to redo.

---

## Phase 1: Spawn `post_process_one_tx` on a Blocking Thread

### 1.1 Add new fields to `AuthorityState`

**File:** `crates/sui-core/src/authority.rs`

Add two new fields to `AuthorityState` (around line 965):

```rust
/// Tracks transactions whose post-processing (indexing/events) is still in flight.
/// The value is a oneshot::Receiver that CheckpointExecutor can use to wait for
/// completion before advancing its watermark.
pending_post_processing: Arc<DashMap<TransactionDigest, oneshot::Receiver<()>>>,

/// Limits the number of concurrent post-processing tasks to avoid overwhelming
/// the blocking thread pool. Defaults to the number of available CPUs.
post_processing_semaphore: Arc<tokio::sync::Semaphore>,
```

Initialize them in the constructor (`AuthorityState::new` / builder) as:
```rust
pending_post_processing: Arc::new(DashMap::new()),
post_processing_semaphore: Arc::new(tokio::sync::Semaphore::new(num_cpus::get())),
```

Add a public accessor so `CheckpointExecutor` can reference the DashMap:
```rust
pub fn pending_post_processing(
    &self,
) -> &Arc<DashMap<TransactionDigest, oneshot::Receiver<()>>> {
    &self.pending_post_processing
}
```

### 1.2 Modify `post_process_one_tx` to spawn a blocking thread

**File:** `crates/sui-core/src/authority.rs`, `post_process_one_tx` (line 3232)

The call site in `execute_certificate` (~line 2142) remains **unchanged**. All changes
happen inside `post_process_one_tx` itself.

The current function:
1. Checks `self.indexes.is_none()` and returns early if so
2. Computes coins, calls `self.index_tx(...)`, builds events, emits to subscription handler

The new function will:
1. Check `self.indexes.is_none()` and return early if so (unchanged)
2. **After** the early return check, create a `oneshot` channel
3. Insert `(tx_digest, oneshot::Receiver)` into `self.pending_post_processing`
4. Clone the needed arguments and `self` fields
5. `spawn_blocking` the actual indexing/event work
6. Inside the spawned task, on completion (success or failure), send on the
   `oneshot::Sender` and remove the digest from the DashMap

**Conceptual new `post_process_one_tx`:**
```rust
fn post_process_one_tx(
    &self,
    certificate: &VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    inner_temporary_store: &InnerTemporaryStore,
    epoch_store: &Arc<AuthorityPerEpochStore>,
) -> SuiResult {
    if self.indexes.is_none() {
        return Ok(());
    }

    let tx_digest = *certificate.digest();

    // Create notification channel and register in pending map
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    self.pending_post_processing.insert(tx_digest, done_rx);

    // Clone the individual Arc fields needed by the spawned task
    let indexes = self.indexes.clone();           // Option<Arc<IndexStore>>
    let subscription_handler = self.subscription_handler.clone(); // Arc<SubscriptionHandler>
    let metrics = self.metrics.clone();           // Arc<AuthorityMetrics>
    let name = self.name;                         // AuthorityName (Copy)
    let backing_package_store = self.get_backing_package_store().clone(); // Arc<dyn BackingPackageStore>
    let object_store = self.get_object_store().clone(); // Arc<dyn ObjectStore>
    let pending_map = self.pending_post_processing.clone(); // Arc<DashMap<...>>
    let semaphore = self.post_processing_semaphore.clone(); // Arc<Semaphore>

    // Clone the data arguments
    let certificate = certificate.clone();
    let effects = effects.clone();
    let inner_temporary_store = inner_temporary_store.clone();
    let epoch_store = epoch_store.clone();

    // Acquire a semaphore permit, blocking the caller if all permits are held.
    // This provides backpressure: if post-processing can't keep up with
    // execution, execution slows down rather than accumulating unbounded work.
    // Under normal conditions the acquire is instant (num_cpus permits available).
    let permit = {
        let _scope = monitored_scope("Execution::post_process_one_tx::semaphore_acquire");
        semaphore
            .acquire_blocking()
            .expect("post-processing semaphore should not be closed")
    };

    tokio::task::spawn_blocking(move || {
        // Move the permit into the closure so it is held for the duration
        // of the work and released when the closure completes.
        let _permit = permit;

        let _scope = monitored_scope("Execution::post_process_one_tx");

        let result = Self::post_process_one_tx_impl(
            &indexes,
            &subscription_handler,
            &metrics,
            name,
            &backing_package_store,
            &object_store,
            &certificate,
            &effects,
            &inner_temporary_store,
            &epoch_store,
        );

        if let Err(e) = &result {
            metrics.post_processing_total_failures.inc();
            error!(?tx_digest, "tx post processing failed: {e}");
        }

        // Signal completion and remove from pending map.
        let _ = done_tx.send(());
        pending_map.remove(&tx_digest);
    });

    Ok(())
}
```

Note: The semaphore is acquired **synchronously** via `acquire_blocking()` before
`spawn_blocking`. This provides direct backpressure on the execution path:
- Under normal load, permits are available and the acquire is instant
- Under heavy load, the caller blocks until a post-processing task finishes, preventing
  unbounded accumulation of work
- The permit is moved into the `spawn_blocking` closure and held for the duration of
  the actual work, then dropped when the closure completes

### 1.3 Extract the work into a static helper method

The actual indexing and event-emission work currently in `post_process_one_tx` will be
extracted into a new **static** method `post_process_one_tx_impl` (or associated
function) that takes its dependencies explicitly rather than via `&self`:

```rust
fn post_process_one_tx_impl(
    indexes: &Option<Arc<IndexStore>>,
    subscription_handler: &Arc<SubscriptionHandler>,
    metrics: &Arc<AuthorityMetrics>,
    name: AuthorityName,
    backing_package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
    object_store: &Arc<dyn ObjectStore + Send + Sync>,
    certificate: &VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    inner_temporary_store: &InnerTemporaryStore,
    epoch_store: &Arc<AuthorityPerEpochStore>,
) -> SuiResult { ... }
```

This method contains the body of the current `post_process_one_tx` (from the
`monitored_scope` onward), with `self.xyz` references replaced by the explicit params.

The sub-functions it calls also need adaptation:
- **`fullnode_only_get_tx_coins_for_indexing`**: Uses `self.indexes`, `self.name`
  (via `is_validator`), and `self.get_object_store()`. These are all passed in
  explicitly. This can either be refactored into a static method or inlined.
- **`index_tx`**: Uses `self.process_object_index(...)` which in turn uses
  `self.get_backing_package_store()`. Can take the backing package store explicitly.
- **`make_transaction_block_events`**: Uses `self.get_backing_package_store()`.
  Can take it explicitly.

Alternatively, these helper methods can remain as `&self` methods — in that case we
would need to call them before spawning the blocking thread. But since the goal is to
move ALL the work off the critical path, the preferred approach is to refactor them
to take explicit dependencies.

### 1.4 What needs cloning (argument analysis)

**`self` fields cloned** (all cheap Arc clones):
| Field | Type | Used by |
|-------|------|---------|
| `self.indexes` | `Option<Arc<IndexStore>>` | `index_tx` |
| `self.subscription_handler` | `Arc<SubscriptionHandler>` | event emission |
| `self.metrics` | `Arc<AuthorityMetrics>` | metric counters |
| `self.name` | `AuthorityName` | `is_validator` check (Copy type) |
| `self.get_backing_package_store()` | `Arc<dyn BackingPackageStore>` | `process_object_index`, `make_transaction_block_events` |
| `self.get_object_store()` | `Arc<dyn ObjectStore>` | `fullnode_only_get_tx_coins_for_indexing` |
| `self.pending_post_processing` | `Arc<DashMap<...>>` | cleanup on completion |
| `self.post_processing_semaphore` | `Arc<Semaphore>` | concurrency limiting |

**Arguments cloned:**
| Argument | Type | Cost |
|----------|------|------|
| `certificate` | `VerifiedExecutableTransaction` | Moderate (transaction data) |
| `effects` | `TransactionEffects` | Moderate |
| `inner_temporary_store` | `InnerTemporaryStore` | **Expensive** — contains BTreeMaps of objects. Derives `Clone`. |
| `epoch_store` | `Arc<AuthorityPerEpochStore>` | Cheap (Arc clone) |

**Important ordering:** The cloning of `inner_temporary_store` happens inside
`post_process_one_tx`, which is called BEFORE `build_transaction_outputs` consumes
the original at line 2155. The clone is gated behind `self.indexes.is_none()` so it
only happens when indexing is enabled.

### 1.5 Return type change

Currently `post_process_one_tx` returns `SuiResult`. The new version always returns
`Ok(())` since errors are handled asynchronously in the spawned task. The call site in
`execute_certificate` already discards the result (`let _ = ...`), so no change is
needed there.

---

## Phase 2: Wait for Post-Processing in `CheckpointExecutor`

### 2.1 Before committing transaction outputs, wait for all checkpoint transactions

**File:** `crates/sui-core/src/checkpoints/checkpoint_executor/mod.rs`

In `parallel_step` (around line 408-435), **before** `build_db_batch` and
`commit_transaction_outputs`, add a step that waits for all transactions in the
checkpoint to finish post-processing:

```rust
// Wait for all post-processing to complete before persisting transaction outputs.
// This ensures that if we crash after persistence, post-processing is guaranteed
// complete — so on restart, skipping re-execution of already-persisted transactions
// does not lose post-processing work.
for tx_digest in &ckpt_state.data.tx_digests {
    if let Some((_, rx)) = self.state.pending_post_processing().remove(tx_digest) {
        let _ = rx.await;
    }
}
```

Since `oneshot::Receiver` is not `Clone`, we **remove** the entry from the DashMap to
take ownership of the receiver. This is fine because:
- There is exactly one waiter (CheckpointExecutor)
- Once we've awaited the receiver, we don't need the entry anymore
- The spawned thread may also call `remove`, but `DashMap::remove` is idempotent — if
  the entry was already removed by CheckpointExecutor, the spawned thread's remove is
  a no-op

Note: The spawned thread sends on the `oneshot::Sender` first, then removes from the
map. CheckpointExecutor removes first, then awaits. This is safe:
- If CheckpointExecutor removes first: it gets the receiver, awaits it, the spawned
  thread sends on the sender (succeeds), then the spawned thread's remove is a no-op.
- If the spawned thread completes first: it sends, then removes. CheckpointExecutor's
  remove returns `None`, so it skips the await (the work is already done).

### 2.2 Also handle `verify_locally_built_checkpoint`

The `verify_locally_built_checkpoint` path (validators, line 505) does not run
`post_process_one_tx` because `fullnode_only_get_tx_coins_for_indexing` returns `None`
for validators (line 5122: `self.is_validator(epoch_store)` check), and validators
typically don't have `self.indexes` set. No changes needed on that path.

### 2.3 Pipeline stage

Consider adding a new pipeline stage `WaitForPostProcessing` before `BuildDbBatch`
for observability. This is optional but recommended for monitoring.

---

## Detailed File Change List

| File | Change |
|------|--------|
| `crates/sui-core/src/authority.rs` | Add `pending_post_processing: Arc<DashMap<...>>` and `post_processing_semaphore: Arc<Semaphore>` fields to `AuthorityState` |
| `crates/sui-core/src/authority.rs` | Initialize both fields in constructor (semaphore with `num_cpus::get()` permits) |
| `crates/sui-core/src/authority.rs` | Add `pending_post_processing()` accessor |
| `crates/sui-core/src/authority.rs` | Rewrite `post_process_one_tx` to: check indexes, insert into DashMap, clone fields + args, `spawn_blocking` |
| `crates/sui-core/src/authority.rs` | Extract `post_process_one_tx_impl` static method with explicit dependency params |
| `crates/sui-core/src/authority.rs` | Refactor `fullnode_only_get_tx_coins_for_indexing`, `index_tx`/`process_object_index`, and `make_transaction_block_events` to accept explicit deps instead of `&self` (or create static variants) |
| `crates/sui-core/src/checkpoints/checkpoint_executor/mod.rs` | Before `commit_transaction_outputs`, iterate checkpoint tx digests, remove from DashMap, and await any pending oneshot receivers |

---

## Risks and Mitigations

1. **`InnerTemporaryStore` clone cost**: This is a `BTreeMap`-heavy struct. The clone
   is O(n) in objects touched. Mitigation: only clone when `indexes.is_some()`, and
   replaces the much more expensive indexing work that was previously inline.

2. **Spawned task accumulation**: If post-processing is slower than execution, spawned
   tasks could accumulate. Mitigation: a semaphore (default `num_cpus` permits) is
   acquired synchronously before spawning. If all permits are held, the caller blocks
   until one completes, providing direct backpressure on execution. Under normal load
   permits are available and the acquire is instant.

3. **Epoch boundary**: At end-of-epoch, all post-processing must complete before
   reconfiguration. The existing `bump_highest_executed_checkpoint` gate in
   `CheckpointExecutor` ensures this, since it won't advance past the last checkpoint
   until all its transactions are done.

4. **Error handling**: Post-processing errors are already logged and ignored (`let _ =`).
   This doesn't change — errors on the spawned thread are logged the same way.

5. **Oneshot channel race**: The spawned thread sends then removes; CheckpointExecutor
   removes then awaits. Both orderings are safe (see 2.1 analysis).

---

## Testing Strategy

### Prerequisite: Preserve synchronous post-processing mode

Add a configuration flag (e.g., `post_process_one_tx_sync: bool` on `NodeConfig` or
`AuthorityState`) that, when set, causes `post_process_one_tx` to run the work inline
(synchronously) instead of spawning a blocking thread. This is the current behavior.
This flag serves two purposes:
- Acts as a safety valve / rollback mechanism in production
- Enables the dual-fullnode comparison test below

### Test 1: Dual-fullnode index comparison

**File:** `crates/sui-e2e-tests/tests/` (new test file or added to `full_node_tests.rs`)

Run a test cluster with two fullnodes:
- **Fullnode A**: async post-processing (new behavior, the default)
- **Fullnode B**: sync post-processing (old behavior, via the config flag)

Execute a workload of transactions (transfers, Move calls, etc.) for at least one
epoch. At the end of the epoch, compare the `IndexStore` state between the two
fullnodes. Specifically, verify that the following tables are identical:
- `transactions_seq` (transaction digest → sequence number)
- `transaction_order` (sequence number → transaction digest)
- `transactions_from_addr` (sender → transactions)
- `transactions_to_addr` (recipient → transactions)

This verifies that async post-processing produces the same index results as
synchronous post-processing.

Use `TestCluster::spawn_new_fullnode()` to start the two fullnodes with different
configs.

### Test 2: Consistency check in `check_system_consistency`

**File:** `crates/sui-core/src/authority.rs`, in `check_system_consistency` (line 4090)

Add a new check: when `expensive_safety_check_config.enable_secondary_index_checks()`
is true and `self.indexes` is `Some`, verify that every checkpointed transaction has an
entry in `transactions_seq`.

Implementation:
```rust
if expensive_safety_check_config.enable_secondary_index_checks()
    && let Some(indexes) = &self.indexes
{
    // Existing verify_indexes call...

    // New: verify all checkpointed transactions are indexed
    info!("Verifying all checkpointed transactions are in transactions_seq");
    let highest_executed = self
        .checkpoint_store
        .get_highest_executed_checkpoint_seq_number()
        .expect("Failed to get highest executed checkpoint")
        .expect("No executed checkpoints");

    for seq in 0..=highest_executed {
        let checkpoint = self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(seq)
            .expect("Failed to get checkpoint")
            .expect("Checkpoint missing");
        let contents = self
            .checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("Failed to get checkpoint contents")
            .expect("Checkpoint contents missing");
        for digests in contents.iter() {
            let tx_digest = digests.transaction;
            assert!(
                indexes.get_transaction_seq(&tx_digest)
                    .expect("Failed to read transactions_seq")
                    .is_some(),
                "Transaction {tx_digest} from checkpoint {seq} missing from transactions_seq"
            );
        }
    }
    info!("All checkpointed transactions verified in transactions_seq");
}
```

This check runs at epoch boundaries when expensive safety checks are enabled,
providing ongoing verification that async post-processing hasn't dropped any
transactions.

### Baseline testing

- Existing tests should pass without modification (behavior is preserved).
- Run `cargo simtest -p sui-e2e-tests` for full integration coverage.
