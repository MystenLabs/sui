// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Initialization and ownership of the fork's underlying services.
//!
//! `ServiceManager` owns the durable RPC-store state under the fork data directory. Opening it
//! validates or creates fork metadata, opens the local `sui-rpc-store` RocksDB instance, and
//! records the fork chain identifier before any readers are handed out.
//!
//! After the startup path creates Simulacrum, this module can start the embedded `sui-rpc-store`
//! indexer. The indexer consumes checkpoints produced by Simulacrum, saves them into the local RPC
//! store, and runs the checkpoint broadcast pipeline used by RPC subscriptions.
//!
//! The module deliberately stays below orchestration concerns. `startup` chooses the remote
//! checkpoint, initializes remote readers, seeds Simulacrum state, and builds the RPC server,
//! while this module keeps the opened store and indexer service alive for the rest of the process.

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::ensure;
use prometheus::Registry;
use rand::rngs::OsRng;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tokio::time::Instant;

use simulacrum::Simulacrum;
use sui_consistent_store::ChainId;
use sui_consistent_store::Db;
use sui_consistent_store::DbOptions;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_futures::service::Service;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_rpc_store::CommitterLayer;
use sui_rpc_store::Indexer;
use sui_rpc_store::PipelineLayer;
#[cfg(test)]
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::Store;
use sui_rpc_store::default_rocksdb_config;
use sui_rpc_store::indexer::balance::Balance;
use sui_rpc_store::indexer::epochs::Epochs;
use sui_rpc_store::indexer::event_bitmap::EventBitmap;
use sui_rpc_store::indexer::object_by_owner::ObjectByOwner;
use sui_rpc_store::indexer::object_by_type::ObjectByType;
use sui_rpc_store::indexer::package_versions::PackageVersions;
use sui_rpc_store::indexer::transaction_bitmap::TransactionBitmap;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::ingestion::SimulacrumIngestion;
use crate::local_store::LocalStore;
use crate::store::ForkStore;

const RPC_STORE_DIR: &str = "rpc_store";
const FORK_METADATA_FILE: &str = "fork_metadata.json";
const FORK_METADATA_FORMAT_VERSION: u32 = 1;
const FORK_CHAIN_ID_PIPELINE: &str = "sui_fork";
const METRICS_PREFIX: &str = "sui_fork_rpc_store";
const INDEXED_CHECKPOINT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const INDEXED_CHECKPOINT_TIMEOUT: Duration = Duration::from_secs(30);

type ForkedSimulacrum = Simulacrum<OsRng, ForkStore>;

/// The pipelines the embedded indexer owns, by name, matching the set
/// [`ServiceManager::pipeline_layer`] enables, which the test at the bottom of this file pins.
///
/// These are the only pipelines whose watermarks anything writes, so they are also the only ones
/// that can bound the fork's indexed tip. The column families `LocalStore` writes are committed in
/// the same batch as their data and never lag, so including them would peg the bound at `None`
/// forever. Two callers need the names rather than the layer, the reader's pipeline set in
/// [`LocalStore::new`] and the watermarks the seed load writes.
pub(crate) const FORK_INDEXER_PIPELINES: &[&str] = &[
    ObjectByOwner::NAME,
    ObjectByType::NAME,
    Balance::NAME,
    PackageVersions::NAME,
    TransactionBitmap::NAME,
    EventBitmap::NAME,
    Epochs::NAME,
];

/// Opened fork services backed by `sui-rpc-store`.
///
/// The manager owns the local RPC store and, once started, keeps the embedded `sui-rpc-store`
/// indexer alive for local Simulacrum checkpoints.
pub(crate) struct ServiceManager {
    db: Db,
    schema: Arc<RpcStoreSchema>,
    metadata: Metadata,
    indexer_pipelines: Vec<&'static str>,
    /// Handle to the running indexer. Holding it keeps the indexer alive (dropping the `Service`
    /// stops the background tasks), and [`Self::indexer_stopped`] joins it to observe failures.
    /// Behind an async mutex because `Service::join` needs exclusive access while the manager is
    /// shared behind the `Context`.
    indexer_service: Option<tokio::sync::Mutex<Service>>,
}

/// What fork a data directory belongs to, written once and thereafter compared on every open.
/// Equality is the whole check, so every field is part of the fork's identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Metadata {
    format_version: u32,
    network: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
    chain_identifier: [u8; 32],
}

