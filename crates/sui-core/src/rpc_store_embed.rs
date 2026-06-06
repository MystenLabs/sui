// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Startup orchestration for the embedded `sui-rpc-store` indexer.
//!
//! When a fullnode is configured with
//! [`RpcConfig::use_experimental_rpc_store`], it serves the rpc-api
//! index surface from an embedded [`sui_rpc_store`] instance instead of
//! the legacy `rpc-index`. This module owns the lifecycle of that
//! instance:
//!
//! 1. Open the rpc-store database under the node's `db_path()`.
//! 2. Compare its persisted per-pipeline watermarks against the
//!    perpetual store's currently-available checkpoint range `[L, T]`
//!    (`L` = lowest available, `T` = highest executed) and [`decide`]
//!    what to do: resume as-is, (re)seed the history cohort, or
//!    (re)restore the live cohort.
//! 3. Bulk-load the live cohort from the perpetual store and seed the
//!    history cohort when needed (blocking, before the node starts
//!    executing).
//! 4. Build the read handle the rpc-api serves through, hand the store
//!    to the pruner, and spawn the tip-following indexer fed by the
//!    perpetual store ([`PerpetualStoreIngestionClient`]) and the
//!    checkpoint executor's broadcast stream
//!    ([`BroadcastStreamingClient`]).
//!
//! The live cohort (live-object-derivable indexes) is restored to the
//! tip and follows forward. The history cohort (ledger-history bitmaps,
//! `tx_seq` maps, per-epoch metadata) is seeded to the lowest available
//! checkpoint and backfilled upward; the synchronizer's dynamic cohort
//! lets it catch up to the live frontier without stalling tip
//! snapshots.

use std::sync::Arc;

use anyhow::Context as _;
use prometheus::Registry;
use sui_config::NodeConfig;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Watermark;
use sui_consistent_store::restore::RestoreDriverConfig;
use sui_consistent_store::restore::metrics::RestoreMetrics;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::BoxedStreamingClient;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::service::Service;
use sui_rpc_store::ConsistencyConfig;
use sui_rpc_store::HISTORY_COHORT;
use sui_rpc_store::Indexer;
use sui_rpc_store::LIVE_COHORT;
use sui_rpc_store::METRICS_PREFIX;
use sui_rpc_store::PipelineLayer;
use sui_rpc_store::RestoreLayer;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::Store as RpcStore;
use sui_rpc_store::default_rocksdb_config;
use sui_rpc_store::restore_indexes;
use sui_rpc_store::seed_history_cohort;
use sui_types::digests::ChainIdentifier;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ObjectStore;
use tokio::sync::broadcast;
use tracing::info;

use crate::authority::authority_store::AuthorityStore;
use crate::authority::authority_store_tables::AuthorityPerpetualTables;
use crate::checkpoints::CheckpointStore;
use crate::rpc_store_ingestion_client::PerpetualStoreIngestionClient;
use crate::rpc_store_restore_source::PerpetualStoreRestoreSource;
use crate::rpc_store_streaming_client::BroadcastStreamingClient;
use crate::storage::RocksDbStore;

/// Subdirectory of the node's `db_path()` holding the rpc-store.
const RPC_STORE_DIR: &str = "rpc_store";

/// Number of in-memory snapshots retained for consistent reads.
/// Mirrors the standalone `sui-rpc-node` default; with the default
/// stride of 1 this is roughly a 32-checkpoint consistency window.
const SNAPSHOT_CAPACITY: usize = 32;

/// What the startup orchestration does with the on-disk rpc-store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bootstrap {
    /// The on-disk state resumes within the available range; open it
    /// and follow the tip with no blocking work.
    Resume,

    /// The live cohort is fine, but the history cohort is missing or
    /// has fallen below the available floor; (re)seed it in place
    /// without disturbing the live cohort.
    SeedHistory,

    /// (Re)bulk-load the live cohort from the perpetual store, then
    /// seed the history cohort. `clear` wipes the database first (for
    /// out-of-range or wrong-chain data); otherwise the restore
    /// resumes from any in-progress per-shard cursors.
    Restore { clear: bool },
}

