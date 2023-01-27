// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerTransactionStatsProcessorMetrics;
use sui_indexer::models::transaction_stats::{commit_transaction_stats, NewTransactionStats};
use sui_indexer::models::transactions::{read_last_transaction, read_transactions_since};
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const TRANSACTION_STATS_COMPUTATION_WINDOW_IN_SECONDS: i64 = 5;

pub struct TransactionStatsProcessor {
    pg_connection_pool: Arc<PgConnectionPool>,
    pub transaction_stats_processor_metrics: IndexerTransactionStatsProcessorMetrics,
}

impl TransactionStatsProcessor {
    pub fn new(
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> TransactionStatsProcessor {
        let transaction_stats_processor_metrics =
            IndexerTransactionStatsProcessorMetrics::new(prometheus_registry);
        Self {
            pg_connection_pool,
            transaction_stats_processor_metrics,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer transaction stats processor started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        loop {
            let mut last_txn_opt = read_last_transaction(&mut pg_pool_conn)?;
            while last_txn_opt.is_none() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                last_txn_opt = read_last_transaction(&mut pg_pool_conn)?;
            }
            // unwrap is safe here because we know last_txn_opt is not None
            let last_txn = last_txn_opt.unwrap();
            let end_timestamp = last_txn
                .transaction_time
                .ok_or(IndexerError::TransactionTimeNotAvailable)?;
            let since = end_timestamp
                .checked_sub_signed(chrono::Duration::seconds(
                    TRANSACTION_STATS_COMPUTATION_WINDOW_IN_SECONDS,
                ))
                .ok_or(IndexerError::TimestampOverflow)?;
            let transactions = read_transactions_since(&mut pg_pool_conn, since)?;
            let tps = (transactions.len() as f32)
                / (TRANSACTION_STATS_COMPUTATION_WINDOW_IN_SECONDS as f32);
            let stats = NewTransactionStats {
                computation_time: chrono::NaiveDateTime::from_timestamp_millis(
                    chrono::Utc::now().timestamp_millis(),
                )
                .ok_or(IndexerError::TimestampOverflow)?,
                start_txn_time: since,
                end_txn_time: end_timestamp,
                tps,
            };
            commit_transaction_stats(&mut pg_pool_conn, vec![stats])?;
            self.transaction_stats_processor_metrics
                .total_transaction_stats_processed
                .inc();
            tokio::time::sleep(std::time::Duration::from_secs(
                TRANSACTION_STATS_COMPUTATION_WINDOW_IN_SECONDS as u64,
            ))
            .await;
        }
    }
}
