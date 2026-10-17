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
use sui_rpc_cursor::Position;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

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
            for await scanned in scan {
                let scanned = match scanned {
                    Ok(scanned) => scanned,
                    Err(e) => {
                        guard.terminate(SubscriptionTerminationReason::BackfillError);
                        yield Err(e);
                        return;
                    }
                };

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

/// Whether `token` is a transaction or event cursor minted by the Postgres path, which records
/// checkpoint 0 in place of the item's real checkpoint.
fn has_placeholder_checkpoint(token: &CursorToken) -> bool {
    match token.position {
        Position::Checkpoints { .. } => false,
        Position::Transactions { checkpoint, tx_seq } => checkpoint == 0 && tx_seq != 0,
        Position::Events {
            checkpoint, tx_seq, ..
        } => checkpoint == 0 && tx_seq != 0,
    }
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
        let result = scan_with_retry::<S>(reader, limits, cp_bounds.clone(), after, filter).await?;

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

#[cfg(test)]
mod tests {
    use sui_rpc_cursor::CursorKind;

    use super::*;

    /// One scan output, reduced to what the handoff decisions look at.
    #[derive(Clone, Debug)]
    enum Out {
        Match(u64),
        /// A page's end cursor; its coverage goes through `covered_checkpoint`, as in `scan_page`.
        End(CursorToken),
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
