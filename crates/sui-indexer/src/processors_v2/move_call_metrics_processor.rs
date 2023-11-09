// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::info;

use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const MOVE_CALL_PROCESSOR_BATCH_SIZE: i64 = 1000;

pub struct MoveCallMetricsProcessor<S> {
    pub store: S,
    pub move_call_processor_batch_size: i64,
}

impl<S> MoveCallMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S) -> MoveCallMetricsProcessor<S> {
        let move_call_processor_batch_size = std::env::var("MOVE_CALL_PROCESSOR_BATCH_SIZE")
            .map(|s| s.parse::<i64>().unwrap_or(MOVE_CALL_PROCESSOR_BATCH_SIZE))
            .unwrap_or(MOVE_CALL_PROCESSOR_BATCH_SIZE);
        Self {
            store,
            move_call_processor_batch_size,
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
                tx.tx_sequence_number < last_processed_tx_seq + self.move_call_processor_batch_size
            } else {
                true
            } {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_tx = self.store.get_latest_stored_transaction().await?;
            }

            self.store
                .persist_move_calls_in_tx_range(
                    last_processed_tx_seq + 1,
                    last_processed_tx_seq + self.move_call_processor_batch_size + 1,
                )
                .await?;
            last_processed_tx_seq += self.move_call_processor_batch_size;
            info!("Persisted move_calls at tx seq: {}", last_processed_tx_seq);

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
