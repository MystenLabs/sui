// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scan-then-live subscription driver: stream the items matching a filter, in checkpoint order,
//! resuming from a cursor or checkpoint. Two phases meet with no gap and no duplicate:
//!
//! 1. Backfill ([`backfill`]): scan the index forward from the resume point, chasing the indexer
//!    tip. Pages are digest-only, so each item's fields hydrate lazily and a page's reads coalesce
//!    through the `KvLoader`.
//! 2. Live ([`live`]): follow the shared checkpoint broadcast, matching in memory.
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
//! indexing pipelines have reached that end; otherwise an item near the tip could hydrate against a
//! not-yet-indexed store and resolve to null permanently, since the stream never revisits.
//!
//! # Cursors and anomalies
//!
//! Both phases mint the same opaque cursor, so a client resumes from any delivered one. A gap between
//! the phases, or a subscriber that falls behind the broadcast buffer, disconnects with
//! `reconnect_error`, and the client reconnects and resumes from its last cursor.
//!
//! What varies per feed (which fields match, how cursors wrap, which scan RPC to call) is supplied
//! by the [`Subscribable`] implementation for that feed; everything here is shared.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_graphql::OutputType;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_stream::stream;
use backoff::ExponentialBackoff;
use futures::Stream;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

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

/// A feed that can be streamed with the scan-then-live driver. One implementation per subscribable
/// GraphQL type (transactions, events) supplies its cursor, filter, and matching, and the free
/// functions here drive the shared machinery over it.
pub(super) trait Subscribable {
    /// The GraphQL node delivered in each edge.
    type Item: OutputType + Send + 'static;
    /// The opaque cursor minted for resumption.
    type Cursor: CursorType
        + Eq
        + Clone
        + Send
        + Sync
        + TryFrom<CursorToken, Error = anyhow::Error>
        + 'static;
    /// The filter selecting matching items.
    type Filter: Clone + Send + Sync + 'static;
    /// The raw scan payload one page yields before conversion to edges.
    type ScanItem: Send + 'static;

    /// Scan one page of matches over `cp_bounds`, starting after `page`'s cursor.
    fn scan<'a>(
        reader: &'a AlphaLedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<Self::Cursor>,
        filter: &'a Self::Filter,
    ) -> impl Future<Output = Result<StreamPage<Self::ScanItem>, RpcError>> + Send + 'a;

    /// Build the GraphQL node from one scanned item's payload. The driver mints the cursor from the
    /// item's position and assembles the edge.
    fn build_node(scope: &Scope, payload: &Self::ScanItem) -> Result<Self::Item, RpcError>;

    /// The filter-matching edges in one live checkpoint, in delivery order.
    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        package_store: &Arc<StreamingPackageStore>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &Self::Filter,
    ) -> Result<Vec<Edge<String, Self::Item, EmptyFields>>, RpcError>;
}

/// One backfill item: a matching `edge`, or a coverage marker (`edge: None`) whose `checkpoint` is
/// the fully-scanned frontier.
struct Scanned<S: Subscribable> {
    checkpoint: u64,
    edge: Option<Edge<String, S::Item, EmptyFields>>,
}