impl ServiceManager {
    /// Open the fork's durable state under `root`, creating it if this is a new fork directory.
    ///
    /// Everything a reader depends on is established here, before any reader exists. Metadata is
    /// written or matched against what is already on disk, the rpc-store is opened, and the chain
    /// identifier is recorded. Reusing a directory that describes a different network or
    /// checkpoint fails rather than reinterpreting it.
    pub(crate) fn open(
        root: &Path,
        network: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
        chain_identifier: ChainIdentifier,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(root)
            .with_context(|| format!("failed to create fork data directory {}", root.display()))?;

        let metadata = Metadata {
            format_version: FORK_METADATA_FORMAT_VERSION,
            network,
            forked_at_checkpoint,
            chain_identifier: *chain_identifier.as_bytes(),
        };
        Self::load_or_write_metadata(root, &metadata)?;

        let db_path = Self::rpc_store_path(root);
        let (db, schema) = Db::open::<RpcStoreSchema>(
            &db_path,
            DbOptions {
                rocksdb: default_rocksdb_config(),
                ..DbOptions::default()
            },
        )
        .with_context(|| format!("failed to open rpc store at {}", db_path.display()))?;
        let schema = Arc::new(schema);

        Self::seed_chain_identifier(&db, metadata.chain_identifier)?;

        Ok(Self {
            db,
            schema,
            metadata,
            indexer_pipelines: Vec::new(),
            indexer_service: None,
        })
    }

