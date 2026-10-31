// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transactions as a scan-then-live feed: each matching transaction is delivered as its own edge,
//! ordered by checkpoint then by position within the checkpoint. The cursor places the transaction
//! by checkpoint and index within it, and matching tests a transaction's contents against the
//! filter.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::proto::sui::rpc::v2;

use crate::api::types::ledger_cursor::TransactionKey;
use crate::api::types::ledger_cursor::scan::LedgerPage;
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

use super::TransactionSubscriptionEdge;
use super::scan_then_live::Subscribable;
use super::scan_then_live::ledger_scan_moved;

impl Subscribable for Transaction {
    type Item = Self;
    type Cursor = CTransaction;
    type Filter = TransactionFilter;
    type ScanItem = v2::ExecutedTransaction;
    type Output = TransactionSubscriptionEdge;

    fn subscription_type() -> &'static str {
        "transactions"
    }

    fn output(cursor: String, node: Option<Transaction>) -> TransactionSubscriptionEdge {
        TransactionSubscriptionEdge { cursor, node }
    }

    fn scan<'a>(
        reader: &'a LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CTransaction>,
        filter: &'a TransactionFilter,
    ) -> impl Future<Output = Result<LedgerPage<v2::ExecutedTransaction, CTransaction>, RpcError>>
    + Send
    + 'a {
        Transaction::scan_grpc(reader, cp_bounds, page, page.limit(), filter)
    }

    fn resume_from(cursor: &CTransaction) -> Option<u64> {
        cursor.last_checkpoint()
    }

    fn made_progress(end: &CTransaction, after: &CTransaction, reached_tip: bool) -> bool {
        ledger_scan_moved(end, after, reached_tip)
    }

    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        checkpoint: u64,
        payload: &v2::ExecutedTransaction,
    ) -> Result<Self, RpcError> {
        let scope = Scope::for_backfill(caches.clone(), resolver_limits.clone(), checkpoint);
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
        // `transactions` holds every transaction in the checkpoint, in order.
        for (index, tx) in checkpoint.transactions.iter().enumerate() {
            if !filter.matches(&tx.contents) {
                continue;
            }
            let transaction = Transaction::with_contents(scope.clone(), tx.contents.clone())?;
            let cursor = TransactionToken::at(TransactionKey {
                checkpoint: checkpoint.summary.sequence_number,
                transaction_index: index as u64,
            })
            .encode_cursor();
            edges.push(Edge::new(cursor, transaction));
        }
        Ok(edges)
    }

    fn boundary_after(checkpoint: &ProcessedCheckpoint) -> Option<CTransaction> {
        Some(TransactionToken::checkpoint_start(
            checkpoint.summary.sequence_number + 1,
        ))
    }
}