/// Subscribe to items matching `filter`: backfill from `resume` toward the tip, then follow live.
/// The handoff is pinned mid-scan, once the scan frontier comes within `handoff_threshold` of the
/// tip, so even a deep backfill catches up within one connection instead of lagging the receiver.
pub(super) fn subscribe<S: Subscribable>(
    reader: AlphaLedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    after: Option<S::Cursor>,
    after_checkpoint: Option<u64>,
    scan_page_size: usize,
    handoff_threshold: u64,
) -> impl Stream<Item = Result<Edge<String, S::Item, EmptyFields>, RpcError>> {
    stream! {
        let mut pending_receiver = None;
        let mut handoff: Option<u64> = None;
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it. `after` and/or
        // `afterCheckpoint` resume the backfill; neither means live-only.
        if after.is_some() || after_checkpoint.is_some() {
            let checkpoint_lo = after_checkpoint.map_or(0, |cp| cp.saturating_add(1));
            let scan = backfill::<S>(
                reader,
                package_store.clone(),
                resolver_limits.clone(),
                watermarks_rx,
                filter.clone(),
                after,
                checkpoint_lo,
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
        for await edge in live::<S>(receiver, last_checkpoint, package_store, resolver_limits, filter) {
            yield edge;
        }
    }
}

/// Scan matches from the resume point toward the tip, yielding each match and a per-page coverage
/// marker. Open-ended: the caller stops it once the handoff is covered.
fn backfill<S: Subscribable>(
    reader: AlphaLedgerGrpcReader,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    mut after: Option<S::Cursor>,
    checkpoint_lo: u64,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Scanned<S>, RpcError>> {
    stream! {
        // Finalized, indexed data: fields resolve lazily through the index. cvat is None (uniform
        // with live), so the scan range is supplied explicitly rather than derived from the scope.
        let scope = Scope::for_backfilled_transactions(package_store, resolver_limits);
        let limits = PageLimits {
            default: scan_page_size as u32,
            max: scan_page_size as u32,
        };

        // The scan applies both resume channels together: `after` (the precise `options.after`
        // position) and the checkpoint window `[checkpoint_lo, ..]`, so the effective start is
        // whichever is later. Open above: the caller stops the scan once the handoff is covered.
        let cp_bounds = checkpoint_lo..=u64::MAX;

        loop {
            let (items, next_after) = match scan_page::<S>(
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

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching items. Drops the
/// one-checkpoint seam overlap (the resubscribe/tip race), gap-checks, and disconnects on anomalies.
fn live<S: Subscribable>(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    package_store: Arc<StreamingPackageStore>,
    resolver_limits: sui_package_resolver::Limits,
    filter: S::Filter,
) -> impl Stream<Item = Result<Edge<String, S::Item, EmptyFields>, RpcError>> {
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
                    // Deliver each matching item as its own payload, ordered within the checkpoint.
                    // Empty checkpoints yield nothing.
                    let edges = S::matching_edges(&checkpoint, &package_store, &resolver_limits, &filter)?;
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
async fn scan_page<S: Subscribable>(
    reader: &AlphaLedgerGrpcReader,
    scope: &Scope,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> Result<(Vec<Scanned<S>>, S::Cursor), RpcError> {
    loop {
        let result = scan_with_retry::<S>(reader, limits, cp_bounds.clone(), after, filter).await?;

        // No end cursor means nothing past the indexer tip is indexed yet; wait, then re-request. A
        // range that was scanned but matched nothing still returns a boundary cursor, not `None`.
        let Some(end_cursor) = result.last_cursor() else {
            tokio::time::sleep(BACKFILL_POLL_INTERVAL).await;
            continue;
        };
        let end_cursor = CursorToken::decode(end_cursor).context("Failed to decode scan cursor")?;

        // Hold the page until both pipelines have indexed through its end.
        wait_for_pipelines_catching_up_at(end_cursor.position.checkpoint(), watermarks_rx).await?;

        // A forward page keeps `items` in ascending order, one delivered edge each. The decoded
        // token yields both the checkpoint and the edge cursor, so it is only decoded once.
        let mut items = Vec::with_capacity(result.items.len() + 1);
        for item in &result.items {
            let token =
                CursorToken::decode(&item.cursor).context("Failed to decode scan item cursor")?;
            let checkpoint = token.position.checkpoint();
            let cursor = S::Cursor::try_from(token)
                .context("Unexpected position in scan item cursor")?
                .encode_cursor();
            items.push(Scanned {
                checkpoint,
                edge: Some(Edge::new(cursor, S::build_node(scope, &item.payload)?)),
            });
        }
        // Coverage marker at the fully-scanned end: advances the handoff even on a match-less page.
        items.push(Scanned {
            checkpoint: covered_checkpoint(&end_cursor),
            edge: None,
        });

        let next_after =
            S::Cursor::try_from(end_cursor).context("Unexpected position in scan end cursor")?;
        return Ok((items, next_after));
    }
}

/// Scan one page over `cp_bounds`, retrying transient failures under a bounded budget.
async fn scan_with_retry<S: Subscribable>(
    reader: &AlphaLedgerGrpcReader,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
) -> Result<StreamPage<S::ScanItem>, RpcError> {
    let page = Page::from_params(
        limits,
        Some(limits.default as u64),
        after.cloned(),
        None,
        None,
    )
    .map_err(anyhow::Error::from)?;

    backoff::future::retry(scan_backoff(), || async {
        S::scan(reader, cp_bounds.clone(), &page, filter)
            .await
            .map_err(|e| {
                warn!(error = ?e, "scan failed, retrying");
                backoff::Error::transient(e)
            })
    })
    .await
}
