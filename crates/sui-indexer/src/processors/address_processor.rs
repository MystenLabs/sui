// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::info;

use crate::errors::IndexerError;
use crate::store::IndexerStore;

const ADDRESS_STATS_BATCH_SIZE: i64 = 100;

pub struct AddressProcessor<S> {
    pub store: S,
}

impl<S> AddressProcessor<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> AddressProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer address async processor started...");
        let mut last_processed_addr_checkpoint =
            self.store.get_last_address_processed_checkpoint().await?;
        // process another batch of events, 100 checkpoints at a time,
        // otherwise sleep for 3 seconds
        loop {
            let latest_checkpoint = self.store.get_latest_checkpoint_sequence_number().await?;
            if latest_checkpoint >= last_processed_addr_checkpoint + ADDRESS_STATS_BATCH_SIZE {
                let addr_stats = self
                    .store
                    .calculate_address_stats(
                        last_processed_addr_checkpoint + ADDRESS_STATS_BATCH_SIZE,
                    )
                    .await?;
                self.store.persist_address_stats(&addr_stats).await?;
                last_processed_addr_checkpoint += ADDRESS_STATS_BATCH_SIZE;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        }
    }
}
