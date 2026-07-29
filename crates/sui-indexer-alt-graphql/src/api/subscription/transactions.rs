// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction subscription.
//!
//! Streams the transactions matching a filter, in checkpoint order, resuming from a cursor or a
//! checkpoint. Assembled in two phases that meet at a pinned `handoff` with no gap and no duplicate
//! ([`transactions_stream`]):
//!
//! 1. Backfill ([`backfill_transactions`]): scan the matches in `(resume, handoff]` from the index
//!    ([`scan_grpc_page`]). Pages are digest-only; each
//!    transaction's fields hydrate lazily from the index, so a page's reads coalesce through the
//!    `KvLoader`.
//! 2. Live ([`live_transactions`]): follow the shared checkpoint broadcast, matching each
//!    checkpoint's transactions in memory.
//!
//! # Handoff, by example
//!
//! A client resumes after checkpoint 5, and the live tip is currently at 10:
//!
//! - Phase 1 subscribes to the live broadcast (which will deliver checkpoint 11 next) and pins
//!   `handoff = 10`, then backfills the filter's matches in checkpoints 6..=10.
//! - Phase 1 stops once it has covered checkpoint 10.
//! - Phase 2 takes over from the live broadcast: checkpoint 11, 12, 13, ...
//!
//! The seam at 10 -> 11 has no gap and no duplicate. Subscribing *before* reading the tip is what
//! guarantees it: the live feed's first checkpoint is always `handoff + 1` or earlier (any overlap
//! is dropped), never past it. See [`live_transactions`] for the overlap skip and the gap check.
//!
//! Pinning the handoff up front is what makes the seam reachable: the live receiver is already
//! subscribed and buffering everything past `handoff`, so backfill only has to paginate the fixed
//! range up to `handoff` while the live buffer holds the rest. That buffer is bounded (256
//! checkpoints, about 60s at 4 cp/s), which is the window backfill has to reach `handoff` before the
//! live subscriber lags and disconnects; comfortably enough for a bounded range.
//!
//! # Catch-up gate
//!
//! Backfill delivers finalized transactions whose fields resolve lazily from the index, so before
//! delivering anything it waits for the indexing pipelines to reach `handoff`. Otherwise a
//! transaction near the tip could hydrate against a store that has not indexed it yet and resolve to
//! null, permanently, since the stream never revisits. The wait is bounded by how far the indexer
//! lags the live tip, not by how far back the resume point is.
//!
//! # Coverage
//!
//! Pagination ends its stream both when it drains the requested range (reached `handoff`) and when
//! it hits the indexer's current tip below it, and `has_next_page` cannot tell the two apart. So
//! backfill derives the covered checkpoint from each page's cursor (a `Boundary` cursor is a scan
//! frontier; an `Item` cursor's checkpoint may still hold later matches) and terminates only once
//! coverage reaches `handoff`. This also carries a sparse filter through stretches that match
//! nothing, since a boundary cursor advances coverage even on a page with no matches.
//!
//! # Cursors
//!
//! Both phases mint the same opaque `CursorToken` (as a `CTransaction`), so a client can resume from
//! any delivered cursor: the backfill carries pagination's cursor through, and live mints an
//! equivalent token from the checkpoint and transaction sequence numbers.
//!
//! # Anomalies
//!
//! A gap between backfill and live, or a subscriber that lags the broadcast buffer, disconnects with
//! `reconnect_error`; the client reconnects and resumes from its last cursor.

use std::ops::RangeBounds;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_stream::stream;
use backoff::ExponentialBackoff;
use backoff::backoff::Backoff;
use futures::Stream;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionConnection;
use crate::api::types::transaction::TransactionToken;
use crate::api::types::transaction::build_grpc_connection;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::pagination::PageLimits;
use crate::scope::Scope;
use crate::task::streaming::CheckpointBroadcaster;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamingPackageStore;
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::streaming::broadcast_error;
use crate::task::streaming::reconnect_error;
use crate::task::streaming::wait_for_pipelines_catching_up_at;
use crate::task::watermark::Watermarks;

