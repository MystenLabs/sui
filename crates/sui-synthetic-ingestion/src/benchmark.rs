// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::synthetic_ingestion::SyntheticIngestion;
use crate::tps_tracker::TpsTracker;
use crate::{IndexerProgress, SyntheticIngestionConfig};
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;

/// A trait that can be implemented on top of any indexer to benchmark its throughput.
/// It will generate synthetic transactions and checkpoints as ingestion source.
#[async_trait::async_trait]
pub trait BenchmarkableIndexer {
    /// Allows the benchmark to subscribe and monitor the committed checkpoints progress.
    /// This is needed both in order to log periodic throughput, but also
    /// to know when the benchmark can stop.
    fn subscribe_to_committed_checkpoints(&self) -> watch::Receiver<Option<IndexerProgress>>;
    /// Start the indexer. Note that we only start a timer before calling this function.
    /// So the implementation should only start the indexer when this function is called.
    async fn start(&mut self);
    /// Stop the indexer. This would allow the benchmark to exit.
    async fn stop(mut self);
}

pub async fn run_benchmark<I: BenchmarkableIndexer>(
    config: SyntheticIngestionConfig,
    mut indexer: I,
) -> u64 {
    let ingestion = SyntheticIngestion::new(config.clone());
    let starting_checkpoint = ingestion.get_last_checkpoint();
    let expected_last_checkpoint = starting_checkpoint.sequence_number + config.num_checkpoints;
    ingestion.run();

    let mut rx = indexer.subscribe_to_committed_checkpoints();
    let mut tps_tracker = TpsTracker::new(Duration::from_secs(1));
    info!("Starting benchmark...");
    indexer.start().await;

    let last_checkpoint = loop {
        if let Err(err) = rx.changed().await {
            info!("Error polling from watch channel: {:?}", err);
        }
        let committed_checkpoint = rx.borrow_and_update().clone();
        if let Some(checkpoint) = committed_checkpoint {
            tps_tracker.update(checkpoint.clone());
            let seq = checkpoint.checkpoint;
            if seq >= expected_last_checkpoint {
                tps_tracker.finish(checkpoint);
                break seq;
            }
        }
    };
    indexer.stop().await;
    last_checkpoint
}

#[cfg(test)]
mod test {
    use crate::benchmark::{run_benchmark, BenchmarkableIndexer};
    use crate::{IndexerProgress, SyntheticIngestionConfig};
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::sync::watch;

    struct MockIndexer {
        ingestion_dir: PathBuf,
        committed_checkpoint_tx: Option<watch::Sender<Option<IndexerProgress>>>,
        committed_checkpoint_rx: watch::Receiver<Option<IndexerProgress>>,
    }

    impl MockIndexer {
        fn new(ingestion_dir: PathBuf) -> Self {
            let (committed_checkpoint_tx, committed_checkpoint_rx) = watch::channel(None);
            Self {
                ingestion_dir,
                committed_checkpoint_tx: Some(committed_checkpoint_tx),
                committed_checkpoint_rx,
            }
        }
    }

    #[async_trait::async_trait]
    impl BenchmarkableIndexer for MockIndexer {
        fn subscribe_to_committed_checkpoints(&self) -> watch::Receiver<Option<IndexerProgress>> {
            self.committed_checkpoint_rx.clone()
        }

        async fn start(&mut self) {
            let tx = self.committed_checkpoint_tx.take().unwrap();
            let mut checkpoint = 0;
            let dir = self.ingestion_dir.clone();
            tokio::task::spawn(async move {
                loop {
                    let path = dir.join(format!("{}.chk", checkpoint));
                    if std::fs::metadata(&path).is_err() {
                        break;
                    }
                    tx.send(Some(IndexerProgress {
                        checkpoint,
                        network_total_transactions: 0,
                    }))
                    .unwrap();
                    checkpoint += 1;
                }
            });
        }

        async fn stop(mut self) {}
    }

    #[tokio::test]
    async fn test_run_ingestion_benchmark() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let config = SyntheticIngestionConfig {
            ingestion_dir: tmp_dir.path().to_path_buf(),
            checkpoint_size: 10,
            num_checkpoints: 10,
        };
        let indexer = MockIndexer::new(tmp_dir.path().to_path_buf());
        let last_checkpoint =
            tokio::time::timeout(Duration::from_secs(10), run_benchmark(config, indexer))
                .await
                .unwrap();
        assert_eq!(last_checkpoint, 10);
    }
}
