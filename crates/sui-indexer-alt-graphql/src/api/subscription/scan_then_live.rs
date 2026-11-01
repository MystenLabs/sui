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
//! so reading is handed off to the live phase at `handoff + 1`. The coverage claim the ledger reports
//! on each page lets the handoff advance even through stretches that match nothing. Every payload
//! goes out only past the position the client's latest cursor resumes from, so the two phases leave
//! no gap and repeat nothing.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use async_graphql::OutputType;
use async_graphql::connection::CursorType;
use async_stream::stream;
use backoff::ExponentialBackoff;
use futures::Stream;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

use crate::api::types::ledger_cursor::scan::LedgerPage;
use crate::config::SubscriptionConfig;
use crate::error::RpcError;
use crate::error::bad_user_input;
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
/// reconnect, so a stalled ledger does not hold subscription slots indefinitely.
const BACKFILL_POLL_DEADLINE: Duration = Duration::from_secs(180);

/// The filter-matching items in one live checkpoint, each with its cursor, in delivery order.
type LiveMatches<S> = Vec<(<S as Subscribable>::Cursor, <S as Subscribable>::Item)>;

/// A feed that can be streamed with the scan-then-live driver. One implementation per subscribable
/// GraphQL type supplies its cursor, filter, matching, and payload shape, and the free functions
/// here drive the shared machinery over it.
pub(super) trait Subscribable {
    /// The GraphQL node delivered in each edge.
    type Item: OutputType + Send + 'static;
    /// The opaque cursor minted for resumption.
    type Cursor: CursorType + Clone + Send + Sync + 'static;
    /// A position in the feed's order: the boundary just before an item, whether or not an item
    /// sits there.
    type Position: Ord + Copy + Send + Sync + 'static;
    /// The filter selecting matching items.
    type Filter: Clone + Send + Sync + 'static;
    /// The raw scan payload one page yields before conversion to edges.
    type ScanItem: Send + 'static;

    /// The GraphQL payload delivered for each item: an edge carrying the cursor and the node.
    type Output: OutputType + Send + 'static;

    /// Scan one page of matches over `cp_bounds`, starting after `page`'s cursor.
    fn scan<'a>(
        reader: &'a LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<Self::Cursor>,
        filter: &'a Self::Filter,
    ) -> impl Future<Output = Result<LedgerPage<Self::ScanItem, Self::Cursor>, RpcError>> + Send + 'a;

    /// The last checkpoint a resume cursor shows the client has seen any of, if it shows one.
    fn resume_from(cursor: &Self::Cursor) -> Option<u64>;

    /// Where a scan after `cursor` starts: every item before this position comes at or before the
    /// cursor.
    fn resume_position(cursor: &Self::Cursor) -> Self::Position;

    /// The position at the start of `checkpoint`, before its first item.
    fn checkpoint_start(checkpoint: u64) -> Self::Position;

    /// Build the GraphQL node from one scanned item's payload, which sits in `checkpoint`,
    /// constructing whatever scope the feed resolves against. The driver assembles the edge from
    /// the item's cursor and this node.
    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        checkpoint: u64,
        payload: &Self::ScanItem,
    ) -> Result<Self::Item, RpcError>;

    /// The filter-matching items in one live checkpoint, each with its cursor, in delivery order.
    fn matching_items(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &Self::Filter,
    ) -> Result<LiveMatches<Self>, RpcError>;

    /// Assemble one delivered payload from its cursor and node: a match passes the node, a progress
    /// payload passes `None`. A feed that does not opt into progress is only ever called with `Some`.
    fn output(cursor: String, node: Option<Self::Item>) -> Self::Output;

    /// The cursor marking the end of `checkpoint`, minted for a live progress payload: at the start
    /// of the next checkpoint, so a scan starting after it covers everything from there on.
    /// Returning `None` suppresses live progress for the feed; backfill progress is gated
    /// separately, by the per-subscription opt-in.
    fn boundary_after(_checkpoint: &ProcessedCheckpoint) -> Option<Self::Cursor> {
        None
    }

    /// The feed's name, used to label its lifecycle metrics.
    fn subscription_type() -> &'static str;
}

