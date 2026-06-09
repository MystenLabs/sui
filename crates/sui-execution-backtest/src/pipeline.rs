// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Epoch resolution and the two-stage fetch/execute pipeline. A producer task fetches + indexes
//! checkpoints with `--concurrency` in flight and pushes them into a bounded channel; the execute
//! stream drains that channel, flattens each checkpoint into per-transaction work units, and
//! re-executes up to `--execute-concurrency` of them concurrently on blocking workers. Decoupling
//! fetch from execute keeps the cores fed regardless of fetch latency (the fetch source is the
//! usual bottleneck) and bounds memory to the in-flight + buffered checkpoints.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result};
use futures::{Stream, StreamExt as _};
use sui_execution::Executor;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::ChainIdentifier;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::metrics::ExecutionMetrics;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::StatusFilter;
use crate::execute::{CheckpointStats, execute_one_transaction};
use crate::grpc::{EpochBounds, RpcClient};
use crate::store::{PackageCache, ScanStore};

/// Per-epoch resolved context, shared (by Arc) across all of the epoch's checkpoint workers.
pub(crate) struct EpochCtx {
    pub(crate) epoch: u64,
    pub(crate) protocol_config: Arc<ProtocolConfig>,
    /// Version-correct executor for full execution. Owns its own `MoveRuntime`.
    pub(crate) executor: Arc<dyn Executor + Send + Sync>,
    /// Execution metrics (shared across all epochs).
    pub(crate) metrics: Arc<ExecutionMetrics>,
    /// The epoch's start timestamp (the first checkpoint's `timestamp_ms`), which is the value the
    /// executor expects for `epoch_timestamp_ms` (constant across the epoch, as `TxContext` sees it
    /// on chain) — *not* the per-checkpoint timestamp.
    pub(crate) epoch_start_timestamp_ms: u64,
    /// The epoch's reference gas price, used to meter execution faithfully (the gas accounting and
    /// `TxContext` differ from passing the tx's own price as the RGP).
    pub(crate) reference_gas_price: u64,
}

/// A checkpoint that has been fetched and indexed by the prefetch stage and is ready to execute.
/// Holds only cheap handles (Arcs); the bulky object index is shared.
pub(crate) struct PreparedCheckpoint {
    pub(crate) cp: u64,
    pub(crate) ctx: Arc<EpochCtx>,
    pub(crate) chain_id: ChainIdentifier,
    /// The full checkpoint (shared); we iterate its `transactions` during execution.
    pub(crate) checkpoint: Arc<Checkpoint>,
    pub(crate) objects: Arc<BTreeMap<ObjectKey, Object>>,
    pub(crate) latest: Arc<BTreeMap<ObjectID, SequenceNumber>>,
    pub(crate) tombstones: Arc<BTreeMap<ObjectID, BTreeSet<SequenceNumber>>>,
    pub(crate) packages: Arc<PackageCache>,
}

/// Output of the prefetch stage handed to the execute stage over the pipeline channel.
pub(crate) enum Prefetched {
    /// A fetched + indexed checkpoint ready to execute.
    Ready(PreparedCheckpoint),
    /// Fetch (or the prefetch task itself) failed; carries the already-tallied error so the
    /// consumer can fold it into the totals. Nothing to execute.
    Failed(CheckpointStats),
}

/// A unit of execution work, after the prepared checkpoints are flattened into individual
/// transactions. Per-transaction units keep the blocking pool load-balanced and let a single global
/// concurrency cap bound total in-flight executions regardless of how transactions cluster into
/// checkpoints.
enum Work {
    /// Execute transaction `idx` of the (shared) prepared checkpoint against the (shared) store.
    Tx(Arc<PreparedCheckpoint>, Arc<ScanStore>, usize),
    /// A pre-tallied result (fetch error) to fold straight into the totals.
    Done(CheckpointStats),
}