/// Decide what bootstrap action the embedded store needs from the
/// persisted framework state and the perpetual store's available
/// range.
///
/// - `live_resume` is `Some(c)` when every [`LIVE_COHORT`] pipeline
///   has a committed watermark, where `c` is the lowest checkpoint tip
///   indexing would resume from across them
///   (`min(checkpoint_hi_inclusive) + 1`); `None` when any live
///   pipeline lacks a watermark (never restored, or a restore that did
///   not finish).
/// - `history_resume` is the same for the [`HISTORY_COHORT`], but a
///   missing watermark maps to `0`: an unwatermarked pipeline resumes
///   at `first_checkpoint`, which the embedded path leaves at its `0`
///   default, so the history cohort backfills from genesis.
/// - `chain_matches` is `Some(false)` when the database is bound to a
///   different chain, `Some(true)` when it matches, and `None` when no
///   chain id has been recorded yet.
/// - `lowest_available` is `L`, the lowest checkpoint the perpetual
///   store can still serve.
fn decide(
    live_resume: Option<u64>,
    history_resume: u64,
    chain_matches: Option<bool>,
    lowest_available: u64,
) -> Bootstrap {
    // A database bound to another chain is unusable; wipe and rebuild.
    if chain_matches == Some(false) {
        return Bootstrap::Restore { clear: true };
    }

    let Some(live_resume) = live_resume else {
        // The live cohort never finished restoring. Resume the restore
        // in place -- the driver picks up from its per-shard cursors --
        // rather than clearing partial progress.
        return Bootstrap::Restore { clear: false };
    };

    // The live cohort's indexes reference checkpoints the perpetual
    // store has since pruned; the bulk-loaded data is unusable.
    if live_resume < lowest_available {
        return Bootstrap::Restore { clear: true };
    }

    // The live cohort is in range; the history cohort either was never
    // seeded above the floor or fell behind it. Re-seed it alone.
    if history_resume < lowest_available {
        return Bootstrap::SeedHistory;
    }

    Bootstrap::Resume
}

/// A bootstrapped embedded rpc-store, ready to hand to the pruner and
/// the rpc-api read path and to start tip indexing.
pub struct EmbeddedRpcStore {
    /// Shared store handle. Cloned for the pruner (via [`Self::store`])
    /// and for the tip indexer.
    store: RpcStore,

    /// Read handle exposing the rpc-store's index surface to
    /// `sui-rpc-api`.
    reader: RpcStoreReader,

    /// Local checkpoint source for the tip ingestion client.
    ingestion_source: RocksDbStore,

    chain_id: ChainIdentifier,

    /// The running tip-indexer service, populated by
    /// [`Self::spawn_indexer`]. Held so the node keeps it alive;
    /// dropping it aborts the indexer's tasks.
    indexer: Option<Service>,
}

