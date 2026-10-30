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
//! so reading is handed off to the live phase at `handoff + 1` with no gap or duplicate. The coverage
//! claim the ledger reports on each page lets the handoff advance even through stretches that match
//! nothing.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use async_graphql::OutputType;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_stream::stream;
use backoff::ExponentialBackoff;
use futures::Stream;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

use crate::api::types::ledger_cursor::scan::LedgerPage;
use crate::api::types::ledger_cursor::scan::ScanCursor;
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

/// A feed that can be streamed with the scan-then-live driver. One implementation per subscribable
/// GraphQL type supplies its cursor, filter, matching, and payload shape, and the free functions
/// here drive the shared machinery over it.
pub(super) trait Subscribable {
    /// The GraphQL node delivered in each edge.
    type Item: OutputType + Send + 'static;
    /// The opaque cursor minted for resumption.
    type Cursor: CursorType + Clone + Send + Sync + 'static;
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

    /// Whether a scan page that ended at `end` moved past `after`, the cursor it started from.
    /// `reached_tip` is whether the page ended with `LEDGER_TIP`: at the ledger's indexed tip, or,
    /// for an `after` ahead of that tip, where `after` resumes. Only such a page can end where it
    /// started.
    fn made_progress(end: &Self::Cursor, after: &Self::Cursor, reached_tip: bool) -> bool;

    /// Build the GraphQL node from one scanned item's payload, which sits in `checkpoint`,
    /// constructing whatever scope the feed resolves against. The driver mints the cursor from the
    /// item's position and assembles the edge.
    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        checkpoint: u64,
        payload: &Self::ScanItem,
    ) -> Result<Self::Item, RpcError>;

    /// The filter-matching edges in one live checkpoint, in delivery order.
    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &Self::Filter,
    ) -> Result<Vec<Edge<String, Self::Item, EmptyFields>>, RpcError>;

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

/// One backfill scan output: either a matching edge at the checkpoint its payload names, or a
/// coverage marker at a page's coverage claim, at or below which every match has been scanned
/// (advances the handoff even on a match-less page), carrying the page's end cursor, the resume
/// point past everything scanned so far.
enum Scanned<I: OutputType> {
    Match {
        checkpoint: u64,
        edge: Edge<String, I, EmptyFields>,
    },
    Covered {
        checkpoint: u64,
        cursor: String,
    },
}

/// Where the backfill scan hands off to the live broadcast. The scan delivers every match at or
/// below the pinned handoff; live delivers everything above it.
struct Handoff {
    /// Pin once the scan frontier is within this many checkpoints of the streamed tip.
    threshold: u64,
    /// The last checkpoint the client has seen any of, when known up front (from `afterCheckpoint`
    /// or the `after` cursor).
    resume_from: Option<u64>,
    /// The client resumed from a cursor. Its checkpoint is unknown up front (a legacy cursor), or
    /// comes from the client, so `resume_from` is raised to the first scan output, which the ledger
    /// only returns once it is past the cursor.
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

/// Where a subscription resumes, from its `after` cursor and `afterCheckpoint`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResumePoint {
    /// The last checkpoint the client has seen any of, when known up front: the later of
    /// `afterCheckpoint` and the cursor's.
    checkpoint: Option<u64>,
    /// Whether the client resumed from a cursor.
    from_cursor: bool,
}

impl<I: OutputType> Scanned<I> {
    /// The checkpoint this output sits at (the match's, or the page's coverage claim).
    fn checkpoint(&self) -> u64 {
        match self {
            Scanned::Match { checkpoint, .. } | Scanned::Covered { checkpoint, .. } => *checkpoint,
        }
    }
}

/// Decides when the driver emits a progress payload and with what cursor. Every progress cursor
/// sits at or past the last one delivered and goes out only after all matches up to it have been
/// delivered, so resuming after it neither skips nor repeats.
///
/// Backfill and live produce progress differently. Backfill reuses each scan page's own end cursor
/// at the page boundary, so it needs no pacing. Live has no scan cursor to reuse: it mints one, and
/// only after a run of checkpoints that matched nothing, so a sparse filter does not emit a payload
/// on every checkpoint.
struct ProgressTracker {
    /// Whether this subscription opted into progress payloads. When false, nothing is emitted on
    /// either phase and every payload is a match.
    enabled: bool,
    /// Emit live progress after this many consecutive live checkpoints with no match (0 disables
    /// live progress; backfill is unaffected).
    unmatched_threshold: u64,
    /// Consecutive live checkpoints with no match since the last match or progress payload.
    consecutive_unmatched: u64,
    /// Cursor of the last payload the backfill delivered, match or progress. Backfill emits progress
    /// at a page end only when the page's end cursor moved past this, so a client is never handed a
    /// cursor it already holds.
    last_delivered_cursor: Option<String>,
    /// Highest checkpoint a delivered progress cursor covers. A backfill page can scan past the
    /// handoff, so live must not deliver at or below this, or the resume point would move backward.
    covered_checkpoint: Option<u64>,
}

