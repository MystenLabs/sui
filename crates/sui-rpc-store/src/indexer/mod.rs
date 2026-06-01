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
//! writes against a [`Connection`] from
//! [`sui_consistent_store::Store`].
//!
//! Every pipeline targets the same backing [`RpcStoreSchema`].

pub mod balance;
pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod effects;
pub mod epochs;
pub mod event_bitmap;
pub mod events;
pub mod live_objects;
pub mod object_by_owner;
pub mod object_by_type;
pub mod objects;
pub mod package_versions;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::btree_map::Entry;
use std::num::NonZero;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use prometheus::Registry;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::Synchronizer;
use sui_indexer_alt_framework as framework;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::BoxedStreamingClient;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::IngestionService;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
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

/// Metrics prefix used for both the framework indexer and the
/// underlying ingestion service. Surfaced as a constant so the
/// prefix is consistent across the metrics built in [`Indexer::new`]
/// and the ones the standalone-binary entry point builds when it
/// constructs the [`IngestionClient`] / [`IngestionService`] from
/// `ClientArgs`.
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
/// ([`object_by_owner`](crate::indexer::object_by_owner) etc.) to
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
/// Used to populate the latest-version views
/// ([`live_objects`](crate::indexer::live_objects)) and the
/// diff-based indexes once the prior state has been retracted.
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
/// off does not stall snapshots.
///
/// After pipelines are registered, [`Self::run`] installs the
/// synchronizer onto the store and starts the framework indexer.
pub struct Indexer {
    indexer: framework::Indexer<Store>,

    /// Synchronizer coordinating per-pipeline writes against
    /// cross-pipeline snapshots. Owned here until [`Self::run`]
    /// hands it to [`sui_consistent_store::Store::install_sync`].
    sync: Synchronizer,
}

impl Indexer {
    /// Open the database at `path` with [`RpcStoreSchema`] and
    /// construct an [`Indexer`] backed by it.
    ///
    /// `ingestion_client` is the pull-side checkpoint source; the
    /// optional `streaming_client` is the live-tail source. Both
    /// are trait objects so callers (standalone binary, embedded
    /// fullnode) can supply implementations sourced from wherever
    /// makes sense for their environment.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        path: impl AsRef<Path>,
        indexer_args: IndexerArgs,
        ingestion_client: Arc<dyn IngestionClientTrait>,
        streaming_client: Option<BoxedStreamingClient>,
        consistency_config: crate::config::ConsistencyConfig,
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
            consistency_config,
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
    pub async fn from_store(
        store: Store,
        indexer_args: IndexerArgs,
        ingestion_client: Arc<dyn IngestionClientTrait>,
        streaming_client: Option<BoxedStreamingClient>,
        consistency_config: crate::config::ConsistencyConfig,
        ingestion_config: IngestionConfig,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let metrics_prefix = Some(METRICS_PREFIX);

        let stride = NonZero::new(consistency_config.stride)
            .context("ConsistencyConfig::stride must be non-zero")?;
        let sync = Synchronizer::new(
            store.db().clone(),
            stride,
            consistency_config.buffer_size,
            indexer_args.first_checkpoint,
        );

        let ingestion_metrics = IngestionMetrics::new(metrics_prefix, registry);
        let ingestion_client =
            IngestionClient::from_trait(ingestion_client, ingestion_metrics.clone());

        let ingestion_service = IngestionService::with_clients(
            ingestion_client,
            streaming_client,
            ingestion_config,
            ingestion_metrics,
        );

        let indexer = framework::Indexer::with_ingestion_service(
            store,
            indexer_args,
            ingestion_service,
            metrics_prefix,
            registry,
        )
        .await
        .context("Failed to construct framework indexer")?;

        Ok(Self { indexer, sync })
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
            live_objects,
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
        add!(self::live_objects::LiveObjects, live_objects);

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

    /// Register a single sequential pipeline. The pipeline is
    /// announced to the synchronizer before being handed to the
    /// framework indexer so that, by the time the first batch
    /// flows through, the synchronizer task is already waiting on
    /// the pipeline's queue.
    async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> anyhow::Result<()>
    where
        H: sequential::Handler<Store = Store> + Send + Sync + 'static,
    {
        self.sync
            .register_pipeline(H::NAME)
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
        let mut sync_join_set = self
            .indexer
            .store()
            .install_sync(self.sync)
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

        let s_indexer = self.indexer.run().await?;
        Ok(s_indexer.attach(s_sync))
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointError;
    use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointResult;
    use sui_types::digests::ChainIdentifier;

    use super::*;

    /// Stub [`IngestionClientTrait`] for orchestrator wiring
    /// tests. Reports a fixed chain id and latest checkpoint and
    /// fails any actual fetch; suitable for tests that only need
    /// `Indexer::from_store` to construct (which probes
    /// `latest_checkpoint_number` once) and never run the
    /// ingestion loop.
    struct StubIngestionClient;

    #[async_trait]
    impl IngestionClientTrait for StubIngestionClient {
        async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
            Ok(ChainIdentifier::from(
                sui_types::digests::CheckpointDigest::new([0u8; 32]),
            ))
        }

        async fn checkpoint(&self, _checkpoint: u64) -> CheckpointResult {
            Err(CheckpointError::NotFound)
        }

        async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
            Ok(0)
        }
    }

    async fn build_indexer(layer: PipelineLayer) -> Indexer {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let mut indexer = Indexer::new(
            dir.path().join("db"),
            IndexerArgs::default(),
            Arc::new(StubIngestionClient),
            None,
            crate::config::ConsistencyConfig::default(),
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

    /// `indexes_only` registers exactly the six index pipelines
    /// and no raw-chain-data ones. Validates that the per-pipeline
    /// enable/disable toggle is wired through to the framework
    /// indexer.
    #[tokio::test]
    async fn indexes_only_registers_only_index_pipelines() {
        let indexer = build_indexer(PipelineLayer::indexes_only()).await;
        let names: std::collections::BTreeSet<_> = indexer.pipelines().collect();
        assert_eq!(
            names,
            std::collections::BTreeSet::from([
                "object_by_owner",
                "object_by_type",
                "balance",
                "package_versions",
                "transaction_bitmap",
                "event_bitmap",
            ])
        );
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
                "live_objects",
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
}
