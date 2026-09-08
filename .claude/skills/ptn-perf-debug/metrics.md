# Metrics reference for PTN performance debugging

Pointers, not an exhaustive list. Verify names against the source before querying; grep for
`register_*_with_registry!` in the crate named.

## Cluster average vs outliers

- Start with cluster-wide aggregates (`avg by (...)`, `sum(rate(...))`, `histogram_quantile` over `sum by (le)`) to
  understand system behavior.
- Then always check the spread: `max`/`min` across hosts, `topk(3, ...)`, `bottomk(3, ...)`. One slow validator can
  stall consensus (leader timeouts, missing ancestors) or checkpoint certification for the whole cluster.
- If one host is an outlier, decide hardware vs software: compare `process_cpu_seconds_total`, load, disk write
  latency, NIC errors, and `perf` output against a healthy peer (see host-tools.md). Same binary + same config +
  different profile shape => hardware or host environment. Same profile shape but slower => hardware. Different
  profile shape => software path only taken on that host (config, state size, peer position, restart history).

## monitored_scope metrics

Defined in `crates/mysten-metrics/src/lib.rs` (`monitored_scope(name)` RAII guard and `in_monitored_scope` future
extension). Emitted as gauge vectors labelled by `name`:

| metric | meaning |
|---|---|
| `monitored_scope_duration_ns` | total ns accumulated inside the scope |
| `monitored_scope_iterations` | total entries into the scope |
| `monitored_scope_entrance` | currently inside the scope (up/down) |
| `monitored_future_active_duration_ns` | for `in_monitored_scope` futures only: ns actually spent polling |

Useful derivations:
- Utilization: `rate(monitored_scope_duration_ns[5m]) / 1e9`. For a single-threaded scope (select loop, or behind a
  lock), 1.0 means that stage is saturated and is the bottleneck. For scopes entered concurrently the value is
  "cores' worth of time"; divide by the pool size.
- Mean latency per entry: `rate(duration_ns) / rate(iterations)`.
- Stuck detection: `monitored_scope_entrance` pinned above zero and flat.
- CPU vs wait for futures: `active_duration_ns / duration_ns`. Low ratio = waiting, not working.

Caveat: a scope that spans an `.await` counts wait time as duration. `*::notify_read_*` scopes (from
`mysten-common/src/sync/notify_read.rs`) are always pure waiting: high means the producer stage is behind, never CPU.

### Scopes by pipeline stage (high value means...)

Ingress / verification
- `AuthorityServer::wait_for_effects::notify_read_executed_effects_finalized`, `TransactionOrchestrator::notify_read_*`:
  client-side waits for finality; high => downstream consensus/execution slow, not local CPU.
- `ValidateBatch`, `VerifyAndVoteBatch` (consensus_validator.rs), `VerifyConsensusTransaction`: signature/tx
  verification of incoming blocks; high => crypto verification CPU-bound, throttles block acceptance.
- `await_backpressure` (backpressure.rs): any sustained value => execution cannot keep up and ingress is throttled.

Consensus core (consensus/core)
- `CoreThread::loop::*` (`add_blocks`, `new_block`, `check_block_refs`, `get_missing`, ...): single-threaded core
  dispatch loop. Utilization near 1.0 => consensus throughput ceiling.
- `Core::*`: the work under the core lock; whichever sub-name dominates is the cost center.
- `BlockManager::try_accept_blocks*`, `try_unsuspend_blocks_for_latest_gc_round`: high => many suspended blocks,
  i.e. missing ancestors / sync trouble.
- `CommitFinalizer::process_commit`: high => finalization lagging commit rate.
- `BlockSync::Periodic::*`, `FetchMissingBlocksScheduler`: high => node is behind and back-filling. `RoundProber`:
  high => slow RTT to peers.