/// One backfill scan output: either a match at the checkpoint its payload names, or a coverage
/// marker at a page's coverage claim, at or below which every match has been scanned (advances the
/// handoff even on a match-less page), carrying the page's end cursor, the resume point past
/// everything scanned so far.
enum Scanned<C, I> {
    Match { checkpoint: u64, cursor: C, node: I },
    Covered { checkpoint: u64, cursor: C },
}

/// Where the backfill scan hands off to the live broadcast. The scan delivers matches at or below
/// the pinned handoff; live delivers the rest, from the checkpoint after it.
struct Handoff {
    /// Pin once the scan frontier is within this many checkpoints of the streamed tip.
    threshold: u64,
    pinned: Option<u64>,
}

/// What the backfill does with one scan output, as decided by [`Handoff::step`].
enum ScanStep<C, I> {
    /// Deliver this match.
    Deliver { cursor: C, node: I },
    /// Keep scanning.
    Continue,
    /// The scan has covered the handoff; live takes over.
    Done,
}

impl<C, I> Scanned<C, I> {
    /// The checkpoint this output sits at (the match's, or the page's coverage claim).
    fn checkpoint(&self) -> u64 {
        match self {
            Scanned::Match { checkpoint, .. } | Scanned::Covered { checkpoint, .. } => *checkpoint,
        }
    }
}

/// Tracks what the client holds, and decides when the driver emits a progress payload.
///
/// The committed position is where the client's latest cursor resumes: every match before it has
/// been delivered. A payload, match or progress, goes out only when its cursor resumes past the
/// committed position, which it then becomes. So resuming from the latest cursor neither skips nor
/// repeats, and the client's resume point never moves backward. Live drops what the backfill
/// already delivered, or what a progress payload already covered, by position.
///
/// Backfill and live produce progress differently. Backfill reuses each scan page's own end cursor
/// at the page boundary, so it needs no pacing. Live has no scan cursor to reuse: it mints one, and
/// only after a run of checkpoints that matched nothing, so a sparse filter does not emit a payload
/// on every checkpoint.
struct DeliveryTracker<P> {
    /// Whether this subscription opted into progress payloads. When false, no progress is emitted
    /// on either phase and every payload is a match.
    progress_enabled: bool,
    /// Emit live progress after this many consecutive live checkpoints with no match (0 disables
    /// live progress; backfill is unaffected).
    unmatched_threshold: u64,
    /// Consecutive live checkpoints with no match since the last match or progress payload.
    consecutive_unmatched: u64,
    /// Where the client's latest cursor resumes, or `None` before it holds one.
    committed: Option<P>,
}

impl<P: Ord + Copy> DeliveryTracker<P> {
    fn new(progress_enabled: bool, unmatched_threshold: u64, committed: Option<P>) -> Self {
        Self {
            progress_enabled,
            unmatched_threshold,
            consecutive_unmatched: 0,
            committed,
        }
    }

    /// Commit a match whose cursor resumes at `position`, or return `false` if the client already
    /// holds it. A committed match ends the run of unmatched live checkpoints.
    fn commit_match(&mut self, position: P) -> bool {
        let advanced = self.advance(position);
        if advanced {
            self.consecutive_unmatched = 0;
        }
        advanced
    }

    /// Commit the end of a backfill page, whose cursor resumes at `position`, as progress. Returns
    /// `false` when progress is off, or when the client already holds a cursor at or past it (a full
    /// page ends on its last match, so it emits nothing).
    fn commit_page_end(&mut self, position: P) -> bool {
        self.progress_enabled && self.advance(position)
    }

    /// Commit progress at `position`, the boundary after a live checkpoint that delivered no match,
    /// only once the unmatched run reaches the threshold. Returns whether to emit it.
    fn commit_unmatched(&mut self, position: P) -> bool {
        if !self.progress_enabled || self.unmatched_threshold == 0 {
            return false;
        }
        self.consecutive_unmatched += 1;
        if self.consecutive_unmatched < self.unmatched_threshold {
            return false;
        }
        self.consecutive_unmatched = 0;
        self.advance(position)
    }

    /// Move the committed position to `position`, if it is past it.
    fn advance(&mut self, position: P) -> bool {
        if self
            .committed
            .is_some_and(|committed| position <= committed)
        {
            return false;
        }
        self.committed = Some(position);
        true
    }
}

impl Handoff {
    fn new(threshold: u64) -> Self {
        Self {
            threshold,
            pinned: None,
        }
    }

