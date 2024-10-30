// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{BenchmarkConfig, IngestionConfig, IngestionSources, UploadOptions};
use crate::database::ConnectionPool;
use crate::db::{reset_database, run_migrations};
use crate::errors::IndexerError;
use crate::indexer::Indexer;
use crate::metrics::IndexerMetrics;
use crate::store::PgIndexerStore;
use std::path::PathBuf;
use sui_synthetic_ingestion::benchmark::{run_benchmark, BenchmarkableIndexer};
use sui_synthetic_ingestion::{IndexerProgress, SyntheticIngestionConfig};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub async fn run_indexer_benchmark(
    config: BenchmarkConfig,
    pool: ConnectionPool,
    metrics: IndexerMetrics,
) {
    if config.reset_db {
        reset_database(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();
    } else {
        run_migrations(pool.dedicated_connection().await.unwrap())
            .await
            .unwrap();
    }
    let store = PgIndexerStore::new(pool, UploadOptions::default(), metrics.clone());
    let ingestion_dir = config
        .workload_dir
        .clone()
        .unwrap_or_else(|| tempfile::tempdir().unwrap().into_path());
    // If we are using a non-temp directory, we should not delete the ingestion directory.
    let gc_checkpoint_files = config.workload_dir.is_none();
    let synthetic_ingestion_config = SyntheticIngestionConfig {
        ingestion_dir: ingestion_dir.clone(),
        checkpoint_size: config.checkpoint_size,
        num_checkpoints: config.num_checkpoints,
        starting_checkpoint: config.starting_checkpoint,
    };
    let indexer = BenchmarkIndexer::new(store, metrics, ingestion_dir, gc_checkpoint_files);
    run_benchmark(synthetic_ingestion_config, indexer).await;
}

pub struct BenchmarkIndexer {
    inner: Option<BenchmarkIndexerInner>,
    cancel: CancellationToken,
    committed_checkpoints_rx: watch::Receiver<Option<IndexerProgress>>,
    handle: Option<JoinHandle<anyhow::Result<(), IndexerError>>>,
}

struct BenchmarkIndexerInner {
    ingestion_dir: PathBuf,
    gc_checkpoint_files: bool,
    store: PgIndexerStore,
    metrics: IndexerMetrics,
    committed_checkpoints_tx: watch::Sender<Option<IndexerProgress>>,
}

impl BenchmarkIndexer {
    pub fn new(
        store: PgIndexerStore,
        metrics: IndexerMetrics,
        ingestion_dir: PathBuf,
        gc_checkpoint_files: bool,
    ) -> Self {
        let cancel = CancellationToken::new();
        let (committed_checkpoints_tx, committed_checkpoints_rx) = watch::channel(None);
        Self {
            inner: Some(BenchmarkIndexerInner {
                ingestion_dir,
                gc_checkpoint_files,
                store,
                metrics,
                committed_checkpoints_tx,
            }),
            cancel,
            committed_checkpoints_rx,
            handle: None,
        }
    }
}

#[async_trait::async_trait]
impl BenchmarkableIndexer for BenchmarkIndexer {
    fn subscribe_to_committed_checkpoints(&self) -> watch::Receiver<Option<IndexerProgress>> {
        self.committed_checkpoints_rx.clone()
    }

    async fn start(&mut self) {
        let BenchmarkIndexerInner {
            ingestion_dir,
            gc_checkpoint_files,
            store,
            metrics,
            committed_checkpoints_tx,
        } = self.inner.take().unwrap();
        let ingestion_config = IngestionConfig {
            sources: IngestionSources {
                data_ingestion_path: Some(ingestion_dir),
                ..Default::default()
            },
            gc_checkpoint_files,
            ..Default::default()
        };
        let cancel = self.cancel.clone();
        let handle = tokio::task::spawn(async move {
            Indexer::start_writer(
                ingestion_config,
                store,
                metrics,
                Default::default(),
                None,
                cancel,
                Some(committed_checkpoints_tx),
            )
            .await
        });
        self.handle = Some(handle);
    }

    async fn stop(mut self) {
        self.cancel.cancel();
        self.handle.unwrap().await.unwrap().unwrap();
    }
}
