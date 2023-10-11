// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use tracing::{error, info};

use crate::errors::IndexerError;
use crate::models_v2::address_metrics::{
    dedup_addresses, AddressInfoToCommit, StoredActiveAddress, StoredAddress,
};
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const ADDRESS_PROCESSOR_BATCH_SIZE: i64 = 1000;

pub struct AddressMetricsProcessor<S> {
    pub store: S,
}

impl<S> AddressMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> AddressMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer address metrics async processor started...");
        let latest_address_metrics = self
            .store
            .get_latest_address_metrics()
            .await
            .unwrap_or_default();
        let mut last_end_cp_seq = latest_address_metrics.checkpoint;
        loop {
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < last_end_cp_seq + ADDRESS_PROCESSOR_BATCH_SIZE
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }

            let end_cp = self
                .store
                .get_checkpoints_in_range(
                    last_end_cp_seq + ADDRESS_PROCESSOR_BATCH_SIZE,
                    last_end_cp_seq + ADDRESS_PROCESSOR_BATCH_SIZE + 1,
                )
                .await?
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read checkpoint from PG for address metrics".to_string(),
                ))?
                .clone();

            // +1 b/c get_tx_indices_in_checkpoint_range is left-inclusive, right-exclusive,
            // but we want left-exclusive, right-inclusive, as latest_address_metrics has been processed.
            let txs = self
                .store
                .get_transactions_in_checkpoint_range(
                    latest_address_metrics.checkpoint + 1,
                    end_cp.sequence_number + 1,
                )
                .await?;
            let start_tx_seq = txs
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read first tx from PG for address metrics".to_string(),
                ))?
                .tx_sequence_number;
            let end_tx_seq = txs
                .last()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read last tx from PG for address metrics".to_string(),
                ))?
                .tx_sequence_number;

            let tx_timestamp_map = txs
                .iter()
                .map(|tx| (tx.tx_sequence_number, tx.timestamp_ms))
                .collect::<HashMap<_, _>>();

            let stored_senders = self
                .store
                .get_senders_in_tx_range(start_tx_seq, end_tx_seq + 1)
                .await?;
            let stored_recipients = self
                .store
                .get_recipients_in_tx_range(start_tx_seq, end_tx_seq + 1)
                .await?;
            // replace cp seq with its timestamp
            let senders_to_commit: Vec<AddressInfoToCommit> = stored_senders
                .into_iter()
                .filter_map(|sender| {
                    if let Some(timestamp_ms) = tx_timestamp_map.get(&sender.tx_sequence_number) {
                        Some(AddressInfoToCommit {
                            address: sender.sender,
                            tx_seq: sender.tx_sequence_number,
                            timestamp_ms: *timestamp_ms,
                        })
                    } else {
                        error!(
                            "Failed to find timestamp for tx {}",
                            sender.tx_sequence_number
                        );
                        None
                    }
                })
                .collect();
            let recipients_to_commit: Vec<AddressInfoToCommit> = stored_recipients
                .into_iter()
                .filter_map(|recipient| {
                    if let Some(timestamp_ms) = tx_timestamp_map.get(&recipient.tx_sequence_number)
                    {
                        Some(AddressInfoToCommit {
                            address: recipient.recipient,
                            tx_seq: recipient.tx_sequence_number,
                            timestamp_ms: *timestamp_ms,
                        })
                    } else {
                        error!(
                            "Failed to find timestamp for tx {}",
                            recipient.tx_sequence_number
                        );
                        None
                    }
                })
                .collect();
            let sneders_recipients_to_commit: Vec<AddressInfoToCommit> = senders_to_commit
                .clone()
                .into_iter()
                .chain(recipients_to_commit.into_iter())
                .collect::<Vec<AddressInfoToCommit>>();

            // de-dup senders with earliest and latest timestamps
            let active_addresses_to_commit: Vec<StoredActiveAddress> =
                dedup_addresses(senders_to_commit)
                    .into_iter()
                    .map(StoredActiveAddress::from)
                    .collect();
            let addresses_to_commit: Vec<StoredAddress> =
                dedup_addresses(sneders_recipients_to_commit);

            let end_cp_seq = end_cp.sequence_number;
            self.store.persist_addresses(addresses_to_commit).await?;
            self.store
                .persist_active_addresses(active_addresses_to_commit)
                .await?;
            info!(
                "Persisted addresses and active addresses for checkpoint: {}",
                end_cp_seq,
            );

            let address_metrics_to_commit = self.store.calculate_address_metrics(end_cp).await?;
            self.store
                .persist_address_metrics(address_metrics_to_commit)
                .await?;
            last_end_cp_seq = end_cp_seq;
            info!("Persisted address metrics for checkpoint: {}", end_cp_seq);
        }
    }
}