Consensus handler (crates/sui-core/src/consensus_handler.rs)
- `ConsensusCommitHandler::handle_consensus_commit`: single-threaded, ordered. Utilization near 1.0 => the handler is
  the bottleneck. Sub-stages: `collect_transactions_to_schedule`, `create_pending_checkpoints`,
  `filter_consensus_txns`, `deduplicate_consensus_txns`, `build_commit_handler_input`,
  `process_execution_time_observations`, `deserialize_worker` (real CPU, deserialization), `order_by_gas_price`.
- `ConsensusHandler::enqueue`: high => execution scheduler back-pressuring the handler.
- `AuthorityPerEpochStore::insert_tx_key`, `insert_finalized_transactions`: high => RocksDB write latency stalling
  the handler.

Execution (authority.rs, execution_driver.rs, execution_scheduler_impl.rs)
- `ExecutionDriver::loop`: utilization near 1.0 => driver saturated. `ExecutionDriver::acquire_permit`: high =>
  execution concurrency limit is the binding constraint.
- `Execution::try_execute_immediately` (whole path), `Execution::prepare_certificate` (Move VM, real CPU),
  `Execution::load_input_objects` (object reads / cache misses), `Execution::commit_certificate` (write-out),
  `Execution::read_child_object` (dynamic-field heavy workload).
- `wait_for_barrier_dependencies`, `SettlementScheduler::*`: high => settlement/barrier serialization stalls.
- `Execution::post_process_one_tx[::semaphore_acquire]`: fullnode indexing cost / permit starvation.

Checkpoints (crates/sui-core/src/checkpoints)
- `BuildCheckpoints`: utilization near 1.0 => builder is the bottleneck. Sub-stages `CheckpointBuilder::causal_sort`
  (large checkpoints), `write_checkpoint` (RocksDB), `wait_for_transactions_sequenced` (waiting on consensus).
- `CheckpointNotifyRead`, `CheckpointBuilder::notify_read_executed_effects`: builder blocked on execution (there is a
  60s stall log in notify_read.rs for this).
- `CheckpointAggregator`: high => waiting on peer signatures (look for a slow peer).
- `CheckpointExecutor::parallel_step` (saturation), `execute_transactions`, `notify_read_executed_effects_digests`
  (execution backlog), `finalize_checkpoint` (storage), `notify_read_state_hasher` (hashing lags).
- `AccumulateCheckpoint`, `AccumulateRunningRoot`: state hashing CPU-bound, stalls finalization.

Storage (writeback_cache.rs, authority_store_pruner.rs)
- `WritebackCache::build_db_batch`: serialization CPU per tx. `WritebackCache::commit_transaction_outputs`, `flush`:
  disk/WAL write latency; this serializes the commit path.
- `ObjectsLivePruner`, `EffectsLivePruner`, `Prune*ForEligibleEpochs`: pruner competing with the write path for I/O.

Channels: `monitored_channel_inflight` / `_sent` / `_received` (labelled `name`) show backlog on `monitored_mpsc`
channels; a growing inflight count points straight at the consumer stage.

## Task and future counts per spawn site

`spawn_monitored_task!`, `spawn_logged_monitored_task!`, and `monitored_future!` (same file) keep an up/down gauge
of live tasks/futures per call site:

| metric | label | source |
|---|---|---|
| `monitored_tasks` | `callsite` = `file:line` (or `file:name` when a name is given) | `spawn_monitored_task!`, `spawn_logged_monitored_task!` |
| `monitored_futures` | same | `monitored_future!` |

Use them to answer "how many tasks from this spawn site are active right now": `monitored_tasks{callsite=~".*consensus_handler.*"}`.
A count that grows without bound is a leak or a stage whose consumer stopped; a count pinned at a semaphore or pool
limit means that stage is the ceiling; a count that goes to zero when work is pending means the spawner is stuck
upstream. `topk(10, monitored_tasks)` per host is a quick way to spot an unexpected pileup. Because the label is
`file:line`, the same site shifts name between SHAs; match on the file name rather than the line when comparing
across deploys. `spawn_logged_monitored_task!` also logs `Spawning future <callsite>` / `Future <callsite> completed`
at INFO, greppable in Loki when you need per-task timing.

