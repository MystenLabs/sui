// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction subscription: stream the transactions matching a filter, in checkpoint order,
//! resuming from a cursor or checkpoint. Two phases meet with no gap and no duplicate:
//!
//! 1. Backfill ([`backfill_transactions`]): scan the index forward from the resume point, chasing
//!    the indexer tip. Pages are digest-only, so each transaction's fields hydrate lazily and a
//!    page's reads coalesce through the `KvLoader`.
//! 2. Live ([`live_transactions`]): follow the shared checkpoint broadcast, matching in memory.
//!
//! # The seam
//!
//! Once the scan reaches within `handoff_threshold` of the live tip, the subscription subscribes to
//! the broadcast and pins the `handoff` to the tip. Subscribing *before* pinning is what closes the
//! seam: the live feed's first checkpoint is then `handoff + 1`, never past it (any overlap is
//! dropped), so nothing is missed or delivered twice. Pinning mid-scan rather than up front lets even
//! a deep backfill catch up within one connection, the receiver only ever buffers the last
//! `handoff_threshold` checkpoints, so it never lags into a disconnect.
//!
//! # Coverage and freshness
//!
//! The scan reports a coverage marker at each page's end even when the page matched nothing, so a
//! sparse filter still advances the handoff through empty stretches. Each page is held until both
//! indexing pipelines have reached that end; otherwise a transaction near the tip could hydrate
//! against a not-yet-indexed store and resolve to null permanently, since the stream never revisits.
//!
//! # Cursors and anomalies
//!
//! Both phases mint the same opaque cursor, so a client resumes from any delivered one. A gap between
//! the phases, or a subscriber that falls behind the broadcast buffer, disconnects with
//! `reconnect_error`, and the client reconnects and resumes from its last cursor.

use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_stream::stream;
use backoff::ExponentialBackoff;
use backoff::backoff::Backoff;
use futures::Stream;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::lookups::CheckpointBounds;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
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

/// One backfill item: a matching `edge`, or a coverage marker (`edge: None`) whose `checkpoint` is
/// the fully-scanned frontier.
struct Scanned {
    checkpoint: u64,
    edge: Option<Edge<String, Transaction, EmptyFields>>,
}

/// Subscribe to transactions matching `filter`: backfill from `resume` toward the tip, then follow
/// live. The handoff is pinned mid-scan, once the scan frontier comes within `handoff_threshold` of
/// the tip, so even a deep backfill catches up within one connection instead of lagging the receiver.
pub(super) fn transactions_stream(
    reader: AlphaLedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: TransactionFilter,
    resume: Option<ResumeFrom>,
    scan_page_size: usize,
    handoff_threshold: u64,
) -> impl Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>> {
    stream! {
        let mut pending_receiver = None;
        let mut handoff: Option<u64> = None;
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it.
        if let Some(resume) = resume {
            let scan = backfill_transactions(
                reader,
                package_store.clone(),
                resolver_limits.clone(),
                watermarks_rx,
                filter.clone(),
                resume,
                scan_page_size,
            );
            for await scanned in scan {
                let Scanned { checkpoint, edge } = match scanned {
                    Ok(scanned) => scanned,
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                };

                // Resubscribe-first and pin once the scan frontier is within threshold of the tip.
                if pending_receiver.is_none()
                    && broadcast.network_tip().saturating_sub(checkpoint) <= handoff_threshold
                {
                    pending_receiver = Some(broadcast.broadcaster().resubscribe());
                    handoff = Some(broadcast.network_tip());
                }

                match edge {
                    // A match: deliver it, unless it is past the handoff (then it is live's).
                    Some(edge) => {
                        if handoff.is_some_and(|h| checkpoint > h) {
                            break;
                        }
                        yield Ok(edge);
                    }
                    // A coverage marker (`checkpoint` is the fully-scanned frontier): stop once it
                    // has covered the handoff.
                    None => {
                        if handoff.is_some_and(|h| checkpoint >= h) {
                            break;
                        }
                    }
                }
            }
            last_checkpoint = handoff;
        }

        // Phase 2: follow live from `handoff + 1` (a fresh receiver if there was no backfill).
        let receiver = pending_receiver.unwrap_or_else(|| broadcast.broadcaster().resubscribe());
        for await edge in live_transactions(receiver, last_checkpoint, package_store, resolver_limits, filter) {
            yield edge;
        }
    }
}

/// Scan matches from `resume` toward the tip, yielding each match and a per-page coverage marker.
/// Open-ended: the caller stops it once the handoff is covered.
fn backfill_transactions(
    reader: AlphaLedgerGrpcReader,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    mut filter: TransactionFilter,
    resume: ResumeFrom,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Scanned, RpcError>> {
    stream! {
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

        // Open-ended above (the caller stops at the handoff). An empty range means the filter
        // excludes everything, so there is nothing to backfill.
        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint().map(u64::from),
            filter.at_checkpoint().map(u64::from),
            filter.before_checkpoint().map(u64::from),
            0,
            u64::MAX,
        ) else {
            return;
        };

        loop {
            let (items, next_after) = match scan_page(
                &reader,
                &scope,
                &limits,
                cp_bounds.clone(),
                after.as_ref(),
                &filter,
                &mut watermarks_rx,
            )
            .await
            {
                Ok(page) => page,
                Err(e) => {
                    yield Err(e);
                    return;
                }
            };
            for item in items {
                yield Ok(item);
            }
            after = Some(next_after);
        }
    }
}

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

        // Hold the page until both pipelines have indexed through its end.
        wait_for_pipelines_catching_up_at(end_cursor.position.checkpoint(), watermarks_rx).await?;

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
        );
        return Ok((items, next_after));
    }
}

/// Scan one page over `cp_bounds`, retrying transient failures under a bounded budget. Returns the
/// page with its result so the caller can convert it.
async fn scan_with_retry(
    reader: &AlphaLedgerGrpcReader,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&CTransaction>,
    filter: &TransactionFilter,
) -> Result<(Page<CTransaction>, StreamPage<v2::ExecutedTransaction>), RpcError> {
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
        match Transaction::scan_grpc(reader, cp_bounds.clone(), &page, filter).await {
            Ok(result) => return Ok((page, result)),
            Err(e) => match backoff.next_backoff() {
                Some(delay) => {
                    warn!(error = ?e, "scan_grpc failed, retrying");
                    tokio::time::sleep(delay).await;
                }
                None => return Err(e),
            },
        }
    }
}