    /// Decide what to do with one scan output, given this replica's streamed `tip`.
    ///
    /// Pins first if the scan frontier is within the threshold of `tip`. `resubscribe` must
    /// subscribe the live receiver and then return the streamed tip, so live sees every checkpoint
    /// above the pin. A match past the handoff belongs to live, so the scan stops there.
    fn step<C, I>(
        &mut self,
        scanned: Scanned<C, I>,
        tip: u64,
        resubscribe: impl FnOnce() -> u64,
    ) -> ScanStep<C, I> {
        if self.pinned.is_none() && tip.saturating_sub(scanned.checkpoint()) <= self.threshold {
            self.pinned = Some(resubscribe());
        }

        match scanned {
            Scanned::Match {
                checkpoint,
                cursor,
                node,
            } => {
                if self.pinned.is_none_or(|h| checkpoint <= h) {
                    ScanStep::Deliver { cursor, node }
                } else {
                    ScanStep::Done
                }
            }
            Scanned::Covered { checkpoint, .. } => {
                if self.pinned.is_some_and(|h| checkpoint >= h) {
                    ScanStep::Done
                } else {
                    ScanStep::Continue
                }
            }
        }
    }

    /// The last checkpoint the scan was responsible for; live delivers from the next one.
    fn pinned(&self) -> Option<u64> {
        self.pinned
    }
}

/// Whether a subscription resuming after `checkpoint` starts more than `max_ahead` checkpoints past
/// the streamed `tip`. A resume point that names no checkpoint is never too far ahead.
fn is_too_far_ahead(checkpoint: Option<u64>, tip: u64, max_ahead: u64) -> bool {
    checkpoint.is_some_and(|checkpoint| checkpoint > tip.saturating_add(max_ahead))
}