## Latency and throughput metrics, by stage

Triage order: client `latency_s` -> `transaction_driver_settlement_finality_latency` -> consensus commit latency ->
execution queue age -> RocksDB stalls -> checkpoint ages -> stress client `no_gas` / in-flight. Consensus metrics are
registered under a `consensus` prefix, so `block_commit_latency` is scraped as `consensus_block_commit_latency`.

Stress client (crates/sui-benchmark/src/drivers/bench_driver.rs, scraped from :8081, label `workload`)
- `latency_s` (e2e histogram, ground truth for user latency), `num_success` (achieved TPS), `num_submitted`,
  `num_error{type=rpc|execution}` (rpc = submission/connectivity failures), `num_in_flight` (pinned at the cap =>
  client blocked on the network), `cpu_usage` (near 100% => stress host is the bottleneck),
  `validators_in_tx_cert{validator}` / `validators_in_effects_cert` (a validator missing here is silently slow).
- The periodic stat line in `journalctl -u stress` prints `TPS`, latency percentiles, `no_gas`, `submitted`,
  `in_flight`. High `no_gas` with `in_flight` at the cap => the client ran out of gas objects / in-flight budget
  (`--in-flight-ratio`, `--num-workers`); fix the client before blaming the cluster. `no_gas` = 0 and TPS below
  target => the cluster is the bottleneck.
- Log lines to grep: `Transaction execution got error`, `failed unexpectedly`, `Soft bundle execution failed`.

Fullnode / orchestrator (transaction_orchestrator.rs, sui-json-rpc/src/metrics.rs, sui-node/src/metrics.rs)
- `tx_orchestrator_settlement_finality_latency`, `tx_orchestrator_request_latency`,
  `tx_orchestrator_wait_for_finality_timeout` (the main "network too slow" counter), `tx_orchestrator_req_in_flight`.
- `req_latency_by_route{route}`, `inflight_rpc_requests_by_route`, `server_errors_by_route`, `grpc_request_latency`.
- Fullnode lag: `highest_synced_checkpoint - last_executed_checkpoint`, `last_executed_checkpoint_age`.

Validator ingress (authority_server.rs, consensus_adapter.rs, transaction_driver/metrics.rs, authority.rs)
- `validator_service_submit_transaction_latency` (validator-side SLO), `_submit_transaction_consensus_latency`
  (isolates consensus wait), `validator_service_tx_verification_latency`, `validator_service_inflight_transactions`.
- Overload: `validator_service_num_rejected_tx_during_overload`, `authority_overload_status`,
  `authority_load_shedding_percentage`, `transaction_overload_sources` (which queue tripped).
- Consensus adapter: `sequencing_certificate_latency` (submit-to-ack; the classic "consensus is slow" signal),
  `sequencing_certificate_inflight`, `sequencing_in_flight_semaphore_wait`, `sequencing_certificate_failures`.
- Transaction driver (client side of validators): `transaction_driver_settlement_finality_latency` (best e2e),
  `transaction_driver_submit_transaction_retries`, `transaction_driver_validator_submit_transaction_errors{validator}`
  (identifies the bad validator), `validator_client_observed_latency{validator}`.
- Throughput: `rate(total_transaction_effects)` is the real validator TPS.

Consensus (consensus/core/src/metrics.rs; prefix `consensus_`)
- Latency: `block_commit_latency`, `proposed_block_commit_latency` (if only this is bad, this node is the problem),
  `commit_round_advancement_interval` (growing => stalling), `quorum_receive_latency` (network health canary).
- Progress: `last_committed_leader_round`, `last_commit_index`, `threshold_clock_round`; `highest_accepted_authority_round`
  and `block_receive_delay{authority}` pick out the lagging peer.
