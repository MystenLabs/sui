// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{info, warn};

use sui_json_rpc_types::CheckpointId;

use crate::errors::IndexerError;
use crate::models::addresses::{dedup_from_addresses, dedup_from_and_to_addresses};
use crate::store::IndexerStore;
use crate::types::AddressData;

const ADDRESS_STATS_BATCH_SIZE: usize = 100;
const DB_COMMIT_RETRY_INTERVAL_IN_MILLIS: u64 = 100;

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
        // process another batch of events, 100 checkpoints at a time, otherwise sleep for 3 seconds
        loop {
            let latest_checkpoint = self
                .store
                .get_latest_tx_checkpoint_sequence_number()
                .await?;
            if latest_checkpoint >= last_processed_addr_checkpoint + ADDRESS_STATS_BATCH_SIZE as i64
            {
                let cursor = match last_processed_addr_checkpoint {
                    -1 => None,
                    _ => Some(CheckpointId::SequenceNumber(
                        last_processed_addr_checkpoint as u64,
                    )),
                };
                let checkpoints = self
                    .store
                    .get_checkpoints(cursor, ADDRESS_STATS_BATCH_SIZE)
                    .await?;

                for checkpoint in &checkpoints {
                    let timestamp = checkpoint.timestamp_ms;
                    let cp_sender_recipient_data = self
                        .store
                        .get_recipients_data_by_checkpoint(checkpoint.sequence_number)
                        .await?;

                    let sender_data = cp_sender_recipient_data
                        .iter()
                        .map(|d| (d.transaction_digest.clone(), d.sender.clone()))
                        .collect::<Vec<(String, String)>>();
                    let recipient_data = cp_sender_recipient_data
                        .iter()
                        .map(|d| (d.transaction_digest.clone(), d.recipient.clone()))
                        .collect::<Vec<(String, String)>>();

                    let from_address_data = sender_data
                        .iter()
                        .map(|(tx, sender)| AddressData {
                            account_address: sender.clone(),
                            transaction_digest: tx.clone(),
                            timestamp_ms: timestamp as i64,
                        })
                        .collect::<Vec<AddressData>>();
                    let from_and_to_address_data = sender_data
                        .iter()
                        .chain(recipient_data.iter())
                        .map(|(tx, sender)| AddressData {
                            account_address: sender.clone(),
                            transaction_digest: tx.clone(),
                            timestamp_ms: timestamp as i64,
                        })
                        .collect::<Vec<AddressData>>();

                    let addresses = dedup_from_and_to_addresses(from_and_to_address_data);
                    let active_addresses: Vec<crate::models::addresses::ActiveAddress> =
                        dedup_from_addresses(from_address_data);

                    // retry here to avoid duplicate DB reads
                    let mut address_commit_res = self
                        .store
                        .persist_addresses(&addresses, &active_addresses)
                        .await;
                    while let Err(e) = address_commit_res {
                        warn!(
                        "Indexer address commit failed with error: {:?}, retrying after {:?} milli-secs...",
                        e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                    );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                        ))
                        .await;
                        address_commit_res = self
                            .store
                            .persist_addresses(&addresses, &active_addresses)
                            .await;
                    }
                }

                let addr_stats = self
                    .store
                    .calculate_address_stats(
                        last_processed_addr_checkpoint + ADDRESS_STATS_BATCH_SIZE as i64,
                    )
                    .await?;
                self.store.persist_address_stats(&addr_stats).await?;
                info!(
                    "Processed addresses and address stats for checkpoint: {}",
                    last_processed_addr_checkpoint
                );
                last_processed_addr_checkpoint += ADDRESS_STATS_BATCH_SIZE as i64;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        }
    }
}
