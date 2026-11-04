// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Indexer pipelines that populate the `sui-rpc-store` schema
//! from observed [`Checkpoint`]s, plus the orchestrator
//! ([`Indexer`]) that wires them up against a shared
//! [`Synchronizer`].
//!
//! Each pipeline submodule implements the
//! `Processor` + `sequential::Handler` pair the
//! `sui-indexer-alt-framework` drives: `process` turns a checkpoint
//! into a `Vec<Value>` (with the heavy lifting done in the
//! processor-pool, off the commit hot path), `batch` folds many
//! values into a single `Batch`, and `commit` stages the batch's
//! writes against a [`sui_consistent_store::Connection`] from
//! [`sui_consistent_store::Store`].
//!
//! Every pipeline targets the same backing [`RpcStoreSchema`].

pub mod balance;
pub mod checkpoint_broadcast;
pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod effects;
pub mod epochs;
pub mod event_bitmap;
pub mod events;
pub mod object_by_owner;
pub mod object_by_type;
pub mod object_version_by_checkpoint;
pub mod objects;
pub mod package_versions;
pub mod pruner;
pub mod restore;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::btree_map::Entry;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use prometheus::Registry;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Synchronizer;
use sui_consistent_store::restore_state;
use sui_indexer_alt_framework as framework;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ArcStreamingClient;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;
use sui_indexer_alt_framework::pipeline::sequential::{self};
use sui_indexer_alt_framework::service::Service;
use sui_types::base_types::ObjectID;
use sui_types::digests::ObjectDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use crate::RpcStoreSchema;
use crate::config::PipelineLayer;
use crate::config::PrunerConfig;
use crate::indexer::pruner::PrunerMetrics;
use crate::indexer::restore::LIVE_COHORT;

/// Metrics prefix used for both the framework indexer and the
/// underlying ingestion service. Surfaced as a constant so the
/// prefix is consistent across the metrics built in [`Indexer::new`]
/// and the ones the standalone-binary entry point builds when it
/// constructs the [`IngestionClient`] from `ClientArgs`.
pub const METRICS_PREFIX: &str = "rpc_store_indexer";

/// The schema parameter the framework's `Store` / pipelines bind
/// to.
pub type Schema = RpcStoreSchema;

/// The store type pipelines commit through.
pub type Store = sui_consistent_store::Store<Schema>;

/// The sequence number of the first transaction in `checkpoint`.
///
/// `network_total_transactions` is the cumulative network-wide tx
/// count *after* this checkpoint executes, so subtracting the
/// number of transactions the checkpoint contains gives the
/// `tx_seq` of its first entry.
pub fn first_tx_seq(checkpoint: &Checkpoint) -> u64 {
    checkpoint.summary.network_total_transactions - checkpoint.transactions.len() as u64
}

/// The `tx_seq` of the transaction at index `i` within
/// `checkpoint`.
pub fn tx_seq_at(checkpoint: &Checkpoint, i: usize) -> u64 {
    first_tx_seq(checkpoint) + i as u64
}

/// First-seen input version of every object that existed before
/// the checkpoint and was used as an input to some transaction in
/// it. Mirrors the helper of the same name in
/// `sui-indexer-alt-consistent-store::handlers`.
///
/// Objects created or unwrapped within the checkpoint are
/// excluded. Used by the diff-based indexes
/// ([`object_by_owner`] etc.) to
/// remove the rows that the *prior* state contributed before
/// re-inserting the rows that the *posterior* state contributes.
pub fn checkpoint_input_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut from_this_checkpoint = HashSet::new();
    let mut input_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let input_objects_map: BTreeMap<_, _> = tx
            .input_objects(&checkpoint.object_set)
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            let Some(version) = change.input_version else {
                continue;
            };

            if from_this_checkpoint.contains(&id) {
                continue;
            }

            let Entry::Vacant(entry) = input_objects.entry(id) else {
                continue;
            };

            let input_object = *input_objects_map
                .get(&(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in input_objects"))?;

            // Input digests are only populated in Effects V2. For Effects V1, we need to
            // compute the digest from the input object's contents.
            let digest = change.input_digest.unwrap_or_else(|| input_object.digest());
            entry.insert((input_object, digest));
        }

        for change in tx.effects.object_changes() {
            if change.output_version.is_some() {
                from_this_checkpoint.insert(change.id);
            }
        }
    }
    Ok(input_objects)
}