    /// Return the checkpoint an existing fork directory was forked at, or `None` if `root` holds
    /// no fork yet.
    ///
    /// The metadata sidecar is read without opening the store, so startup can decide whether it is
    /// resuming a fork or creating one before committing to either. A directory that belongs to a
    /// different network, or to a different checkpoint than the one requested, is an error.
    pub(crate) fn existing_forked_checkpoint(
        root: &Path,
        network: &str,
        requested_checkpoint: Option<CheckpointSequenceNumber>,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        let path = Self::metadata_path(root);
        if !path.exists() {
            return Ok(None);
        }
        let stored: Metadata = serde_json::from_slice(
            &fs::read(&path)
                .with_context(|| format!("failed to read fork metadata {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse fork metadata {}", path.display()))?;
        ensure!(
            stored.network == network,
            "fork metadata network {} does not match requested network {}. Use a different --data-dir.",
            stored.network,
            network,
        );
        if let Some(checkpoint) = requested_checkpoint {
            ensure!(
                stored.forked_at_checkpoint == checkpoint,
                "fork metadata checkpoint {} does not match requested checkpoint {}. Use a different --data-dir.",
                stored.forked_at_checkpoint,
                checkpoint,
            );
        }
        Ok(Some(stored.forked_at_checkpoint))
    }

    /// Start the embedded rpc-store indexer over locally produced checkpoints.
    ///
    /// Ingestion reads checkpoints back out of `simulacrum`, starting at the checkpoint after the
    /// fork point, because everything at or below it is pre-fork state the seed load already
    /// placed. Registers the pipelines in [`Self::pipeline_layer`] plus the broadcast pipeline
    /// that feeds RPC subscriptions, and errors if an indexer is already running.
    pub(crate) async fn start_indexer(
        &mut self,
        simulacrum: Arc<RwLock<ForkedSimulacrum>>,
        checkpoint_sender: broadcast::Sender<Arc<Checkpoint>>,
        registry: &Registry,
    ) -> anyhow::Result<()> {
        ensure!(
            self.indexer_service.is_none(),
            "fork rpc-store indexer is already running",
        );

        let first_checkpoint = self
            .metadata
            .forked_at_checkpoint
            .checked_add(1)
            .context("forked_at_checkpoint cannot be u64::MAX")?;
        let ingestion_metrics = IngestionMetrics::new(Some(METRICS_PREFIX), registry);
        let ingestion_client = IngestionClient::from_trait(
            Arc::new(SimulacrumIngestion::new(
                simulacrum,
                self.chain_identifier(),
            )),
            ingestion_metrics,
        );
        let store = Store::new(self.db.clone(), self.schema.clone());
        let mut indexer = Indexer::from_store(
            store,
            IndexerArgs {
                first_checkpoint: Some(first_checkpoint),
                ..IndexerArgs::default()
            },
            ingestion_client,
            None,
            sui_rpc_store::ConsistencyConfig::default(),
            None,
            IngestionConfig::default(),
            registry,
        )
        .await
        .context("failed to construct fork rpc-store indexer")?;

        let committer_config = CommitterConfig::default();
        indexer
            .add_pipelines(Self::pipeline_layer(), committer_config.clone())
            .await
            .context("failed to register fork rpc-store pipelines")?;
        indexer
            .add_checkpoint_broadcast(checkpoint_sender, committer_config)
            .await
            .context("failed to register fork checkpoint broadcast pipeline")?;

        self.indexer_pipelines = indexer.pipelines().collect();
        self.indexer_service = Some(tokio::sync::Mutex::new(
            indexer
                .run()
                .await
                .context("failed to start fork rpc-store indexer")?,
        ));
        Ok(())
    }

    /// The pipelines the embedded indexer owns.
    ///
    /// Only those whose column families the fork does not write itself. The fork is in the same
    /// position as a fullnode running this indexer beside a perpetual store, in that it already
    /// holds the raw chain data and serves it directly, so re-deriving it here would write every
    /// one of those rows twice per checkpoint. It has to be the indexer that stands down rather
    /// than `LocalStore`, because the indexer's own ingestion source is the fork's store
    /// ([`crate::ingestion::SimulacrumIngestion`] reads each checkpoint back out of it), so those
    /// rows must already be there before ingestion can run at all.
    ///
    /// Two of the enabled pipelines are load-bearing in ways that are easy to miss.
    /// `package_versions` is the only writer for a package published after the fork point, since
    /// `stage_local_object_diff` never stages a package-version row, so disabling it would
    /// silently lose those rows. `balance` accumulates through a merge operator, so its writers
    /// must stay disjoint. The fork writes pre-fork balances during the seed load and the indexer
    /// writes post-fork ones, and any overlap doubles the value rather than being idempotent.
    fn pipeline_layer() -> PipelineLayer {
        PipelineLayer {
            object_by_owner: Some(CommitterLayer::default()),
            object_by_type: Some(CommitterLayer::default()),
            balance: Some(CommitterLayer::default()),
            package_versions: Some(CommitterLayer::default()),
            transaction_bitmap: Some(CommitterLayer::default()),
            event_bitmap: Some(CommitterLayer::default()),
            epochs: Some(CommitterLayer::default()),
            // Left off deliberately: `checkpoint_summary`,
            // `checkpoint_contents`, `checkpoint_seq_by_digest`,
            // `transactions`, `effects`, `events`, `tx_seq_by_digest`,
            // `tx_metadata_by_seq`, `objects`, and
            // `object_version_by_checkpoint` are all written synchronously by
            // `LocalStore`.
            ..PipelineLayer::default()
        }
    }

    /// Return a bare stock reader over the fork's rpc-store, used by tests to assert on raw rows.
    /// Production reads flow through [`LocalStore`] and `ForkStore`.
    #[cfg(test)]
    pub(crate) fn reader(&self) -> RpcStoreReader {
        RpcStoreReader::new(self.db.clone(), self.schema.clone())
    }

    /// Return a [`LocalStore`] handle over the same rpc-store, pinned at the fork checkpoint.
    /// Cheap to call, because the underlying db and schema are shared.
    pub(crate) fn local_store(&self) -> LocalStore {
        LocalStore::new(
            self.db.clone(),
            self.schema.clone(),
            self.metadata.forked_at_checkpoint,
        )
    }

    /// Resolve when the embedded indexer stops, with `Ok(())` if all its tasks completed
    /// (unexpected while the fork is serving), or the task error if any pipeline failed or
    /// panicked. Pends forever if the indexer was never started.
    pub(crate) async fn indexer_stopped(&self) -> anyhow::Result<()> {
        let Some(service) = &self.indexer_service else {
            return std::future::pending().await;
        };
        service.lock().await.join().await
    }

    /// Wait until the indexer has committed `checkpoint` on every pipeline it owns, so a caller
    /// that just produced a checkpoint can rely on the derived indexes reflecting it.
    ///
    /// Polls rather than subscribing, and gives up after [`INDEXED_CHECKPOINT_TIMEOUT`] so a
    /// stalled pipeline surfaces as an error instead of hanging the caller forever.
    pub(crate) async fn wait_for_indexed_checkpoint(
        &self,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        let deadline = Instant::now() + INDEXED_CHECKPOINT_TIMEOUT;
        loop {
            if self
                .highest_indexed_checkpoint()?
                .is_some_and(|indexed| indexed >= checkpoint)
            {
                return Ok(());
            }

            ensure!(
                Instant::now() < deadline,
                "timed out waiting for rpc-store to index checkpoint {checkpoint}",
            );
            tokio::time::sleep(INDEXED_CHECKPOINT_POLL_INTERVAL).await;
        }
    }

    /// Return the highest checkpoint every indexer pipeline has committed, which is the lowest of
    /// their watermarks, since a checkpoint is only fully indexed once the slowest pipeline has
    /// it.
    ///
    /// Returns `None` when the value is not yet meaningful, either because no indexer is running
    /// or because a pipeline has not written a watermark at all.
    fn highest_indexed_checkpoint(&self) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        if self.indexer_pipelines.is_empty() {
            return Ok(None);
        }

        let framework = FrameworkSchema::new(self.db.clone());
        let mut indexed: Option<CheckpointSequenceNumber> = None;
        for pipeline in &self.indexer_pipelines {
            let Some(watermark) = framework
                .watermarks
                .get(&PipelineTaskKey::new(*pipeline))
                .with_context(|| format!("failed to read {pipeline} watermark"))?
            else {
                return Ok(None);
            };
            indexed = Some(
                indexed.map_or(watermark.checkpoint_hi_inclusive, |checkpoint| {
                    checkpoint.min(watermark.checkpoint_hi_inclusive)
                }),
            );
        }
        Ok(indexed)
    }

