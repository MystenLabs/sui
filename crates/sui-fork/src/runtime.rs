// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Runtime state for a local Sui fork.
//!
//! `ForkRuntime` owns the durable RPC-store state under the fork data
//! directory. Opening it validates or creates fork metadata, opens the local
//! `sui-rpc-store` RocksDB instance, records the fork chain identifier, and
//! refreshes RPC-store pruning metadata before any readers are handed out.
//!
//! After the startup path creates Simulacrum, this module can start the embedded
//! `sui-rpc-store` indexer. The indexer consumes checkpoints produced by
//! Simulacrum, saves them into the local RPC store, and runs the checkpoint
//! broadcast pipeline used by RPC subscriptions.
//!
//! The module deliberately stays below orchestration concerns. `startup`
//! chooses the remote checkpoint, initializes remote readers, seeds Simulacrum
//! state, and builds the RPC server; this module keeps the opened store and
//! indexer service alive for the rest of the process.

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
use sui_rpc_store::Indexer;
use sui_rpc_store::PipelineLayer;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::Store;
use sui_rpc_store::default_rocksdb_config;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::ingestion::SimulacrumIngestion;
use crate::live_state::LiveState;
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

/// Opened fork runtime state backed by `sui-rpc-store`.
///
/// The runtime owns the local RPC store and, once started, keeps the embedded
/// `sui-rpc-store` indexer alive for local Simulacrum checkpoints.
pub(crate) struct ForkRuntime {
    db: Db,
    schema: Arc<RpcStoreSchema>,
    /// Fork-owned `ObjectID -> current live version` pointer table, kept in its
    /// own store beside the rpc-store; stock `sui-rpc-store` has no `ObjectID`-keyed
    /// current-version pointer. See [`crate::live_state`] and [`LocalStore`].
    live_state: Arc<LiveState>,
    metadata: ForkMetadata,
    indexer_pipelines: Vec<&'static str>,
    /// Handle to the running indexer. Holding it keeps the indexer alive
    /// (dropping the `Service` stops the background tasks), and
    /// [`Self::indexer_stopped`] joins it to observe failures. Behind an async
    /// mutex because `Service::join` needs exclusive access while the runtime
    /// is shared behind the `Context`.
    indexer_service: Option<tokio::sync::Mutex<Service>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ForkMetadata {
    format_version: u32,
    network: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
    chain_identifier: [u8; 32],
}

impl ForkRuntime {
    pub(crate) fn open(
        root: &Path,
        network: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
        chain_identifier: ChainIdentifier,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(root)
            .with_context(|| format!("failed to create fork data directory {}", root.display()))?;

        let metadata = ForkMetadata {
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
        schema
            .refresh_pruning_atomics()
            .context("failed to refresh rpc store pruning watermarks")?;

        Self::seed_chain_identifier(&db, metadata.chain_identifier)?;

        let live_state =
            Arc::new(LiveState::open(root).context("failed to open fork live-state store")?);

        Ok(Self {
            db,
            schema,
            live_state,
            metadata,
            indexer_pipelines: Vec::new(),
            indexer_service: None,
        })
    }

    pub(crate) fn existing_forked_checkpoint(
        root: &Path,
        network: &str,
        requested_checkpoint: Option<CheckpointSequenceNumber>,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        let path = Self::metadata_path(root);
        if !path.exists() {
            return Ok(None);
        }
        let stored: ForkMetadata = serde_json::from_slice(
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
            .add_pipelines(PipelineLayer::all(), committer_config.clone())
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

    pub(crate) fn reader(&self) -> RpcStoreReader {
        RpcStoreReader::new(self.db.clone(), self.schema.clone())
    }

    pub(crate) fn local_store(&self) -> LocalStore {
        LocalStore::new(
            self.db.clone(),
            self.schema.clone(),
            self.live_state.clone(),
        )
    }

    /// Resolves when the embedded indexer stops: `Ok(())` if all its tasks
    /// completed (unexpected while the fork is serving), or the task error if
    /// any pipeline failed or panicked. Pends forever if the indexer was never
    /// started.
    pub(crate) async fn indexer_stopped(&self) -> anyhow::Result<()> {
        let Some(service) = &self.indexer_service else {
            return std::future::pending().await;
        };
        service.lock().await.join().await
    }

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

    fn chain_identifier(&self) -> ChainIdentifier {
        CheckpointDigest::new(self.metadata.chain_identifier).into()
    }

    fn rpc_store_path(root: &Path) -> PathBuf {
        root.join(RPC_STORE_DIR)
    }

    fn metadata_path(root: &Path) -> PathBuf {
        root.join(FORK_METADATA_FILE)
    }

    fn load_or_write_metadata(root: &Path, expected: &ForkMetadata) -> anyhow::Result<()> {
        let path = Self::metadata_path(root);
        if path.exists() {
            let stored: ForkMetadata = serde_json::from_slice(
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

    #[test]
    fn open_writes_metadata_and_seeds_chain_identifier() {
        let dir = tempfile::tempdir().unwrap();
        let chain_identifier = get_mainnet_chain_identifier();

        let runtime =
            ForkRuntime::open(dir.path(), "mainnet".to_owned(), 42, chain_identifier).unwrap();

        assert_eq!(
            runtime.metadata,
            ForkMetadata {
                format_version: FORK_METADATA_FORMAT_VERSION,
                network: "mainnet".to_owned(),
                forked_at_checkpoint: 42,
                chain_identifier: *chain_identifier.as_bytes(),
            },
        );
        assert!(ForkRuntime::metadata_path(dir.path()).exists());
        assert!(ForkRuntime::rpc_store_path(dir.path()).exists());

        let framework = FrameworkSchema::new(runtime.db.clone());
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
        drop(ForkRuntime::open(
            dir.path(),
            "mainnet".to_owned(),
            42,
            chain_identifier,
        ));

        let err = ForkRuntime::open(dir.path(), "mainnet".to_owned(), 43, chain_identifier)
            .err()
            .expect("metadata mismatch should fail");

        assert!(
            format!("{err:#}").contains("fork metadata"),
            "unexpected error: {err:#}",
        );
    }
}
