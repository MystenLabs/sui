// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::{SuiTransactionResponse, TransactionsPage};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionQuery;
use tokio::time::sleep;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerTransactionHandlerMetrics;
use sui_indexer::models::transaction_logs::{commit_transction_log, read_transaction_log};
use sui_indexer::models::transactions::commit_transactions;
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

        let mut next_cursor = None;
        let txn_log = read_transaction_log(&mut pg_pool_conn)?;
        if let Some(tx_dig) = txn_log.next_cursor_tx_digest {
            let tx_digest = tx_dig.parse().map_err(|e| {
                IndexerError::TransactionDigestParsingError(format!(
                    "Failed parsing transaction digest {:?} with error: {:?}",
                    tx_dig, e
                ))
            })?;
            next_cursor = Some(tx_digest);
        }

        loop {
            self.transaction_handler_metrics
                .total_transaction_page_fetch_attempt
                .inc();
            let page = get_transaction_page(self.rpc_client.clone(), next_cursor).await?;
            self.transaction_handler_metrics
                .total_transaction_page_received
                .inc();
            let txn_digest_vec = page.data;
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
            info!(
                "Received transaction responses for {} transactions with next cursor: {:?}",
                txn_response_res_vec.len(),
                page.next_cursor,
            );

            let mut errors = vec![];
            let resp_vec: Vec<SuiTransactionResponse> = txn_response_res_vec
                .into_iter()
                .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
                .collect();

            log_errors_to_pg(&mut pg_pool_conn, errors);
            commit_transactions(&mut pg_pool_conn, resp_vec)?;
            // Transaction page's next cursor can be None when latest transaction page is
            // reached, if we use the None cursor to read transactions, it will read from genesis,
            // thus here we do not commit / use the None cursor.
            // This will cause duplidate run of the current batch, but will not cause duplidate rows
            // b/c of the uniqueness restriction of the table.
            if let Some(next_cursor_val) = page.next_cursor {
                // canonical txn digest is Base58 encoded
                commit_transction_log(&mut pg_pool_conn, Some(next_cursor_val.base58_encode()))?;
                self.transaction_handler_metrics
                    .total_transactions_processed
                    .inc_by(txn_count as u64);
                next_cursor = page.next_cursor;
            }
            self.transaction_handler_metrics
                .total_transaction_page_committed
                .inc();
            if txn_count < TRANSACTION_PAGE_SIZE || page.next_cursor.is_none() {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
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
