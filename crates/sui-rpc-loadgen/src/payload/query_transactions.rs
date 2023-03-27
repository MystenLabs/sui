// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};

use crate::payload::validation::cross_validate_entities;
use crate::payload::{ProcessPayload, QueryTransactions, RpcCommandProcessor, SignerInfo};
use async_trait::async_trait;
use futures::future::join_all;
use std::time::Instant;
use sui_json_rpc_types::{
    Page, SuiTransactionResponse, SuiTransactionResponseQuery, TransactionsPage,
};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionFilter;
use tracing::debug;
use tracing::log::warn;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a QueryTransactions> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a QueryTransactions,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;
        let filter = match (op.from_address, op.to_address) {
            (Some(_), Some(_)) => {
                return Err(anyhow!("Cannot specify both from_address and to_address"));
            }
            (Some(address), None) => Some(TransactionFilter::FromAddress(address)),
            (None, Some(address)) => Some(TransactionFilter::ToAddress(address)),
            (None, None) => None,
        };
        let query = SuiTransactionResponseQuery {
            filter,
            options: None, // not supported on indexer
        };

        let results: Vec<TransactionsPage> = Vec::new();

        // Paginate results, if any
        while results.is_empty() || results.iter().any(|r| r.has_next_page) {
            let cursor = if results.is_empty() {
                None
            } else {
                match (
                    results.get(0).unwrap().next_cursor,
                    results.get(1).unwrap().next_cursor,
                ) {
                    (Some(first_cursor), Some(second_cursor)) => {
                        if first_cursor != second_cursor {
                            warn!("Cursors are not the same, received {} vs {}. Selecting the first cursor to continue", first_cursor, second_cursor);
                        }
                        Some(first_cursor)
                    }
                    (Some(cursor), None) | (None, Some(cursor)) => Some(cursor),
                    (None, None) => None,
                }
            };

            let results = join_all(clients.iter().enumerate().map(|(i, client)| {
                let with_query = query.clone();
                async move {
                    let start_time = Instant::now();
                    let transactions = query_transaction_blocks(client, with_query, cursor, None)
                        .await
                        .unwrap();
                    let elapsed_time = start_time.elapsed();
                    debug!(
                        "QueryTransactions Request latency {:.4} for rpc at url {i}",
                        elapsed_time.as_secs_f64()
                    );
                    transactions
                }
            }))
            .await;

            // compare results
            let transactions: Vec<Vec<SuiTransactionResponse>> =
                results.iter().map(|page| page.data.clone()).collect();

            cross_validate_entities(&transactions, "Transactions");
        }

        Ok(())
    }
}

async fn query_transaction_blocks(
    client: &SuiClient,
    query: SuiTransactionResponseQuery,
    cursor: Option<TransactionDigest>,
    limit: Option<usize>, // TODO: we should probably set a limit and paginate
) -> Result<Page<SuiTransactionResponse, TransactionDigest>> {
    let transactions = client
        .read_api()
        .query_transaction_blocks(query, cursor, limit, true)
        .await
        .unwrap();
    Ok(transactions)
}
