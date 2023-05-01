// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use crate::payload::validation::cross_validate_entities;
use crate::payload::{
    AddressQueryType, ProcessPayload, QueryTransactionBlocks, RpcCommandProcessor, SignerInfo,
};
use async_trait::async_trait;
use futures::future::join_all;
use sui_json_rpc_types::{
    Page, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, TransactionBlocksPage, TransactionFilter,
};
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use tracing::log::warn;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a QueryTransactionBlocks> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a QueryTransactionBlocks,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;
        let address_type = &op.address_type;
        if op.addresses.is_empty() {
            warn!("No addresses provided, skipping query");
            return Ok(());
        }
        let filters = {
            let mut from: Vec<Option<TransactionFilter>> = op
                .addresses
                .iter()
                .map(|address| Some(TransactionFilter::FromAddress(*address)))
                .collect();

            let mut to = op
                .addresses
                .iter()
                .map(|address| Some(TransactionFilter::ToAddress(*address)))
                .collect();

            match address_type {
                AddressQueryType::From => from,
                AddressQueryType::To => to,
                AddressQueryType::Both => from.drain(..).chain(to.drain(..)).collect(),
            }
        };

        let queries: Vec<SuiTransactionBlockResponseQuery> = filters
            .into_iter()
            .map(|filter| SuiTransactionBlockResponseQuery {
                filter,
                options: Some(SuiTransactionBlockResponseOptions::full_content()),
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

                results = join_all(clients.iter().enumerate().map(|(_i, client)| {
                    let with_query = query.clone();
                    async move {
                        query_transaction_blocks(client, with_query, cursor, None)
                            .await
                            .unwrap()
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
