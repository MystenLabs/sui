// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use crate::schema_v2::tx_count_metrics;

use super::checkpoints::StoredCheckpoint;
use super::transactions::StoredTransactionSuccessCommandCount;

#[derive(Clone, Debug, Default, Queryable, Insertable)]
#[diesel(table_name = tx_count_metrics)]
pub struct StoredTxCountMetrics {
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub timestamp_ms: i64,
    pub total_transaction_blocks: i64,
    pub total_successful_transaction_blocks: i64,
    pub total_successful_transactions: i64,
    pub network_total_transaction_blocks: i64,
    pub network_total_successful_transactions: i64,
    pub network_total_successful_transaction_blocks: i64,
}

#[derive(Debug, Clone)]
pub struct TxCountMetricsDelta {
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub timestamp_ms: i64,
    pub total_transaction_blocks: i64,
    pub total_successful_transaction_blocks: i64,
    pub total_successful_transactions: i64,
}

impl TxCountMetricsDelta {
    pub fn get_tx_count_metrics_delta(
        tx_cmd_count_batch: &[StoredTransactionSuccessCommandCount],
        end_cp: &StoredCheckpoint,
    ) -> Self {
        let checkpoint_sequence_number = end_cp.sequence_number;
        let epoch = end_cp.epoch;
        let timestamp_ms = end_cp.timestamp_ms;

        let total_transaction_blocks = tx_cmd_count_batch.len() as i64;
        let total_successful_transaction_blocks = tx_cmd_count_batch
            .iter()
            .filter(|tx_cmd_count| tx_cmd_count.success_command_count > 0)
            .count() as i64;
        let total_successful_transactions =
            tx_cmd_count_batch.iter().fold(0, |acc, tx_cmd_count| {
                acc + tx_cmd_count.success_command_count as i64
            });
        Self {
            checkpoint_sequence_number,
            epoch,
            timestamp_ms,
            total_transaction_blocks,
            total_successful_transaction_blocks,
            total_successful_transactions,
        }
    }
}

impl StoredTxCountMetrics {
    pub fn combine_tx_count_metrics_delta(
        last_tx_count_metrics: &StoredTxCountMetrics,
        delta: &TxCountMetricsDelta,
    ) -> StoredTxCountMetrics {
        StoredTxCountMetrics {
            checkpoint_sequence_number: delta.checkpoint_sequence_number,
            epoch: delta.epoch,
            timestamp_ms: delta.timestamp_ms,
            total_transaction_blocks: delta.total_transaction_blocks,
            total_successful_transaction_blocks: delta.total_successful_transaction_blocks,
            total_successful_transactions: delta.total_successful_transactions,
            network_total_transaction_blocks: last_tx_count_metrics
                .network_total_transaction_blocks
                + delta.total_transaction_blocks,
            network_total_successful_transactions: last_tx_count_metrics
                .network_total_successful_transactions
                + delta.total_successful_transactions,
            network_total_successful_transaction_blocks: last_tx_count_metrics
                .network_total_successful_transaction_blocks
                + delta.total_successful_transaction_blocks,
        }
    }
}
