// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tap::tap::TapFallible;
use tracing::{error, info};

use crate::metrics::IndexerMetrics;
use crate::store::IndexerAnalyticalStore;
use crate::types::IndexerResult;

const ADDRESS_PROCESSOR_BATCH_SIZE: usize = 80000;
const PARALLELISM: usize = 10;

pub struct AddressMetricsProcessor<S> {
    pub store: S,
    metrics: IndexerMetrics,
    pub address_processor_batch_size: usize,
    pub address_processor_parallelism: usize,
}

impl<S> AddressMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S, metrics: IndexerMetrics) -> AddressMetricsProcessor<S> {
        let address_processor_batch_size = std::env::var("ADDRESS_PROCESSOR_BATCH_SIZE")
            .map(|s| s.parse::<usize>().unwrap_or(ADDRESS_PROCESSOR_BATCH_SIZE))
            .unwrap_or(ADDRESS_PROCESSOR_BATCH_SIZE);
        let address_processor_parallelism = std::env::var("ADDRESS_PROCESSOR_PARALLELISM")
            .map(|s| s.parse::<usize>().unwrap_or(PARALLELISM))
            .unwrap_or(PARALLELISM);
        Self {
            store,
            metrics,
            address_processor_batch_size,
            address_processor_parallelism,
        }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer address metrics async processor started...");
        let latest_tx_seq = self
            .store
            .get_address_metrics_last_processed_tx_seq()
            .await?;
        let mut last_processed_tx_seq = latest_tx_seq.unwrap_or_default().seq;
        loop {
            let mut latest_tx = self.store.get_latest_stored_transaction().await?;
            while if let Some(tx) = latest_tx {
                tx.tx_sequence_number
                    < last_processed_tx_seq + self.address_processor_batch_size as i64
            } else {
                true
            } {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_tx = self.store.get_latest_stored_transaction().await?;
            }

            let mut persist_tasks = vec![];
            let batch_size = self.address_processor_batch_size;
            let step_size = batch_size / self.address_processor_parallelism;
            for chunk_start_tx_seq in (last_processed_tx_seq + 1
                ..last_processed_tx_seq + batch_size as i64 + 1)
                .step_by(step_size)
            {
                let active_address_store = self.store.clone();
                persist_tasks.push(tokio::task::spawn_blocking(move || {
                    active_address_store.persist_active_addresses_in_tx_range(
                        chunk_start_tx_seq,
                        chunk_start_tx_seq + step_size as i64,
                    )
                }));
            }
            for chunk_start_tx_seq in (last_processed_tx_seq + 1
                ..last_processed_tx_seq + batch_size as i64 + 1)
                .step_by(step_size)
            {
                let address_store = self.store.clone();
                persist_tasks.push(tokio::task::spawn_blocking(move || {
                    address_store.persist_addresses_in_tx_range(
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
                    error!("Error joining address persist tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error persisting addresses or active addresses: {:?}", e);
                })?;
            last_processed_tx_seq += self.address_processor_batch_size as i64;
            info!(
                "Persisted addresses and active addresses for tx seq: {}",
                last_processed_tx_seq,
            );
            self.metrics
                .latest_address_metrics_tx_seq
                .set(last_processed_tx_seq);

            let mut last_processed_tx = self.store.get_tx(last_processed_tx_seq).await?;
            while last_processed_tx.is_none() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                last_processed_tx = self.store.get_tx(last_processed_tx_seq).await?;
            }
            // unwrap is safe here b/c we just checked that it's not None
            let last_processed_cp = last_processed_tx.unwrap().checkpoint_sequence_number;
            self.store
                .calculate_and_persist_address_metrics(last_processed_cp)
                .await?;
            info!(
                "Persisted address metrics for checkpoint: {}",
                last_processed_cp
            );
        }
    }
}