impl EmbeddedRpcStore {
    /// Open the rpc-store, bring it in line with the perpetual store's
    /// available range (restoring / seeding as needed), and build the
    /// store and read handles.
    ///
    /// Blocks while restoring the live cohort. Call before the node
    /// starts executing checkpoints, so the perpetual store's range is
    /// stable for the duration of the restore.
    pub async fn bootstrap(
        config: &NodeConfig,
        authority_store: &Arc<AuthorityStore>,
        checkpoint_store: &Arc<CheckpointStore>,
        ingestion_source: RocksDbStore,
        chain_identifier: ChainIdentifier,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let perpetual = authority_store.perpetual_tables.clone();
        let path = config.db_path().join(RPC_STORE_DIR);
        let db_options = DbOptions {
            rocksdb: default_rocksdb_config(),
            snapshot_capacity: SNAPSHOT_CAPACITY,
        };
        let (db, schema) = Db::open::<RpcStoreSchema>(&path, db_options)
            .context("opening the embedded rpc-store database")?;
        let schema = Arc::new(schema);

        // The perpetual store's currently-available checkpoint range.
        // `highest_executed` is `None` only on a node's very first boot
        // (genesis is executed later in startup), in which case there is
        // nothing to bulk-load and the indexer builds both cohorts from
        // genesis as the node executes.
        let highest_executed = checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .context("reading highest executed checkpoint")?;
        let lowest_available = lowest_available_checkpoint(&perpetual, checkpoint_store)?;

        let chain_id = ChainId(*chain_identifier.as_bytes());
        let live_resume = cohort_resume(&db, LIVE_COHORT)?;
        let history_resume = cohort_resume(&db, HISTORY_COHORT)?.unwrap_or(0);
        let chain_matches = stored_chain_id(&db)?.map(|stored| stored == chain_id);

        let action = decide(live_resume, history_resume, chain_matches, lowest_available);
        info!(
            ?action,
            lowest_available,
            ?highest_executed,
            "bootstrapping embedded rpc-store",
        );

        match action {
            Bootstrap::Resume => {}
            Bootstrap::SeedHistory => {
                seed_history(
                    &db,
                    &schema,
                    &perpetual,
                    checkpoint_store,
                    lowest_available,
                    chain_id,
                )?;
            }
            Bootstrap::Restore { clear } => {
                if clear {
                    db.clear_all()
                        .context("clearing the out-of-range embedded rpc-store")?;
                }
                // A synced node enabling the embedded store for the
                // first time (or recovering an out-of-range one):
                // bulk-load the live cohort, then seed the history cohort
                // so it backfills `(L, T]`. When `highest_executed` is
                // `None` (a fresh node, nothing executed yet) there is
                // nothing to load -- every pipeline stays unwatermarked
                // so the indexer builds both cohorts from genesis as
                // checkpoints execute.
                if let Some(target) = highest_executed {
                    restore_live(
                        db.clone(),
                        schema.clone(),
                        perpetual.clone(),
                        target,
                        chain_id,
                        registry,
                    )
                    .await?;
                    // `L == 0` means genesis is still available, so the
                    // history cohort backfills from checkpoint 0 with no
                    // seed (an unwatermarked pipeline resumes at
                    // `first_checkpoint = 0`).
                    if lowest_available > 0 {
                        seed_history(
                            &db,
                            &schema,
                            &perpetual,
                            checkpoint_store,
                            lowest_available,
                            chain_id,
                        )?;
                    }
                }
            }
        }

        let store = sui_consistent_store::Store::new(db.clone(), schema.clone());
        let reader = RpcStoreReader::new(db, schema);

        Ok(Self {
            store,
            reader,
            ingestion_source,
            chain_id: chain_identifier,
            indexer: None,
        })
    }

    /// A clone of the store handle, for the pruner's history-cohort
    /// pruning ([`sui_rpc_store::prune_history_cohort`]).
    pub fn store(&self) -> RpcStore {
        self.store.clone()
    }

    /// A clone of the read handle, for the rpc-api read path
    /// ([`crate::storage::RpcStoreReadStore`]).
    pub fn reader(&self) -> RpcStoreReader {
        self.reader.clone()
    }

    /// Build and start the tip-following indexer over the embedded
    /// store, retaining its [`Service`] so the node keeps it running.
    ///
    /// `checkpoint_sender` is the checkpoint executor's broadcast
    /// stream; when present it drives a low-latency
    /// [`BroadcastStreamingClient`], with the perpetual-store ingestion
    /// client filling any gap. When absent (e.g. on a node that does
    /// not run the rpc servers) the ingestion client polls the
    /// perpetual store alone.
    pub async fn spawn_indexer(
        &mut self,
        checkpoint_sender: Option<broadcast::Sender<Arc<Checkpoint>>>,
        registry: &Registry,
    ) -> anyhow::Result<()> {
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(PerpetualStoreIngestionClient::new(
                self.ingestion_source.clone(),
                self.chain_id,
            )),
            ingestion_metrics,
        );
        let streaming_client: Option<BoxedStreamingClient> = checkpoint_sender.map(|sender| {
            Box::new(BroadcastStreamingClient::new(sender, self.chain_id)) as BoxedStreamingClient
        });