    /// The forked-from chain's identifier, as recorded in fork metadata.
    fn chain_identifier(&self) -> ChainIdentifier {
        CheckpointDigest::new(self.metadata.chain_identifier).into()
    }

    /// Where the rpc-store's RocksDB lives inside a fork data directory.
    fn rpc_store_path(root: &Path) -> PathBuf {
        root.join(RPC_STORE_DIR)
    }

    /// Where the fork metadata sidecar lives inside a fork data directory.
    fn metadata_path(root: &Path) -> PathBuf {
        root.join(FORK_METADATA_FILE)
    }

    /// Establish that `root` belongs to the fork described by `expected`, writing the metadata if
    /// the directory is new and rejecting it if what is already there disagrees.
    fn load_or_write_metadata(root: &Path, expected: &Metadata) -> anyhow::Result<()> {
        let path = Self::metadata_path(root);
        if path.exists() {
            let stored: Metadata = serde_json::from_slice(
                &fs::read(&path)
                    .with_context(|| format!("failed to read fork metadata {}", path.display()))?,
            )
            .with_context(|| format!("failed to parse fork metadata {}", path.display()))?;
            ensure!(
                stored == *expected,
                "fork metadata at {} does not match requested fork. Use a different --data-dir.",
                path.display(),
            );
            return Ok(());
        }

        // Temp-file + rename, like the other metadata sidecars: a crash
        // mid-write must not leave a truncated fork_metadata.json, which
        // open() would reject as a mismatched fork on every later launch.
        crate::metadata::write_json_exclusive(&path, expected, "fork metadata")
    }

    /// Record the forked-from chain identifier in the rpc-store, under the fork's own pipeline
    /// key.
    ///
    /// The store pins each pipeline to the chain it first ingested and refuses checkpoints from
    /// another one, so every identifier already present must equal `expected`. One that disagrees
    /// means this directory holds a different chain's data, whatever the metadata sidecar says.
    fn seed_chain_identifier(db: &Db, expected: [u8; 32]) -> anyhow::Result<()> {
        let expected = ChainId(expected);
        let framework = FrameworkSchema::new(db.clone());
        for entry in framework
            .chain_ids
            .iter(..)
            .context("failed to iterate rpc store chain identifiers")?
        {
            let (_, chain_id) = entry.context("failed to read rpc store chain identifier")?;
            ensure!(
                chain_id == expected,
                "rpc store chain identifier does not match fork metadata. Use a different --data-dir.",
            );
        }

        let mut batch = db.batch();
        batch
            .put(
                &framework.chain_ids,
                &PipelineTaskKey::new(FORK_CHAIN_ID_PIPELINE),
                &expected,
            )
            .context("failed to stage rpc store chain identifier")?;
        batch
            .commit()
            .context("failed to commit rpc store chain identifier")
    }
}

