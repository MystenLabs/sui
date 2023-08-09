// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{info, warn};

use sui_json_rpc_types::{
    CheckpointId, SuiTransactionBlockEvents, SuiTransactionBlockResponseOptions,
};

use crate::errors::IndexerError;
use crate::models::addresses::{dedup_from_addresses, dedup_from_and_to_addresses};
use crate::store::IndexerStore;
use crate::types::{AddressData, CheckpointTransactionBlockResponse};

const ADDRESS_STATS_BATCH_SIZE: usize = 100;
const DB_COMMIT_RETRY_INTERVAL_IN_MILLIS: u64 = 100;
const PG_TX_READ_CHUNK_SIZE: usize = 1000;

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
                let tx_vec: Vec<sui_types::digests::TransactionDigest> = checkpoints
                    .into_iter()
                    .flat_map(|c| c.transactions)
                    .collect();

                for tx_chunk in tx_vec.chunks(PG_TX_READ_CHUNK_SIZE) {
                    let tx_digest_strs = tx_chunk
                        .iter()
                        .map(|tx| tx.to_string())
                        .collect::<Vec<String>>();
                    let txs = self
                        .store
                        .multi_get_transactions_by_digests(&tx_digest_strs)
                        .await?;
                    let tx_option = SuiTransactionBlockResponseOptions {
                        show_input: true,
                        show_raw_input: true,
                        show_effects: true,
                        ..Default::default()
                    };
                    let sui_tx_resp_handles = txs.into_iter().map(|tx| {
                        self.store
                            .compose_sui_transaction_block_response(tx, Some(&tx_option))
                    });
                    let sui_tx_resp_vec = futures::future::join_all(sui_tx_resp_handles)
                        .await
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        // NOTE: need to set events to empty to avoid type hardening error
                        .map(|mut resp| {
                            resp.events = Some(SuiTransactionBlockEvents { data: vec![] });
                            resp
                        })
                        .collect::<Vec<_>>();

                    // retrieve address and active address data from txs
                    let checkpoint_tx_resp_vec = sui_tx_resp_vec
                        .into_iter()
                        .map(CheckpointTransactionBlockResponse::try_from)
                        .collect::<Result<Vec<_>, _>>()?;
                    let from_and_to_address_data: Vec<AddressData> = checkpoint_tx_resp_vec
                        .iter()
                        .flat_map(|tx| tx.get_from_and_to_addresses())
                        .collect();
                    let from_address_data: Vec<AddressData> = checkpoint_tx_resp_vec
                        .iter()
                        .map(|tx| tx.get_from_address())
                        .collect();
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