        let mut indexer = Indexer::from_store(
            self.store.clone(),
            IndexerArgs::default(),
            ingestion_client,
            streaming_client,
            ConsistencyConfig::default(),
            // Pruning is driven by the validator's `AuthorityStorePruner`
            // (history cohort only), not the rpc-store's own pruner.
            None,
            IngestionConfig::default(),
            registry,
        )
        .await
        .context("constructing the embedded rpc-store indexer")?;
        indexer
            .add_pipelines(PipelineLayer::embedded(), CommitterConfig::default())
            .await
            .context("registering embedded rpc-store pipelines")?;
        let service = indexer
            .run()
            .await
            .context("starting the embedded rpc-store indexer")?;
        self.indexer = Some(service);
        Ok(())
    }
}

/// The lowest checkpoint the perpetual store can still serve: one past
/// the higher of the object-store and checkpoint-store pruned
/// watermarks (both inclusive). `0` when nothing has been pruned.
fn lowest_available_checkpoint(
    perpetual: &AuthorityPerpetualTables,
    checkpoint_store: &CheckpointStore,
) -> anyhow::Result<u64> {
    let object_pruned = perpetual
        .get_highest_pruned_checkpoint()
        .context("reading object store pruned watermark")?;
    let checkpoint_pruned = checkpoint_store
        .get_highest_pruned_checkpoint_seq_number()
        .context("reading checkpoint store pruned watermark")?;
    Ok(object_pruned
        .into_iter()
        .chain(checkpoint_pruned)
        .max()
        .map(|pruned| pruned + 1)
        .unwrap_or(0))
}

/// The lowest checkpoint tip indexing would resume from across a
/// cohort: `min(checkpoint_hi_inclusive) + 1`. `None` if any pipeline
/// in the cohort has no committed watermark.
fn cohort_resume(db: &Db, cohort: &[&str]) -> anyhow::Result<Option<u64>> {
    let framework = db.framework();
    let mut min_hi: Option<u64> = None;
    for name in cohort {
        let key = PipelineTaskKey::new(*name);
        let Some(watermark) = framework
            .watermarks
            .get(&key)
            .with_context(|| format!("reading watermark for {name}"))?
        else {
            return Ok(None);
        };
        min_hi = Some(match min_hi {
            Some(hi) => hi.min(watermark.checkpoint_hi_inclusive),
            None => watermark.checkpoint_hi_inclusive,
        });
    }
    Ok(min_hi.map(|hi| hi + 1))
}

