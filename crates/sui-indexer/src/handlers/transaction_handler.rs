// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use sui_json_rpc_types::{SuiTransactionResponse, TransactionsPage};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionQuery;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::establish_connection;
use sui_indexer::models::transaction_logs::{commit_transction_log, read_transaction_log};
use sui_indexer::models::transactions::commit_transactions;
use sui_indexer::utils::log_errors_to_pg;

const TRANSACTION_PAGE_SIZE: usize = 100;

pub struct TransactionHandler {
    rpc_client: SuiClient,
}

impl TransactionHandler {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn start(&self) {
        let mut pg_conn = establish_connection();
        let mut next_cursor = None;
        // retry
        let txn_log = read_transaction_log(&mut pg_conn).unwrap();
        if let Some(txn_digest) = txn_log.next_cursor_tx_digest {
            next_cursor = TransactionDigest::from_string(txn_digest);
        }

        loop {
            // retry
            let page = self.get_transaction_page(next_cursor).await.unwrap();
            let txn_digest_vec = page.data;
            let txn_response_res_vec = join_all(
                txn_digest_vec
                    .into_iter()
                    .map(|tx_digest| self.get_transaction_response(tx_digest)),
            )
            .await;

            let mut errors = vec![];
            let resp_vec: Vec<SuiTransactionResponse> = txn_response_res_vec
                .into_iter()
                .filter_map(|f| f.map_err(|e| errors.push(e.clone())).ok())
                .collect();
            log_errors_to_pg(errors);

            // retry, and then log to error table
            commit_transactions(&mut pg_conn, resp_vec).unwrap();
            commit_transction_log(&mut pg_conn, page.next_cursor.map(|d| d.to_string())).unwrap();
            next_cursor = page.next_cursor;
        }
    }

    // retry
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
                None,
            )
            .await
            .map_err(|e| {
                IndexerError::FullNodeReadingError(format!(
                    "Failed reading transaction page with cursor {:?} and err: {:?}",
                    cursor, e
                ))
            })
    }

    // retry
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
