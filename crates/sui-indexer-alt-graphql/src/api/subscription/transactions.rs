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
use crate::task::streaming::StreamingPackageStore;
<<<<<<< HEAD
=======
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::streaming::broadcast_error;
use crate::task::streaming::reconnect_error;
use crate::task::streaming::wait_for_ledger_grpc_catching_up_at;
use crate::task::watermark::Watermarks;
>>>>>>> 90fe6c0dbd4 ([indexer-alt] Gate subscription readiness and gap recovery on ledger_grpc)

use super::scan_then_live::Subscribable;

impl Subscribable for Transaction {
    type Item = Self;
    type Cursor = CTransaction;
    type Filter = TransactionFilter;
    type ScanItem = v2::ExecutedTransaction;

    fn scan<'a>(
        reader: &'a AlphaLedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CTransaction>,
        filter: &'a TransactionFilter,
    ) -> impl Future<Output = Result<StreamPage<v2::ExecutedTransaction>, RpcError>> + Send + 'a
    {
        Transaction::scan_grpc(reader, cp_bounds, page, filter)
    }

    fn build_node(scope: &Scope, payload: &v2::ExecutedTransaction) -> Result<Self, RpcError> {
        transaction_from_stream_item(scope.clone(), payload)
    }

<<<<<<< HEAD
    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        package_store: &Arc<StreamingPackageStore>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &TransactionFilter,
    ) -> Result<Vec<Edge<String, Self, EmptyFields>>, RpcError> {
        let scope = Scope::for_streamed_checkpoint(
            package_store.clone(),
            resolver_limits.clone(),
            checkpoint.clone(),
=======
/// The highest checkpoint fully scanned as of `token`: its own checkpoint for a `Boundary`, one less
/// for an `Item` (whose checkpoint may still hold later matches).
fn covered_checkpoint(token: &CursorToken) -> u64 {
    let checkpoint = token.position.checkpoint();
    match token.kind {
        CursorKind::Boundary => checkpoint,
        CursorKind::Item => checkpoint.saturating_sub(1),
    }
}

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching transactions. Drops the
/// one-checkpoint seam overlap (the resubscribe/tip race), gap-checks, and disconnects on anomalies.
fn live_transactions(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    filter: TransactionFilter,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    stream! {
        let mut delivered_live = false;
        loop {
            match receiver.recv().await {
                Ok(checkpoint) => {
                    let seq = checkpoint.summary.sequence_number;
                    if let Some(last) = last_checkpoint {
                        // Already covered by the scan (resubscribe/tip overlap): skip.
                        if seq <= last {
                            continue;
                        }
                        if seq > last + 1 {
                            warn!(
                                last_checkpoint = last,
                                received = seq,
                                "Unexpected gap between scan and live; disconnecting"
                            );
                            yield Err(reconnect_error());
                            return;
                        }
                    }
                    // Deliver each matching transaction as its own payload, ordered within the
                    // checkpoint. Empty checkpoints yield nothing.
                    let edges = matching_edges(&checkpoint, &package_store, &resolver_limits, &filter)?;
                    for edge in edges {
                        yield Ok(edge);
                    }
                    last_checkpoint = Some(seq);
                    delivered_live = true;
                }
                // A lag before the first live checkpoint is catch-up overflow (likely kv-rpc lag).
                Err(broadcast::error::RecvError::Lagged(missed)) if !delivered_live => {
                    warn!(missed, "Subscriber fell behind during catch-up; disconnecting");
                    yield Err(reconnect_error());
                    return;
                }
                Err(e) => {
                    yield Err(broadcast_error(e));
                    return;
                }
            }
        }
    }
}

/// The filter-matching transactions in one live checkpoint, in order.
fn matching_edges(
    checkpoint: &Arc<ProcessedCheckpoint>,
    package_store: &Arc<StreamingPackageStore>,
    resolver_limits: &sui_package_resolver::Limits,
    filter: &TransactionFilter,
) -> Result<Vec<Edge<String, Transaction, EmptyFields>>, RpcError> {
    let scope = Scope::for_streamed_checkpoint(
        package_store.clone(),
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

/// Retry policy for the backfill scan: jittered exponential backoff, giving up after 60s (past which
/// a failure is unlikely to be a transient blip).
fn scan_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(5),
        max_elapsed_time: Some(Duration::from_secs(60)),
        ..Default::default()
    }
}

/// Scan the next page of matches, waiting at the indexer tip if nothing is ready yet. Returns the
/// matches, a trailing coverage marker, and the cursor to resume from.
async fn scan_page(
    reader: &AlphaLedgerGrpcReader,
    scope: &Scope,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&CTransaction>,
    filter: &TransactionFilter,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> Result<(Vec<Scanned>, CTransaction), RpcError> {
    loop {
        let (page, result) =
            scan_with_retry(reader, limits, cp_bounds.clone(), after, filter).await?;

        // No end cursor means nothing past the indexer tip is indexed yet; wait, then re-request. A
        // range that was scanned but matched nothing still returns a boundary cursor, not `None`.
        let Some(end_cursor) = result.last_cursor() else {
            tokio::time::sleep(BACKFILL_POLL_INTERVAL).await;
            continue;
        };
        let end_cursor = CursorToken::decode(end_cursor).context("Failed to decode scan cursor")?;

        // Hold the page until the ledger gRPC service has indexed through its end.
        wait_for_ledger_grpc_catching_up_at(end_cursor.position.checkpoint(), watermarks_rx)
            .await?;

        // Each match's checkpoint, read from the raw item cursors before the page is converted.
        let mut checkpoints = Vec::with_capacity(result.items.len());
        for item in &result.items {
            let token =
                CursorToken::decode(&item.cursor).context("Failed to decode scan item cursor")?;
            checkpoints.push(token.position.checkpoint());
        }

        // Reuse the shared page-to-connection conversion for the edges themselves. A forward page
        // keeps `items` order, so its edges pair positionally with `checkpoints`.
        let edges = build_grpc_connection(scope.clone(), &page, result)?.edges;
        assert_eq!(edges.len(), checkpoints.len(), "one edge per scanned item");

        let mut items = Vec::with_capacity(edges.len() + 1);
        for (i, edge) in edges.into_iter().enumerate() {
            items.push(Scanned {
                checkpoint: checkpoints[i],
                edge: Some(edge),
            });
        }
        // Coverage marker at the fully-scanned end: advances the handoff even on a match-less page.
        items.push(Scanned {
            checkpoint: covered_checkpoint(&end_cursor),
            edge: None,
        });

        let next_after = OpaqueCursor::new(
            end_cursor
                .try_into()
                .context("Unexpected end cursor position")?,
>>>>>>> 90fe6c0dbd4 ([indexer-alt] Gate subscription readiness and gap recovery on ledger_grpc)
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