/// Last-seen output version of every object that was created or
/// modified by some transaction in the checkpoint and is still
/// live at the end. Mirrors the helper of the same name in
/// `sui-indexer-alt-consistent-store::handlers`.
///
/// Used to populate the checkpoint-pinned
/// [`object_version_by_checkpoint`] index and the diff-based indexes
/// once the prior state has been retracted.
pub fn checkpoint_output_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut output_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let output_objects_map: BTreeMap<_, _> = tx
            .output_objects(&checkpoint.object_set)
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            // Clear the previous entry, in case it was created within this checkpoint.
            output_objects.remove(&id);

            let (Some(version), Some(digest)) = (change.output_version, change.output_digest)
            else {
                continue;
            };

            let output_object = *output_objects_map
                .get(&(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in output_objects"))?;

            output_objects.insert(id, (output_object, digest));
        }
    }
    Ok(output_objects)
}

/// Top-level orchestrator. Wraps a [`framework::Indexer`] over the
/// [`Store`] for [`RpcStoreSchema`] together with a
/// [`Synchronizer`] coordinating cross-pipeline snapshots, and
/// exposes the per-pipeline registration shape this crate needs.
///
/// Construct one of two ways:
///
/// - [`Indexer::new`] opens the [`Db`] / [`Store`] internally —
///   typical for the standalone binary path.
/// - [`Indexer::from_store`] takes an already-opened [`Store`] —
///   typical for the embedded-fullnode path where the fullnode
///   shares the underlying database with this indexer for direct
///   reads (and possibly for its own raw-chain-data writes).
///
/// Pipelines are registered through [`Self::add_pipelines`], which
/// honours the per-pipeline enable/disable knobs encoded in a
/// [`PipelineLayer`]. Disabled pipelines are skipped entirely —
/// the [`Synchronizer`] only barriers across pipelines that were
/// actually registered, so leaving the raw-chain-data pipelines
/// off does not stall snapshots. The layer is the only pipeline
/// selector: the framework's `IndexerArgs::pipeline` filter is
/// refused, because a pipeline it skipped would still be registered
/// with the [`Synchronizer`] and hold snapshots back forever.
///
/// Every registered [`LIVE_COHORT`] pipeline is a required snapshot
/// member ([`Synchronizer::register_required_pipeline`]): each
/// snapshot includes every live index's writes through its
/// checkpoint, even when a crash left their watermarks uneven, so
/// the newest snapshot is always a valid point for live-index reads.
/// Lagging history pipelines do not hold snapshots back.
///
/// After pipelines are registered, [`Self::run`] installs the
/// synchronizer onto the store and starts the framework indexer.
pub struct Indexer {
    indexer: framework::Indexer<Store>,

    /// Synchronizer coordinating per-pipeline writes against
    /// cross-pipeline snapshots. Owned here until [`Self::run`]
    /// hands it to [`sui_consistent_store::Store::install_sync`].
    sync: Synchronizer,

    /// Pruning policy and its metrics, present when pruning is
    /// enabled. [`Self::run`] starts the background pruner from
    /// these and attaches it to the composed service. `None` leaves
    /// pruning off (the embedded-fullnode and test defaults).
    pruner: Option<(PrunerConfig, Arc<PrunerMetrics>)>,
}

