// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db::DbConfig;
use crate::ingestion::IngestionConfig;
use crate::pipeline::PipelineConfig;
use crate::{Indexer, IndexerConfig};
use std::path::PathBuf;
use sui_synthetic_ingestion::benchmark::{run_benchmark, BenchmarkableIndexer};
use sui_synthetic_ingestion::{IndexerProgress, SyntheticIngestionConfig};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(clap::Args, Debug, Clone)]
pub struct BenchmarkConfig {
    #[command(flatten)]
    pipeline_config: PipelineConfig,

    /// Only run the following pipelines -- useful for backfills. If not provided, all pipelines
    /// will be run.
    #[arg(long, action = clap::ArgAction::Append)]
    pipeline: Vec<String>,

    /// Number of checkpoints to ingest.
    #[arg(long, default_value_t = 2000)]
    num_checkpoints: u64,

    /// Number of transactions in a checkpoint.
    #[arg(long, default_value_t = 200)]
    checkpoint_size: u64,

    /// Path to workload directory. If not provided, a temporary directory will be created.
    /// If provided, synthetic workload generator will either load data from it if it exists or generate new data.
    /// This avoids repeat generation of the same data.
    #[arg(long)]
    workload_dir: Option<PathBuf>,

    /// If true, reset the database before running the benchmark.
    #[arg(long, default_value_t = false)]
    reset_database: bool,
}

pub async fn run_indexer_benchmark(
    db_config: DbConfig,
    bench_config: BenchmarkConfig,
) -> anyhow::Result<()> {
    let BenchmarkConfig {
        pipeline_config,
        pipeline,
        num_checkpoints,
        checkpoint_size,
        workload_dir,
        reset_database,
    } = bench_config;

    if reset_database {
        crate::db::reset_database(db_config.clone(), false).await?;
    }

    let ingestion_dir = workload_dir.unwrap_or_else(|| tempfile::tempdir().unwrap().into_path());
    let indexer = BenchmarkIndexer::new(
        db_config,
        pipeline_config,
        pipeline,
        num_checkpoints,
        checkpoint_size,
        ingestion_dir.clone(),
    )
    .await;
    let starting_checkpoint = std::cmp::max(1, indexer.get_starting_checkpoint());
    let synthetic_ingestion_config = SyntheticIngestionConfig {
        ingestion_dir,
        checkpoint_size,
        num_checkpoints,
        starting_checkpoint,
    };

    run_benchmark(synthetic_ingestion_config, indexer).await;

    Ok(())
}

pub struct BenchmarkIndexer {
    inner: Option<BenchmarkIndexerInner>,
    committed_checkpoints_rx: watch::Receiver<Option<IndexerProgress>>,
}

struct BenchmarkIndexerInner {
    indexer: Indexer,
    cancel: CancellationToken,
    num_checkpoints: u64,
    checkpoint_size: u64,
    committed_checkpoints_tx: watch::Sender<Option<IndexerProgress>>,
}

impl BenchmarkIndexer {
    pub async fn new(
        db_config: DbConfig,
        pipeline_config: PipelineConfig,
        pipeline: Vec<String>,
        num_checkpoints: u64,
        checkpoint_size: u64,
        ingestion_dir: PathBuf,
    ) -> Self {
        let indexer_config = IndexerConfig {
            ingestion_config: IngestionConfig {
                remote_store_url: None,
                local_ingestion_path: Some(ingestion_dir),
                checkpoint_buffer_size: IngestionConfig::DEFAULT_CHECKPOINT_BUFFER_SIZE,
                ingest_concurrency: IngestionConfig::DEFAULT_INGEST_CONCURRENCY,
                retry_interval: IngestionConfig::DEFAULT_RETRY_INTERVAL,
            },
            pipeline_config,
            first_checkpoint: None,
            last_checkpoint: Some(num_checkpoints),
            pipeline,
            metrics_address: IndexerConfig::DEFAULT_METRICS_ADDRESS.parse().unwrap(),
        };
        let cancel = CancellationToken::new();
        let mut indexer = Indexer::new(db_config, indexer_config, cancel.clone())
            .await
            .unwrap();
        indexer.register_pipelines().await.unwrap();
        let (committed_checkpoints_tx, committed_checkpoints_rx) = watch::channel(None);
        Self {
            inner: Some(BenchmarkIndexerInner {
                indexer,
                cancel,
                num_checkpoints,
                checkpoint_size,
                committed_checkpoints_tx,
            }),
            committed_checkpoints_rx,
        }
    }

    pub fn get_starting_checkpoint(&self) -> u64 {
        self.inner
            .as_ref()
            .unwrap()
            .indexer
            .get_first_checkpoint_from_watermark()
    }
}

#[async_trait::async_trait]
impl BenchmarkableIndexer for BenchmarkIndexer {
    fn subscribe_to_committed_checkpoints(&self) -> watch::Receiver<Option<IndexerProgress>> {
        self.committed_checkpoints_rx.clone()
    }

    async fn start(&mut self) {
        let BenchmarkIndexerInner {
            indexer,
            cancel,
            num_checkpoints,
            checkpoint_size,
            committed_checkpoints_tx,
        } = self.inner.take().unwrap();
        let expected_total_transactions = checkpoint_size * num_checkpoints;
        let h_indexer = indexer.run().await.unwrap();
        tokio::task::spawn(async move {
            cancel.cancelled().await;
            let _ = h_indexer.await;
            committed_checkpoints_tx
                .send(Some(IndexerProgress {
                    checkpoint: num_checkpoints,
                    network_total_transactions: expected_total_transactions,
                }))
                .unwrap();
        });
    }

    async fn stop(self) {}
}
