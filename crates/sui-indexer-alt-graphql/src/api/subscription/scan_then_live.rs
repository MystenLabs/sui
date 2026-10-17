// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scan-then-live subscription driver: stream the items matching a filter, in checkpoint order,
//! resuming from a cursor or checkpoint, in two phases:
//!
//! 1. Backfill ([`backfill`]): scan indexed data forward from the resume point to catch up, using
//!    the same pagination path.
//! 2. Live ([`live`]): read matching items from the checkpoint broadcast.
//!
//! Once the scan comes within `handoff_threshold` of the tip, the driver subscribes to the broadcast,
//! so reading is handed off to the live phase at `handoff + 1` with no gap or duplicate. A coverage
//! marker at each page's end lets the handoff advance even through stretches that match nothing.

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
use sui_rpc_cursor::CursorToken;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::debug;
use tracing::warn;

use crate::config::SubscriptionConfig;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::has_placeholder_checkpoint;
use crate::error::out_of_range;
use crate::error::resolve_ledger_out_of_range;
use crate::error::upcast;
use crate::pagination::Page;
use crate::pagination::PageLimits;
use crate::task::streaming::CheckpointBroadcaster;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamedCaches;
use crate::task::streaming::SubscriberLimit;
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::streaming::SubscriptionLifecycleGuard;
use crate::task::streaming::SubscriptionTerminationReason;
use crate::task::streaming::broadcast_error;
use crate::task::streaming::reconnect_error;
use crate::task::streaming::wait_for_pipelines_catching_up_at;

use super::Error;
use crate::task::watermark::Watermarks;

/// How long to wait before re-requesting when the scan has drained the indexer's current tip but not
/// yet reached the handoff. The wait doubles while the ledger makes no progress, up to
/// `BACKFILL_POLL_MAX_INTERVAL`.
const BACKFILL_POLL_INTERVAL: Duration = Duration::from_millis(100);
const BACKFILL_POLL_MAX_INTERVAL: Duration = Duration::from_secs(2);

/// How long the backfill waits for the ledger to make progress before asking the client to
/// reconnect, so a stalled ledger does not hold subscription slots indefinitely. Generous enough
/// for the chain to reach a resume point up to `max_start_checkpoints_ahead_of_tip` ahead.
const BACKFILL_POLL_DEADLINE: Duration = Duration::from_secs(180);

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

    /// Build the GraphQL node from one scanned item's payload, constructing whatever scope the feed
    /// resolves against. The driver mints the cursor from the item's position and assembles the edge.
    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        payload: &Self::ScanItem,
    ) -> Result<Self::Item, RpcError>;

    /// The filter-matching edges in one live checkpoint, in delivery order.
    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &Self::Filter,
    ) -> Result<Vec<Edge<String, Self::Item, EmptyFields>>, RpcError>;

    /// The feed's name, used to label its lifecycle metrics.
    fn subscription_type() -> &'static str;
}

/// One backfill scan output: either a matching edge, or a coverage marker whose `checkpoint` is the
/// fully-scanned frontier (advances the handoff even on a match-less page).
enum Scanned<I: OutputType> {
    Match {
        checkpoint: u64,
        edge: Edge<String, I, EmptyFields>,
    },
    Covered {
        checkpoint: u64,
    },
}

/// Where the backfill scan hands off to the live broadcast. The scan delivers every match at or
/// below the pinned handoff; live delivers everything above it.
struct Handoff {
    /// Pin once the scan frontier is within this many checkpoints of the streamed tip.
    threshold: u64,
    /// The checkpoint the client resumed after (from `after` and/or `afterCheckpoint`).
    resume_from: Option<u64>,
    /// The `after` cursor's checkpoint is a placeholder (see [`has_placeholder_checkpoint`]), so
    /// `resume_from` is raised to the first scan output, which the ledger only returns once it is
    /// past the cursor.
    resume_unknown: bool,
    pinned: Option<u64>,
}

/// What the backfill does with one scan output, as decided by [`Handoff::step`].
enum ScanStep<I: OutputType> {
    /// Deliver this match.
    Deliver(Edge<String, I, EmptyFields>),
    /// Keep scanning.
    Continue,
    /// The scan has covered the handoff; live takes over.
    Done,
}

impl<I: OutputType> Scanned<I> {
    /// The checkpoint this output sits at (the match's, or the covered frontier).
    fn checkpoint(&self) -> u64 {
        match self {
            Scanned::Match { checkpoint, .. } | Scanned::Covered { checkpoint } => *checkpoint,
        }
    }
}

impl Handoff {
    fn new(threshold: u64, resume_from: Option<u64>) -> Self {
        Self {
            threshold,
            resume_from,
            resume_unknown: false,
            pinned: None,
        }
    }

    /// Take the resume point from the first scan output, for an `after` cursor whose checkpoint is
    /// a placeholder.
    fn resume_from_first_output(mut self) -> Self {
        self.resume_unknown = true;
        self
    }