/// Subscribe to items matching `filter`: backfill from `resume` toward the tip, then follow live.
/// The handoff is pinned mid-scan, once the scan frontier comes within `handoff_threshold` of the
/// tip, so even a deep backfill catches up within one connection instead of lagging the receiver.
#[allow(clippy::type_complexity)]
pub(super) fn subscribe<S: Subscribable>(
    reader: LedgerGrpcReader,
    broadcast: Arc<SubscriptionBroadcast>,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    after: Option<S::Cursor>,
    after_checkpoint: Option<u64>,
    send_progress: bool,
    subscriber_limit: SubscriberLimit,
    config: SubscriptionConfig,
) -> Result<impl Stream<Item = Result<S::Output, RpcError>>, RpcError<Error>> {
    // The last checkpoint the client has seen any of, when it has seen one: the later of the
    // cursor's and `afterCheckpoint`.
    let resume_checkpoint = after
        .as_ref()
        .and_then(S::resume_from)
        .max(after_checkpoint);

    // Reject a resume point more than `max_ahead` checkpoints past the streamed tip before
    // admitting, so bad input never claims a slot or lands in the subscription lifecycle metrics.
    // There is nothing to backfill ahead of the tip, so such a request would only wait for the
    // chain to reach it, and a far-future one would hold the connection open indefinitely.
    let max_ahead = config.max_start_checkpoints_ahead_of_tip;
    if is_too_far_ahead(resume_checkpoint, broadcast.network_tip(), max_ahead) {
        return Err(bad_user_input(Error::TooFarAheadOfTip { max: max_ahead }));
    }

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

    // Progress payloads are opt-in per subscription; once enabled, this threshold paces how often
    // live progress fires.
    let unmatched_threshold = config.unmatched_checkpoints_before_progress;

    // The client holds every match before the later of its cursor and `afterCheckpoint`. With
    // neither, it holds nothing and the subscription is live-only.
    let committed = after
        .as_ref()
        .map(S::resume_position)
        .max(after_checkpoint.map(|checkpoint| S::checkpoint_start(checkpoint.saturating_add(1))));

    Ok(stream! {
        let mut pending_receiver = None;
        let mut handoff = Handoff::new(handoff_threshold);
        let mut tracker = DeliveryTracker::new(send_progress, unmatched_threshold, committed);
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it. `after` and/or
        // `afterCheckpoint` resume the backfill; neither means live-only.
        if committed.is_some() {
            // The resume point can be ahead of this server's watermarks (a cursor minted from the
            // live stream, or by a more caught-up server). Waiting for them to reach it raises the
            // ledger readers' monotonic-read floor past it, so no replica scans from a view behind
            // the resume point. `scan_page` then waits for each page's end the same way, keeping
            // the floor at or above every later resume point.
            if let Some(checkpoint) = resume_checkpoint
                && let Err(e) = wait_for_pipelines_catching_up_at(checkpoint, &mut watermarks_rx).await
            {
                guard.terminate(SubscriptionTerminationReason::BackfillError);
                yield Err(RpcError::from(e));
                return;
            }

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
            for await scanned in scan {
                let scanned = match scanned {
                    Ok(scanned) => scanned,
                    Err(e) => {
                        guard.terminate(SubscriptionTerminationReason::BackfillError);
                        yield Err(e);
                        return;
                    }
                };

                // A page's end is progress once every match on it has been delivered. A match past
                // the handoff ends the scan before its coverage marker is reached, so `page_end`
                // for this page would not run; that is fine, live resumes from the handoff.
                //
                // Consecutive match-less pages each send one, uncoalesced: at most one small payload
                // per ledger request (which the scan already paces, waiting at the tip and never
                // resending an end that did not move), each moving a deep backfill's resume point on.
                let page_end = match &scanned {
                    Scanned::Covered { cursor, .. } => Some(cursor.clone()),
                    Scanned::Match { .. } => None,
                };

                let step = handoff.step(scanned, broadcast.network_tip(), || {
                    pending_receiver = Some(broadcast.broadcaster().resubscribe());
                    broadcast.network_tip()
                });
                if let Some(end_cursor) = page_end
                    && tracker.commit_page_end(S::resume_position(&end_cursor))
                {
                    yield Ok(S::output(end_cursor.encode_cursor(), None));
                }
                match step {
                    ScanStep::Deliver { cursor, node } => {
                        if tracker.commit_match(S::resume_position(&cursor)) {
                            yield Ok(S::output(cursor.encode_cursor(), Some(node)));
                            guard.record_backfill_delivered();
                        }
                    }
                    ScanStep::Continue => {}
                    ScanStep::Done => break,
                }
            }
            last_checkpoint = handoff.pinned();
        }

        // Phase 2: follow live from `handoff + 1` (a fresh receiver if there was no backfill).
        let receiver = pending_receiver.unwrap_or_else(|| broadcast.broadcaster().resubscribe());
        for await delivery in
            live::<S>(receiver, last_checkpoint, caches, resolver_limits, filter, tracker, guard)
        {
            yield delivery;
        }
    })
}

