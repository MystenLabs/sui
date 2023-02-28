// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use prometheus::Registry;
use std::sync::Arc;
use sui_json_rpc_types::{SuiTransactionResponse, TransactionsPage};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionQuery;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerTransactionHandlerMetrics;
use sui_indexer::models::checkpoints::get_checkpoint;
use sui_indexer::models::transactions::{commit_transactions, read_latest_processed_checkpoint};
use sui_indexer::utils::log_errors_to_pg;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const TRANSACTION_PAGE_SIZE: usize = 100;

pub struct TransactionHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    pub transaction_handler_metrics: IndexerTransactionHandlerMetrics,
}

impl TransactionHandler {
    pub fn new(
        rpc_client: SuiClient,
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            rpc_client,
            pg_connection_pool,
            transaction_handler_metrics: IndexerTransactionHandlerMetrics::new(prometheus_registry),
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer transaction handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        let mut next_checkpoint_to_process =
            read_latest_processed_checkpoint(&mut pg_pool_conn)? + 1;

        loop {
            let checkpoint_db_read_guard = self
                .transaction_handler_metrics
                .checkpoint_db_read_request_latency
                .start_timer();
            let mut checkpoint_opt = get_checkpoint(&mut pg_pool_conn, next_checkpoint_to_process);
            // this often happens when the checkpoint is not yet committed to the database
            while checkpoint_opt.is_err() {
                // this sleep is necessary to avoid blocking the checkpoint commit.
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                checkpoint_opt = get_checkpoint(&mut pg_pool_conn, next_checkpoint_to_process);
            }
            // unwrap is safe here b/c of the check above.
            let checkpoint = checkpoint_opt.unwrap();
            checkpoint_db_read_guard.stop_and_record();

            let request_guard = self
                .transaction_handler_metrics
                .full_node_read_request_latency
                .start_timer();
            let mut errors = vec![];
            let txn_str_vec: Vec<String> = checkpoint
                .transactions
                .iter()
                .filter_map(|t| {
                    t.clone()
                        .ok_or_else(|| {
                            IndexerError::PostgresReadError(format!(
                                "Read null transaction digests from checkpoint: {:?}",
                                checkpoint
                            ))
                        })
                        .map_err(|e| errors.push(e))
                        .ok()
                })
                .collect();
            let txn_digest_vec: Vec<TransactionDigest> = txn_str_vec
                .into_iter()
                .map(|txn_str| {
                    txn_str.parse().map_err(|e| {
                        IndexerError::PostgresReadError(format!(
                            "Failed to decode transaction digest: {:?} with err: {:?}",
                            txn_str, e
                        ))
                    })
                })
                .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
                .collect();
            let txn_count = txn_digest_vec.len();
            self.transaction_handler_metrics
                .total_transactions_received
                .inc_by(txn_count as u64);
            let txn_response_res_vec = join_all(
                txn_digest_vec
                    .into_iter()
                    .map(|tx_digest| get_transaction_response(self.rpc_client.clone(), tx_digest)),
            )
            .await;
            let resp_vec: Vec<SuiTransactionResponse> = txn_response_res_vec
                .into_iter()
                .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
                .collect();
            request_guard.stop_and_record();
            info!(
                "Received transaction responses for {} transaction(s) in checkpoint {}",
                resp_vec.len(),
                next_checkpoint_to_process,
            );
            log_errors_to_pg(&mut pg_pool_conn, errors);

            let db_guard = self
                .transaction_handler_metrics
                .db_write_request_latency
                .start_timer();
            commit_transactions(&mut pg_pool_conn, resp_vec)?;
            next_checkpoint_to_process += 1;
            self.transaction_handler_metrics
                .total_transactions_processed
                .inc_by(txn_count as u64);
            db_guard.stop_and_record();
        }
    }
}

pub async fn get_transaction_page(
    rpc_client: SuiClient,
    cursor: Option<TransactionDigest>,
) -> Result<TransactionsPage, IndexerError> {
    rpc_client
        .read_api()
        .get_transactions(
            TransactionQuery::All,
            cursor,
            Some(TRANSACTION_PAGE_SIZE),
            false,
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed reading transaction page with cursor {:?} and err: {:?}",
                cursor, e
            ))
        })
}

pub async fn get_transaction_response(
    rpc_client: SuiClient,
    tx_digest: TransactionDigest,
) -> Result<SuiTransactionResponse, IndexerError> {
    rpc_client
        .read_api()
        .get_transaction(tx_digest)
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed reading transaction response with tx digest {:?} and err: {:?}",
                tx_digest, e
            ))
        })
}