/// How long to wait before re-requesting when the scan has drained the indexer's current tip but not
/// yet reached the handoff.
const BACKFILL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Where a backfill resumes from.
pub(super) enum ResumeFrom {
    /// A transaction cursor from a prior delivery.
    Cursor(CTransaction),
    /// A checkpoint sequence number (`afterCheckpoint`); backfill starts at `checkpoint + 1`.
    Checkpoint(u64),
}

/// Subscribe to transactions matching `filter`, backfilling from `resume` then following live.
///
/// Phase 1 pins `handoff` to the current tip and backfills the matches in `(resume, handoff]`
/// through the indexed pagination path. Phase 2 follows the pinned receiver from `handoff + 1`. The
/// receiver is subscribed *before* the tip is sampled, so the live feed's first checkpoint is
/// `<= handoff + 1`: the seam has no gap and no duplicate.
pub(super) fn transactions_stream(
    reader: AlphaLedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: TransactionFilter,
    resume: Option<ResumeFrom>,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    stream! {
        let mut last_checkpoint: Option<u64> = None;
        let mut pending_receiver = None;

        // Phase 1: backfill `(resume, handoff]`, if resuming.
        if let Some(resume) = resume {
            // Subscribe first, then pin the tip, so live buffers everything past the handoff while
            // the backfill runs.
            let receiver = broadcast.broadcaster().resubscribe();
            let handoff = broadcast.network_tip();
            for await edge in backfill_transactions(
                reader,
                package_store.clone(),
                resolver_limits.clone(),
                watermarks_rx,
                filter.clone(),
                resume,
                handoff,
                scan_page_size,
            ) {
                yield edge;
            }
            last_checkpoint = Some(handoff);
            pending_receiver = Some(receiver);
        }

        // Phase 2: follow live from `handoff + 1` (a fresh receiver if there was no backfill).
        let receiver = pending_receiver.unwrap_or_else(|| broadcast.broadcaster().resubscribe());
        for await edge in live_transactions(receiver, last_checkpoint, package_store, resolver_limits, filter) {
            yield edge;
        }
    }
}

/// Backfill the matches in `(resume, handoff]` by scanning the index ([`scan_grpc_page`]):
/// digest-only pages whose fields hydrate lazily from the index.
///
/// Before delivering anything, wait for the indexing pipelines to reach `handoff` so those lazy
/// reads resolve against present data rather than returning null. The wait is bounded by how far the
/// indexer lags the live tip (normally small), not by how far back `resume` is.
///
/// Termination keys off coverage, not `has_next_page`: pagination ends the stream both when it
/// drains the requested range (reached `handoff`) and when it hits the indexer's current tip below
/// it, and `has_next_page` cannot tell the two apart. So each page's covered checkpoint is derived
/// from its cursor, and the backfill ends only once coverage reaches `handoff`.
fn backfill_transactions(
    reader: AlphaLedgerGrpcReader,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    mut filter: TransactionFilter,
    resume: ResumeFrom,
    handoff: u64,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    stream! {
        if let Err(e) = wait_for_pipelines_catching_up_at(handoff, &mut watermarks_rx).await {
            yield Err(RpcError::from(e));
            return;
        }

        // Finalized, indexed data: fields resolve lazily through the index. cvat is None (uniform
        // with live), so the scan range is supplied explicitly rather than derived from the scope.
        let scope = Scope::for_backfilled_transactions(package_store, resolver_limits);
        let limits = PageLimits {
            default: scan_page_size as u32,
            max: scan_page_size as u32,
        };

        // A cursor resume seeds the first page's `after`; a checkpoint resume becomes an internal
        // lower bound (`afterCheckpoint` starts at `checkpoint + 1`).
        let mut after: Option<CTransaction> = None;
        match resume {
            ResumeFrom::Cursor(cursor) => after = Some(cursor),
            ResumeFrom::Checkpoint(cp) => filter.after_checkpoint = Some(UInt53::from(cp)),
        }

        // Scan the filter's matches within its checkpoint predicates, capped above by the handoff
        // where live takes over. An empty range means the filter excludes everything at or below the
        // handoff, so there is nothing to backfill.
        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint().map(u64::from),
            filter.at_checkpoint().map(u64::from),
            filter.before_checkpoint().map(u64::from),
            0,
            handoff,
        ) else {
            return;
        };

        loop {
            let conn = match scan_with_retry(&reader, &scope, &limits, cp_bounds.clone(), after.as_ref(), &filter).await {
                Ok(conn) => conn,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };

            let has_next = conn.page_info.has_next_page;
            let end_cursor = conn.page_info.end_cursor.clone();
            for edge in conn.edges {
                yield Ok(edge);
            }

            // Advance to the scan frontier and stop once it has covered through the handoff.
            if let Some(encoded) = end_cursor {
                let cursor = match CTransaction::decode_cursor(&encoded) {
                    Ok(cursor) => cursor,
                    Err(_) => {
                        yield Err(RpcError::from(anyhow::anyhow!(
                            "Invalid page cursor from pagination"
                        )));
                        return;
                    }
                };
                let covered = covered_checkpoint(&cursor);
                after = Some(cursor);
                if covered >= handoff {
                    break;
                }
            } else if !has_next {
                // Nothing scanned and nothing more to page: the available range is drained below the
                // handoff. With the up-front wait this should not happen; break rather than spin.
                break;
            }

            // Reached the indexer's current tip below the handoff; wait for it to advance before
            // re-requesting from the frontier. Reachable only if the bitmap index lags the pipeline
            // the wait gated on.
            if !has_next {
                tokio::time::sleep(BACKFILL_POLL_INTERVAL).await;
            }
        }
    }
}