/// Scan matches from the resume point toward the tip, yielding each match and a per-page coverage
/// marker. Open-ended: the caller stops it once the handoff is covered.
fn backfill<S: Subscribable>(
    reader: LedgerGrpcReader,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    mut after: Option<S::Cursor>,
    checkpoint_lo: u64,
    scan_page_size: usize,
) -> impl Stream<Item = Result<Scanned<S::Cursor, S::Item>, RpcError>> {
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

/// Whether a scan page that ended at `end` moved past `after`, the cursor it started from. A page
/// that scanned nothing new (at the ledger's tip, or past it, where the ledger echoes the request's
/// start) ends where `after` resumes.
fn moved_past<S: Subscribable>(end: &S::Cursor, after: &S::Cursor) -> bool {
    S::resume_position(end) > S::resume_position(after)
}

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching items the client does
/// not already hold. Drops the one-checkpoint seam overlap (the resubscribe/tip race), gap-checks,
/// and disconnects on anomalies.
fn live<S: Subscribable>(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    filter: S::Filter,
    mut tracker: DeliveryTracker<S::Position>,
    guard: SubscriptionLifecycleGuard,
) -> impl Stream<Item = Result<S::Output, RpcError>> {
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

                    // Deliver each matching item as its own payload, ordered within the checkpoint,
                    // skipping those the client already holds: the rest of a resume cursor's
                    // checkpoint up to the cursor, or what backfill progress past the handoff
                    // covered. Checkpoints that deliver nothing may advance the resume point as
                    // progress.
                    let items = match S::matching_items(&checkpoint, &caches, &resolver_limits, &filter) {
                        Ok(items) => items,
                        Err(e) => {
                            guard.terminate(SubscriptionTerminationReason::Error);
                            yield Err(e);
                            return;
                        }
                    };
                    let mut delivered_match = false;
                    for (cursor, node) in items {
                        if !tracker.commit_match(S::resume_position(&cursor)) {
                            continue;
                        }
                        delivered_match = true;
                        yield Ok(S::output(cursor.encode_cursor(), Some(node)));
                        guard.record_delivered(checkpoint.summary.timestamp_ms);
                    }

                    if !delivered_match
                        && let Some(boundary) = S::boundary_after(&checkpoint)
                        && tracker.commit_unmatched(S::resume_position(&boundary))
                    {
                        // If the feed sends progress and the unmatched run is long enough, emit one
                        // progress payload at the boundary past this checkpoint, rather than one on
                        // every match-less checkpoint.
                        yield Ok(S::output(boundary.encode_cursor(), None));
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
    reader: &LedgerGrpcReader,
    caches: &Arc<StreamedCaches>,
    resolver_limits: &sui_package_resolver::Limits,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> Result<(Vec<Scanned<S::Cursor, S::Item>>, S::Cursor), RpcError> {
    let mut poll = LedgerPoll::new();
    loop {
        let result = scan_with_retry::<S>(reader, limits, cp_bounds.clone(), after, filter).await?;

        // No end cursor means nothing past the indexer tip is indexed yet; wait, then re-request. A
        // range that was scanned but matched nothing still returns a boundary cursor, not `None`.
        let Some(end_cursor) = result.last_cursor else {
            poll.wait().await?;
            continue;
        };

        // A page that did not move past where it started: wait for the ledger to index more before
        // asking again, rather than re-polling at full speed.
        if after.is_some_and(|after| !moved_past::<S>(&end_cursor, after)) {
            poll.wait().await?;
            continue;
        }

        // Hold the page until all pipelines have indexed through the checkpoint its end cursor
        // resumes in, which is at or past its coverage claim. The delivered items' nested queries
        // resolve against indexed data, and the ledger readers' floor reaches the next page's
        // resume point.
        if let Some(checkpoint) = S::resume_from(&end_cursor) {
            wait_for_pipelines_catching_up_at(checkpoint, watermarks_rx).await?;
        }

        // A forward page keeps `items` in ascending order, one delivered edge each.
        let mut items = Vec::with_capacity(result.items.len() + 1);
        for item in result.items {
            items.push(Scanned::Match {
                checkpoint: item.checkpoint,
                node: S::build_node(caches, resolver_limits, item.checkpoint, &item.payload)?,
                cursor: item.cursor,
            });
        }

        // Coverage marker at the page's coverage claim: advances the handoff even on a match-less
        // page, and carries the page's end cursor so the backfill can deliver it as progress. A
        // page without a claim (some ledger servers send none for a page that scanned nothing)
        // adds no marker; a later page's claim covers it.
        if let Some(checkpoint) = result.covered_checkpoint {
            items.push(Scanned::Covered {
                checkpoint,
                cursor: end_cursor.clone(),
            });
        }

        return Ok((items, end_cursor));
    }
}

/// Scan one page over `cp_bounds`, retrying transient failures under a bounded budget.
async fn scan_with_retry<S: Subscribable>(
    reader: &LedgerGrpcReader,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
) -> Result<LedgerPage<S::ScanItem, S::Cursor>, RpcError> {
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

#[cfg(test)]
mod tests {
    use crate::api::types::ledger_cursor::LedgerKey;
    use crate::api::types::ledger_cursor::Point;
    use crate::api::types::ledger_cursor::TransactionKey;
    use crate::api::types::transaction::Transaction;
    use crate::api::types::transaction::TransactionToken;

    use super::*;

    fn tx(checkpoint: u64, transaction_index: u64) -> TransactionKey {
        TransactionKey {
            checkpoint,
            transaction_index,
        }
    }

    /// One scan output, reduced to what the handoff decisions look at.
    #[derive(Clone, Copy, Debug)]
    enum Out {
        /// A match in this checkpoint, as its payload names it.
        Match(u64),
        /// A page's end, with the coverage claim it carried.
        End(u64),
    }

    /// Runs scan outputs through [`Handoff::step`] against a fixed streamed `tip`, as the backfill
    /// loop in `subscribe` does. Returns the delivered match checkpoints and the checkpoint live
    /// starts after (`None` if still scanning).
    fn drive(threshold: u64, tip: u64, outputs: &[Out]) -> (Vec<u64>, Option<u64>) {
        let mut handoff = Handoff::new(threshold);
        let mut delivered = vec![];
        for out in outputs {
            // Matches carry their checkpoint as the node, so delivery order is observable.
            let scanned = match *out {
                Out::Match(checkpoint) => Scanned::Match {
                    checkpoint,
                    cursor: (),
                    node: checkpoint,
                },
                Out::End(checkpoint) => Scanned::Covered {
                    checkpoint,
                    cursor: (),
                },
            };
            match handoff.step(scanned, tip, || tip) {
                ScanStep::Deliver { node, .. } => delivered.push(node),
                ScanStep::Continue => {}
                ScanStep::Done => return (delivered, handoff.pinned()),
            }
        }
        (delivered, None)
    }

    /// The streamed tip (11) is one checkpoint ahead of the index (10) when the handoff pins. The
    /// first page drains the ledger tip and claims coverage only through 10, so the scan must keep
    /// going until the index has 11 and deliver it; live starts after 11.
    #[test]
    fn handoff_waits_for_the_index_to_reach_the_streamed_tip() {
        let outputs = [
            Out::Match(10),
            Out::End(10),
            // Next page, once the index reaches 11.
            Out::Match(11),
            Out::End(11),
        ];
        let (delivered, live_after) = drive(5, 11, &outputs);
        assert_eq!(delivered, vec![10, 11]);
        assert_eq!(live_after, Some(11));
    }

    /// With nothing left to scan, a page claims coverage only up to the index tip, so a streamed tip
    /// ahead of it keeps the backfill open rather than skipping the checkpoints in between.
    #[test]
    fn claim_below_the_streamed_tip_keeps_the_backfill_open() {
        let (delivered, live_after) = drive(5, 11, &[Out::End(10)]);
        assert!(delivered.is_empty());
        assert_eq!(live_after, None, "checkpoint 11 is not covered yet");
    }

    /// The client resumes after a transaction in checkpoint 10, but this replica has only streamed
    /// up to 9. The scan hands off at the streamed tip, and live takes over at checkpoint 10.
    #[test]
    fn cursor_ahead_of_the_streamed_tip_hands_off_at_the_tip() {
        let outputs = [Out::Match(10), Out::Match(10), Out::Match(11)];
        let (delivered, live_after) = drive(5, 9, &outputs);
        assert_eq!((delivered, live_after), (vec![], Some(9)));
    }

    /// Live drops the items the client already holds, by position: everything up to its cursor in
    /// the cursor's own checkpoint.
    #[test]
    fn live_drops_what_the_client_holds() {
        let resume = Transaction::resume_position(&TransactionToken::at(tx(10, 1)));
        let mut tracker = DeliveryTracker::new(false, 20, Some(resume));
        let delivered: Vec<_> = (0..4)
            .filter(|index| tracker.commit_match(tx(10, *index).successor()))
            .collect();
        assert_eq!(delivered, [2, 3]);
    }

    /// The caught-up case: pin at the streamed tip once the frontier is within the threshold,
    /// deliver matches up to it, and hand off after it.
    #[test]
    fn caught_up_resume_hands_off_at_streamed_tip() {
        let outputs = [
            Out::Match(12),
            Out::End(14),
            Out::Match(16),
            Out::Match(20),
            Out::Match(21),
        ];
        let (delivered, live_after) = drive(5, 20, &outputs);
        assert_eq!(delivered, vec![12, 16, 20]);
        assert_eq!(live_after, Some(20));
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

    /// A page moves past its `after` cursor only when it ends at a later position, whatever bytes
    /// its server encodes the end in. A page that ends where `after` resumes (at the ledger's tip,
    /// or echoing the start of a request past it) has not moved.
    #[test]
    fn scan_progress_is_compared_by_position() {
        let ledger = |bytes: &'static [u8], point| {
            TransactionToken::from_ledger(bytes::Bytes::from_static(bytes), point)
        };
        let after = ledger(b"a", Point::At(tx(16, 3)));

        // Ending where the page started, in the same bytes or in another server's.
        let echo = Point::Before(tx(16, 4));
        assert!(!moved_past::<Transaction>(&ledger(b"a", echo), &after));
        assert!(!moved_past::<Transaction>(&ledger(b"b", echo), &after));

        // Past the start, at an item or a boundary.
        assert!(moved_past::<Transaction>(
            &ledger(b"b", Point::At(tx(16, 4))),
            &after
        ));
        assert!(moved_past::<Transaction>(
            &ledger(b"b", Point::checkpoint_start(17)),
            &after
        ));

        // A cursor GraphQL minted compares the same way.
        let live = TransactionToken::at(tx(16, 3));
        assert!(!moved_past::<Transaction>(&ledger(b"a", echo), &live));
    }

    /// A page with no matches ends past the last delivered cursor, so its end is progress. The same
    /// end offered again is not re-emitted.
    #[test]
    fn backfill_empty_page_delivers_progress() {
        let mut tracker = DeliveryTracker::new(true, 20, None);
        assert!(tracker.commit_page_end(11));
        assert!(!tracker.commit_page_end(11));
        assert!(tracker.commit_page_end(15));
    }

    /// A full page ends at its last match, so the client already holds that cursor.
    #[test]
    fn backfill_page_ending_at_its_last_match_has_no_progress() {
        let mut tracker = DeliveryTracker::new(true, 20, None);
        assert!(tracker.commit_match(11));
        assert!(!tracker.commit_page_end(11));
    }

    /// Live delivers one progress cursor per run of unmatched checkpoints that reaches the
    /// threshold, and a match restarts the run.
    #[test]
    fn live_progress_fires_once_per_unmatched_run() {
        let mut tracker = DeliveryTracker::new(true, 3, None);
        let emitted: Vec<u64> = (21..=26)
            .map(|checkpoint| checkpoint + 1)
            .filter(|boundary| tracker.commit_unmatched(*boundary))
            .collect();
        assert_eq!(emitted, vec![24, 27]);

        // Checkpoints 27 and 28 are unmatched, then 29 matches: the run restarts after it.
        assert!(!tracker.commit_unmatched(28));
        assert!(!tracker.commit_unmatched(29));
        assert!(tracker.commit_match(30));
        assert!(!tracker.commit_unmatched(31));
        assert!(!tracker.commit_unmatched(32));
        assert!(tracker.commit_unmatched(33));
    }

    /// The last backfill page can scan past the handoff. Live delivers neither matches nor progress
    /// behind its end, and live progress past it advances the resume point.
    #[test]
    fn live_progress_does_not_move_behind_backfill_progress() {
        let mut tracker = DeliveryTracker::new(true, 1, None);
        assert!(tracker.commit_page_end(31));
        assert!((21..=30).all(|checkpoint| !tracker.commit_match(checkpoint + 1)));
        assert!(!tracker.commit_unmatched(31));
        assert!(tracker.commit_unmatched(32));
    }

    /// A zero threshold disables live progress, while backfill progress still fires: the
    /// server-wide knob tunes live pacing without silencing the backfill resume point.
    #[test]
    fn zero_threshold_disables_live_progress() {
        let mut tracker = DeliveryTracker::new(true, 0, None);
        assert!((0..100).all(|checkpoint| !tracker.commit_unmatched(checkpoint + 1)));
        assert!(tracker.commit_page_end(11));
    }

    /// Without the per-subscription opt-in, neither a backfill page end nor an unmatched live run
    /// delivers anything, and the resume point stays with the last match.
    #[test]
    fn disabled_progress_delivers_nothing() {
        let mut tracker = DeliveryTracker::new(false, 1, None);
        assert!(!tracker.commit_page_end(10));
        assert!((11..100).all(|checkpoint| !tracker.commit_unmatched(checkpoint + 1)));
        assert!(tracker.commit_match(5));
    }

    /// A resume checkpoint is measured against the tip in checkpoints, and a resume point that
    /// names none (one at the start of genesis) is never too far ahead.
    #[test]
    fn only_a_named_checkpoint_is_too_far_ahead() {
        assert!(!is_too_far_ahead(Some(102), 99, 3));
        assert!(is_too_far_ahead(Some(103), 99, 3));
        assert!(!is_too_far_ahead(None, 99, 0));
    }
}
