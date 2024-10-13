// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{
    BenchmarkConfig, IngestionConfig, IngestionSources, SnapshotLagConfig, UploadOptions,
};
use crate::database::ConnectionPool;
use crate::db::{reset_database, run_migrations};
use crate::indexer::Indexer;
use crate::metrics::IndexerMetrics;
use crate::store::PgIndexerStore;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tempfile::tempdir;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

mod synthetic_ingestion;

pub async fn run_benchmark(
    config: BenchmarkConfig,
    pool: ConnectionPool,
    metrics: IndexerMetrics,
) -> anyhow::Result<()> {
    let ingestion_dir = tempdir()?;
    let path = ingestion_dir.path().to_path_buf();
    let (wamup_finish_sender, wamup_finish_receiver) = oneshot::channel();

    let ingestion_task = tokio::task::spawn(async move {
        synthetic_ingestion::run_synthetic_ingestion(
            path,
            config.checkpoint_size,
            config.num_checkpoints,
            config.warmup_tx_count,
            wamup_finish_sender,
        )
        .await
    });
    wamup_finish_receiver.await?;

    if config.reset_database {
        reset_database(pool.dedicated_connection().await?).await?;
    } else {
        run_migrations(pool.dedicated_connection().await?).await?;
    }

    let store = PgIndexerStore::new(pool, UploadOptions::default(), metrics.clone());
    let ingestion_config = IngestionConfig {
        sources: IngestionSources {
            data_ingestion_path: Some(ingestion_dir.into_path()),
            ..Default::default()
        },
        ..Default::default()
    };
    let cancel = CancellationToken::new();

    Indexer::start_writer(
        &ingestion_config,
        store,
        metrics,
        SnapshotLagConfig::default(),
        None,
        cancel.clone(),
    )
    .await?;

    ingestion_task.await?;
    cancel.cancel();
    Ok(())
}

pub struct TpsLogger {
    /// Name of the logger.
    name: &'static str,
    /// Print TPS information every `log_tx_frequency` transactions.
    log_tx_frequency: u64,
    prev_state: Option<LoggerState>,
}

struct LoggerState {
    tx_count: u64,
    checkpoint: CheckpointSequenceNumber,
    timer: std::time::Instant,
}

impl TpsLogger {
    pub fn new(name: &'static str, log_tx_frequency: u64) -> Self {
        Self {
            name,
            log_tx_frequency,
            prev_state: None,
        }
    }

    pub fn log(&mut self, cur_tx_count: u64, cur_checkpoint: CheckpointSequenceNumber) {
        let Some(prev_state) = &self.prev_state else {
            self.prev_state = Some(LoggerState {
                tx_count: cur_tx_count,
                checkpoint: cur_checkpoint,
                timer: std::time::Instant::now(),
            });
            return;
        };
        let tx_delta = cur_tx_count - prev_state.tx_count;
        if tx_delta >= self.log_tx_frequency {
            let checkpoint_delta = cur_checkpoint - prev_state.checkpoint;
            let elapsed = prev_state.timer.elapsed();
            let tps = tx_delta as f64 / elapsed.as_secs_f64();
            info!(
                "[{}] Total transactions processed: {}, total checkpoints: {}. \
                TPS: {:.2}. Checkpoints per second: {:.2}",
                self.name,
                cur_tx_count,
                cur_checkpoint,
                tps,
                checkpoint_delta as f64 / elapsed.as_secs_f64()
            );
            self.prev_state = Some(LoggerState {
                tx_count: cur_tx_count,
                checkpoint: cur_checkpoint,
                timer: std::time::Instant::now(),
            });
        }
    }
}