impl ProgressTracker {
    fn new(enabled: bool, unmatched_threshold: u64) -> Self {
        Self {
            enabled,
            unmatched_threshold,
            consecutive_unmatched: 0,
            last_delivered_cursor: None,
            covered_checkpoint: None,
        }
    }

    /// Remember the last cursor handed to the client, so a later page end can tell whether it
    /// advanced past it.
    fn record_delivered(&mut self, cursor: &str) {
        self.last_delivered_cursor = Some(cursor.to_owned());
    }

    /// The progress cursor to emit for a scan page that covered through `checkpoint` and ended at
    /// `end_cursor`, or `None` when it did not advance past the last cursor delivered (a full page
    /// ends on its last match, so it emits nothing). Records the cursor when it does advance.
    fn progress_if_advanced(&mut self, checkpoint: u64, end_cursor: String) -> Option<String> {
        if !self.enabled {
            return None;
        }
        if self.last_delivered_cursor.as_ref() == Some(&end_cursor) {
            return None;
        }
        self.covered_checkpoint = Some(checkpoint);
        self.last_delivered_cursor = Some(end_cursor.clone());
        Some(end_cursor)
    }

    /// Whether a delivered progress cursor already covers live `checkpoint`.
    fn covers(&self, checkpoint: u64) -> bool {
        self.covered_checkpoint.is_some_and(|c| checkpoint <= c)
    }

    /// Reset the unmatched run after a live checkpoint that matched.
    fn reset_unmatched_run(&mut self) {
        self.consecutive_unmatched = 0;
    }

