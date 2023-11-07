// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use rayon::prelude::*;
use std::collections::HashMap;
use tap::tap::TapFallible;
use tracing::{error, info};

use crate::errors::IndexerError;
use crate::models_v2::address_metrics::{
    dedup_addresses, AddressInfoToCommit, StoredActiveAddress, StoredAddress,
};
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const ADDRESS_PROCESSOR_BATCH_SIZE: i64 = 1000;
const PARALLEL_DOWNLOAD_CHUNK_SIZE: i64 = 100;
const PARALLEL_COMMIT_CHUNK_SIZE: usize = 10000;

pub struct AddressMetricsProcessor<S> {
    pub store: S,
    pub parallel_commit_chunk_size: usize,
}

impl<S> AddressMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S) -> AddressMetricsProcessor<S> {
        let parallel_commit_chunk_size = std::env::var("ADDRESS_CHUNK_SIZE")
            .map(|s| s.parse::<usize>().unwrap_or(PARALLEL_COMMIT_CHUNK_SIZE))
            .unwrap_or(PARALLEL_COMMIT_CHUNK_SIZE);
        Self {
            store,
            parallel_commit_chunk_size,
        }
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

            let mut parallel_download_tasks = vec![];
            for chunk_start_cp_seq in ((last_end_cp_seq + 1)
                ..last_end_cp_seq + ADDRESS_PROCESSOR_BATCH_SIZE + 1)
                .step_by(PARALLEL_DOWNLOAD_CHUNK_SIZE as usize)
            {
                let chunk_end_cp_seq = chunk_start_cp_seq + PARALLEL_DOWNLOAD_CHUNK_SIZE;
                let store = self.store.clone();
                parallel_download_tasks.push(tokio::task::spawn(async move {
                    store
                        .get_tx_timestamps_in_checkpoint_range(chunk_start_cp_seq, chunk_end_cp_seq)
                        .await
                }));
            }

            let tx_timestamps = futures::future::join_all(parallel_download_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining tx timestamps download tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error downloading tx timestamps: {:?}", e);
                })?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

            let start_tx_seq = tx_timestamps
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read first tx from PG for address metrics".to_string(),
                ))?
                .tx_sequence_number;
            let end_tx_seq = tx_timestamps
                .last()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read last tx from PG for address metrics".to_string(),
                ))?
                .tx_sequence_number;
            let tx_timestamp_map = tx_timestamps
                .iter()
                .map(|tx| (tx.tx_sequence_number, tx.timestamp_ms))
                .collect::<HashMap<_, _>>();

            let get_senders_handle = self
                .store
                .get_senders_in_tx_range(start_tx_seq, end_tx_seq + 1);
            let get_recipients_handle = self
                .store
                .get_recipients_in_tx_range(start_tx_seq, end_tx_seq + 1);
            let (stored_senders_res, stored_recipients_res) =
                tokio::join!(get_senders_handle, get_recipients_handle);
            let stored_senders = stored_senders_res?;
            let stored_senders_count = stored_senders.len();
            let senders_to_commit: Vec<AddressInfoToCommit> = stored_senders
                .into_par_iter()
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
            if stored_senders_count != senders_to_commit.len() {
                error!(
                    "Failed to find timestamp for {} senders in checkpoint {}",
                    stored_senders_count - senders_to_commit.len(),
                    end_cp.sequence_number
                );
                continue;
            }
            let stored_recipients = stored_recipients_res?;
            let stored_recipients_count = stored_recipients.len();
            let recipients_to_commit: Vec<AddressInfoToCommit> = stored_recipients
                .into_par_iter()
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
            if stored_recipients_count != recipients_to_commit.len() {
                error!(
                    "Failed to find timestamp for {} recipients in checkpoint {}",
                    stored_recipients_count - recipients_to_commit.len(),
                    end_cp.sequence_number
                );
                continue;
            }

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
            let addr_count = addresses_to_commit.len();
            let active_addr_count = active_addresses_to_commit.len();
            info!(
                "Indexed {} addresses and {} active addresses for checkpoint: {}",
                addr_count, active_addr_count, end_cp_seq,
            );

            let address_chunk_to_commit = addresses_to_commit
                .into_iter()
                .chunks(self.parallel_commit_chunk_size)
                .into_iter()
                .map(|chunk| chunk.collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let parallel_address_commit_tasks = address_chunk_to_commit
                .into_iter()
                .map(|chunk| {
                    let store = self.store.clone();
                    tokio::task::spawn_blocking(move || store.persist_addresses(chunk))
                })
                .collect::<Vec<_>>();

            let active_address_chunk_to_commit = active_addresses_to_commit
                .into_iter()
                .chunks(self.parallel_commit_chunk_size)
                .into_iter()
                .map(|chunk| chunk.collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let parallel_active_address_commit_tasks = active_address_chunk_to_commit
                .into_iter()
                .map(|chunk| {
                    let store = self.store.clone();
                    tokio::task::spawn_blocking(move || store.persist_active_addresses(chunk))
                })
                .collect::<Vec<_>>();

            futures::future::join_all(parallel_address_commit_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining address persist tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error persisting addresses: {:?}", e);
                })?;
            futures::future::join_all(parallel_active_address_commit_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining active address persist tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error persisting active addresses: {:?}", e);
                })?;
            info!(
                "Persisted {} addresses and {} active addresses for checkpoint: {}",
                addr_count, active_addr_count, end_cp_seq,
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
