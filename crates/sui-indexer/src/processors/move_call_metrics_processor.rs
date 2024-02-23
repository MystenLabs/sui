// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tap::tap::TapFallible;
use tracing::{error, info};

use crate::metrics::IndexerMetrics;
use crate::store::IndexerAnalyticalStore;
use crate::types::IndexerResult;

const MOVE_CALL_PROCESSOR_BATCH_SIZE: usize = 80000;
const PARALLELISM: usize = 10;

pub struct MoveCallMetricsProcessor<S> {
    pub store: S,
    metrics: IndexerMetrics,
    pub move_call_processor_batch_size: usize,
    pub move_call_processor_parallelism: usize,
}

impl<S> MoveCallMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> MoveCallMetricsProcessor<S> {
        let move_call_processor_batch_size = std::env::var("MOVE_CALL_PROCESSOR_BATCH_SIZE")
            .map(|s| s.parse::<usize>().unwrap_or(MOVE_CALL_PROCESSOR_BATCH_SIZE))
            .unwrap_or(MOVE_CALL_PROCESSOR_BATCH_SIZE);
        let move_call_processor_parallelism = std::env::var("MOVE_CALL_PROCESSOR_PARALLELISM")
            .map(|s| s.parse::<usize>().unwrap_or(PARALLELISM))
            .unwrap_or(PARALLELISM);
        Self {
            store,
            metrics,
            move_call_processor_batch_size,
            move_call_processor_parallelism,
        }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer move call metrics async processor started...");
        let latest_move_call_tx_seq = self.store.get_latest_move_call_tx_seq().await?;
        let mut last_processed_tx_seq = latest_move_call_tx_seq.unwrap_or_default().seq;
        let latest_move_call_epoch = self.store.get_latest_move_call_metrics().await?;
        let mut last_processed_epoch = latest_move_call_epoch.unwrap_or_default().epoch;
        loop {
            let mut latest_tx = self.store.get_latest_stored_transaction().await?;
            while if let Some(tx) = latest_tx {
                tx.tx_sequence_number
                    < last_processed_tx_seq + self.move_call_processor_batch_size as i64
            } else {
                true
            } {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_tx = self.store.get_latest_stored_transaction().await?;
            }

            let batch_size = self.move_call_processor_batch_size;
            let step_size = batch_size / self.move_call_processor_parallelism;
            let mut persist_tasks = vec![];
            for chunk_start_tx_seq in (last_processed_tx_seq + 1
                ..last_processed_tx_seq + batch_size as i64 + 1)
                .step_by(step_size)
            {
                let move_call_store = self.store.clone();
                persist_tasks.push(tokio::task::spawn_blocking(move || {
                    move_call_store.persist_move_calls_in_tx_range(
                        chunk_start_tx_seq,
                        chunk_start_tx_seq + step_size as i64,
                    )
                }));
            }
            futures::future::join_all(persist_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining move call persist tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error persisting move calls: {:?}", e);
                })?;
            last_processed_tx_seq += batch_size as i64;
            info!("Persisted move_calls at tx seq: {}", last_processed_tx_seq);
            self.metrics
                .latest_move_call_metrics_tx_seq
                .set(last_processed_tx_seq);

            let mut tx = self.store.get_tx(last_processed_tx_seq).await?;
            while tx.is_none() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                tx = self.store.get_tx(last_processed_tx_seq).await?;
            }
            let cp_seq = tx.unwrap().checkpoint_sequence_number;
            let mut cp = self.store.get_cp(cp_seq).await?;
            while cp.is_none() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                cp = self.store.get_cp(cp_seq).await?;
            }
            let end_epoch = cp.unwrap().epoch;
            for epoch in last_processed_epoch + 1..end_epoch {
                self.store
                    .calculate_and_persist_move_call_metrics(epoch)
                    .await?;
                info!("Persisted move_call_metrics for epoch: {}", epoch);
            }
            last_processed_epoch = end_epoch - 1;
        }
    }
}
