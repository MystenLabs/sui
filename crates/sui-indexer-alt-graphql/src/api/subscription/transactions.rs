// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transactions as a scan-then-live feed: each matching transaction is delivered as its own edge,
//! ordered by checkpoint then by position within the checkpoint. The cursor is a two-part position
//! (checkpoint, transaction), and matching tests a transaction's contents against the filter.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;

use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionToken;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::transaction_from_stream_item;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamedCaches;

use super::scan_then_live::Subscribable;

impl Subscribable for Transaction {
    type Item = Self;
    type Cursor = CTransaction;
    type Filter = TransactionFilter;
    type ScanItem = v2::ExecutedTransaction;

    fn subscription_type() -> &'static str {
        "transactions"
    }

    fn scan<'a>(
        reader: &'a AlphaLedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CTransaction>,
        filter: &'a TransactionFilter,
    ) -> impl Future<Output = Result<StreamPage<v2::ExecutedTransaction>, RpcError>> + Send + 'a
    {
        Transaction::scan_grpc(reader, cp_bounds, page, filter)
    }

    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        payload: &v2::ExecutedTransaction,
    ) -> Result<Self, RpcError> {
        let scope = Scope::for_indexed(caches.clone(), resolver_limits.clone());
        transaction_from_stream_item(scope, payload)
    }

    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &TransactionFilter,
    ) -> Result<Vec<Edge<String, Self, EmptyFields>>, RpcError> {
        let scope = Scope::for_streamed_checkpoint(
            caches.clone(),
            resolver_limits.clone(),
            checkpoint.clone(),
        );
        let mut edges = Vec::new();
        for tx in &checkpoint.transactions {
            if !filter.matches(&tx.contents) {
                continue;
            }
            let transaction = Transaction::with_contents(scope.clone(), tx.contents.clone())?;
            let cursor =
                TransactionToken::cursor(checkpoint.summary.sequence_number, tx.tx_sequence_number)
                    .encode_cursor();
            edges.push(Edge::new(cursor, transaction));
        }
        Ok(edges)
    }
}
