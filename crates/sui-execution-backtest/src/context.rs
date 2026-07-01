// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The reconstructed execution context: the per-epoch [`EpochCtx`] (version-correct executor,
//! protocol config, epoch-start timestamp, RGP) and the per-checkpoint [`PreparedCheckpoint`] that
//! execution runs against, plus [`resolve_epoch_work`] which resolves an epoch range into the
//! [`EpochCtx`] map and the overall checkpoint range to hand to the framework `Indexer`. The
//! streaming/concurrency machinery itself now lives in that `Indexer` (see [`crate::handler`]).

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use sui_execution::Executor;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::ObjectID;
use sui_types::digests::ChainIdentifier;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::metrics::ExecutionMetrics;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tracing::{info, warn};

use crate::grpc::{EpochBounds, RpcClient};

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
    /// The system (framework) packages live during this epoch, keyed by id. Unlike normal packages,
    /// these keep a stable id but are upgraded (new bytecode) across protocol versions, so they
    /// can't be fetched "latest" and replayed faithfully — they are loaded version-correctly from
    /// the framework snapshot for the epoch's protocol version (see [`load_epoch_system_packages`]).
    pub(crate) system_packages: Arc<BTreeMap<ObjectID, Object>>,
}

/// A checkpoint that has been indexed and is ready to execute. Holds only cheap handles (Arcs); the
/// bulky object index is shared.
pub(crate) struct PreparedCheckpoint {
    pub(crate) cp: u64,
    pub(crate) ctx: Arc<EpochCtx>,
    pub(crate) chain_id: ChainIdentifier,
    /// The full checkpoint (shared); we iterate its `transactions` during execution.
    pub(crate) checkpoint: Arc<Checkpoint>,
    /// The checkpoint's object index, shared with the execute stage's [`ScanStore`]. Read directly
    /// to find candidate coin types and to triage divergences against the materialized read set.
    pub(crate) objects: Arc<BTreeMap<ObjectKey, Object>>,
}

/// Resolve every epoch in `epochs` to its checkpoint range + protocol version, building a per-epoch
/// [`EpochCtx`] (executor, protocol config, epoch-start timestamp, RGP). Returns the epoch→ctx map
/// together with the inclusive checkpoint range spanning all of the epochs (handed to the indexer's
/// ingestion service). `first_bounds` is the already-resolved bounds for the first epoch (so it
/// isn't fetched twice). `max_checkpoints_per_epoch` caps each epoch from its first checkpoint.
pub(crate) async fn resolve_epoch_work(
    rpc: &RpcClient,
    chain: Chain,
    epochs: RangeInclusive<u64>,
    max_checkpoints_per_epoch: Option<u64>,
    first_bounds: EpochBounds,
    execution_metrics: &Arc<ExecutionMetrics>,
) -> Result<(BTreeMap<u64, Arc<EpochCtx>>, u64, u64)> {
    // System packages are reconstructed from the framework snapshot, which only faithfully
    // represents released networks (mainnet/testnet). A non-prod chain (devnet/localnet/custom, all
    // `Chain::Unknown`) may run framework bytecode ahead of, or unrelated to, any snapshot, so the
    // reconstructed system packages — and thus the backtest — can silently diverge from reality (see
    // [`load_epoch_system_packages`]).
    if chain == Chain::Unknown {
        warn!(
            "backtesting a non-prod network (chain id resolves to Unknown): system packages are \
             reconstructed from the framework snapshot and may not match the network's actual \
             framework, so results may be unreliable"
        );
    }
    let start_epoch = *epochs.start();
    let mut epoch_ctxs: BTreeMap<u64, Arc<EpochCtx>> = BTreeMap::new();
    let mut first_checkpoint = u64::MAX;
    let mut last_checkpoint = 0u64;
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
        let system_packages = Arc::new(
            load_epoch_system_packages(bounds.protocol_version)
                .with_context(|| format!("resolving framework packages for epoch {epoch}"))?,
        );
        let ctx = Arc::new(EpochCtx {
            epoch,
            protocol_config: Arc::new(protocol_config),
            executor,
            metrics: execution_metrics.clone(),
            epoch_start_timestamp_ms: bounds.epoch_start_timestamp_ms,
            reference_gas_price: bounds.reference_gas_price,
            system_packages,
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
        first_checkpoint = first_checkpoint.min(bounds.first_checkpoint);
        last_checkpoint = last_checkpoint.max(last);
        epoch_ctxs.insert(epoch, ctx);
    }
    Ok((epoch_ctxs, first_checkpoint, last_checkpoint))
}

/// The system (framework) packages live at `protocol_version`, as package `Object`s keyed by id.
/// `load_bytecode_snapshot` resolves the sparse protocol-version → snapshot mapping itself (the
/// greatest snapshot at or below the version). The reconstructed object's version number is
/// synthetic (`OBJECT_START_VERSION`); only its bytecode and linkage matter for execution, and
/// those resolve system packages by id, not version.
///
/// This is faithful only for released networks (mainnet/testnet), whose framework is exactly what a
/// published snapshot captured. On devnet the running framework can be *ahead* of any snapshot (so
/// the greatest-snapshot-≤-version lookup returns stale bytecode), and on localnet it is whatever
/// was built locally (no snapshot corresponds to it at all) — in both cases the reconstruction is
/// silently wrong. [`resolve_epoch_work`] warns when the chain is non-prod (`Chain::Unknown`).
///
/// TODO: If a GraphQL checkpoint source is added, revisit this — GraphQL can read the actual
///       framework objects at a checkpoint (as `sui-replay-2` does), which is more principled than
///       reconstructing them from the snapshot and would also fix the non-prod case above.
fn load_epoch_system_packages(protocol_version: u64) -> Result<BTreeMap<ObjectID, Object>> {
    let packages =
        sui_framework_snapshot::load_bytecode_snapshot(protocol_version).with_context(|| {
            format!("loading framework snapshot for protocol version {protocol_version}")
        })?;
    Ok(packages
        .into_iter()
        .map(|package| {
            let object = package.genesis_object();
            (object.id(), object)
        })
        .collect())
}
