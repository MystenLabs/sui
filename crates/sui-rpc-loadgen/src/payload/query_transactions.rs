// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};

use crate::payload::validation::cross_validate_entities;
use crate::payload::{
    AddressType, ProcessPayload, QueryTransactions, RpcCommandProcessor, SignerInfo,
};
use async_trait::async_trait;
use futures::future::join_all;
use std::time::Instant;
use sui_json_rpc_types::{
    Page, SuiTransactionBlockResponse, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{SuiAddress, TransactionDigest};
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

        let filters: Vec<Option<TransactionFilter>> = match (
            op.address,
            op.address_type.as_ref(),
            op.from_file,
        ) {
            (Some(address), Some(address_type), Some(false)) => match address_type {
                AddressType::FromAddress => vec![Some(TransactionFilter::FromAddress(address))],
                AddressType::ToAddress => vec![Some(TransactionFilter::ToAddress(address))],
            },
            (None, Some(address_type), Some(true)) => {
                // TODO: actually load the addresses too
                let addresses: Vec<SuiAddress> = self
                    .get_addresses()
                    .iter()
                    .map(|address| *address)
                    .collect();

                let filters = match address_type {
                    AddressType::FromAddress => addresses
                        .iter()
                        .map(|address| Some(TransactionFilter::FromAddress(*address)))
                        .collect(),
                    AddressType::ToAddress => addresses
                        .iter()
                        .map(|address| Some(TransactionFilter::ToAddress(*address)))
                        .collect(),
                };
                filters
            }
            (None, None, None) => vec![None],
            _ => {
                return Err(anyhow!(
                    "Invalid combination. Provide (address, address_type, false), (None, address_type, true), or (None, None, None)"
                ));
            }
        };

        let queries: Vec<SuiTransactionBlockResponseQuery> = filters
            .into_iter()
            .map(|filter| SuiTransactionBlockResponseQuery {
                filter,
                options: None,
            })
            .collect();

        // todo: can map this
        for query in queries {
            let mut results: Vec<TransactionBlocksPage> = Vec::new();

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

                results = join_all(clients.iter().enumerate().map(|(i, client)| {
                    let with_query = query.clone();
                    async move {
                        let start_time = Instant::now();
                        let transactions =
                            query_transaction_blocks(client, with_query, cursor, None)
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

                let transactions: Vec<Vec<SuiTransactionBlockResponse>> =
                    results.iter().map(|page| page.data.clone()).collect();
                cross_validate_entities(&transactions, "Transactions");
            }
        }
        Ok(())
    }
}

async fn query_transaction_blocks(
    client: &SuiClient,
    query: SuiTransactionBlockResponseQuery,
    cursor: Option<TransactionDigest>,
    limit: Option<usize>, // TODO: we should probably set a limit and paginate
) -> Result<Page<SuiTransactionBlockResponse, TransactionDigest>> {
    let transactions = client
        .read_api()
        .query_transaction_blocks(query, cursor, limit, true)
        .await
        .unwrap();
    Ok(transactions)
}