/// Resolve every epoch in `epochs` to its checkpoint range + protocol version, build a per-epoch
/// [`EpochCtx`] (executor, protocol config, epoch-start timestamp, RGP), and flatten into a single
/// list of (checkpoint, ctx) work items. `first_bounds` is the already-resolved bounds for the
/// first epoch (so it isn't fetched twice). `max_checkpoints_per_epoch` caps each epoch from its
/// first checkpoint.
pub(crate) async fn resolve_epoch_work(
    rpc: &RpcClient,
    ingestion: &IngestionClient,
    chain: Chain,
    epochs: RangeInclusive<u64>,
    max_checkpoints_per_epoch: Option<u64>,
    first_bounds: EpochBounds,
    execution_metrics: &Arc<ExecutionMetrics>,
) -> Result<Vec<(u64, Arc<EpochCtx>)>> {
    let start_epoch = *epochs.start();
    let mut work: Vec<(u64, Arc<EpochCtx>)> = Vec::new();
    for epoch in epochs {
        let bounds = if epoch == start_epoch {
            first_bounds
        } else {
            rpc.epoch_bounds(epoch)
                .await
                .with_context(|| format!("resolving epoch {epoch}"))?
        };
        let protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::new(bounds.protocol_version), chain);
        let executor = sui_execution::executor(&protocol_config, /* silent */ true)
            .map_err(|e| anyhow::anyhow!("building executor for epoch {epoch}: {e}"))?;
        // The executor expects the epoch *start* timestamp (the first checkpoint's), not a
        // per-checkpoint one.
        let epoch_start_timestamp_ms = ingestion
            .checkpoint(bounds.first_checkpoint)
            .await
            .with_context(|| {
                format!(
                    "fetching first checkpoint {} of epoch {epoch}",
                    bounds.first_checkpoint
                )
            })?
            .checkpoint
            .summary
            .timestamp_ms;
        let ctx = Arc::new(EpochCtx {
            epoch,
            protocol_config: Arc::new(protocol_config),
            executor,
            metrics: execution_metrics.clone(),
            epoch_start_timestamp_ms,
            reference_gas_price: bounds.reference_gas_price,
        });

        let last = match max_checkpoints_per_epoch {
            Some(cap) => bounds.last_checkpoint.min(
                bounds
                    .first_checkpoint
                    .saturating_add(cap)
                    .saturating_sub(1),
            ),
            None => bounds.last_checkpoint,
        };
        let count = last
            .saturating_sub(bounds.first_checkpoint)
            .saturating_add(1);
        info!(
            epoch,
            first_checkpoint = bounds.first_checkpoint,
            last_checkpoint = last,
            checkpoints = count,
            protocol_version = bounds.protocol_version,
            "queued epoch"
        );
        for cp in bounds.first_checkpoint..=last {
            work.push((cp, ctx.clone()));
        }
    }
    Ok(work)
}

/// Create the bounded pipeline channel.
pub(crate) fn pipeline_channel(
    prefetch_depth: usize,
) -> (Sender<Prefetched>, Receiver<Prefetched>) {
    tokio::sync::mpsc::channel::<Prefetched>(prefetch_depth)
}

/// Spawn the producer task: fetch + index checkpoints with `fetch_concurrency` in flight (each
/// prefetch is its own task, so indexing parallelizes across the runtime's workers instead of
/// serializing on the consumer) and push them into `tx`.
pub(crate) fn spawn_producer(
    work: Vec<(u64, Arc<EpochCtx>)>,
    ingestion: Arc<IngestionClient>,
    packages: Arc<PackageCache>,
    fetch_concurrency: usize,
    tx: Sender<Prefetched>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut prefetch = futures::stream::iter(work.into_iter().map(|(cp, ctx)| {
            let ingestion = ingestion.clone();
            let packages = packages.clone();
            tokio::spawn(async move { prefetch_checkpoint(cp, ctx, ingestion, packages).await })
        }))
        .buffer_unordered(fetch_concurrency);
        while let Some(joined) = prefetch.next().await {
            let item = match joined {
                Ok(item) => item,
                Err(e) => {
                    error!("prefetch task panicked: {e}");
                    Prefetched::Failed(CheckpointStats {
                        fetch_errors: 1,
                        ..Default::default()
                    })
                }
            };
            if tx.send(item).await.is_err() {
                break;
            }
        }
    })
}

