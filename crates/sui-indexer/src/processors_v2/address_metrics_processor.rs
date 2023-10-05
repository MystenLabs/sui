// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use tracing::{error, info, warn};

use crate::models::addresses;
use crate::models_v2::address_metrics::{
    dedup_addresses, AddressInfoToCommit, DerivedAddressInfo, StoredActiveAddress, StoredAddress,
    StoredAddressMetrics,
};
use crate::models_v2::tx_indices;
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

pub struct AddressMetricsProcessor<S> {
    pub store: S,
}

impl<S> AddressMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> NetworkMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer address metrics async processor started...");
        loop {
            // TODOggao: unwrap or default for the first time
            let latest_address_metrics = self.store.get_latest_address_metrics().await?;
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < latest_address_metrics.checkpoint + 1000
            {
                std::sleep::sleep(std::time::Duration::from_secs(1));
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }

            // +1 b/c get_tx_indices_in_checkpoint_range is left-inclusive, right-exclusive,
            // but we want left-exclusive, right-inclusive, as latest_address_metrics has been processed.
            let cps = self
                .store
                .get_checkpoints_in_range(
                    latest_address_metrics.checkpoint + 1,
                    latest_stored_checkpoint.sequence_number + 1,
                )
                .await?;
            let cp_timestamp_map = cps
                .iter()
                .map(|cp| (cp.sequence_number, cp.timestamp_ms))
                .collect::<HashMap<_, _>>();
            let tx_indices_batch = self
                .store
                .get_tx_indices_in_checkpoint_range(
                    latest_address_metrics.checkpoint + 1,
                    latest_stored_checkpoint.sequence_number + 1,
                )
                .await?;

            let senders: Vec<DerivedAddressInfo> = tx_indices_batch
                .iter()
                .map(|tx_index| tx_index.get_senders_address_info())
                .collect::<Result<Vec<SuiAddress>, _>>()
                .map(|vecs| vecs.concat())?;
            let recipients: Vec<DerivedAddressInfo> = tx_indices_batch
                .iter()
                .map(|tx_index| tx_index.get_recipients_address_info())
                .collect::<Result<Vec<SuiAddress>, _>>()
                .map(|vecs| vecs.concat())?;
            // replace cp seq with its timestamp
            let senders_to_commit: Vec<AddressInfoToCommit> = senders
                .into_iter()
                .filter_map(|sender| {
                    if let Some(timestamp_ms) = cp_timestamp_map.get(&sender.checkpoint) {
                        Some(AddressInfoToCommit {
                            address: sender.address,
                            tx: sender.tx,
                            timestamp_ms: *timestamp_ms,
                        })
                    } else {
                        error!(
                            "Failed to find timestamp for checkpoint {}",
                            sender.checkpoint
                        );
                        None
                    }
                })
                .collect();
            let recipients_to_commit: Vec<AddressInfoToCommit> = recipients
                .into_iter()
                .filter_map(|recipient| {
                    if let Some(timestamp_ms) = cp_timestamp_map.get(&recipient.checkpoint) {
                        Some(AddressInfoToCommit {
                            address: recipient.address,
                            tx: recipient.tx,
                            timestamp_ms: *timestamp_ms,
                        })
                    } else {
                        error!(
                            "Failed to find timestamp for checkpoint {}",
                            recipient.checkpoint
                        );
                        None
                    }
                })
                .collect();
            let sneders_recipients_to_commit: Vec<AddressInfoToCommit> = senders_to_commit
                .into_iter()
                .chain(recipients_to_commit.into_iter())
                .collect::<Vec<AddressInfoToCommit>>();
            // de-dup senders with earliest and latest timestamps
            let active_addresses_to_commit: Vec<StoredActiveAddress> =
                dedup_addresses(&senders_to_commit)
                    .into_iter()
                    .map(StoredActiveAddress::from)
                    .collect();
            let addresses_to_commit: Vec<StoredAddress> =
                dedup_addresses(&sneders_recipients_to_commit);

            // TODOggao: need to update addresses in the table
            self.store.persist_addresses(addresses_to_commit).await?;
            self.store
                .persist_active_addresses(active_addresses_to_commit)
                .await?;
            info!(
                "Persisted addresses and active addresses for checkpoint: {}",
                latest_stored_checkpoint.sequence_number,
            );

            let address_metrics_to_commit =
                self.store.calculate_address_metrics(&checkpoint).await?;
            self.store
                .persist_address_metrics(address_metrics_to_commit)
                .await?;
            info!(
                "Persisted address metrics for checkpoint: {}",
                latest_stored_checkpoint.sequence_number,
            );
        }
        Ok(())
    }
}