- Proposer: `block_proposal_interval`, `block_proposal_leader_wait_ms`, `leader_timeout_total{timeout_type}`,
  `core_skipped_proposals{reason}`, `proposed_block_transactions` / `proposed_block_size` (tiny blocks under load =>
  proposal starvation), `core_lock_enqueued - core_lock_dequeued` (core backlog).
- Sync: `block_manager_suspended_blocks`, `block_manager_missing_ancestors`, `missing_blocks_after_fetch_total`,
  `synchronizer_fetch_failures`, `commit_sync_quorum_index - commit_sync_local_index` (commit lag).
- Exclusion: `round_tracker_last_propagation_delay` ("am I being excluded"), `reputation_scores{authority}`,
  `num_of_bad_nodes`.
- Network: `_inbound_request_latency{route}`, `_outbound_request_latency{route}`, `_outbound_request_errors{route,status}`,
  `_outbound_inflight_requests`, `priority_submission_backpressure`.

Consensus handler and execution (authority.rs, consensus_handler.rs, execution_cache/metrics.rs)
- `consensus_handler_processed{class}` (flat => stuck), `consensus_committed_user_transactions`,
  `consensus_handler_deferred_transactions` / `_congested_transactions` (hot shared object), `_cancelled_transactions`,
  `consensus_calculated_throughput`, `consensus_timestamp_bias`.
- `execution_driver_dispatch_queue` (main backlog gauge), `execution_queueing_delay_s`,
  `transaction_manager_transaction_queue_age_s` (clearest saturation signal), `transaction_manager_num_pending_certificates`
  (waiting on deps), `execution_driver_executed_transactions`, `execution_cache_backpressure_status`.
- `authority_state_internal_execution_latency`, `_execution_load_input_objects_latency` (DB reads),
  `_commit_certificate_latency` (write path), `execution_gas_latency_ratio` (drops when the machine is slow, not the load heavy).

Checkpoints (checkpoints/metrics.rs, checkpoint_executor/metrics.rs, sui-network/src/state_sync/metrics.rs)
- `last_constructed_checkpoint` vs `last_certified_checkpoint` (gap => signature aggregation; check
  `checkpoint_participation{signer}` for who stopped signing), `last_created_checkpoint_age`, `last_certified_checkpoint_age`.
- `last_executed_checkpoint_age`, `checkpoint_exec_latency`, `checkpoint_executor_pipeline_stage_wait_duration_ns`
  (which executor stage bottlenecks), `highest_known_checkpoint - highest_synced_checkpoint` (state-sync lag),
  `checkpoint_summary_age`.
- `remote_checkpoint_forks`, `split_brain_checkpoint_forks`: must be 0.

Storage (typed-store/src/metrics.rs, labels `cf_name`, `db_name`)
- `rocksdb_is_write_stopped` (instant explanation for a throughput cliff), `rocksdb_actual_delayed_write_rate`,
  `rocksdb_num_level0_files`, `rocksdb_estimate_pending_compaction_bytes`, `rocksdb_num_running_compactions`.
- `rocksdb_write_batch_commit_latency_seconds`, `rocksdb_put_latency_seconds`, `rocksdb_get_latency_seconds`,
  `rocksdb_multiget_latency_seconds`, `rocksdb_num_very_slow_batch_writes`, `rocksdb_background_errors`.
- Where a write spends time: `write_delay_nanos`, `write_wal_nanos`, `write_memtable_nanos`, `db_mutex_lock_nanos`.

Process level (mysten-metrics)
- `thread_stall_duration_sec`: the tokio runtime was blocked; any sizeable value shows up as latency everywhere.
- `monitored_tasks{callsite}` / `monitored_futures{callsite}`: see "Task and future counts per spawn site" above.
- `uptime` (and `consensus_uptime`) reset => process restarted; explains sudden discontinuities.
- `process_cpu_seconds_total`, `process_resident_memory_bytes`, `process_open_fds`; node-exporter (:9091) for
  disk, NIC, load per host. `system_invariant_violations{name}`: always investigate.