/// Prefetch stage: fetch one checkpoint and build its (shared, read-only) object index +
/// tombstone set. Pure I/O + indexing — no execution. Fetch failures are folded into a
/// `Prefetched::Failed` tally rather than aborting the scan.
async fn prefetch_checkpoint(
    cp: u64,
    ctx: Arc<EpochCtx>,
    ingestion: Arc<IngestionClient>,
    packages: Arc<PackageCache>,
) -> Prefetched {
    let envelope = match ingestion.checkpoint(cp).await {
        Ok(envelope) => envelope,
        Err(e) => {
            error!(checkpoint = cp, "fetch failed: {e:#}");
            return Prefetched::Failed(CheckpointStats {
                fetch_errors: 1,
                ..Default::default()
            });
        }
    };
    let chain_id = envelope.chain_id;
    let checkpoint = envelope.checkpoint;
    let (objects, latest) = ScanStore::index_object_set(&checkpoint.object_set);
    // Tombstones (deleted/wrapped/unwrapped-then-deleted) across all of the checkpoint's
    // transactions, so child reads can tell a dynamic field was *removed* even though the bundled
    // object set still carries a stale pre-deletion version of it.
    let mut tombstones: BTreeMap<ObjectID, BTreeSet<SequenceNumber>> = BTreeMap::new();
    for executed in &checkpoint.transactions {
        for (id, ver) in executed.effects.all_tombstones() {
            tombstones.entry(id).or_default().insert(ver);
        }
    }
    Prefetched::Ready(PreparedCheckpoint {
        cp,
        ctx,
        chain_id,
        checkpoint,
        objects: Arc::new(objects),
        latest: Arc::new(latest),
        tombstones: Arc::new(tombstones),
        packages,
    })
}

/// Transform the stream of prefetched checkpoints into a stream of per-transaction execution-result
/// tallies: drain `rx`, flatten each prepared checkpoint into per-transaction work units
/// (`flat_map` processes one checkpoint's transactions at a time, so memory stays bounded to the
/// in-flight checkpoints), and execute up to `execute_concurrency` of them concurrently on blocking
/// workers. `cp_counter` is bumped per checkpoint for progress reporting.
pub(crate) fn stream_to_execution_results(
    rx: Receiver<Prefetched>,
    status: StatusFilter,
    execute_concurrency: usize,
    cp_counter: Arc<AtomicU64>,
) -> Pin<Box<dyn Stream<Item = CheckpointStats> + Send>> {
    Box::pin(
        futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })
        .flat_map(move |item| match item {
            Prefetched::Failed(stats) => futures::stream::iter(vec![Work::Done(stats)]),
            Prefetched::Ready(prepared) => {
                cp_counter.fetch_add(1, Ordering::Relaxed);
                futures::stream::iter(checkpoint_to_work_units(prepared))
            }
        })
        .map(move |work| async move {
            match work {
                Work::Done(stats) => stats,
                Work::Tx(prepared, store, i) => tokio::task::spawn_blocking(move || {
                    execute_one_transaction(&prepared, &store, i, status)
                })
                .await
                .unwrap_or_else(|e| {
                    error!("execution task panicked: {e}");
                    CheckpointStats {
                        reconstruction_errors: 1,
                        ..Default::default()
                    }
                }),
            }
        })
        .buffer_unordered(execute_concurrency),
    )
}

/// Map a prepared checkpoint to its per-transaction work units (one `Work::Tx` per transaction;
/// per-tx units keep the blocking pool load-balanced) plus the shared read-only store they all
/// execute against.
fn checkpoint_to_work_units(prepared: PreparedCheckpoint) -> Vec<Work> {
    // One read-only store shared across the checkpoint's transactions (cheap Arc clones).
    let store = Arc::new(ScanStore::new(
        prepared.objects.clone(),
        prepared.latest.clone(),
        prepared.tombstones.clone(),
        prepared.packages.clone(),
    ));
    let prepared = Arc::new(prepared);
    (0..prepared.checkpoint.transactions.len())
        .map(|i| Work::Tx(prepared.clone(), store.clone(), i))
        .collect()
}
