// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;

use crate::models_v2::address_metrics::{StoredActiveAddress, StoredAddress, StoredAddressMetrics};
use crate::models_v2::checkpoints::StoredCheckpoint;
use crate::models_v2::move_call_metrics::{StoredMoveCall, StoredMoveCallMetrics};
use crate::models_v2::network_metrics::StoredNetworkMetrics;
use crate::models_v2::transactions::StoredTransaction;
use crate::models_v2::tx_count_metrics::StoredTxCountMetrics;
use crate::models_v2::tx_indices::{StoredTxCalls, StoredTxRecipients, StoredTxSenders};
use crate::types_v2::IndexerResult;

#[async_trait]
pub trait IndexerAnalyticalStore {
    async fn get_latest_stored_checkpoint(&self) -> IndexerResult<StoredCheckpoint>;
    async fn get_checkpoints_in_range(
        &self,
        start_checkpoint: i64,
        end_checkpoint: i64,
    ) -> IndexerResult<Vec<StoredCheckpoint>>;
    async fn get_transactions_in_checkpoint_range(
        &self,
        start_checkpoint: i64,
        end_checkpoint: i64,
    ) -> IndexerResult<Vec<StoredTransaction>>;
    async fn get_estimated_count(&self, table: &str) -> IndexerResult<i64>;

    // for network metrics including TPS and counts of objects etc.
    async fn get_latest_tx_count_metrics(&self) -> IndexerResult<StoredTxCountMetrics>;
    async fn get_peak_network_peak_tps(&self, epoch: i64, day: i64) -> IndexerResult<f64>;
    async fn persist_tx_count_metrics(
        &self,
        tx_count_metrics: StoredTxCountMetrics,
    ) -> IndexerResult<()>;
    async fn persist_network_metrics(
        &self,
        network_metrics: StoredNetworkMetrics,
    ) -> IndexerResult<()>;

    // for address metrics
    async fn get_latest_address_metrics(&self) -> IndexerResult<StoredAddressMetrics>;
    async fn persist_addresses(&self, addresses: Vec<StoredAddress>) -> IndexerResult<()>;
    async fn get_senders_in_tx_range(
        &self,
        start_tx_seq: i64,
        end_tx_seq: i64,
    ) -> IndexerResult<Vec<StoredTxSenders>>;
    async fn get_recipients_in_tx_range(
        &self,
        start_tx_seq: i64,
        end_tx_seq: i64,
    ) -> IndexerResult<Vec<StoredTxRecipients>>;
    async fn persist_active_addresses(
        &self,
        active_addresses: Vec<StoredActiveAddress>,
    ) -> IndexerResult<()>;
    async fn calculate_address_metrics(
        &self,
        checkpoint: StoredCheckpoint,
    ) -> IndexerResult<StoredAddressMetrics>;
    async fn persist_address_metrics(
        &self,
        address_metrics: StoredAddressMetrics,
    ) -> IndexerResult<()>;

    // for move call metrics
    async fn get_latest_move_call_metrics(&self) -> IndexerResult<StoredMoveCallMetrics>;
    async fn get_move_calls_in_tx_range(
        &self,
        start_tx_seq: i64,
        end_tx_seq: i64,
    ) -> IndexerResult<Vec<StoredTxCalls>>;
    async fn persist_move_calls(&self, move_calls: Vec<StoredMoveCall>) -> IndexerResult<()>;
    async fn calculate_move_call_metrics(
        &self,
        checkpoint: StoredCheckpoint,
    ) -> IndexerResult<Vec<StoredMoveCallMetrics>>;
    async fn persist_move_call_metrics(
        &self,
        move_call_metrics: Vec<StoredMoveCallMetrics>,
    ) -> IndexerResult<()>;
}
