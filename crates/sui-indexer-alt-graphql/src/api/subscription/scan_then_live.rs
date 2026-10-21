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
use crate::task::streaming::wait_for_pipelines_catching_up_at_transaction;

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

    /// The checkpoint a scanned item sits in, read from its payload.
    fn checkpoint(payload: &Self::ScanItem) -> Result<u64, RpcError>;

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

    /// The feed's name, used to label its lifecycle metrics.
    fn subscription_type() -> &'static str;
}

/// One backfill scan output: either a matching edge at the checkpoint its payload names, or a
/// coverage marker at a page's coverage claim, at or below which every match has been scanned
/// (advances the handoff even on a match-less page).
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
    /// The checkpoint the client resumed after, when known up front (from `afterCheckpoint` or a
    /// checkpoint cursor).
    resume_from: Option<u64>,
    /// The client resumed from a transaction or event cursor, whose checkpoint is not known up
    /// front, so `resume_from` is raised to the first scan output, which the ledger only returns
    /// once it is past the cursor.
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
    /// The last checkpoint the client has seen in full, when known up front: the later of
    /// `afterCheckpoint` and a checkpoint cursor's checkpoint.
    checkpoint: Option<u64>,
    /// The transaction a transaction or event cursor sits in.
    tx_seq: Option<u64>,
}

impl<I: OutputType> Scanned<I> {
    /// The checkpoint this output sits at (the match's, or the page's coverage claim).
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
    /// not known up front.
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

impl ResumePoint {
    fn new(after: Option<&CursorToken>, after_checkpoint: Option<u64>) -> Self {
        let (cursor_checkpoint, tx_seq) = match after.map(|token| token.position) {
            None => (None, None),
            Some(Position::Checkpoints { checkpoint }) => (Some(checkpoint), None),
            Some(Position::Transactions { tx_seq, .. } | Position::Events { tx_seq, .. }) => {
                (None, Some(tx_seq))
            }
        };
        Self {
            // `None` orders before any checkpoint, so this is the later of the two when both are set.
            checkpoint: cursor_checkpoint.max(after_checkpoint),
            tx_seq,
        }
    }

    /// Whether the client asked to resume at all; with neither `after` nor `afterCheckpoint`, the
    /// subscription is live-only.
    fn is_set(&self) -> bool {
        self.checkpoint.is_some() || self.tx_seq.is_some()
    }

    /// Whether this resume point sits more than `max_ahead` checkpoints past the streamed tip: the
    /// checkpoint `tip`, after which the network holds `tip_network_total_transactions`
    /// transactions (`None` if not yet known).
    ///
    /// A transaction or event cursor is measured in transactions. Its allowance is `max_ahead`
    /// checkpoints at the chain's average transactions per checkpoint up to the tip (at least one),
    /// so with no allowance only transactions past the tip are too far, as for checkpoints. While
    /// the tip's transaction count is unknown, such a cursor is admitted, and the backfill waits for
    /// the pipelines to index it.
    fn is_too_far_ahead(
        &self,
        tip: u64,
        tip_network_total_transactions: Option<u64>,
        max_ahead: u64,
    ) -> bool {
        let checkpoint_too_far = self
            .checkpoint
            .is_some_and(|checkpoint| checkpoint > tip.saturating_add(max_ahead));

        let transaction_too_far = match (self.tx_seq, tip_network_total_transactions) {
            (Some(tx_seq), Some(total)) => {
                let transactions_per_checkpoint = total.div_ceil(tip.saturating_add(1)).max(1);
                let max_tx_seq_exclusive =
                    total.saturating_add(max_ahead.saturating_mul(transactions_per_checkpoint));
                tx_seq >= max_tx_seq_exclusive
            }
            _ => false,
        };

        checkpoint_too_far || transaction_too_far
    }