    /// Decide what to do with one scan output, given this replica's streamed `tip`.
    ///
    /// Pins first if the scan frontier is within the threshold of `tip`. `resubscribe` must
    /// subscribe the live receiver and then return the streamed tip, so live sees every checkpoint
    /// above the pin. A match past the handoff belongs to live, so the scan stops there.
    fn step<I: OutputType>(
        &mut self,
        scanned: Scanned<I>,
        tip: u64,
        resubscribe: impl FnOnce() -> u64,
    ) -> ScanStep<I> {
        if std::mem::take(&mut self.resume_unknown) {
            // The first output is past the cursor: a match sits in the cursor's checkpoint or a
            // later one with no match in between, and a coverage marker's page ends in the
            // checkpoint after the one it covers. Either way the scan stays responsible for it.
            let floor = match &scanned {
                Scanned::Match { checkpoint, .. } => *checkpoint,
                Scanned::Covered { checkpoint } => checkpoint.saturating_add(1),
            };
            self.resume_from = Some(self.resume_from.map_or(floor, |from| from.max(floor)));
        }

        if self.pinned.is_none() && tip.saturating_sub(scanned.checkpoint()) <= self.threshold {
            self.pin(resubscribe());
        }

        match scanned {
            Scanned::Match { checkpoint, edge } => {
                if self.pinned.is_none_or(|h| checkpoint <= h) {
                    ScanStep::Deliver(edge)
                } else {
                    ScanStep::Done
                }
            }
            Scanned::Covered { checkpoint } => {
                if self.pinned.is_some_and(|h| checkpoint >= h) {
                    ScanStep::Done
                } else {
                    ScanStep::Continue
                }
            }
        }
    }

    /// Pin the handoff at the streamed `tip` (read after resubscribing), floored at the resume
    /// point, which can be ahead of this replica's tip behind a load balancer. Flooring here keeps
    /// the scan responsible for the matches after an `after` cursor in its own checkpoint.
    fn pin(&mut self, tip: u64) {
        self.pinned = Some(match self.resume_from {
            Some(from) => tip.max(from),
            None => tip,
        });
    }

    /// The last checkpoint the scan was responsible for; live delivers from the next one.
    fn pinned(&self) -> Option<u64> {
        self.pinned
    }
}

/// Subscribe to items matching `filter`: backfill from `resume` toward the tip, then follow live.
/// The handoff is pinned mid-scan, once the scan frontier comes within `handoff_threshold` of the
/// tip, so even a deep backfill catches up within one connection instead of lagging the receiver.
#[allow(clippy::type_complexity)]
pub(super) fn subscribe<S: Subscribable>(
    reader: AlphaLedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    after: Option<S::Cursor>,
    after_checkpoint: Option<u64>,
    subscriber_limit: SubscriberLimit,
    config: SubscriptionConfig,
) -> Result<impl Stream<Item = Result<Edge<String, S::Item, EmptyFields>, RpcError>>, RpcError<Error>>
where
    for<'a> CursorToken: From<&'a S::Cursor>,
{
    // The checkpoint to resume after: the later of the `after` cursor and `afterCheckpoint`. A
    // cursor's checkpoint is read through its `CursorToken` position, so the driver needs no
    // per-feed accessor.
    let start_from = match (
        after
            .as_ref()
            .map(|c| CursorToken::from(c).position.checkpoint()),
        after_checkpoint,
    ) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    // Reject a far-ahead resume before admitting, so bad input never claims a slot or lands in the
    // subscription lifecycle metrics.
    reject_if_start_too_far_ahead(start_from, &broadcast, &config)?;

    // Claim a concurrency slot for the subscription's lifetime; at capacity this is `Err` with no
    // guard created (and so no lifecycle metric).
    let guard = SubscriptionLifecycleGuard::new(
        S::subscription_type(),
        broadcast.metrics(),
        &subscriber_limit,
    )
    .map_err(upcast)?;

    // Size the backfill scan page to the resolve concurrency. Scans are sequential (each needs the
    // previous page's cursor), so feeding one window of `n` concurrent resolutions takes ceil(n /
    // page) scans: a page much smaller than the concurrency makes scanning the bottleneck, a much
    // larger one just holds a bigger page in memory. Matching them is roughly one scan per window.
    let scan_page_size = config.max_concurrent_resolutions;

    // Pin the handoff once the scan comes within half the live buffer of the tip, leaving room for
    // checkpoints that arrive during the handoff so the receiver does not lag.
    let handoff_threshold = config.broadcast_buffer as u64 / 2;

    Ok(stream! {
        let mut pending_receiver = None;
        let mut handoff = Handoff::new(handoff_threshold, start_from);
        if after.as_ref().is_some_and(|c| has_placeholder_checkpoint(&CursorToken::from(c))) {
            handoff = handoff.resume_from_first_output();
        }
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it. `after` and/or
        // `afterCheckpoint` resume the backfill; neither means live-only.
        if after.is_some() || after_checkpoint.is_some() {
            let checkpoint_lo = after_checkpoint.map_or(0, |cp| cp.saturating_add(1));
            let scan = backfill::<S>(
                reader,
                caches.clone(),
                resolver_limits.clone(),
                watermarks_rx,
                filter.clone(),
                after,
                checkpoint_lo,
                scan_page_size,
            );
            let mut scanned_any = false;
            for await scanned in scan {
                let scanned = match scanned {
                    Ok(scanned) => scanned,
                    // Resuming below retained history is the client's request, not a backfill
                    // fault, so it is counted apart from backfill errors.
                    Err(RpcError::OutOfRange(detail)) => {
                        guard.terminate(SubscriptionTerminationReason::OutOfRange);
                        yield Err(if scanned_any {
                            pruned_during_backfill(&detail)
                        } else {
                            RpcError::OutOfRange(detail)
                        });
                        return;
                    }
                    Err(e) => {
                        guard.terminate(SubscriptionTerminationReason::BackfillError);
                        yield Err(e);
                        return;
                    }
                };
                scanned_any = true;

                let step = handoff.step(scanned, broadcast.network_tip(), || {
                    pending_receiver = Some(broadcast.broadcaster().resubscribe());
                    broadcast.network_tip()
                });
                match step {
                    ScanStep::Deliver(edge) => {
                        yield Ok(edge);
                        guard.record_backfill_delivered();
                    }
                    ScanStep::Continue => {}
                    ScanStep::Done => break,
                }
            }
            last_checkpoint = handoff.pinned();
        }

        // Phase 2: follow live from `handoff + 1` (a fresh receiver if there was no backfill).
        let receiver = pending_receiver.unwrap_or_else(|| broadcast.broadcaster().resubscribe());
        for await edge in
            live::<S>(receiver, last_checkpoint, caches, resolver_limits, filter, guard)
        {
            yield edge;
        }
    })
}