    /// The progress cursor to emit for a checkpoint that matched nothing, minted by `cursor` only
    /// once the unmatched run reaches the threshold; `None` otherwise.
    fn progress_after_unmatched(
        &mut self,
        checkpoint: u64,
        cursor: impl FnOnce() -> String,
    ) -> Option<String> {
        if !self.enabled || self.unmatched_threshold == 0 {
            return None;
        }
        self.consecutive_unmatched += 1;
        if self.consecutive_unmatched < self.unmatched_threshold {
            return None;
        }
        self.consecutive_unmatched = 0;
        self.covered_checkpoint = Some(checkpoint);
        Some(cursor())
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

    /// Raise the resume point to the first scan output, for an `after` cursor whose checkpoint is
    /// unknown up front or comes from the client.
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
            // The first output is past the cursor. A match sits in the cursor's checkpoint or a
            // later one, with no match in between. A coverage claim is at least the checkpoint
            // before the cursor's, so the one after the claim is at least the cursor's. Either way
            // the scan stays responsible for the rest of the cursor's checkpoint.
            let floor = match &scanned {
                Scanned::Match { checkpoint, .. } => *checkpoint,
                Scanned::Covered { checkpoint, .. } => checkpoint.saturating_add(1),
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
            Scanned::Covered { checkpoint, .. } => {
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

impl ResumePoint {
    /// `after` is the `after` cursor's [`Subscribable::resume_from`], if the client passed a cursor.
    fn new(after: Option<Option<u64>>, after_checkpoint: Option<u64>) -> Self {
        Self {
            // `None` orders before any checkpoint, so this is the later of the two when both are set.
            checkpoint: after.flatten().max(after_checkpoint),
            from_cursor: after.is_some(),
        }
    }

    /// Whether the client asked to resume at all; with neither `after` nor `afterCheckpoint`, the
    /// subscription is live-only.
    fn is_set(&self) -> bool {
        self.from_cursor || self.checkpoint.is_some()
    }

    /// Whether this resume point's checkpoint sits more than `max_ahead` checkpoints past the
    /// streamed `tip`. A resume point that names no checkpoint is never too far ahead.
    fn is_too_far_ahead(&self, tip: u64, max_ahead: u64) -> bool {
        self.checkpoint
            .is_some_and(|checkpoint| checkpoint > tip.saturating_add(max_ahead))
    }

    /// Wait for every pipeline to index the resume point's checkpoint.
    async fn wait_for_pipelines(
        &self,
        watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
    ) -> Result<(), RpcError> {
        if let Some(checkpoint) = self.checkpoint {
            wait_for_pipelines_catching_up_at(checkpoint, watermarks_rx).await?;
        }
        Ok(())
    }
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
    // Where to resume: the checkpoint the `after` cursor places itself in, and `afterCheckpoint`.
    let resume = ResumePoint::new(after.as_ref().map(S::resume_from), after_checkpoint);

    // Reject a resume point more than `max_ahead` checkpoints past the streamed tip before
    // admitting, so bad input never claims a slot or lands in the subscription lifecycle metrics.
    // There is nothing to backfill ahead of the tip, so such a request would only wait for the
    // chain to reach it, and a far-future one would hold the connection open indefinitely.
    let max_ahead = config.max_start_checkpoints_ahead_of_tip;
    if resume.is_too_far_ahead(broadcast.network_tip(), max_ahead) {
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

    Ok(stream! {
        let mut pending_receiver = None;
        let mut handoff = Handoff::new(handoff_threshold, resume.checkpoint);
        if resume.from_cursor {
            handoff = handoff.resume_from_first_output();
        }
        let mut tracker = ProgressTracker::new(send_progress, unmatched_threshold);
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it. `after` and/or
        // `afterCheckpoint` resume the backfill; neither means live-only.
        if resume.is_set() {
            // The resume point can be ahead of this server's watermarks (a cursor minted from the
            // live stream, or by a more caught-up server). Waiting for them to reach it raises the
            // ledger readers' monotonic-read floor past it, so no replica scans from a view behind
            // the resume point. `scan_page` then waits for each page's end the same way, keeping
            // the floor at or above every later resume point.
            if let Err(e) = resume.wait_for_pipelines(&mut watermarks_rx).await {
                guard.terminate(SubscriptionTerminationReason::BackfillError);
                yield Err(e);
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
                // resending a repeated end cursor), each moving a deep backfill's resume point on.
                let page_end = match &scanned {
                    Scanned::Covered { checkpoint, cursor } => Some((*checkpoint, cursor.clone())),
                    Scanned::Match { .. } => None,
                };

                let step = handoff.step(scanned, broadcast.network_tip(), || {
                    pending_receiver = Some(broadcast.broadcaster().resubscribe());
                    broadcast.network_tip()
                });
                if let ScanStep::Deliver(edge) = &step {
                    tracker.record_delivered(&edge.cursor);
                }
                if let Some((checkpoint, end_cursor)) = page_end
                    && let Some(cursor) = tracker.progress_if_advanced(checkpoint, end_cursor)
                {
                    yield Ok(S::output(cursor, None));
                }
                match step {
                    ScanStep::Deliver(edge) => {
                        yield Ok(S::output(edge.cursor, Some(edge.node)));
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
) -> impl Stream<Item = Result<Scanned<S::Item>, RpcError>> {
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

/// Whether a transaction or event scan page that ended at `end` moved past `after`.
///
/// Only a page that reached the ledger's tip can end where it started: an `ItemLimit` page ends on
/// an item past `after`, and a `ScanLimit` page at a frontier past it. A page at the tip is
/// compared by place, because ledger servers of different versions encode the same position in
/// different bytes. Its place moves whenever it scanned anything, because a page that scans up to
/// the tip claims coverage through it.
///
/// Where either cursor has no place (a legacy `after`, or a page that claimed nothing), the ledger
/// bytes decide. A page that drains the tip echoes `after`'s bytes, and a page whose `after` is
/// ahead of the ledger's indexed tip ends where that cursor resumes, claiming nothing, rather than
/// at the ledger's own tip, so a page never ends behind `after`. A cursor without ledger bytes was
/// never sent to the ledger, so any page moves past it.
pub(super) fn ledger_scan_moved<C: ScanCursor>(end: &C, after: &C, reached_tip: bool) -> bool {
    let end = end.bound();
    let after = after.bound();
    if reached_tip && let (Some(end), Some(after)) = (end.point, after.point) {
        return end > after.gap_after();
    }

    match (end.ledger, after.ledger) {
        (Some(end), Some(after)) => end != after,
        _ => true,
    }
}

/// Follow the live broadcast from `last_checkpoint + 1`, delivering matching items. Drops the
/// one-checkpoint seam overlap (the resubscribe/tip race), gap-checks, and disconnects on anomalies.
fn live<S: Subscribable>(
    mut receiver: CheckpointBroadcaster,
    mut last_checkpoint: Option<u64>,
    caches: Arc<StreamedCaches>,
    resolver_limits: sui_package_resolver::Limits,
    filter: S::Filter,
    mut tracker: ProgressTracker,
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
                    // The last backfill page can scan past the handoff, and its end went out as
                    // progress. The scan found nothing to deliver up to there, so a live match here
                    // means the ledger and the stream disagree. Dropping it keeps the client's
                    // cursor from moving backwards.
                    if tracker.covers(seq) {
                        if let Ok(edges) =
                            S::matching_edges(&checkpoint, &caches, &resolver_limits, &filter)
                            && !edges.is_empty()
                        {
                            warn!(
                                checkpoint = seq,
                                matches = edges.len(),
                                "Dropping live matches in a checkpoint backfill progress already covered"
                            );
                        }
                        last_checkpoint = Some(seq);
                        continue;
                    }

                    // Deliver each matching item as its own payload, ordered within the checkpoint.
                    // Empty checkpoints yield nothing but may advance the resume point as progress.
                    let edges = match S::matching_edges(&checkpoint, &caches, &resolver_limits, &filter) {
                        Ok(edges) => edges,
                        Err(e) => {
                            guard.terminate(SubscriptionTerminationReason::Error);
                            yield Err(e);
                            return;
                        }
                    };
                    if edges.is_empty() {
                        // This live checkpoint matched nothing. If the feed sends progress and the
                        // unmatched run is long enough, emit one progress payload at the boundary
                        // past this checkpoint, rather than one on every match-less checkpoint.
                        if let Some(boundary) = S::boundary_after(&checkpoint)
                            && let Some(cursor) =
                                tracker.progress_after_unmatched(seq, || boundary.encode_cursor())
                        {
                            yield Ok(S::output(cursor, None));
                        }
                    } else {
                        tracker.reset_unmatched_run();
                    }
                    for edge in edges {
                        yield Ok(S::output(edge.cursor, Some(edge.node)));
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
    reader: &LedgerGrpcReader,
    caches: &Arc<StreamedCaches>,
    resolver_limits: &sui_package_resolver::Limits,
    limits: &PageLimits,
    cp_bounds: RangeInclusive<u64>,
    after: Option<&S::Cursor>,
    filter: &S::Filter,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> Result<(Vec<Scanned<S::Item>>, S::Cursor), RpcError> {
    let mut poll = LedgerPoll::new();
    loop {
        let result = scan_with_retry::<S>(reader, limits, cp_bounds.clone(), after, filter).await?;

        // No end cursor means nothing past the indexer tip is indexed yet; wait, then re-request. A
        // range that was scanned but matched nothing still returns a boundary cursor, not `None`.
        let Some(end_cursor) = result.last_cursor else {
            poll.wait().await?;
            continue;
        };

        // A page that reached the ledger's tip without moving: wait for the ledger to index more
        // before asking again, rather than re-polling at full speed.
        let reached_tip = result.end_reason == Some(QueryEndReason::LedgerTip);
        if after.is_some_and(|after| !S::made_progress(&end_cursor, after, reached_tip)) {
            poll.wait().await?;
            continue;
        }

        // Hold the page until all pipelines have indexed through its end: its last item, its
        // coverage claim, and, for a page cut short by the scan limit, the checkpoint after the
        // claim, which its end cursor's ledger bytes sit in. The delivered items' nested queries
        // resolve against indexed data, and the ledger readers' floor reaches the next page's
        // resume point.
        let last_item = result.items.last().map(|item| item.checkpoint);
        let end = match result.end_reason {
            Some(QueryEndReason::ScanLimit) => result.covered_checkpoint.map(|c| c + 1),
            _ => result.covered_checkpoint,
        };
        if let Some(checkpoint) = last_item.max(end) {
            wait_for_pipelines_catching_up_at(checkpoint, watermarks_rx).await?;
        }

        // A forward page keeps `items` in ascending order, one delivered edge each.
        let mut items = Vec::with_capacity(result.items.len() + 1);
        for item in result.items {
            items.push(Scanned::Match {
                checkpoint: item.checkpoint,
                edge: Edge::new(
                    item.cursor.encode_cursor(),
                    S::build_node(caches, resolver_limits, item.checkpoint, &item.payload)?,
                ),
            });
        }

        // Coverage marker at the page's coverage claim: advances the handoff even on a match-less
        // page, and carries the page's end cursor so the backfill can deliver it as progress. A
        // page without a claim (some ledger servers send none for a page that scanned nothing)
        // adds no marker; a later page's claim covers it.
        if let Some(checkpoint) = result.covered_checkpoint {
            items.push(Scanned::Covered {
                checkpoint,
                cursor: end_cursor.encode_cursor(),
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
    use fastcrypto::encoding::Base64;
    use fastcrypto::encoding::Encoding;

    use crate::api::types::ledger_cursor::Point;
    use crate::api::types::ledger_cursor::TransactionKey;
    use crate::api::types::ledger_cursor::legacy::LegacyKind;
    use crate::api::types::ledger_cursor::legacy::LegacyPosition;
    use crate::api::types::ledger_cursor::legacy::LegacyToken;
    use crate::api::types::transaction::CTransaction;
    use crate::api::types::transaction::Transaction;
    use crate::api::types::transaction::TransactionToken;

    use super::*;

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
            let scanned = match *out {
                Out::Match(checkpoint) => Scanned::Match {
                    checkpoint,
                    edge: Edge::new(String::new(), checkpoint),
                },
                Out::End(checkpoint) => Scanned::Covered {
                    checkpoint,
                    cursor: String::new(),
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

    /// The handoff for a transaction or event cursor with no `afterCheckpoint`, seeded from the
    /// first scan output.
    fn after_cursor(threshold: u64) -> Handoff {
        Handoff::new(threshold, None).resume_from_first_output()
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
        let (delivered, live_after) = drive(5, Some(9), 11, &outputs);
        assert_eq!(delivered, vec![10, 11]);
        assert_eq!(live_after, Some(11));
    }

    /// With nothing left to scan, a page claims coverage only up to the index tip, so a streamed tip
    /// ahead of it keeps the backfill open rather than skipping the checkpoints in between.
    #[test]
    fn claim_below_the_streamed_tip_keeps_the_backfill_open() {
        let (delivered, live_after) = drive(5, Some(10), 11, &[Out::End(10)]);
        assert!(delivered.is_empty());
        assert_eq!(live_after, None, "checkpoint 11 is not covered yet");
    }

    /// The client resumes after a transaction in checkpoint 10, but this replica has only streamed
    /// up to 9. The resume point comes from the first scan output, which keeps the rest of
    /// checkpoint 10 with the scan: live skips 10 as already seen.
    #[test]
    fn transaction_cursor_resumes_from_first_match() {
        let outputs = [Out::Match(10), Out::Match(10), Out::Match(11)];

        // Without seeding, the pin sits at the streamed tip and live would re-deliver all of
        // checkpoint 10.
        let (delivered, live_after) = drive(5, None, 9, &outputs);
        assert_eq!((delivered, live_after), (vec![], Some(9)));

        let (delivered, live_after) = drive_handoff(after_cursor(5), 9, &outputs);
        assert_eq!(delivered, vec![10, 10]);
        assert_eq!(live_after, Some(10));
    }

    /// A first page with no match still ends with a coverage claim, at least the checkpoint before
    /// the cursor's. The scan stays responsible through the checkpoint after the claim.
    #[test]
    fn transaction_cursor_resumes_from_first_coverage_claim() {
        // The cursor sits partway through checkpoint 10, and the rest of 10 has no match.
        let outputs = [Out::End(10), Out::Match(11), Out::End(11)];
        let (delivered, live_after) = drive_handoff(after_cursor(5), 9, &outputs);
        assert_eq!(delivered, vec![11]);
        assert_eq!(live_after, Some(11));

        // The cursor sits at the start of checkpoint 10, and the claim covers only 9.
        let outputs = [Out::End(9), Out::Match(10), Out::End(10)];
        let (delivered, live_after) = drive_handoff(after_cursor(5), 9, &outputs);
        assert_eq!(delivered, vec![10]);
        assert_eq!(live_after, Some(10));
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
        let (delivered, live_after) = drive(5, Some(10), 20, &outputs);
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

    /// A page that reaches the tip without scanning anything ends where it started, in whatever
    /// bytes its server encodes that position. Pages at the tip are compared by place; elsewhere an
    /// echo of `after`'s bytes is not progress, and a cursor the ledger never saw (no ledger bytes)
    /// is always moved past.
    #[test]
    fn ledger_scan_progress() {
        let key = |transaction_index| TransactionKey {
            checkpoint: 16,
            transaction_index,
        };
        let ledger = |bytes: &'static [u8], point| {
            CTransaction::from_ledger(bytes::Bytes::from_static(bytes), point)
        };
        let after = ledger(b"a", Some(Point::At(key(3))));
        let same_place = Some(Point::Before(key(4)));

        // Ending at the tip where the page started, in the same bytes or in another server's.
        assert!(!ledger_scan_moved(&ledger(b"a", same_place), &after, true));
        assert!(!ledger_scan_moved(&ledger(b"b", same_place), &after, true));

        // Coverage through the tip moves the place.
        let covered = Some(Point::checkpoint_start(17));
        assert!(ledger_scan_moved(&ledger(b"b", covered), &after, true));

        // Short of the tip, new bytes are a new frontier even at the same place.
        assert!(ledger_scan_moved(&ledger(b"b", same_place), &after, false));
        assert!(!ledger_scan_moved(&after, &after, false));

        // Without a place to compare, the bytes decide.
        assert!(!ledger_scan_moved(
            &ledger(b"a", None),
            &ledger(b"a", None),
            true
        ));
        assert!(ledger_scan_moved(
            &ledger(b"b", None),
            &ledger(b"a", None),
            true
        ));

        let live = TransactionToken::at(key(3));
        assert!(ledger_scan_moved(&ledger(b"a", None), &live, false));
    }

    /// A page with no matches ends past the last delivered cursor, so its end is progress. The same
    /// end cursor offered again is not re-emitted.
    #[test]
    fn backfill_empty_page_delivers_progress() {
        let mut tracker = ProgressTracker::new(true, 20);
        assert_eq!(
            tracker.progress_if_advanced(10, "b11".into()),
            Some("b11".into())
        );
        assert_eq!(tracker.progress_if_advanced(10, "b11".into()), None);
        assert_eq!(
            tracker.progress_if_advanced(14, "b15".into()),
            Some("b15".into())
        );
    }

    /// A page whose last match is followed by scanned checkpoints with no match delivers its end as
    /// progress, moving the resume point past that tail.
    #[test]
    fn backfill_page_with_trailing_empty_range_delivers_progress() {
        let mut tracker = ProgressTracker::new(true, 20);
        tracker.record_delivered("i10");
        assert_eq!(
            tracker.progress_if_advanced(12, "b13".into()),
            Some("b13".into())
        );
    }

    /// A full page ends at its last match, so the client already holds that cursor.
    #[test]
    fn backfill_page_ending_at_its_last_match_has_no_progress() {
        let mut tracker = ProgressTracker::new(true, 20);
        tracker.record_delivered("i10");
        assert_eq!(tracker.progress_if_advanced(9, "i10".into()), None);
    }

    /// Live delivers one progress cursor per run of unmatched checkpoints that reaches the
    /// threshold, and a match restarts the run.
    #[test]
    fn live_progress_fires_once_per_unmatched_run() {
        let mut tracker = ProgressTracker::new(true, 3);
        let mut emitted = vec![];
        for cp in 21..=26 {
            if let Some(c) = tracker.progress_after_unmatched(cp, || format!("b{}", cp + 1)) {
                emitted.push(c);
            }
        }
        assert_eq!(emitted, vec!["b24", "b27"]);

        // Checkpoints 27 and 28 are unmatched, then 29 matches: the run restarts after it.
        assert_eq!(tracker.progress_after_unmatched(27, || "b28".into()), None);
        assert_eq!(tracker.progress_after_unmatched(28, || "b29".into()), None);
        tracker.reset_unmatched_run();
        assert_eq!(tracker.progress_after_unmatched(30, || "b31".into()), None);
        assert_eq!(tracker.progress_after_unmatched(31, || "b32".into()), None);
        assert_eq!(
            tracker.progress_after_unmatched(32, || "b33".into()),
            Some("b33".into())
        );
    }

    /// The last backfill page can scan past the handoff. Live skips the checkpoints its end covers,
    /// delivering neither matches nor progress behind it, and live progress past it advances the
    /// covered checkpoint.
    #[test]
    fn live_progress_does_not_move_behind_backfill_progress() {
        let mut tracker = ProgressTracker::new(true, 1);
        assert!(tracker.progress_if_advanced(30, "b31".into()).is_some());
        assert!((21..=30).all(|cp| tracker.covers(cp)));
        assert!(!tracker.covers(31));
        assert_eq!(
            tracker.progress_after_unmatched(31, || "b32".into()),
            Some("b32".into())
        );
        assert!(tracker.covers(31));
    }

    /// A zero threshold disables live progress, while backfill progress still fires: the
    /// server-wide knob tunes live pacing without silencing the backfill resume point.
    #[test]
    fn zero_threshold_disables_live_progress() {
        let mut tracker = ProgressTracker::new(true, 0);
        for cp in 0..100 {
            assert_eq!(
                tracker.progress_after_unmatched(cp, || unreachable!()),
                None
            );
        }
        assert_eq!(
            tracker.progress_if_advanced(10, "b11".into()),
            Some("b11".into())
        );
    }

    /// Without the per-subscription opt-in, neither a backfill page end nor an unmatched live run
    /// delivers anything, and nothing is marked covered.
    #[test]
    fn disabled_progress_delivers_nothing() {
        let mut tracker = ProgressTracker::new(false, 1);
        assert_eq!(tracker.progress_if_advanced(10, "b11".into()), None);
        assert!(!tracker.covers(10));
        for cp in 11..100 {
            assert_eq!(
                tracker.progress_after_unmatched(cp, || unreachable!()),
                None
            );
        }
    }

    /// A cursor in GraphQL's own format names its checkpoint up front: an item's own, or the one
    /// before a checkpoint's start. A legacy cursor names the checkpoint it is labelled with, or none
    /// if it is unlabelled.
    #[test]
    fn resume_point_from_cursors() {
        let resume = |cursor: CTransaction, after_checkpoint| {
            ResumePoint::new(Some(Transaction::resume_from(&cursor)), after_checkpoint)
        };

        let at = TransactionToken::at(TransactionKey {
            checkpoint: 12,
            transaction_index: 3,
        });
        assert_eq!(
            resume(at.clone(), None),
            ResumePoint {
                checkpoint: Some(12),
                from_cursor: true,
            }
        );
        assert_eq!(resume(at, Some(15)).checkpoint, Some(15));

        assert_eq!(
            resume(TransactionToken::checkpoint_start(11), None).checkpoint,
            Some(10)
        );
        let from_genesis = resume(TransactionToken::checkpoint_start(0), None);
        assert_eq!(from_genesis.checkpoint, None);
        assert!(from_genesis.is_set());

        let legacy = |checkpoint| {
            let token = LegacyToken {
                kind: LegacyKind::Item,
                position: LegacyPosition::Transactions {
                    checkpoint,
                    tx_seq: 5,
                },
            };
            CTransaction::decode_cursor(&Base64::encode(token.encode())).unwrap()
        };
        assert_eq!(resume(legacy(99), None).checkpoint, Some(99));
        assert_eq!(
            resume(legacy(0), None),
            ResumePoint {
                checkpoint: None,
                from_cursor: true,
            }
        );

        assert_eq!(
            ResumePoint::new(None, Some(10)),
            ResumePoint {
                checkpoint: Some(10),
                from_cursor: false,
            }
        );
        assert!(!ResumePoint::new(None, None).is_set());
    }

    /// `afterCheckpoint` and cursors that name a checkpoint are measured against the tip in
    /// checkpoints. A cursor that names none is never too far ahead.
    #[test]
    fn only_a_named_checkpoint_is_too_far_ahead() {
        assert!(!ResumePoint::new(None, Some(102)).is_too_far_ahead(99, 3));
        assert!(ResumePoint::new(None, Some(103)).is_too_far_ahead(99, 3));
        assert!(!ResumePoint::new(Some(Some(102)), None).is_too_far_ahead(99, 3));
        assert!(ResumePoint::new(Some(Some(103)), None).is_too_far_ahead(99, 3));
        assert!(!ResumePoint::new(Some(None), None).is_too_far_ahead(99, 0));
    }
}