/// The chain id the database is bound to, read from the first pipeline
/// that has one recorded. All pipelines pin the same chain, so any one
/// is representative.
fn stored_chain_id(db: &Db) -> anyhow::Result<Option<ChainId>> {
    let framework = db.framework();
    for name in LIVE_COHORT.iter().chain(HISTORY_COHORT) {
        let key = PipelineTaskKey::new(*name);
        if let Some(id) = framework
            .chain_ids
            .get(&key)
            .with_context(|| format!("reading chain id for {name}"))?
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Bulk-load the live cohort from the perpetual store up to
/// `target_checkpoint`, blocking until the restore completes.
async fn restore_live(
    db: Db,
    schema: Arc<RpcStoreSchema>,
    perpetual: Arc<AuthorityPerpetualTables>,
    target_checkpoint: u64,
    chain_id: ChainId,
    registry: &Registry,
) -> anyhow::Result<()> {
    let source = PerpetualStoreRestoreSource::new(perpetual, target_checkpoint, chain_id);
    let metrics = RestoreMetrics::new(Some(METRICS_PREFIX), registry);
    let mut service = restore_indexes(
        db,
        schema,
        source,
        RestoreDriverConfig::default(),
        RestoreLayer::indexes_only(),
        metrics,
    )
    .context("starting the live-cohort restore")?;
    service
        .join()
        .await
        .context("restoring the live cohort from the perpetual store")?;
    Ok(())
}

/// Seed the history cohort to `L - 1` so the backfill resumes at the
/// lowest available checkpoint `L`. The seed watermark's `tx_hi`,
/// epoch, and timestamp come from checkpoint `L - 1`'s summary, so the
/// seeded pruning floor lines up with the first checkpoint the backfill
/// will index.
fn seed_history(
    db: &Db,
    schema: &RpcStoreSchema,
    perpetual: &AuthorityPerpetualTables,
    checkpoint_store: &CheckpointStore,
    lowest_available: u64,
    chain_id: ChainId,
) -> anyhow::Result<()> {
    debug_assert!(lowest_available > 0, "seed_history requires L > 0");
    let anchor = lowest_available - 1;
    let checkpoint = checkpoint_store
        .get_checkpoint_by_sequence_number(anchor)
        .context("reading the history seed-anchor checkpoint")?
        .with_context(|| format!("history seed-anchor checkpoint {anchor} is unavailable"))?;
    let summary = checkpoint.data();
    let watermark = Watermark {
        epoch_hi_inclusive: summary.epoch,
        checkpoint_hi_inclusive: anchor,
        tx_hi: summary.network_total_transactions,
        timestamp_ms_hi_inclusive: summary.timestamp_ms,
    };
    seed_history_cohort(
        db,
        schema,
        watermark,
        chain_id,
        Some(perpetual as &dyn ObjectStore),
    )
    .context("seeding the history cohort")
}

#[cfg(test)]
mod tests {
    use super::*;

    // `L = 0` (nothing pruned): an unseeded history cohort backfills
    // from genesis, so a complete live cohort is enough to resume.
    #[test]
    fn resumes_from_genesis_when_nothing_pruned() {
        assert_eq!(decide(Some(10), 0, Some(true), 0), Bootstrap::Resume);
        // History never seeded (resume 0) is fine at L = 0.
        assert_eq!(decide(Some(10), 0, None, 0), Bootstrap::Resume);
    }

    // Both cohorts resume at or above the available floor.
    #[test]
    fn resumes_when_in_range() {
        assert_eq!(decide(Some(100), 100, Some(true), 100), Bootstrap::Resume);
        assert_eq!(decide(Some(200), 100, Some(true), 100), Bootstrap::Resume);
    }

    // The live cohort never finished restoring: resume the restore in
    // place rather than clearing partial progress.
    #[test]
    fn restores_without_clearing_when_live_uninitialized() {
        assert_eq!(
            decide(None, 0, None, 0),
            Bootstrap::Restore { clear: false }
        );
        assert_eq!(
            decide(None, 50, Some(true), 100),
            Bootstrap::Restore { clear: false }
        );
    }

    // The live cohort references checkpoints the perpetual store has
    // pruned away: wipe and rebuild.
    #[test]
    fn clears_and_restores_when_live_out_of_range() {
        assert_eq!(
            decide(Some(50), 200, Some(true), 100),
            Bootstrap::Restore { clear: true }
        );
    }

    // A database bound to a different chain is always wiped.
    #[test]
    fn clears_and_restores_on_chain_mismatch() {
        assert_eq!(
            decide(Some(200), 200, Some(false), 100),
            Bootstrap::Restore { clear: true }
        );
        // Chain mismatch dominates even an otherwise-resumable state.
        assert_eq!(
            decide(Some(200), 200, Some(false), 0),
            Bootstrap::Restore { clear: true }
        );
    }

    // The live cohort is in range but the history cohort is missing or
    // has fallen behind the floor: re-seed history alone.
    #[test]
    fn seeds_history_when_history_behind_floor() {
        // History never seeded (resume 0) but L > 0.
        assert_eq!(
            decide(Some(200), 0, Some(true), 100),
            Bootstrap::SeedHistory
        );
        // History seeded but below the (advanced) floor.
        assert_eq!(
            decide(Some(200), 50, Some(true), 100),
            Bootstrap::SeedHistory
        );
        // History exactly at the floor resumes.
        assert_eq!(decide(Some(200), 100, Some(true), 100), Bootstrap::Resume);
    }
}