impl Indexer {
    /// Open the database at `path` with [`RpcStoreSchema`] and
    /// construct an [`Indexer`] backed by it.
    ///
    /// `ingestion_client` is the pull-side checkpoint source; the
    /// optional `streaming_client` is the live-tail source, and the
    /// optional `gap_fill_client` fetches the checkpoints in front of
    /// a newly connected stream instead of `ingestion_client`. Callers
    /// (standalone binary, embedded fullnode) build the
    /// [`IngestionClient`] via [`IngestionClient::new`] (driven by
    /// `ClientArgs`) or [`IngestionClient::from_trait`] (wrapping
    /// a custom [`IngestionClientTrait`]), depending on where
    /// checkpoints come from. The [`IngestionMetrics`] handle the
    /// service shares with the client is reused from
    /// [`IngestionClient::metrics`], avoiding double-registration
    /// against `registry`.
    ///
    /// [`IngestionClientTrait`]: sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait
    /// [`IngestionMetrics`]: sui_indexer_alt_framework::metrics::IngestionMetrics
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        path: impl AsRef<Path>,
        indexer_args: IndexerArgs,
        ingestion_client: IngestionClient,
        streaming_client: Option<ArcStreamingClient>,
        gap_fill_client: Option<IngestionClient>,
        consistency_config: crate::config::ConsistencyConfig,
        pruner_config: Option<PrunerConfig>,
        ingestion_config: IngestionConfig,
        db_options: DbOptions,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let (db, schema) = Db::open::<RpcStoreSchema>(path, db_options)
            .context("Failed to open sui-rpc-store database")?;
        let store = sui_consistent_store::Store::new(db, Arc::new(schema));
        Self::from_store(
            store,
            indexer_args,
            ingestion_client,
            streaming_client,
            gap_fill_client,
            consistency_config,
            pruner_config,
            ingestion_config,
            registry,
        )
        .await
    }

    /// Variant of [`Self::new`] that takes an already-opened
    /// [`Store`]. Useful when the caller wants to share the
    /// underlying [`Db`] with other code in the same process (e.g.
    /// a fullnode that reads through [`RpcStoreSchema`] directly,
    /// or writes to the raw-chain-data CFs through a separate
    /// path).
    ///
    /// Fails if `indexer_args.pipeline` is non-empty; select
    /// pipelines through the [`PipelineLayer`] passed to
    /// [`Self::add_pipelines`] instead.
    #[allow(clippy::too_many_arguments)]
    pub async fn from_store(
        store: Store,
        indexer_args: IndexerArgs,
        ingestion_client: IngestionClient,
        streaming_client: Option<ArcStreamingClient>,
        gap_fill_client: Option<IngestionClient>,
        consistency_config: crate::config::ConsistencyConfig,
        pruner_config: Option<PrunerConfig>,
        ingestion_config: IngestionConfig,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let metrics_prefix = Some(METRICS_PREFIX);

        anyhow::ensure!(
            indexer_args.pipeline.is_empty(),
            "The rpc-store indexer selects pipelines through its pipeline layer; \
             IndexerArgs::pipeline must be empty, got {:?}",
            indexer_args.pipeline,
        );

        let sync = Synchronizer::new(
            store.db().clone(),
            consistency_config.buffer_size,
            indexer_args.first_checkpoint,
        );

        let indexer = framework::Indexer::with_ingestion_clients(
            store,
            indexer_args,
            ingestion_client,
            streaming_client,
            gap_fill_client,
            ingestion_config,
            metrics_prefix,
            registry,
        )
        .await
        .context("Failed to construct framework indexer")?;

        // Register the pruner's metrics under its own prefix when
        // pruning is enabled; `run` starts the task from this.
        let pruner = pruner_config.map(|config| (config, PrunerMetrics::new(None, registry)));

        Ok(Self {
            indexer,
            sync,
            pruner,
        })
    }

    /// Borrow the wrapped framework indexer's store. Useful for
    /// embedded callers that want a read handle pointed at the
    /// same [`RpcStoreSchema`] this orchestrator is writing to.
    pub fn store(&self) -> &Store {
        self.indexer.store()
    }

    /// Iterate over the names of every pipeline that has been
    /// registered with this indexer and is enabled (i.e. not
    /// filtered out by `IndexerArgs::pipeline`). Useful for
    /// asserting which pipelines are active before [`Self::run`]
    /// is called.
    pub fn pipelines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.indexer.pipelines()
    }

    /// Register every pipeline that is `Some(_)` in `layer`. The
    /// supplied [`CommitterConfig`] acts as the shared base; each
    /// pipeline's [`CommitterLayer`] overrides individual fields.
    ///
    /// Skipped (`None`) pipelines are not registered with the
    /// [`Synchronizer`] either, so its snapshot barrier still
    /// proceeds without them.
    ///
    /// [`CommitterLayer`]: crate::config::CommitterLayer
    pub async fn add_pipelines(
        &mut self,
        layer: PipelineLayer,
        committer: CommitterConfig,
    ) -> anyhow::Result<()> {
        let PipelineLayer {
            epochs,
            checkpoint_summary,
            checkpoint_contents,
            checkpoint_seq_by_digest,
            transactions,
            tx_seq_by_digest,
            tx_metadata_by_seq,
            effects,
            events,
            objects,
            object_version_by_checkpoint,
            object_by_owner,
            object_by_type,
            balance,
            package_versions,
            transaction_bitmap,
            event_bitmap,
        } = layer;

        macro_rules! add {
            ($handler:expr, $cfg:expr) => {
                if let Some(layer) = $cfg {
                    self.sequential_pipeline(
                        $handler,
                        SequentialConfig {
                            committer: layer.finish(committer.clone()),
                            // The synchronizer requires one
                            // checkpoint per write batch; folding
                            // multiple checkpoints into one batch
                            // trips its out-of-order check.
                            max_batch_checkpoints: Some(1),
                            ..Default::default()
                        },
                    )
                    .await?
                }
            };
        }

        // Raw chain data.
        add!(self::epochs::Epochs, epochs);
        add!(
            self::checkpoint_summary::CheckpointSummary,
            checkpoint_summary
        );
        add!(
            self::checkpoint_contents::CheckpointContents,
            checkpoint_contents
        );
        add!(
            self::checkpoint_seq_by_digest::CheckpointSeqByDigest,
            checkpoint_seq_by_digest
        );
        add!(self::transactions::Transactions, transactions);
        add!(self::tx_seq_by_digest::TxSeqByDigest, tx_seq_by_digest);
        add!(
            self::tx_metadata_by_seq::TxMetadataBySeq,
            tx_metadata_by_seq
        );
        add!(self::effects::Effects, effects);
        add!(self::events::Events, events);
        add!(self::objects::Objects, objects);
        // `object_version_by_checkpoint` needs its restore anchor `T` so
        // its processor scopes floor candidates to the backfill window
        // `[L, T]`. The restore (if any) has already run by the time
        // pipelines are registered, so read it once here.
        let ovbc_anchor = self::object_version_by_checkpoint::restored_anchor(self.store().db())?;
        add!(
            self::object_version_by_checkpoint::ObjectVersionByCheckpoint::with_anchor(ovbc_anchor),
            object_version_by_checkpoint
        );

        // Indexes.
        add!(self::object_by_owner::ObjectByOwner, object_by_owner);
        add!(self::object_by_type::ObjectByType, object_by_type);
        add!(self::balance::Balance, balance);
        add!(self::package_versions::PackageVersions, package_versions);
        add!(
            self::transaction_bitmap::TransactionBitmap,
            transaction_bitmap
        );
        add!(self::event_bitmap::EventBitmap, event_bitmap);

        Ok(())
    }

    /// Register the checkpoint-broadcast pipeline, which re-publishes
    /// each committed checkpoint to `sender` in checkpoint order (see
    /// [`checkpoint_broadcast`]). Call alongside [`Self::add_pipelines`]
    /// and before [`Self::run`].
    ///
    /// Kept separate from [`Self::add_pipelines`] because it carries a
    /// runtime `broadcast::Sender` rather than a [`PipelineLayer`]
    /// toggle — only the standalone `sui-rpc-node`, which hosts the
    /// subscription service, registers it; the embedded fullnode feeds
    /// its subscription service from the checkpoint executor instead.
    ///
    /// As with every synchronizer-coordinated pipeline, registers with
    /// `max_batch_checkpoints = 1` so each `commit` (and thus each
    /// broadcast) is exactly one checkpoint, written as soon as it is
    /// next in order and the committer has room for it.
    pub async fn add_checkpoint_broadcast(
        &mut self,
        sender: tokio::sync::broadcast::Sender<Arc<Checkpoint>>,
        committer: CommitterConfig,
    ) -> anyhow::Result<()> {
        self.sequential_pipeline(
            self::checkpoint_broadcast::CheckpointBroadcast::new(sender),
            SequentialConfig {
                committer,
                max_batch_checkpoints: Some(1),
                ..Default::default()
            },
        )
        .await
    }

    /// Register a single sequential pipeline. The pipeline is
    /// announced to the synchronizer before being handed to the
    /// framework indexer so that, by the time the first batch
    /// flows through, the synchronizer task is already waiting on
    /// the pipeline's queue. A [`LIVE_COHORT`] pipeline is
    /// registered as a required snapshot member.
    ///
    /// Refuses to register a pipeline whose persisted [`RestoreState`]
    /// is still `InProgress`: tip-mode indexing of a pipeline that
    /// has not finished restoring would commit checkpoints atop a
    /// partial bulk-load, producing an inconsistent CF. Pipelines
    /// with no restore row (never restored) and pipelines marked
    /// `Complete` are allowed.
    ///
    /// [`RestoreState`]: sui_consistent_store::RestoreState
    async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> anyhow::Result<()>
    where
        H: sequential::Handler<Store = Store> + Send + Sync + 'static,
    {
        let restore_state = self
            .store()
            .db()
            .framework()
            .restore
            .get(&PipelineTaskKey::new(H::NAME))
            .with_context(|| format!("Reading restore state for pipeline {:?}", H::NAME))?;

        if let Some(state) = restore_state.as_ref().and_then(|s| s.state.as_ref()) {
            match state {
                restore_state::State::InProgress(_) => {
                    anyhow::bail!("Restoration in progress for pipeline {:?}", H::NAME);
                }
                restore_state::State::Complete(_) => {
                    // Restore finished — tip indexing may proceed.
                }
            }
        }

        if LIVE_COHORT.contains(&H::NAME) {
            self.sync.register_required_pipeline(H::NAME)
        } else {
            self.sync.register_pipeline(H::NAME)
        }
        .with_context(|| format!("Failed to add pipeline {:?} to synchronizer", H::NAME))?;

        self.indexer
            .sequential_pipeline(handler, config)
            .await
            .with_context(|| format!("Failed to add pipeline {:?} to indexer", H::NAME))?;

        Ok(())
    }

    /// Install the synchronizer onto the store and start the
    /// framework indexer. Returns a composed [`Service`] handle
    /// that drives both for the lifetime of the indexer.
    pub async fn run(self) -> anyhow::Result<Service> {
        let Self {
            indexer,
            sync,
            pruner: pruner_setup,
        } = self;

        // Capture the store for the pruner before `indexer.run`
        // consumes the framework indexer.
        let store = indexer.store().clone();

        let mut sync_join_set = indexer
            .store()
            .install_sync(sync)
            .context("Failed to install synchronizer onto store")?;

        // Wrap the synchronizer's JoinSet in a `Service` task so it
        // composes with the framework indexer's service via
        // `attach`. Per-pipeline tasks exit naturally once their
        // mpsc senders (held in the store's `Queue`) are dropped,
        // which happens on the framework indexer's shutdown.
        let s_sync = Service::new().spawn(async move {
            while let Some(res) = sync_join_set.join_next().await {
                res.context("Synchronizer task panicked")??;
            }
            Ok(())
        });

        let s_indexer = indexer.run().await?;
        let mut service = s_indexer.attach(s_sync);

        // Attach the background pruner as a secondary task when
        // configured: it advances the retention floor and deletes
        // history without extending the indexer's lifetime.
        if let Some((config, metrics)) = pruner_setup {
            let s_pruner = pruner::start_pruner(store, config, metrics)
                .context("Failed to start the rpc-store pruner")?;
            service = service.attach(s_pruner);
        }

        Ok(service)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointError;
    use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointResult;
    use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
    use sui_indexer_alt_framework::metrics::IngestionMetrics;
    use sui_types::digests::ChainIdentifier;

    use super::*;

    /// Serves a fixed set of checkpoints and reports the highest as the
    /// network tip (0 when empty); every other checkpoint is not found.
    /// The empty default suits tests that only construct the indexer
    /// and never run its ingestion loop.
    #[derive(Default)]
    struct FixedIngestionClient {
        checkpoints: BTreeMap<u64, Checkpoint>,
    }

    #[async_trait]
    impl IngestionClientTrait for FixedIngestionClient {
        async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
            Ok(ChainIdentifier::from(
                sui_types::digests::CheckpointDigest::new([0u8; 32]),
            ))
        }

        async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
            self.checkpoints
                .get(&checkpoint)
                .cloned()
                .ok_or(CheckpointError::NotFound)
        }

        async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
            Ok(self.checkpoints.keys().next_back().copied().unwrap_or(0))
        }
    }

    async fn build_indexer(layer: PipelineLayer) -> Indexer {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), &registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(FixedIngestionClient::default()),
            ingestion_metrics,
        );
        let mut indexer = Indexer::new(
            dir.path().join("db"),
            IndexerArgs::default(),
            ingestion_client,
            None,
            None,
            crate::config::ConsistencyConfig::default(),
            None,
            IngestionConfig::default(),
            DbOptions::default(),
            &registry,
        )
        .await
        .expect("Indexer::new");

        indexer
            .add_pipelines(layer, CommitterConfig::default())
            .await
            .expect("add_pipelines");

        // Keep the tempdir alive for the duration of the test by
        // leaking it — the Indexer holds the DB open, and we want
        // the path to survive until the Indexer is dropped.
        std::mem::forget(dir);
        indexer
    }

    /// `embedded` registers exactly the ten embedded-cohort
    /// pipelines (three live + seven history) and none of the
    /// deactivated raw-chain-data ones, so the synchronizer's
    /// snapshot cohort covers exactly those. Pinned to the
    /// [`LIVE_COHORT`] / [`HISTORY_COHORT`] constants (via the real
    /// `Processor::NAME`s the indexer registers) so the layer and the
    /// restore/seed cohorts cannot drift apart.
    ///
    /// [`LIVE_COHORT`]: crate::indexer::restore::LIVE_COHORT
    /// [`HISTORY_COHORT`]: crate::indexer::restore::HISTORY_COHORT
    #[tokio::test]
    async fn embedded_registers_only_cohort_pipelines() {
        let indexer = build_indexer(PipelineLayer::embedded()).await;
        let names: std::collections::BTreeSet<_> = indexer.pipelines().collect();
        let expected: std::collections::BTreeSet<_> = crate::indexer::restore::LIVE_COHORT
            .iter()
            .chain(crate::indexer::restore::HISTORY_COHORT)
            .copied()
            .collect();
        assert_eq!(names, expected);
    }

    /// `all` registers every pipeline (raw chain data + indexes).
    #[tokio::test]
    async fn all_registers_every_pipeline() {
        let indexer = build_indexer(PipelineLayer::all()).await;
        let names: std::collections::BTreeSet<_> = indexer.pipelines().collect();
        assert_eq!(
            names,
            std::collections::BTreeSet::from([
                // Raw chain data.
                "epochs",
                "checkpoint_summary",
                "checkpoint_contents",
                "checkpoint_seq_by_digest",
                "transactions",
                "tx_seq_by_digest",
                "tx_metadata_by_seq",
                "effects",
                "events",
                "objects",
                "object_version_by_checkpoint",
                // Indexes.
                "object_by_owner",
                "object_by_type",
                "balance",
                "package_versions",
                "transaction_bitmap",
                "event_bitmap",
            ])
        );
    }

    /// Open the rpc-store DB at `path` and write a single
    /// pre-existing `RestoreState` entry for `pipeline` directly
    /// to the framework's `__restore` CF. Returns the [`Store`]
    /// the orchestrator should pick up. Used by the restore-guard
    /// tests below to seed an `InProgress` or `Complete` state
    /// before [`Indexer::from_store`] runs.
    fn open_with_seeded_restore(
        path: &std::path::Path,
        pipeline: &str,
        state: sui_consistent_store::RestoreState,
    ) -> Store {
        let (db, schema) = Db::open::<RpcStoreSchema>(path, DbOptions::default()).unwrap();
        let framework = sui_consistent_store::FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch
            .put(&framework.restore, &PipelineTaskKey::new(pipeline), &state)
            .unwrap();
        batch.commit().unwrap();
        sui_consistent_store::Store::new(db, Arc::new(schema))
    }

    async fn build_indexer_with_store(store: Store) -> anyhow::Result<Indexer> {
        let registry = Registry::new();
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), &registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(FixedIngestionClient::default()),
            ingestion_metrics,
        );
        Indexer::from_store(
            store,
            IndexerArgs::default(),
            ingestion_client,
            None,
            None,
            crate::config::ConsistencyConfig::default(),
            None,
            IngestionConfig::default(),
            &registry,
        )
        .await
    }

    #[tokio::test]
    async fn add_pipelines_refuses_pipeline_with_in_progress_restore() {
        let dir = tempfile::tempdir().unwrap();
        let in_progress = sui_consistent_store::RestoreState {
            state: Some(restore_state::State::InProgress(
                restore_state::InProgress::default(),
            )),
        };
        let store = open_with_seeded_restore(&dir.path().join("db"), "balance", in_progress);

        let mut indexer = build_indexer_with_store(store).await.unwrap();
        let err = indexer
            .add_pipelines(
                PipelineLayer {
                    balance: Some(crate::config::CommitterLayer::default()),
                    ..PipelineLayer::default()
                },
                CommitterConfig::default(),
            )
            .await
            .unwrap_err();
        assert!(
            format!("{err:#}").contains("Restoration in progress for pipeline"),
            "expected restore-in-progress error, got: {err:#}",
        );
    }

    #[tokio::test]
    async fn add_pipelines_allows_pipeline_with_completed_restore() {
        let dir = tempfile::tempdir().unwrap();
        let complete = sui_consistent_store::RestoreState {
            state: Some(restore_state::State::Complete(restore_state::Complete {
                restored_at: 42,
            })),
        };
        let store = open_with_seeded_restore(&dir.path().join("db"), "balance", complete);

        let mut indexer = build_indexer_with_store(store).await.unwrap();
        indexer
            .add_pipelines(
                PipelineLayer {
                    balance: Some(crate::config::CommitterLayer::default()),
                    ..PipelineLayer::default()
                },
                CommitterConfig::default(),
            )
            .await
            .unwrap();
        let names: std::collections::BTreeSet<_> = indexer.pipelines().collect();
        assert_eq!(names, std::collections::BTreeSet::from(["balance"]));
    }

    /// The checkpoint the live pipelines resume from in the restart
    /// tests, and the network tip they index up to.
    const RESTART: u64 = 10;
    const TIP: u64 = 13;

    /// Seed `watermarks` into a fresh store at `path`, then run an
    /// indexer over it with the live cohort plus the
    /// `tx_seq_by_digest` history pipeline, fed checkpoints
    /// `RESTART..=TIP` only. Returns once snapshot `TIP` exists.
    async fn run_until_tip_snapshot(
        path: &std::path::Path,
        watermarks: &[(&str, u64)],
    ) -> (Db, Service) {
        let (db, schema) = Db::open::<RpcStoreSchema>(path, DbOptions::default()).unwrap();
        let framework = sui_consistent_store::FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        for &(pipeline, checkpoint) in watermarks {
            batch
                .put(
                    &framework.watermarks,
                    &PipelineTaskKey::new(pipeline),
                    &sui_consistent_store::Watermark::for_checkpoint(checkpoint),
                )
                .unwrap();
        }
        batch.commit().unwrap();
        let store = sui_consistent_store::Store::new(db.clone(), Arc::new(schema));

        let mut builder =
            sui_types::test_checkpoint_data_builder::TestCheckpointBuilder::new(RESTART);
        let checkpoints = (RESTART..=TIP)
            .map(|checkpoint| (checkpoint, builder.build_checkpoint()))
            .collect();

        let registry = Registry::new();
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), &registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(FixedIngestionClient { checkpoints }),
            ingestion_metrics,
        );
        // Keep a stuck history pipeline on its own ingestion service so
        // its missing checkpoints do not stall the live pipelines' feed.
        let ingestion_config = IngestionConfig {
            min_cohort_boundary: 1,
            ..IngestionConfig::default()
        };
        let mut indexer = Indexer::from_store(
            store,
            IndexerArgs::default(),
            ingestion_client,
            None,
            None,
            crate::config::ConsistencyConfig::default(),
            None,
            ingestion_config,
            &registry,
        )
        .await
        .unwrap();
        indexer
            .add_pipelines(
                PipelineLayer {
                    object_by_owner: Some(crate::config::CommitterLayer::default()),
                    object_by_type: Some(crate::config::CommitterLayer::default()),
                    balance: Some(crate::config::CommitterLayer::default()),
                    tx_seq_by_digest: Some(crate::config::CommitterLayer::default()),
                    ..PipelineLayer::default()
                },
                CommitterConfig::default(),
            )
            .await
            .unwrap();
        let service = indexer.run().await.unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while db.at_snapshot(TIP).is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("live snapshots should reach the tip");

        (db, service)
    }

    /// `pipeline`'s committed checkpoint as captured by snapshot
    /// `checkpoint`.
    fn snapshot_watermark(db: &Db, checkpoint: u64, pipeline: &str) -> Option<u64> {
        db.at_snapshot(checkpoint)
            .unwrap()
            .framework()
            .watermarks
            .get(&PipelineTaskKey::new(pipeline))
            .unwrap()
            .map(|w| w.checkpoint_hi_inclusive)
    }

    #[tokio::test]
    async fn restart_with_uneven_live_watermarks_snapshots_only_complete_live_state() {
        // A crash left `object_by_owner` and `object_by_type` at 10 but
        // `balance` at 9. The history pipeline `tx_seq_by_digest` sits at
        // 0 and checkpoints below 10 are unavailable, so it cannot make
        // progress and must not hold live snapshots back.
        let dir = tempfile::tempdir().unwrap();
        let (db, service) = run_until_tip_snapshot(
            dir.path(),
            &[
                ("object_by_owner", RESTART),
                ("object_by_type", RESTART),
                ("balance", RESTART - 1),
                ("tx_seq_by_digest", 0),
            ],
        )
        .await;

        // Every snapshot, starting at the restart checkpoint, holds every
        // live pipeline committed through that snapshot's checkpoint.
        assert_eq!(db.snapshot_range(), Some(RESTART..=TIP));
        for checkpoint in RESTART..=TIP {
            for pipeline in LIVE_COHORT {
                assert_eq!(
                    snapshot_watermark(&db, checkpoint, pipeline),
                    Some(checkpoint),
                    "snapshot {checkpoint} is missing {pipeline}'s writes",
                );
            }
        }

        drop(service);
    }

    #[tokio::test]
    async fn from_store_refuses_pipeline_filter() {
        // A pipeline the framework's filter skips would never send a
        // batch yet still be registered with the synchronizer, so the
        // filter is refused up front.
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let registry = Registry::new();
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), &registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(FixedIngestionClient::default()),
            ingestion_metrics,
        );
        let Err(err) = Indexer::from_store(
            sui_consistent_store::Store::new(db, Arc::new(schema)),
            IndexerArgs {
                pipeline: vec!["object_by_owner".to_owned()],
                ..IndexerArgs::default()
            },
            ingestion_client,
            None,
            None,
            crate::config::ConsistencyConfig::default(),
            None,
            IngestionConfig::default(),
            &registry,
        )
        .await
        else {
            panic!("a pipeline filter must be refused");
        };
        assert!(
            format!("{err:#}").contains("IndexerArgs::pipeline must be empty"),
            "unexpected error: {err:#}",
        );
    }
}
