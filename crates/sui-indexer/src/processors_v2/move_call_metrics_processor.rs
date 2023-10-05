// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// similar to AddressMetricsProcessor in address_metrics_processor.rs

use tracing::{info, warn};

use crate::models_v2::move_call_metrics::StoredMoveCall;
use crate::store::IndexerAnalyticalStore;
use crate::types_v2::IndexerResult;

pub struct MoveCallMetricsProcessor<S> {
    pub store: S,
}

impl<S> MoveCallMetricsProcessor<S>
where
    S: IndexerAnalyticalStore + Sync + Send + 'static,
{
    pub fn new(store: S) -> MoveCallMetricsProcessor<S> {
        Self { store }
    }

    pub async fn start(&self) -> IndexerResult<()> {
        info!("Indexer move call metrics async processor started...");
        loop {
            let latest_move_call_metrics = self.store.get_latest_move_call_metrics().await?;
            let mut latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            while latest_stored_checkpoint.sequence_number
                < latest_move_call_metrics.checkpoint + 1000
            {
                std::sleep::sleep(std::time::Duration::from_secs(1));
                latest_stored_checkpoint = self.store.get_latest_stored_checkpoint().await?;
            }
            // +1 here b/c get_transactions_in_checkpoint_range is left-inclusive, right-exclusive,
            // but we want left-exclusive, right-inclusive, as latest_tx_count_metrics has been processed.
            let cps = self
                .store
                .get_checkpoints_in_range(
                    latest_address_metrics.checkpoint + 1,
                    latest_stored_checkpoint.sequence_number + 1,
                )
                .await?;
            let cp_epoch_map = cps
                .iter()
                .map(|cp| (cp.sequence_number, cp.epoch))
                .collect::<HashMap<_, _>>();

            let tx_indices_batch = self
                .store
                .get_tx_indices_in_checkpoint_range(
                    latest_move_call_metrics.checkpoint + 1,
                    latest_stored_checkpoint.sequence_number + 1,
                )
                .await?;

            let move_calls = tx_indices_batch
                .iter()
                .map(|tx_index| tx_index.get_move_calls())
                .flatten()
                .collect::<Vec<_>>();

            let move_calls_to_commit = move_calls
                .into_iter()
                .filter_map(|derived_move_call_info| {
                    if let Some(epoch) = cp_epoch_map.get(&derived_move_call_info.checkpoint) {
                        Some(StoredMoveCall {
                            id: None,
                            transaction_sequence_number: derived_move_call_info.tx_sequence_number,
                            checkpoint_sequence_number: derived_move_call_info.checkpoint,
                            epoch: *epoch,
                            move_package: derived_move_call_info.move_package,
                            move_module: derived_move_call_info.move_module,
                            move_function: derived_move_call_info.move_function,
                        })
                    } else {
                        error!("checkpoint {} not found in cp_epoch_map", dmv.checkpoint);
                        None
                    }
                })
                .collect::<Vec<_>>();
            self.store.persist_move_calls(move_calls);
            info!(
                "Persisted move_calls for checkpoint: {}",
                latest_stored_checkpoint.sequence_number
            );

            let move_call_meetrics = self
                .store
                .calculate_move_call_metrics(latest_stored_checkpoint)
                .await?;
            self.store
                .persist_move_call_metrics(move_call_metrics)
                .await?;
            info!(
                "Persisted move_call_metrics for checkpoint: {}",
                latest_stored_checkpoint.sequence_number
            );
        }
    }
}