#[cfg(test)]
mod tests {
    use sui_types::digests::get_mainnet_chain_identifier;

    use super::*;

    /// The indexer must own exactly the column families `LocalStore` does not write, and no
    /// others.
    ///
    /// Both directions bite silently. Enabling one the fork already writes duplicates every row of
    /// it per checkpoint, and for `balance`, which accumulates through a merge operator,
    /// duplication would double the value rather than being idempotent. Disabling one the fork
    /// does not write loses those rows outright. `package_versions` is the only writer for a
    /// package published after the fork point, since `stage_local_object_diff` never stages a
    /// package-version row.
    ///
    /// Neither failure shows up as a test error elsewhere, so the split is pinned here.
    #[test]
    fn indexer_owns_only_the_pipelines_the_fork_does_not_write() {
        let layer = ServiceManager::pipeline_layer();

        for (name, enabled) in [
            ("object_by_owner", layer.object_by_owner.is_some()),
            ("object_by_type", layer.object_by_type.is_some()),
            ("balance", layer.balance.is_some()),
            ("package_versions", layer.package_versions.is_some()),
            ("transaction_bitmap", layer.transaction_bitmap.is_some()),
            ("event_bitmap", layer.event_bitmap.is_some()),
            ("epochs", layer.epochs.is_some()),
        ] {
            assert!(enabled, "{name} has no other writer and must stay enabled");
            assert!(
                FORK_INDEXER_PIPELINES.contains(&name),
                "{name} is registered but missing from FORK_INDEXER_PIPELINES, so nothing \
                 seeds its watermark and the reported indexed tip stays None",
            );
        }

        assert_eq!(
            FORK_INDEXER_PIPELINES.len(),
            7,
            "FORK_INDEXER_PIPELINES lists a pipeline the layer does not register; its \
             watermark would never advance and would pin the reported tip",
        );

        for (name, enabled) in [
            ("checkpoint_summary", layer.checkpoint_summary.is_some()),
            ("checkpoint_contents", layer.checkpoint_contents.is_some()),
            (
                "checkpoint_seq_by_digest",
                layer.checkpoint_seq_by_digest.is_some(),
            ),
            ("transactions", layer.transactions.is_some()),
            ("effects", layer.effects.is_some()),
            ("events", layer.events.is_some()),
            ("tx_seq_by_digest", layer.tx_seq_by_digest.is_some()),
            ("tx_metadata_by_seq", layer.tx_metadata_by_seq.is_some()),
            ("objects", layer.objects.is_some()),
            (
                "object_version_by_checkpoint",
                layer.object_version_by_checkpoint.is_some(),
            ),
        ] {
            assert!(
                !enabled,
                "{name} is written by LocalStore; enabling it here double-writes every row"
            );
        }
    }

    #[test]
    fn open_writes_metadata_and_seeds_chain_identifier() {
        let dir = tempfile::tempdir().unwrap();
        let chain_identifier = get_mainnet_chain_identifier();

        let services =
            ServiceManager::open(dir.path(), "mainnet".to_owned(), 42, chain_identifier).unwrap();

        assert_eq!(
            services.metadata,
            Metadata {
                format_version: FORK_METADATA_FORMAT_VERSION,
                network: "mainnet".to_owned(),
                forked_at_checkpoint: 42,
                chain_identifier: *chain_identifier.as_bytes(),
            },
        );
        assert!(ServiceManager::metadata_path(dir.path()).exists());
        assert!(ServiceManager::rpc_store_path(dir.path()).exists());

        let framework = FrameworkSchema::new(services.db.clone());
        assert_eq!(
            framework
                .chain_ids
                .get(&PipelineTaskKey::new(FORK_CHAIN_ID_PIPELINE))
                .unwrap(),
            Some(ChainId(*chain_identifier.as_bytes())),
        );
    }

    #[test]
    fn open_rejects_mismatched_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let chain_identifier = get_mainnet_chain_identifier();
        drop(ServiceManager::open(
            dir.path(),
            "mainnet".to_owned(),
            42,
            chain_identifier,
        ));

        let err = ServiceManager::open(dir.path(), "mainnet".to_owned(), 43, chain_identifier)
            .err()
            .expect("metadata mismatch should fail");

        assert!(
            format!("{err:#}").contains("fork metadata"),
            "unexpected error: {err:#}",
        );
    }
}
