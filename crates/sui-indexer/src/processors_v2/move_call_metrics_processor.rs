// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

pub struct MoveCallMetricsProcessor<S> {
    pub store: S,
}

impl<S> MoveCallMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Clone + Sync + Send + 'static,
{
    pub fn new(store: S) -> MoveCallMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer move call metrics async processor started...");

        let latest_move_call_metrics = self
            .store
            .get_latest_move_call_metrics()
            .await
            .unwrap_or_default();
        let mut last_end_cp_seq = latest_move_call_metrics.checkpoint_sequence_number;
        loop {
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < last_end_cp_seq + MOVE_CALL_PROCESSOR_BATCH_SIZE
            {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }
            // +1 as latest_move_call_metrics has been processed in last batch
            let end_cp_seq = last_end_cp_seq + MOVE_CALL_PROCESSOR_BATCH_SIZE;
            let mut chunk_start_cp_seq = last_end_cp_seq + 1;
            let mut chunk_end_cp_seq = last_end_cp_seq + PARALLEL_DOWNLOAD_CHUNK_SIZE;
            let mut parallel_download_tasks = vec![];
            while chunk_end_cp_seq < end_cp_seq {
                let store = self.store.clone();
                parallel_download_tasks.push(tokio::task::spawn(async move {
                    store
                        .get_tx_checkpoints_in_checkpoint_range(
                            chunk_start_cp_seq,
                            chunk_end_cp_seq + 1,
                        )
                        .await
                }));
                chunk_start_cp_seq += PARALLEL_DOWNLOAD_CHUNK_SIZE;
                chunk_end_cp_seq += PARALLEL_DOWNLOAD_CHUNK_SIZE;
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
            let move_calls_to_commit = stored_move_calls
                .into_par_iter()
                .filter_map(|call| {
                    let cp = tx_cp_map.get(&call.tx_sequence_number)?;
                    let epoch = cp_epoch_map.get(cp)?;
                    Some(StoredMoveCall {
                        id: None,
                        transaction_sequence_number: call.tx_sequence_number,
                        checkpoint_sequence_number: *cp,
                        epoch: *epoch,
                        move_package: call.package,
                        move_module: call.module,
                        move_function: call.func,
                    })
                })
                .collect::<Vec<StoredMoveCall>>();
            let end_cp_seq = end_cp.sequence_number;
            let move_call_count = move_calls_to_commit.len();
            info!(
                "Indexed {} move_calls at checkpoint: {}",
                move_call_count, end_cp_seq
            );

            let persist_move_call_handle = self.store.persist_move_calls(move_calls_to_commit);
            let calculate_move_call_metrics_handle = self.store.calculate_move_call_metrics(end_cp);
            let (persist_move_call_res, calculate_move_call_metrics_res) =
                tokio::join!(persist_move_call_handle, calculate_move_call_metrics_handle);
            persist_move_call_res?;
            let move_call_metrics = calculate_move_call_metrics_res?;
            self.store
                .persist_move_call_metrics(move_call_metrics)
                .await?;
            last_end_cp_seq = end_cp_seq;
            info!(
                "Persisted {} move_calls and move_call_metrics at checkpoint: {}",
                move_call_count, end_cp_seq
            );
        }
    }
}