/// The error for a backfill that started within retained history but fell below it while catching
/// up, because the server pruned what it was about to scan. The cursors already delivered are at
/// or below the pruned range too, so the client has to pick a new start point.
fn pruned_during_backfill(detail: &str) -> RpcError {
    out_of_range(format!(
        "{detail}. History was pruned while the subscription was catching up, so its cursors can \
         no longer be resumed from; resubscribe with `afterCheckpoint` at or above the lowest \
         available checkpoint"
    ))
}

/// Reject a start point sitting more than `max_ahead` checkpoints past the chain tip. There is
/// nothing to backfill ahead of the tip, so such a request would only wait for the chain to reach
/// it, and a far-future one would hold the connection open indefinitely.
fn reject_if_start_too_far_ahead(
    start_from: Option<u64>,
    broadcast: &SubscriptionBroadcast,
    config: &SubscriptionConfig,
) -> Result<(), RpcError<Error>> {
    let max_ahead = config.max_start_checkpoints_ahead_of_tip;
    if let Some(start) = start_from
        && start > broadcast.network_tip().saturating_add(max_ahead)
    {
        return Err(bad_user_input(Error::TooFarAheadOfTip { max: max_ahead }));
    }
    Ok(())
}

/// Scan matches from the resume point toward the tip, yielding each match and a per-page coverage
/// marker. Open-ended: the caller stops it once the handoff is covered.
fn backfill<S: Subscribable>(
    reader: AlphaLedgerGrpcReader,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    mut after: Option<S::Cursor>,
    checkpoint_lo: u64,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Scanned<S::Item>, RpcError>>
where
    for<'a> CursorToken: From<&'a S::Cursor>,
{
    stream! {
        let limits = PageLimits {
            default: scan_page_size as u32,
            max: scan_page_size as u32,
        };

        // Resume from whichever is later: the `after` cursor or `[checkpoint_lo, ..]`. The upper
        // bound is `u64::MAX` because backfill is open-ended, chasing the moving tip; `subscribe`
        // stops it once the handoff is covered.
        let cp_bounds = checkpoint_lo..=u64::MAX;

        loop {
            let (items, next_after) = match scan_page::<S>(
                &reader,
                &caches,
                &resolver_limits,
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

/// The highest checkpoint fully scanned as of `token`, or `None` if no whole checkpoint is covered.
///
/// Only checkpoints below the cursor's are covered: an `Item` cursor's checkpoint may hold later
/// matches, and a list call that drains ledger tip `T` ends with a `Boundary` cursor at `T + 1`.
fn covered_checkpoint(token: &CursorToken) -> Option<u64> {
    token.position.checkpoint().checked_sub(1)
}

/// Whether a page ending at `end` moved the scan past `after`. A page that drains the ledger's tip
/// ends where it started, echoing `after`, so the scan waits before asking again. The ledger
/// rejects an `after` ahead of its indexed tip as out of range rather than answering from its own
/// tip (#28308), so a page never ends behind `after`.
fn made_progress(end: &CursorToken, after: Option<&CursorToken>) -> bool {
    after != Some(end)
}

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching items. Drops the
/// one-checkpoint seam overlap (the resubscribe/tip race), gap-checks, and disconnects on anomalies.
fn live<S: Subscribable>(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    filter: S::Filter,
    guard: SubscriptionLifecycleGuard,
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
                            guard.terminate(SubscriptionTerminationReason::UnexpectedGap);
                            yield Err(reconnect_error());
                            return;
                        }
                    }
                    // Deliver each matching item as its own payload, ordered within the checkpoint.
                    // Empty checkpoints yield nothing.
                    let edges = match S::matching_edges(&checkpoint, &caches, &resolver_limits, &filter) {
                        Ok(edges) => edges,
                        Err(e) => {
                            guard.terminate(SubscriptionTerminationReason::Error);
                            yield Err(e);
                            return;
                        }
                    };
                    for edge in edges {
                        yield Ok(edge);
                        guard.record_delivered(checkpoint.summary.timestamp_ms);
                    }
                    last_checkpoint = Some(seq);
                    delivered_live = true;
                }
                // A lag before the first live checkpoint is catch-up overflow (likely kv-rpc lag).
                Err(broadcast::error::RecvError::Lagged(missed)) if !delivered_live => {
                    warn!(missed, "Subscriber fell behind during catch-up; disconnecting");
                    guard.terminate(SubscriptionTerminationReason::Lagged);
                    yield Err(reconnect_error());
                    return;
                }
                Err(e) => {
                    guard.terminate(SubscriptionTerminationReason::from_recv_error(&e));
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

/// Paces re-requests while the ledger has nothing past the scan's position: waits double from
/// `BACKFILL_POLL_INTERVAL` up to `BACKFILL_POLL_MAX_INTERVAL`, and once `BACKFILL_POLL_DEADLINE`
/// has passed without progress the client is asked to reconnect.
struct LedgerPoll {
    interval: Duration,
    deadline: tokio::time::Instant,
}

impl LedgerPoll {
    fn new() -> Self {
        Self {
            interval: BACKFILL_POLL_INTERVAL,
            deadline: tokio::time::Instant::now() + BACKFILL_POLL_DEADLINE,
        }
    }

    async fn wait(&mut self) -> Result<(), RpcError> {
        if tokio::time::Instant::now() >= self.deadline {
            warn!("Ledger made no progress within the backfill deadline; disconnecting");
            return Err(reconnect_error());
        }
        tokio::time::sleep(self.interval).await;
        self.interval = (self.interval * 2).min(BACKFILL_POLL_MAX_INTERVAL);
        Ok(())
    }
}

/// Scan the next page of matches, waiting at the indexer tip if nothing is ready yet. Returns the
/// matches, a trailing coverage marker, and the cursor to resume from.
async fn scan_page<S: Subscribable>(
    reader: &AlphaLedgerGrpcReader,
    caches: &Arc<StreamedCaches>,
    resolver_limits: &sui_package_resolver::Limits,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> Result<(Vec<Scanned<S::Item>>, S::Cursor), RpcError>
where
    for<'a> CursorToken: From<&'a S::Cursor>,
{
    let mut poll = LedgerPoll::new();
    loop {
        let ledger_floor = watermarks_rx.borrow().ledger_floor();
        let result = scan_with_retry::<S>(
            reader,
            limits,
            cp_bounds.clone(),
            after,
            filter,
            ledger_floor,
        )
        .await?;

        // No end cursor means nothing past the indexer tip is indexed yet; wait, then re-request. A
        // range that was scanned but matched nothing still returns a boundary cursor, not `None`.
        let Some(end_cursor) = result.last_cursor() else {
            poll.wait().await?;
            continue;
        };
        let end_cursor = CursorToken::decode(end_cursor).context("Failed to decode scan cursor")?;

        // A page that drained the ledger's tip without moving: wait for the ledger to index more
        // before asking again, rather than re-polling at full speed.
        if !made_progress(&end_cursor, after.map(CursorToken::from).as_ref()) {
            poll.wait().await?;
            continue;
        }

        // Hold the page until all pipelines have indexed through its end.
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
            items.push(Scanned::Match {
                checkpoint,
                edge: Edge::new(
                    cursor,
                    S::build_node(caches, resolver_limits, &item.payload)?,
                ),
            });
        }
        // Coverage marker at the fully-scanned end: advances the handoff even on a match-less page.
        if let Some(checkpoint) = covered_checkpoint(&end_cursor) {
            items.push(Scanned::Covered { checkpoint });
        }

        let next_after =
            S::Cursor::try_from(end_cursor).context("Unexpected position in scan end cursor")?;
        return Ok((items, next_after));
    }
}

/// Scan one page over `cp_bounds`, retrying transient failures under a bounded budget.
///
/// The ledger answers `OUT_OF_RANGE` in two directions, told apart against its retention floor
/// (`ledger_floor`, 0 if unknown) by [`classify_scan_error`]. A start below the floor asked for
/// pruned history and fails at once. Behind a load balancer, fullnodes can have different floors,
/// so another replica might still serve the range; it is not retried anyway, because the budget is
/// tuned for brief outages and spending it on a pruned range would delay a client that has to
/// recover from a newer cursor. Anything else (a cursor ahead of the ledger's indexed tip) is the
/// ledger catching up, retried like other transient failures; if the budget runs out first, the
/// client is asked to reconnect rather than told its history is gone.
async fn scan_with_retry<S: Subscribable>(
    reader: &AlphaLedgerGrpcReader,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
    ledger_floor: u64,
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
            .map_err(|e| classify_scan_error(e, ledger_floor))
    })
    .await
    .map_err(|e| match e {
        RpcError::LedgerOutOfRange { .. } => {
            warn!(error = ?e, "ledger did not catch up with the resume point; disconnecting");
            reconnect_error()
        }
        e => e,
    })
}

/// Whether a failed scan is worth retrying: everything is, except a request below the ledger's
/// retention floor.
fn classify_scan_error(err: RpcError, ledger_floor: u64) -> backoff::Error<RpcError> {
    match resolve_ledger_out_of_range(err, ledger_floor) {
        e @ RpcError::OutOfRange(_) => backoff::Error::permanent(e),
        e @ RpcError::LedgerOutOfRange { .. } => {
            debug!(error = ?e, "ledger is behind the scan's resume point, retrying");
            backoff::Error::transient(e)
        }
        e => {
            warn!(error = ?e, "scan failed, retrying");
            backoff::Error::transient(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::Instant;

    use futures::StreamExt;
    use prometheus::Registry;
    use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
    use sui_indexer_alt_reader::package_resolver::DbPackageStore;
    use sui_indexer_alt_reader::package_resolver::PackageCache;
    use sui_indexer_alt_reader::pg_reader::PgReader;
    use sui_indexer_alt_reader::pg_reader::db::DbArgs;
    use sui_rpc_cursor::CursorKind;
    use sui_rpc_cursor::Position;

    use crate::api::types::checkpoint::CCheckpoint;
    use crate::config::Limits;
    use crate::error::code;
    use crate::error::out_of_range;
    use crate::metrics::SubscriptionMetrics;
    use crate::task::streaming::StreamedObjectStore;
    use crate::task::streaming::StreamedTransactionStore;
    use crate::task::streaming::StreamingPackageStore;

    use super::*;

    /// A feed whose scans replay a script instead of calling the reader, so retries are observable.
    struct Scripted;

    /// One scan output, reduced to what the handoff decisions look at.
    #[derive(Clone, Debug)]
    enum Out {
        Match(u64),
        /// A page's end cursor; its coverage goes through `covered_checkpoint`, as in `scan_page`.
        End(CursorToken),
    }

    /// Scan results to hand out in order, one per `scan` call.
    type Script = Arc<Mutex<VecDeque<Result<StreamPage<()>, RpcError>>>>;

    impl Subscribable for Scripted {
        type Item = u64;
        type Cursor = CCheckpoint;
        type Filter = Script;
        type ScanItem = ();

        fn scan<'a>(
            _reader: &'a AlphaLedgerGrpcReader,
            _cp_bounds: RangeInclusive<u64>,
            _page: &'a Page<CCheckpoint>,
            script: &'a Script,
        ) -> impl Future<Output = Result<StreamPage<()>, RpcError>> + Send + 'a {
            let next = script.lock().unwrap().pop_front();
            async move { next.expect("scan called more times than scripted") }
        }

        fn build_node(
            _: &Arc<StreamedCaches>,
            _: &sui_package_resolver::Limits,
            _: &(),
        ) -> Result<u64, RpcError> {
            unreachable!("only scans are exercised")
        }

        fn matching_edges(
            _: &Arc<ProcessedCheckpoint>,
            _: &Arc<StreamedCaches>,
            _: &sui_package_resolver::Limits,
            _: &Script,
        ) -> Result<Vec<Edge<String, u64, EmptyFields>>, RpcError> {
            unreachable!("only scans are exercised")
        }

        fn subscription_type() -> &'static str {
            "scripted"
        }
    }

    /// A reader that is never called: `Scripted` answers every scan itself.
    async fn unused_reader() -> AlphaLedgerGrpcReader {
        let args = LedgerGrpcArgs {
            ledger_grpc_statement_timeout_ms: None,
            ledger_grpc_max_decoding_message_size: 1024,
            ledger_grpc_num_connections: 1,
        };
        AlphaLedgerGrpcReader::new(
            "http://localhost:1".parse().unwrap(),
            args,
            None,
            &Registry::new(),
        )
        .await
        .unwrap()
    }

    async fn scan_scripted(
        script: Vec<Result<StreamPage<()>, RpcError>>,
    ) -> (Result<StreamPage<()>, RpcError>, usize) {
        let script: Script = Arc::new(Mutex::new(script.into()));
        let limits = PageLimits {
            default: 10,
            max: 10,
        };
        let reader = unused_reader().await;
        let result =
            scan_with_retry::<Scripted>(&reader, &limits, 0..=u64::MAX, None, &script, 42).await;
        let unused = script.lock().unwrap().len();
        (result, unused)
    }

    fn boundary(checkpoint: u64) -> Out {
        Out::End(CursorToken::boundary(Position::Checkpoints { checkpoint }))
    }

    fn tx_boundary(checkpoint: u64, tx_seq: u64) -> Out {
        Out::End(CursorToken::boundary(Position::Transactions {
            checkpoint,
            tx_seq,
        }))
    }

    fn tx_item(checkpoint: u64, tx_seq: u64) -> Out {
        Out::End(CursorToken::item(Position::Transactions {
            checkpoint,
            tx_seq,
        }))
    }

    /// Runs scan outputs through [`Handoff::step`] against a fixed streamed `tip`, as the backfill
    /// loop in `subscribe` does. Returns the delivered match checkpoints and the checkpoint live
    /// starts after (`None` if still scanning).
    fn drive(
        threshold: u64,
        resume_from: Option<u64>,
        tip: u64,
        outputs: &[Out],
    ) -> (Vec<u64>, Option<u64>) {
        drive_handoff(Handoff::new(threshold, resume_from), tip, outputs)
    }

    /// [`drive`] with a prepared `handoff`.
    fn drive_handoff(mut handoff: Handoff, tip: u64, outputs: &[Out]) -> (Vec<u64>, Option<u64>) {
        let mut delivered = vec![];
        for out in outputs {
            // Matches carry their checkpoint as the node, so delivery order is observable.
            let scanned = match out {
                Out::Match(checkpoint) => Scanned::Match {
                    checkpoint: *checkpoint,
                    edge: Edge::new(String::new(), *checkpoint),
                },
                Out::End(token) => match covered_checkpoint(token) {
                    Some(checkpoint) => Scanned::Covered { checkpoint },
                    None => continue,
                },
            };
            match handoff.step(scanned, tip, || tip) {
                ScanStep::Deliver(edge) => delivered.push(edge.node),
                ScanStep::Continue => {}
                ScanStep::Done => return (delivered, handoff.pinned()),
            }
        }
        (delivered, None)
    }

    /// A start below the reader's retained history fails on the first scan with a user error,
    /// instead of retrying for the whole backoff budget and then failing as an internal error.
    #[tokio::test]
    async fn out_of_range_scan_fails_without_retrying() {
        let start = Instant::now();
        let (result, unused) = scan_scripted(vec![
            Err(out_of_range("lowest available checkpoint is 42")),
            Ok(StreamPage::default()),
        ])
        .await;

        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(unused, 1, "the out-of-range scan must not be retried");

        let err = async_graphql::Error::from(result.unwrap_err());
        assert_eq!(
            err.message,
            "Requested range is not available: lowest available checkpoint is 42"
        );
        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::OUT_OF_RANGE.into()));
    }

    /// Run `subscribe` over the scripted feed, resuming after `after_checkpoint`, and collect
    /// everything it yields. The streamed tip is 100.
    async fn subscribe_scripted(
        script: Vec<Result<StreamPage<()>, RpcError>>,
        after_checkpoint: Option<u64>,
    ) -> (
        Vec<Result<Edge<String, u64, EmptyFields>, RpcError>>,
        Arc<SubscriptionMetrics>,
    ) {
        let metrics = SubscriptionMetrics::new_for_test();
        let (_tx, rx) = broadcast::channel(16);
        let broadcast = Arc::new(SubscriptionBroadcast::new(rx, 101, metrics.clone()));

        // No database: the scripted feed never resolves a payload.
        let pg_reader = PgReader::new(None, None, DbArgs::default(), &Registry::new())
            .await
            .unwrap();
        let packages = PackageCache::new(DbPackageStore::new(Arc::new(pg_reader.as_data_loader())));
        let caches = Arc::new(StreamedCaches::new(
            Arc::new(StreamingPackageStore::new(Arc::new(packages))),
            Arc::new(StreamedTransactionStore::new()),
            Arc::new(StreamedObjectStore::new()),
        ));
        let (_watermarks_tx, watermarks_rx) =
            watch::channel(Arc::new(Watermarks::for_test(&[("pipeline", 1_000_000)])));

        let script: Script = Arc::new(Mutex::new(script.into()));
        let stream = subscribe::<Scripted>(
            unused_reader().await,
            broadcast,
            caches,
            Limits::default().package_resolver(),
            watermarks_rx,
            script.clone(),
            None,
            after_checkpoint,
            SubscriberLimit::new(1),
            SubscriptionConfig::default(),
        )
        .unwrap_or_else(|_| panic!("subscription should be admitted"));
        let items = stream.collect().await;
        assert!(
            script.lock().unwrap().is_empty(),
            "the script was not used up"
        );
        (items, metrics)
    }

    fn terminations(metrics: &SubscriptionMetrics, reason: &str) -> u64 {
        metrics
            .subscription_terminations
            .with_label_values(&["scripted", reason])
            .get()
    }

    /// A resume below the ledger's retained history ends the subscription with the out-of-range
    /// error before any edge, and counts it as `out_of_range` rather than as a backfill error. The
    /// scan is not retried: the script has no second result.
    #[tokio::test]
    async fn out_of_range_resume_fails_before_any_edge() {
        let script = vec![Err(out_of_range("lowest available checkpoint is 42"))];
        let (items, metrics) = subscribe_scripted(script, Some(5)).await;

        let [Err(err)] = <[_; 1]>::try_from(items).unwrap_or_else(|items| {
            panic!("expected one error and no edges, got {} items", items.len())
        }) else {
            panic!("expected the error before any edge");
        };
        let err = async_graphql::Error::from(err);
        assert_eq!(
            err.message,
            "Requested range is not available: lowest available checkpoint is 42"
        );
        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::OUT_OF_RANGE.into()));

        assert_eq!(terminations(&metrics, "out_of_range"), 1);
        assert_eq!(terminations(&metrics, "backfill_error"), 0);
    }

    /// History pruned under a backfill that has already made progress gets an error that says the
    /// client's cursors are stale, not just that the range is unavailable.
    #[tokio::test]
    async fn pruned_mid_backfill_asks_for_a_new_start() {
        let script = vec![
            Ok(StreamPage::for_test(
                vec![],
                None,
                Some(CursorToken::boundary(Position::Checkpoints { checkpoint: 8 }).encode()),
                None,
            )),
            Err(out_of_range("lowest available checkpoint is 42")),
        ];
        let (items, metrics) = subscribe_scripted(script, Some(5)).await;

        let [Err(err)] = <[_; 1]>::try_from(items).unwrap_or_else(|items| {
            panic!("expected one error and no edges, got {} items", items.len())
        }) else {
            panic!("expected only the error");
        };
        let err = async_graphql::Error::from(err);
        assert!(
            err.message.starts_with(
                "Requested range is not available: lowest available checkpoint is 42. History \
                 was pruned while the subscription was catching up"
            ),
            "{}",
            err.message
        );
        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::OUT_OF_RANGE.into()));
        assert_eq!(terminations(&metrics, "out_of_range"), 1);
    }

    fn ledger_out_of_range(requested_lo: Option<u64>) -> RpcError {
        RpcError::LedgerOutOfRange {
            detail: "requested data above latest available".into(),
            requested_lo,
        }
    }

    /// The ledger rejects a resume point ahead of its tip with `OUT_OF_RANGE` too. At or above its
    /// floor (42 in `scan_scripted`) that is the ledger catching up, so the scan is retried.
    #[tokio::test]
    async fn ahead_of_ledger_scan_is_retried() {
        for requested_lo in [Some(42), Some(500), None] {
            let (result, unused) = scan_scripted(vec![
                Err(ledger_out_of_range(requested_lo)),
                Ok(StreamPage::default()),
            ])
            .await;
            assert!(result.is_ok(), "{requested_lo:?}");
            assert_eq!(unused, 0);
        }
    }

    /// The same error for a request below the floor is pruned history: not retried, and reported
    /// as `OUT_OF_RANGE`.
    #[tokio::test]
    async fn below_floor_ledger_scan_fails_without_retrying() {
        let (result, unused) = scan_scripted(vec![
            Err(ledger_out_of_range(Some(41))),
            Ok(StreamPage::default()),
        ])
        .await;
        assert_eq!(unused, 1, "a below-floor scan must not be retried");
        let err = async_graphql::Error::from(result.unwrap_err());
        let ext = err.extensions.as_ref().expect("No extensions");
        assert_eq!(ext.get("code"), Some(&code::OUT_OF_RANGE.into()));
    }

    /// Other scan failures are still treated as transient and retried.
    #[tokio::test]
    async fn transient_scan_failure_is_retried() {
        let (result, unused) = scan_scripted(vec![
            Err(anyhow::anyhow!("connection reset").into()),
            Ok(StreamPage::default()),
        ])
        .await;

        assert!(result.is_ok());
        assert_eq!(unused, 0);
    }

    #[test]
    fn covered_checkpoint_excludes_the_cursor_checkpoint() {
        // A list call that drains ledger tip 10 ends with a boundary cursor at 11.
        let terminal = CursorToken::boundary(Position::Checkpoints { checkpoint: 11 });
        assert_eq!(covered_checkpoint(&terminal), Some(10));

        let item = CursorToken::item(Position::Transactions {
            checkpoint: 11,
            tx_seq: 3,
        });
        assert_eq!(covered_checkpoint(&item), Some(10));

        let genesis = CursorToken::boundary(Position::Checkpoints { checkpoint: 0 });
        assert_eq!(covered_checkpoint(&genesis), None);
    }

    /// The streamed tip (11) is one checkpoint ahead of the index (10) when the handoff pins. The
    /// first page ends at the ledger tip with a boundary cursor at 11, which covers only 10, so the
    /// scan must keep going until the index has 11 and deliver it; live starts after 11.
    #[test]
    fn handoff_waits_for_the_index_to_reach_the_streamed_tip() {
        let outputs = [
            Out::Match(10),
            boundary(11),
            // Next page, once the index reaches 11.
            Out::Match(11),
            boundary(12),
        ];
        let (delivered, live_after) = drive(5, Some(9), 11, &outputs);
        assert_eq!(delivered, vec![10, 11]);
        assert_eq!(live_after, Some(11));
    }

    /// With nothing left to scan, a terminal page covers only up to the index tip, so a streamed tip
    /// ahead of it keeps the backfill open rather than skipping the checkpoints in between.
    #[test]
    fn empty_terminal_page_does_not_cover_the_streamed_tip() {
        let (delivered, live_after) = drive(5, Some(10), 11, &[boundary(11)]);
        assert!(delivered.is_empty());
        assert_eq!(live_after, None, "checkpoint 11 is not covered yet");
    }

    /// The client resumes after transaction (10, 5), but this replica has only streamed up to 9.
    /// The rest of checkpoint 10 belongs to the scan: live skips 10 as already seen.
    #[test]
    fn resume_ahead_of_streamed_tip_delivers_rest_of_cursor_checkpoint() {
        let outputs = [
            Out::Match(10),
            Out::Match(10),
            Out::Match(11),
            tx_item(11, 0),
        ];
        let (delivered, live_after) = drive(5, Some(10), 9, &outputs);
        assert_eq!(delivered, vec![10, 10]);
        assert_eq!(live_after, Some(10));
    }

    /// A Postgres-minted cursor for transaction (10, 5) records checkpoint 0, so the resume point
    /// is unknown up front. It is taken from the first scan output instead, which keeps the rest of
    /// checkpoint 10 with the scan when this replica has only streamed up to 9.
    #[test]
    fn placeholder_cursor_resumes_from_first_scan_output() {
        let placeholder = CursorToken::item(Position::Transactions {
            checkpoint: 0,
            tx_seq: 5,
        });
        assert!(has_placeholder_checkpoint(&placeholder));

        // Without the placeholder handling, the pin sits at the streamed tip and live would
        // re-deliver all of checkpoint 10.
        let outputs = [Out::Match(10), Out::Match(10), Out::Match(11)];
        let (delivered, live_after) = drive(5, Some(0), 9, &outputs);
        assert_eq!((delivered, live_after), (vec![], Some(9)));

        let handoff = Handoff::new(5, Some(0)).resume_from_first_output();
        let (delivered, live_after) = drive_handoff(handoff, 9, &outputs);
        assert_eq!(delivered, vec![10, 10]);
        assert_eq!(live_after, Some(10));

        // A first page with no match ends at a boundary in the cursor's checkpoint; the scan stays
        // responsible for that checkpoint too.
        let outputs = [tx_boundary(10, 6), Out::Match(10), Out::Match(11)];
        let handoff = Handoff::new(5, Some(0)).resume_from_first_output();
        let (delivered, live_after) = drive_handoff(handoff, 9, &outputs);
        assert_eq!(delivered, vec![10]);
        assert_eq!(live_after, Some(10));
    }

    /// Polls back off to the cap, and the client is asked to reconnect once the ledger has made no
    /// progress for the whole deadline.
    #[tokio::test(start_paused = true)]
    async fn ledger_poll_backs_off_then_gives_up() {
        let start = tokio::time::Instant::now();
        let mut poll = LedgerPoll::new();
        let mut waits = vec![];
        loop {
            let before = tokio::time::Instant::now();
            if poll.wait().await.is_err() {
                break;
            }
            waits.push(before.elapsed().as_millis());
        }

        assert_eq!(waits[..6], [100, 200, 400, 800, 1600, 2000]);
        assert!(waits[6..].iter().all(|w| *w == 2000));
        let elapsed = start.elapsed();
        assert!(elapsed >= BACKFILL_POLL_DEADLINE, "{elapsed:?}");
        assert!(
            elapsed < BACKFILL_POLL_DEADLINE + BACKFILL_POLL_MAX_INTERVAL,
            "{elapsed:?}"
        );
    }

    #[test]
    fn placeholder_checkpoint_needs_a_transaction() {
        let tx =
            |checkpoint, tx_seq| CursorToken::item(Position::Transactions { checkpoint, tx_seq });
        assert!(!has_placeholder_checkpoint(&tx(0, 0)));
        assert!(!has_placeholder_checkpoint(&tx(3, 5)));
        assert!(!has_placeholder_checkpoint(&CursorToken::item(
            Position::Checkpoints { checkpoint: 0 }
        )));
    }

    /// The caught-up case is unchanged: pin at the streamed tip once the frontier is within the
    /// threshold, deliver matches up to it, and hand off after it.
    #[test]
    fn caught_up_resume_hands_off_at_streamed_tip() {
        let outputs = [
            Out::Match(12),
            boundary(15),
            Out::Match(16),
            Out::Match(20),
            Out::Match(21),
        ];
        let (delivered, live_after) = drive(5, Some(10), 20, &outputs);
        assert_eq!(delivered, vec![12, 16, 20]);
        assert_eq!(live_after, Some(20));
    }

    fn cp(kind: CursorKind, checkpoint: u64) -> CursorToken {
        CursorToken {
            kind,
            position: Position::Checkpoints { checkpoint },
        }
    }

    fn tx(kind: CursorKind, checkpoint: u64, tx_seq: u64) -> CursorToken {
        CursorToken {
            kind,
            position: Position::Transactions { checkpoint, tx_seq },
        }
    }

    fn ev(kind: CursorKind, checkpoint: u64, tx_seq: u64, event_index: u32) -> CursorToken {
        CursorToken {
            kind,
            position: Position::Events {
                checkpoint,
                tx_seq,
                event_index,
            },
        }
    }

    use CursorKind::Boundary as B;
    use CursorKind::Item as I;

    /// An empty page that drains the tip echoes the `after` it was given; that is not progress, so
    /// the scan waits rather than re-requesting immediately.
    #[test]
    fn echoed_cursor_is_not_progress() {
        for token in [
            cp(B, 16),
            cp(I, 16),
            tx(B, 16, 3),
            tx(I, 16, 3),
            ev(I, 16, 3, 1),
        ] {
            assert!(!made_progress(&token, Some(&token)), "{token:?}");
        }
        // Any other end is progress: the next page starts from it.
        assert!(made_progress(&cp(B, 17), Some(&cp(B, 16))));
        assert!(made_progress(&tx(B, 17, 4), Some(&tx(I, 16, 3))));
        // With no `after`, the first page always counts.
        assert!(made_progress(&cp(B, 16), None));
    }
}
