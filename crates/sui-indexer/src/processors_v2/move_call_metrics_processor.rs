// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use rayon::prelude::*;
use std::collections::HashMap;
use tap::tap::TapFallible;
use tracing::{error, info};

use crate::errors::IndexerError;
use crate::models_v2::move_call_metrics::StoredMoveCall;
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

const MOVE_CALL_PROCESSOR_BATCH_SIZE: i64 = 1000;
const PARALLEL_DOWNLOAD_CHUNK_SIZE: i64 = 100;
const PARALLEL_COMMIT_CHUNK_SIZE: usize = 8000;

pub struct MoveCallMetricsProcessor<S> {
    pub store: S,
    pub parallel_commit_chunk_size: usize,
}

impl<S> MoveCallMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S) -> MoveCallMetricsProcessor<S> {
        let parallel_commit_chunk_size = std::env::var("MOVE_CALL_CHUNK_SIZE")
            .map(|s| s.parse::<usize>().unwrap_or(PARALLEL_COMMIT_CHUNK_SIZE))
            .unwrap_or(PARALLEL_COMMIT_CHUNK_SIZE);
        Self {
            store,
            parallel_commit_chunk_size,
        }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer move call metrics async processor started...");
        let latest_move_call_metrics = self
            .store
            .get_latest_move_call_metrics()
            .await
            .unwrap_or_default();
        let mut last_end_cp_seq = latest_move_call_metrics.checkpoint_sequence_number;
        let mut last_move_call_epoch = latest_move_call_metrics.epoch;
        loop {
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < last_end_cp_seq + MOVE_CALL_PROCESSOR_BATCH_SIZE
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }

            let mut parallel_download_tasks = vec![];
            for chunk_start_cp_seq in ((last_end_cp_seq + 1)
                ..last_end_cp_seq + MOVE_CALL_PROCESSOR_BATCH_SIZE + 1)
                .step_by(PARALLEL_DOWNLOAD_CHUNK_SIZE as usize)
            {
                let chunk_end_cp_seq = chunk_start_cp_seq + PARALLEL_DOWNLOAD_CHUNK_SIZE;
                let store = self.store.clone();
                parallel_download_tasks.push(tokio::task::spawn(async move {
                    store
                        .get_tx_checkpoints_in_checkpoint_range(
                            chunk_start_cp_seq,
                            chunk_end_cp_seq,
                        )
                        .await
                }));
            }

            let tx_checkpoints = futures::future::join_all(parallel_download_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error joining tx checkpoints download tasks: {:?}", e);
                })?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .tap_err(|e| {
                    error!("Error downloading tx checkpoints: {:?}", e);
                })?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let end_cp_seq = last_end_cp_seq + MOVE_CALL_PROCESSOR_BATCH_SIZE;
            let cps = self
                .store
                .get_checkpoints_in_range(last_end_cp_seq + 1, end_cp_seq + 1)
                .await?;
            info!(
                "Downloaded checkpoints and transactions from checkpoint {} to checkpoint {}",
                last_end_cp_seq + 1,
                end_cp_seq
            );

            let end_cp = cps
                .last()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read checkpoint from PG for move call metrics".to_string(),
                ))?
                .clone();
            let cp_epoch_map = cps
                .par_iter()
                .map(|cp| (cp.sequence_number, cp.epoch))
                .collect::<HashMap<_, _>>();
            let tx_cp_map = tx_checkpoints
                .par_iter()
                .map(|tx| (tx.tx_sequence_number, tx.checkpoint_sequence_number))
                .collect::<HashMap<_, _>>();
            let start_tx_seq = tx_checkpoints
                .first()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read first tx from PG for move call metrics".to_string(),
                ))?
                .tx_sequence_number;
            let end_tx_seq = tx_checkpoints
                .last()
                .ok_or(IndexerError::PostgresReadError(
                    "Cannot read last tx from PG for move call metrics".to_string(),
                ))?
                .tx_sequence_number;
            let stored_move_calls = self
                .store
                .get_move_calls_in_tx_range(start_tx_seq, end_tx_seq + 1)
                .await?;
            let stored_move_calls_count = stored_move_calls.len();
            let move_calls_to_commit = stored_move_calls
                .into_par_iter()
                .filter_map(|call| {
                    if let Some(cp) = tx_cp_map.get(&call.tx_sequence_number) {
                        if let Some(epoch) = cp_epoch_map.get(cp) {
                            Some(StoredMoveCall {
                                id: None,
                                transaction_sequence_number: call.tx_sequence_number,
                                checkpoint_sequence_number: *cp,
                                epoch: *epoch,
                                move_package: call.package,
                                move_module: call.module,
                                move_function: call.func,
                            })
                        } else {
                            error!("Failed to find epoch for checkpoint: {}", cp);
                            None
                        }
                    } else {
                        error!(
                            "Failed to find checkpoint for tx: {}",
                            call.tx_sequence_number
                        );
                        None
                    }
                })
                .collect::<Vec<StoredMoveCall>>();
            if stored_move_calls_count != move_calls_to_commit.len() {
                error!(
                    "Error enriching data of move calls to commit: {} != {}",
                    stored_move_calls_count,
                    move_calls_to_commit.len()
                );
                continue;
            }

            let end_cp_seq = end_cp.sequence_number;
            let end_cp_epoch = end_cp.epoch;
            let move_call_count = move_calls_to_commit.len();
            info!(
                "Indexed {} move_calls at checkpoint: {}",
                move_call_count, end_cp_seq
            );

            let move_call_chunk_to_commit = move_calls_to_commit
                .into_iter()
                .chunks(self.parallel_commit_chunk_size)
                .into_iter()
                .map(|chunk| chunk.collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let mut parallel_commit_tasks = vec![];
            for move_call_chunk in move_call_chunk_to_commit {
                let store = self.store.clone();
                parallel_commit_tasks.push(tokio::task::spawn_blocking(move || {
                    store.persist_move_calls(move_call_chunk)
                }));
            }
            futures::future::join_all(parallel_commit_tasks)
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
            info!(
                "Persisted {} move_calls at checkpoint: {}",
                move_call_count, end_cp_seq
            );
            if end_cp_epoch > last_move_call_epoch {
                let move_call_metrics = self.store.calculate_move_call_metrics(end_cp).await?;
                self.store
                    .persist_move_call_metrics(move_call_metrics)
                    .await?;
                info!("Persisted move_call_metrics at epoch: {}", end_cp_epoch);
                last_move_call_epoch = end_cp_epoch;
            }
            last_end_cp_seq = end_cp_seq;
        }
    }
}