    /// Wait for every pipeline to index the resume point: its checkpoint, and the checkpoint holding
    /// its transaction.
    async fn wait_for_pipelines(
        &self,
        watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
    ) -> anyhow::Result<()> {
        if let Some(checkpoint) = self.checkpoint {
            wait_for_pipelines_catching_up_at(checkpoint, watermarks_rx).await?;
        }
        if let Some(tx_seq) = self.tx_seq {
            wait_for_pipelines_catching_up_at_transaction(tx_seq, watermarks_rx).await?;
        }
        Ok(())
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
    mut watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    filter: S::Filter,
    after: Option<S::Cursor>,
    after_checkpoint: Option<u64>,
    subscriber_limit: SubscriberLimit,
    config: SubscriptionConfig,
) -> Result<impl Stream<Item = Result<Edge<String, S::Item, EmptyFields>, RpcError>>, RpcError<Error>>
where
    for<'a> CursorToken: From<&'a S::Cursor>,
{
    // Where to resume: a transaction or event cursor by its transaction, a checkpoint cursor and
    // `afterCheckpoint` by checkpoint.
    let resume = ResumePoint::new(
        after.as_ref().map(CursorToken::from).as_ref(),
        after_checkpoint,
    );

    // Reject a far-ahead resume before admitting, so bad input never claims a slot or lands in the
    // subscription lifecycle metrics.
    reject_if_start_too_far_ahead(&resume, &broadcast, &config)?;

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
        let mut handoff = Handoff::new(handoff_threshold, resume.checkpoint);
        if resume.tx_seq.is_some() {
            handoff = handoff.resume_from_first_output();
        }
        let mut last_checkpoint: Option<u64> = None;

        // Phase 1: scan toward the tip, pinning the live receiver near it. `after` and/or
        // `afterCheckpoint` resume the backfill; neither means live-only.
        if resume.is_set() {
            // The resume point can be ahead of this server's watermarks (a cursor minted from the
            // live stream, or by a more caught-up server). Waiting for them to reach it raises the
            // ledger readers' monotonic-read floor past it, so no replica scans from a view behind
            // the resume point. `scan_page` then waits for each page's end cursor the same way,
            // keeping the floor at or above every later resume cursor.
            if let Err(e) = resume.wait_for_pipelines(&mut watermarks_rx).await {
                guard.terminate(SubscriptionTerminationReason::BackfillError);
                yield Err(e.into());
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
    resume: &ResumePoint,
    broadcast: &SubscriptionBroadcast,
    config: &SubscriptionConfig,
) -> Result<(), RpcError<Error>> {
    let max_ahead = config.max_start_checkpoints_ahead_of_tip;
    if resume.is_too_far_ahead(
        broadcast.network_tip(),
        broadcast.tip_network_total_transactions(),
        max_ahead,
    ) {
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

/// Whether a page ending at `end` moved the scan past `after`. A page that drains the ledger's tip
/// ends where it started, echoing `after`, so the scan waits before asking again. The ledger
/// rejects an `after` ahead of its indexed tip as out of range rather than answering from its own
/// tip (#28308), so a page never ends behind `after`.
fn made_progress(end: &CursorToken, after: Option<&CursorToken>) -> bool {
    after.is_none_or(|after| !same_scan_position(end, after))
}

/// Whether two cursors sit at the same place in the scan: the same kind at the same position. A
/// transaction or event position is its transaction (and event index); the checkpoint those
/// cursors also carry is not compared.
fn same_scan_position(a: &CursorToken, b: &CursorToken) -> bool {
    a.kind == b.kind
        && match (a.position, b.position) {
            (Position::Checkpoints { checkpoint: a }, Position::Checkpoints { checkpoint: b }) => {
                a == b
            }
            (
                Position::Transactions { tx_seq: a, .. },
                Position::Transactions { tx_seq: b, .. },
            ) => a == b,
            (
                Position::Events {
                    tx_seq: a_tx_seq,
                    event_index: a_event_index,
                    ..
                },
                Position::Events {
                    tx_seq: b_tx_seq,
                    event_index: b_event_index,
                    ..
                },
            ) => (a_tx_seq, a_event_index) == (b_tx_seq, b_event_index),
            _ => false,
        }
}

/// Block until every pipeline has indexed the checkpoint holding `position`: a checkpoint cursor's
/// checkpoint, or the one holding a transaction or event cursor's transaction.
async fn wait_for_pipelines_reaching(
    position: Position,
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> anyhow::Result<()> {
    match position {
        Position::Checkpoints { checkpoint } => {
            wait_for_pipelines_catching_up_at(checkpoint, watermarks_rx).await
        }
        Position::Transactions { tx_seq, .. } | Position::Events { tx_seq, .. } => {
            wait_for_pipelines_catching_up_at_transaction(tx_seq, watermarks_rx).await
        }
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

        // Hold the page until all pipelines have indexed through its end: the delivered items'
        // nested queries resolve against indexed data, and the ledger readers' floor reaches the
        // next page's resume cursor.
        wait_for_pipelines_reaching(end_cursor.position, watermarks_rx).await?;

        // A forward page keeps `items` in ascending order, one delivered edge each.
        let mut items = Vec::with_capacity(result.items.len() + 1);
        for item in &result.items {
            let token =
                CursorToken::decode(&item.cursor).context("Failed to decode scan item cursor")?;
            let cursor = S::Cursor::try_from(token)
                .context("Unexpected position in scan item cursor")?
                .encode_cursor();
            let checkpoint = S::checkpoint(&item.payload)?;
            items.push(Scanned::Match {
                checkpoint,
                edge: Edge::new(
                    cursor,
                    S::build_node(caches, resolver_limits, checkpoint, &item.payload)?,
                ),
            });
        }
        // Coverage marker at the page's coverage claim: advances the handoff even on a match-less
        // page. A page without a claim (some ledger servers send none for a page that scanned
        // nothing) adds no marker; a later page's claim covers it.
        if let Some(checkpoint) = result.covered_checkpoint() {
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
    use sui_rpc_cursor::CursorKind::Boundary as B;
    use sui_rpc_cursor::CursorKind::Item as I;

    use super::*;

    /// One scan output, reduced to what the handoff decisions look at.
    #[derive(Clone, Copy, Debug)]
    enum Out {
        /// A match in this checkpoint, as its payload names it.
        Match(u64),
        /// A page's end, with the coverage claim it carried, if any. As in `scan_page`, a page
        /// without a claim adds no coverage marker.
        End(Option<u64>),
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
                Out::End(Some(checkpoint)) => Scanned::Covered { checkpoint },
                Out::End(None) => continue,
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

    /// The streamed tip (11) is one checkpoint ahead of the index (10) when the handoff pins. The
    /// first page drains the ledger tip and claims coverage only through 10, so the scan must keep
    /// going until the index has 11 and deliver it; live starts after 11.
    #[test]
    fn handoff_waits_for_the_index_to_reach_the_streamed_tip() {
        let outputs = [
            Out::Match(10),
            Out::End(Some(10)),
            // Next page, once the index reaches 11.
            Out::Match(11),
            Out::End(Some(11)),
        ];
        let (delivered, live_after) = drive(5, Some(9), 11, &outputs);
        assert_eq!(delivered, vec![10, 11]);
        assert_eq!(live_after, Some(11));
    }

    /// With nothing left to scan, a page claims coverage only up to the index tip, so a streamed tip
    /// ahead of it keeps the backfill open rather than skipping the checkpoints in between.
    #[test]
    fn claim_below_the_streamed_tip_keeps_the_backfill_open() {
        let (delivered, live_after) = drive(5, Some(10), 11, &[Out::End(Some(10))]);
        assert!(delivered.is_empty());
        assert_eq!(live_after, None, "checkpoint 11 is not covered yet");
    }

    /// A ledger server may answer a page that scanned nothing (caught up at its tip) without a
    /// coverage claim. Such a page covers nothing, so the backfill stays open until a later page
    /// scans and claims coverage through the streamed tip.
    #[test]
    fn page_without_a_claim_defers_coverage_to_a_later_claim() {
        let (delivered, live_after) = drive(5, Some(10), 11, &[Out::End(None)]);
        assert!(delivered.is_empty());
        assert_eq!(live_after, None);

        let outputs = [
            Out::End(None),
            Out::End(None),
            Out::Match(11),
            Out::End(Some(11)),
        ];
        let (delivered, live_after) = drive(5, Some(10), 11, &outputs);
        assert_eq!(delivered, vec![11]);
        assert_eq!(live_after, Some(11));
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
        let outputs = [Out::End(Some(10)), Out::Match(11), Out::End(Some(11))];
        let (delivered, live_after) = drive_handoff(after_cursor(5), 9, &outputs);
        assert_eq!(delivered, vec![11]);
        assert_eq!(live_after, Some(11));

        // The cursor sits at the start of checkpoint 10, and the claim covers only 9.
        let outputs = [Out::End(Some(9)), Out::Match(10), Out::End(Some(10))];
        let (delivered, live_after) = drive_handoff(after_cursor(5), 9, &outputs);
        assert_eq!(delivered, vec![10]);
        assert_eq!(live_after, Some(10));

        // A first page without a claim adds no output, so the resume point comes from the next.
        let outputs = [Out::End(None), Out::Match(10), Out::End(Some(10))];
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
            Out::End(Some(14)),
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

    /// Transaction and event cursors are compared by kind and position; the checkpoint they also
    /// carry can differ between servers for the same position.
    #[test]
    fn made_progress_ignores_the_checkpoint_label() {
        assert!(!made_progress(&tx(B, 0, 3), Some(&tx(B, 16, 3))));
        assert!(!made_progress(&ev(I, 16, 3, 1), Some(&ev(I, 0, 3, 1))));

        assert!(made_progress(&tx(I, 16, 3), Some(&tx(B, 16, 3))));
        assert!(made_progress(&tx(B, 16, 4), Some(&tx(B, 16, 3))));
        assert!(made_progress(&ev(I, 16, 3, 2), Some(&ev(I, 16, 3, 1))));
        assert!(made_progress(&ev(I, 16, 4, 1), Some(&ev(I, 16, 3, 1))));
    }

    #[test]
    fn resume_point_takes_the_transaction_not_the_checkpoint_label() {
        assert_eq!(ResumePoint::new(None, None), ResumePoint::default());
        assert!(!ResumePoint::default().is_set());

        let resume = ResumePoint::new(Some(&tx(I, 0, 5)), None);
        assert_eq!(
            resume,
            ResumePoint {
                checkpoint: None,
                tx_seq: Some(5),
            }
        );
        assert!(resume.is_set());
        assert_eq!(
            ResumePoint::new(Some(&ev(I, 99, 5, 2)), Some(7)),
            ResumePoint {
                checkpoint: Some(7),
                tx_seq: Some(5),
            }
        );

        // A checkpoint cursor's checkpoint is its position: the later of it and `afterCheckpoint`.
        let at_checkpoint = |checkpoint| ResumePoint {
            checkpoint: Some(checkpoint),
            tx_seq: None,
        };
        assert_eq!(
            ResumePoint::new(Some(&cp(I, 12)), Some(10)),
            at_checkpoint(12)
        );
        assert_eq!(
            ResumePoint::new(Some(&cp(I, 8)), Some(10)),
            at_checkpoint(10)
        );
        assert_eq!(ResumePoint::new(None, Some(10)), at_checkpoint(10));
    }

    /// The tip is checkpoint 99, after which the network holds 1000 transactions: 10 per checkpoint
    /// on average.
    #[test]
    fn start_too_far_ahead_is_measured_in_transactions_for_transaction_cursors() {
        let too_far =
            |resume: ResumePoint, max_ahead| resume.is_too_far_ahead(99, Some(1000), max_ahead);
        let at_tx = |tx_seq| ResumePoint::new(Some(&tx(I, 0, tx_seq)), None);

        // With no allowance, only transactions past the tip are too far.
        assert!(!too_far(at_tx(999), 0));
        assert!(too_far(at_tx(1000), 0));

        // Three checkpoints of allowance are thirty transactions.
        assert!(!too_far(at_tx(1029), 3));
        assert!(too_far(at_tx(1030), 3));

        // The cursor's checkpoint label plays no part.
        assert!(!too_far(
            ResumePoint::new(Some(&tx(I, 1_000_000, 1029)), None),
            3
        ));
        assert!(!too_far(
            ResumePoint::new(Some(&ev(I, 1_000_000, 1029, 4)), None),
            3
        ));
        assert!(too_far(ResumePoint::new(Some(&ev(I, 0, 1030, 0)), None), 3));

        // `afterCheckpoint` is still measured in checkpoints.
        assert!(!too_far(ResumePoint::new(None, Some(102)), 3));
        assert!(too_far(ResumePoint::new(None, Some(103)), 3));
        assert!(too_far(ResumePoint::new(Some(&tx(I, 0, 5)), Some(103)), 3));

        // Before the first broadcast the tip's transaction count is unknown: a transaction cursor
        // is admitted, while `afterCheckpoint` is still checked.
        let resume = ResumePoint::new(Some(&tx(I, 0, 1_000_000)), None);
        assert!(!resume.is_too_far_ahead(99, None, 0));
        let resume = ResumePoint::new(Some(&tx(I, 0, 1_000_000)), Some(103));
        assert!(resume.is_too_far_ahead(99, None, 3));
    }
}