/// The highest checkpoint a backfill page has fully scanned, from its end cursor. A `Boundary`
/// cursor is a scan frontier, so everything through its checkpoint is covered; an `Item` cursor is a
/// returned match whose checkpoint may still hold later matches, so coverage is vouched only through
/// the previous checkpoint.
fn covered_checkpoint(cursor: &CTransaction) -> u64 {
    let token = CursorToken::from(&**cursor);
    let Position::Transactions { checkpoint, .. } = token.position else {
        return 0;
    };
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

/// The backfill scan's retry policy: jittered exponential backoff up to a bounded budget. 60s is the
/// top of the window where a failure is still plausibly a transient blip/deploy; beyond it we give
/// up rather than retry indefinitely (per industry retry-budget guidance).
fn scan_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(5),
        max_elapsed_time: Some(Duration::from_secs(60)),
        ..Default::default()
    }
}

/// Scan one page of the indexed transactions over `cp_bounds` ([`scan_grpc_page`]), retrying
/// transient failures under a bounded budget. The page is rebuilt per attempt because [`Page`] is
/// not `Clone`. A rolling indexer deploy (the common transient outage) is absorbed invisibly;
/// anything still failing once the budget is exhausted propagates, ending the subscription so the
/// client reconnects and resumes from its cursor.
async fn scan_with_retry(
    reader: &AlphaLedgerGrpcReader,
    scope: &Scope,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&CTransaction>,
    filter: &TransactionFilter,
) -> Result<TransactionConnection, RpcError> {
    let mut backoff = scan_backoff();
    loop {
        let page = Page::from_params(
            limits,
            Some(limits.default as u64),
            after.cloned(),
            None,
            None,
        )
        .map_err(anyhow::Error::from)?;
        match scan_grpc_page(reader, scope.clone(), cp_bounds.clone(), page, filter).await {
            Ok(conn) => return Ok(conn),
            Err(e) => match backoff.next_backoff() {
                Some(delay) => {
                    warn!(error = ?e, "scan_grpc_page failed, retrying");
                    tokio::time::sleep(delay).await;
                }
                None => return Err(e),
            },
        }
    }
}

/// Scan one page of indexed transactions over `cp_bounds` and build a connection. Composes the
/// shared [`Transaction::scan_grpc`] + [`build_grpc_connection`] rather than going through
/// `Transaction::paginate_grpc`, because the backfill supplies the checkpoint range directly and its
/// scope has no `checkpoint_viewed_at` (it does not resolve as of a single consistent checkpoint).
async fn scan_grpc_page(
    reader: &AlphaLedgerGrpcReader,
    scope: Scope,
    cp_bounds: impl RangeBounds<u64>,
    page: Page<CTransaction>,
    filter: &TransactionFilter,
) -> Result<TransactionConnection, RpcError> {
    if page.limit() == 0 {
        return Ok(Connection::new(false, false).into());
    }

    let result = Transaction::scan_grpc(reader, cp_bounds, &page, filter).await?;
    build_grpc_connection(scope, &page, result)
}
