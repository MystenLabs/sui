// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::{SuiTransactionResponse, TransactionsPage};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionQuery;
use tokio::time::sleep;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::models::transaction_logs::{commit_transction_log, read_transaction_log};
use sui_indexer::models::transactions::commit_transactions;
use sui_indexer::utils::log_errors_to_pg;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

use fastcrypto::encoding::{Base64, Encoding};

const TRANSACTION_PAGE_SIZE: usize = 100;

pub struct TransactionHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
}

impl TransactionHandler {
    pub fn new(rpc_client: SuiClient, pg_connection_pool: Arc<PgConnectionPool>) -> Self {
        Self {
            rpc_client,
            pg_connection_pool,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer transaction handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let mut next_cursor = None;
        let txn_log = read_transaction_log(&mut pg_pool_conn)?;
        if let Some(txn_digest) = txn_log.next_cursor_tx_digest {
            let bytes = Base64::decode(txn_digest.as_str()).map_err(|e| {
                IndexerError::TransactionDigestParsingError(format!(
                    "Failed decoding bytes from txn digest string {:?} with error {:?}",
                    txn_digest, e
                ))
            })?;
            let digest = TransactionDigest::try_from(bytes.as_slice()).map_err(|e| {
                IndexerError::TransactionDigestParsingError(format!(
                    "Failed parsing transaction digest {:?} with error: {:?}",
                    txn_digest, e
                ))
            })?;
            next_cursor = Some(digest);
        }

        loop {
            let page = self.get_transaction_page(next_cursor).await?;
            let txn_digest_vec = page.data;
            let txn_count = txn_digest_vec.len();
            let txn_response_res_vec = join_all(
                txn_digest_vec
                    .into_iter()
                    .map(|tx_digest| self.get_transaction_response(tx_digest)),
            )
            .await;

            let mut errors = vec![];
            let resp_vec: Vec<SuiTransactionResponse> = txn_response_res_vec
                .into_iter()
                .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
                .collect();

            log_errors_to_pg(&mut pg_pool_conn, errors);
            commit_transactions(&mut pg_pool_conn, resp_vec)?;
            // canonical txn digest is Base64 encoded
            commit_transction_log(&mut pg_pool_conn, page.next_cursor.map(|d| d.encode()))?;
            next_cursor = page.next_cursor;
            if txn_count < TRANSACTION_PAGE_SIZE {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
        }
    }

    async fn get_transaction_page(
        &self,
        cursor: Option<TransactionDigest>,
    ) -> Result<TransactionsPage, IndexerError> {
        self.rpc_client
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

    async fn get_transaction_response(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<SuiTransactionResponse, IndexerError> {
        self.rpc_client
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
}
