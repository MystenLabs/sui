// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::SubscriptionMetrics;
use futures::{StreamExt, stream::FuturesUnordered};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use sui_inverted_index::BitmapQuery;
use sui_types::full_checkpoint_content::Checkpoint;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tokio::time::sleep;
use tracing::info;
use tracing::trace;
use tracing::warn;

mod matcher;

const CHECKPOINT_MAILBOX_SIZE: usize = 1024;
/// Amortizes admission select/receive overhead while bounding how many
/// requests can delay the next checkpoint poll.
const ADMISSION_TURN_LIMIT: usize = 128;
const SUBSCRIPTION_CHANNEL_SIZE: usize = 256;
const DEFAULT_MAX_SUBSCRIBERS: usize = 1024;
/// Bound on each shard task's mailbox (registrations, checkpoint fan-out,
/// and lag teardowns from the dispatcher).
const SHARD_MAILBOX_SIZE: usize = 64;

/// Default for [`SubscriptionService::build`]'s `watermark_interval`: ~5
/// seconds at mainnet checkpoint cadence.
const DEFAULT_WATERMARK_INTERVAL: u32 = 25;

/// Default for [`SubscriptionService::build`]'s `shards`: the host's
/// available parallelism, floor 1.
fn default_shards() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// Poll interval while waiting for the index to catch up to a checkpoint
/// before delivering it (see [`IndexedCheckpointFn`]).
const INDEX_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);
/// Upper bound on how long delivery of a single checkpoint waits for the
/// index. A healthy index catches up in milliseconds; the bound just keeps
/// a stalled indexer from wedging the subscription stream forever.
const INDEX_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Reads the highest checkpoint the index has committed (the embedded
/// rpc-store's live-cohort watermark), or `None` if it has indexed nothing
/// yet. When supplied to [`SubscriptionService::build`], the service holds a
/// checkpoint back until the index has committed it, so a client that
/// observes a checkpoint (e.g. via `execute_transaction_and_wait_for_checkpoint`)
/// can immediately read that checkpoint's indexed state.
pub type IndexedCheckpointFn = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// Which item stream a subscriber asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionKind {
    Checkpoints,
    Transactions,
    Events,
}

impl SubscriptionKind {
    pub(crate) fn metric_label(self) -> &'static str {
        match self {
            Self::Checkpoints => "checkpoint",
            Self::Transactions => "transaction",
            Self::Events => "event",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SubscriptionTerminationReason {
    ClientClosed,
    SlowConsumer,
    SourceLag,
    ServiceShutdown,
}

impl SubscriptionTerminationReason {
    fn metric_label(self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed",
            Self::SlowConsumer => "slow_consumer",
            Self::SourceLag => "source_lag",
            Self::ServiceShutdown => "service_shutdown",
        }
    }
}

pub(crate) struct SubscriptionLifecycleGuard {
    kind: SubscriptionKind,
    filtered: bool,
    reservation: SubscriberReservation,
    inflight_subscribers: prometheus::IntGauge,
    terminations_total: prometheus::IntCounterVec,
    termination_reason: SubscriptionTerminationReason,
}

impl SubscriptionLifecycleGuard {
    pub(crate) fn new(
        kind: SubscriptionKind,
        filtered: bool,
        reservation: SubscriberReservation,
        metrics: &SubscriptionMetrics,
    ) -> Self {
        reservation.increment_resident_counts(kind, filtered);
        let inflight_subscribers = metrics
            .inflight_subscribers
            .with_label_values(&[kind.metric_label(), if filtered { "true" } else { "false" }]);
        inflight_subscribers.inc();

        Self {
            kind,
            filtered,
            reservation,
            inflight_subscribers,
            terminations_total: metrics.terminations_total.clone(),
            termination_reason: SubscriptionTerminationReason::ServiceShutdown,
        }
    }

    /// Finalizes the subscription now, recording `reason` instead of the
    /// default `service_shutdown`: consuming `self` runs `Drop`, which
    /// decrements the counts/gauge and increments the termination counter.
    pub(crate) fn terminate(mut self, reason: SubscriptionTerminationReason) {
        self.termination_reason = reason;
    }
}

impl Drop for SubscriptionLifecycleGuard {
    fn drop(&mut self) {
        self.reservation
            .decrement_resident_counts(self.kind, self.filtered);
        self.inflight_subscribers.dec();
        self.terminations_total
            .with_label_values(&[
                self.kind.metric_label(),
                self.termination_reason.metric_label(),
            ])
            .inc();
    }
}

/// What a subscriber asked for. `query: None` = unfiltered (stream
/// everything).
pub struct SubscriptionSpec {
    pub kind: SubscriptionKind,
    pub query: Option<BitmapQuery>,
}

/// One message per checkpoint per subscriber that either matched or is due a
/// progress frame.
pub enum SubscriptionUpdate {
    Matched(MatchedCheckpoint),
    /// The stream advanced through `checkpoint` without a match and the
    /// configured watermark interval elapsed. `tx_hi` is that checkpoint's
    /// `network_total_transactions` (the exclusive tx-seq upper bound), used
    /// to mint the boundary cursor position.
    WatermarkTick {
        checkpoint: u64,
        tx_hi: u64,
    },
}

pub struct MatchedCheckpoint {
    pub checkpoint: Arc<Checkpoint>,
    pub matches: SubscriptionMatches,
}

/// Kind-specific match payload. Indices are within-checkpoint and ascending.
pub enum SubscriptionMatches {
    /// Checkpoint subscription: the checkpoint matched (some tx satisfied the
    /// filter, or the subscription is unfiltered).
    Checkpoint,
    /// Transaction subscription: matched transaction indices, run-length
    /// encoded as half-open ranges -- ascending, non-overlapping, and
    /// maximally coalesced. Bounds the payload at O(runs) for
    /// densely-matching filters (e.g. unanchored negation).
    Transactions(Vec<std::ops::Range<u32>>),
    /// Transaction subscription without a filter: every transaction in the
    /// checkpoint matched. O(1) representation of "all"; never constructed
    /// for a checkpoint with no transactions.
    AllTransactions,
    /// Event subscription: per matched transaction index, matched event
    /// indices.
    Events(Vec<(u32, Vec<u32>)>),
    /// Event subscription without a filter: every event in the checkpoint
    /// matched. O(1) representation of "all"; never constructed for a
    /// checkpoint with no events.
    AllEvents,
}

impl SubscriptionMatches {
    /// Matched transaction indices of a transaction-subscription payload,
    /// ascending; `None` for other payload kinds. `tx_count` is the
    /// checkpoint's transaction count, used to expand
    /// [`Self::AllTransactions`].
    pub fn transaction_indices(
        &self,
        tx_count: u32,
    ) -> Option<Box<dyn Iterator<Item = u32> + Send + '_>> {
        match self {
            Self::Transactions(ranges) => Some(Box::new(ranges.iter().flat_map(Clone::clone))),
            Self::AllTransactions => Some(Box::new(0..tx_count)),
            _ => None,
        }
    }

    /// Matched `(transaction index, event index)` pairs of an
    /// event-subscription payload, ascending; `None` for other payload
    /// kinds. `checkpoint` is used to expand [`Self::AllEvents`].
    pub fn event_indices<'a>(
        &'a self,
        checkpoint: &'a Checkpoint,
    ) -> Option<Box<dyn Iterator<Item = (u32, u32)> + Send + 'a>> {
        match self {
            Self::Events(txs) => Some(Box::new(
                txs.iter()
                    .flat_map(|(tx, evs)| evs.iter().map(move |&ev| (*tx, ev))),
            )),
            Self::AllEvents => Some(Box::new(
                checkpoint
                    .transactions
                    .iter()
                    .enumerate()
                    .flat_map(|(tx_idx, tx)| {
                        let event_count =
                            tx.events.as_ref().map(|e| e.data.len()).unwrap_or(0) as u32;
                        (0..event_count).map(move |ev| (tx_idx as u32, ev))
                    }),
            )),
            _ => None,
        }
    }
}

struct SubscriptionRequest {
    spec: SubscriptionSpec,
    response_sender: oneshot::Sender<mpsc::Receiver<SubscriptionUpdate>>,
    reservation: SubscriberReservation,
}

/// Every request owns a reservation, so pending plus resident subscriptions
/// remain bounded by the configured subscriber limit without a transport bound.
#[allow(clippy::disallowed_methods)]
fn subscription_admission_channel() -> (
    mpsc::UnboundedSender<SubscriptionRequest>,
    mpsc::UnboundedReceiver<SubscriptionRequest>,
) {
    mpsc::unbounded_channel()
}

enum AdmissionState {
    Accepting,
    WaitingForShard(SubscriptionRequest),
    Closed,
}

enum SubscriptionServiceEvent {
    Checkpoint(Result<Arc<Checkpoint>, broadcast::error::RecvError>),
    AdmissionRequest(SubscriptionRequest),
    AdmissionClosed,
    ShardCapacity {
        shard: usize,
        permit: mpsc::OwnedPermit<ShardMsg>,
    },
    AdmissionCanceled,
}

#[derive(Clone)]
pub struct SubscriptionServiceHandle {
    admission_sender: mpsc::UnboundedSender<SubscriptionRequest>,
    counters: Arc<SubscriberCounts>,
    metrics: SubscriptionMetrics,
}

impl SubscriptionServiceHandle {
    pub async fn register_subscription(
        &self,
        spec: SubscriptionSpec,
    ) -> Option<mpsc::Receiver<SubscriptionUpdate>> {
        let reservation = match self.counters.try_reserve() {
            Some(reservation) => reservation,
            None => {
                trace!(
                    "failed to register new subscriber: hit maximum number of subscribers {}",
                    self.counters.limit
                );
                return None;
            }
        };

        let (response_sender, response_receiver) = oneshot::channel();
        let request = SubscriptionRequest {
            spec,
            response_sender,
            reservation,
        };
        self.admission_sender.send(request).ok()?;

        response_receiver.await.ok()
    }

    pub(crate) fn stream_metrics(
        &self,
        kind: SubscriptionKind,
    ) -> crate::metrics::SubscriptionStreamMetrics {
        self.metrics.stream_metrics(kind.metric_label())
    }
}

/// Shared subscription admission and lifecycle accounting.
///
/// `reserved` is the admission authority and counts pending, in-flight, and
/// resident subscriptions. `total` and the filtered counters track resident
/// subscriptions through their lifecycle.
pub(crate) struct SubscriberCounts {
    limit: usize,
    reserved: AtomicUsize,
    total: AtomicUsize,
    filtered_tx: AtomicUsize,
    filtered_event: AtomicUsize,
}

impl SubscriberCounts {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            reserved: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            filtered_tx: AtomicUsize::new(0),
            filtered_event: AtomicUsize::new(0),
        }
    }

    fn try_reserve(self: &Arc<Self>) -> Option<SubscriberReservation> {
        self.reserved
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |reserved| {
                (reserved < self.limit).then_some(reserved + 1)
            })
            .ok()?;
        Some(SubscriberReservation {
            counters: Arc::clone(self),
        })
    }

    #[cfg(test)]
    fn reserved(&self) -> usize {
        self.reserved.load(Ordering::Relaxed)
    }
}

pub(crate) struct SubscriberReservation {
    counters: Arc<SubscriberCounts>,
}

impl SubscriberReservation {
    fn increment_resident_counts(&self, kind: SubscriptionKind, filtered: bool) {
        self.counters.total.fetch_add(1, Ordering::Relaxed);
        if filtered {
            match kind {
                SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                    self.counters.filtered_tx.fetch_add(1, Ordering::Relaxed);
                }
                SubscriptionKind::Events => {
                    self.counters.filtered_event.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn decrement_resident_counts(&self, kind: SubscriptionKind, filtered: bool) {
        self.counters.total.fetch_sub(1, Ordering::Relaxed);
        if filtered {
            match kind {
                SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                    self.counters.filtered_tx.fetch_sub(1, Ordering::Relaxed);
                }
                SubscriptionKind::Events => {
                    self.counters.filtered_event.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }
}

impl Drop for SubscriberReservation {
    fn drop(&mut self) {
        let previous = self.counters.reserved.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "subscriber reservation count underflow");
    }
}

enum ShardMsg {
    /// A checkpoint plus its pre-extracted dimension keys.
    Checkpoint(Arc<Checkpoint>, Arc<matcher::CheckpointKeys>),
    /// An admitted subscriber (handshake already completed by the dispatcher).
    Register {
        spec: SubscriptionSpec,
        sender: mpsc::Sender<SubscriptionUpdate>,
        guard: SubscriptionLifecycleGuard,
    },
    /// Drop every subscriber on this shard with the supplied bounded reason.
    Clear(SubscriptionTerminationReason),
}

/// One worker task owning a partition of the subscribers: it evaluates their
/// filters against pre-extracted checkpoint keys and delivers their updates.
struct SubscriptionShard {
    mailbox: mpsc::Receiver<ShardMsg>,
    matcher: matcher::SubscriptionMatcher,
    /// Checkpoints a subscriber may go without any frame before a standalone
    /// watermark tick is delivered (see `RpcConfig::subscription_watermark_interval`).
    watermark_interval: u32,
}

impl SubscriptionShard {
    async fn run(mut self) {
        while let Some(msg) = self.mailbox.recv().await {
            self.handle_msg(msg);
        }
        self.matcher
            .clear(SubscriptionTerminationReason::ServiceShutdown);
    }

    fn handle_msg(&mut self, msg: ShardMsg) {
        match msg {
            ShardMsg::Register {
                spec,
                sender,
                guard,
            } => {
                self.matcher.insert(spec, sender, guard);
            }
            ShardMsg::Checkpoint(checkpoint, keys) => {
                self.matcher
                    .dispatch_with_keys(&checkpoint, &keys, self.watermark_interval);
            }
            ShardMsg::Clear(reason) => {
                self.matcher.clear(reason);
            }
        }
    }

    /// Synchronously process every message already in the mailbox: a
    /// test-only stand-in for the spawned `run` loop that keeps the actor
    /// tests deterministic.
    #[cfg(test)]
    fn drain(&mut self) {
        while let Ok(msg) = self.mailbox.try_recv() {
            self.handle_msg(msg);
        }
    }
}

pub struct SubscriptionService {
    // Broadcast receiver for `Checkpoint`s published by the Checkpoint Executor.
    //
    // The executor publishes non-blocking, so a slow service can fall behind
    // and observe `RecvError::Lagged`; checkpoints delivered between lags arrive
    // in-order.
    checkpoint_mailbox: broadcast::Receiver<Arc<Checkpoint>>,
    admission_mailbox: mpsc::UnboundedReceiver<SubscriptionRequest>,
    /// Registration targets: one mailbox per shard task, each owning a
    /// partition of the subscribers.
    shards: Vec<mpsc::Sender<ShardMsg>>,
    /// Rotating tie-break cursor for shards with equal free mailbox capacity.
    next_shard: usize,
    /// Filtered-subscriber counts per key space, shared with the shards;
    /// gates per-checkpoint key extraction.
    counters: Arc<SubscriberCounts>,

    // When set, delivery of a checkpoint waits until the index has committed
    // it (see [`IndexedCheckpointFn`]). `None` preserves the immediate-delivery
    // behavior used with the legacy synchronously-committed index.
    indexed_checkpoint: Option<IndexedCheckpointFn>,

    metrics: SubscriptionMetrics,
}

impl SubscriptionService {
    /// `None` defaults `watermark_interval` to 25 checkpoints,
    /// `max_subscribers` to 1024, and `shards` to the host's available
    /// parallelism, with a minimum of one shard.
    pub fn build(
        registry: &prometheus::Registry,
        indexed_checkpoint: Option<IndexedCheckpointFn>,
        watermark_interval: Option<u32>,
        max_subscribers: Option<usize>,
        shards: Option<u32>,
    ) -> (
        broadcast::Sender<Arc<Checkpoint>>,
        SubscriptionServiceHandle,
    ) {
        let metrics = SubscriptionMetrics::new(registry);
        let max_subscribers = max_subscribers.unwrap_or(DEFAULT_MAX_SUBSCRIBERS);
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(CHECKPOINT_MAILBOX_SIZE);
        let (admission_sender, admission_mailbox) = subscription_admission_channel();
        let counters = Arc::new(SubscriberCounts::new(max_subscribers));
        let handle = SubscriptionServiceHandle {
            admission_sender,
            counters: Arc::clone(&counters),
            metrics: metrics.clone(),
        };

        let watermark_interval = watermark_interval
            .unwrap_or(DEFAULT_WATERMARK_INTERVAL)
            .max(1);
        let shards = shards.unwrap_or_else(default_shards).max(1);
        let mut shard_senders = Vec::with_capacity(shards as usize);
        for _ in 0..shards {
            let (sender, shard_mailbox) = mpsc::channel(SHARD_MAILBOX_SIZE);
            tokio::spawn(
                SubscriptionShard {
                    mailbox: shard_mailbox,
                    matcher: matcher::SubscriptionMatcher::default(),
                    watermark_interval,
                }
                .run(),
            );
            shard_senders.push(sender);
        }

        tokio::spawn(
            Self {
                checkpoint_mailbox,
                admission_mailbox,
                shards: shard_senders,
                next_shard: 0,
                counters,
                indexed_checkpoint,
                metrics,
            }
            .start(),
        );

        (checkpoint_sender, handle)
    }

    async fn start(mut self) {
        let mut admission_state = AdmissionState::Accepting;
        loop {
            let event = match &mut admission_state {
                AdmissionState::Accepting => {
                    tokio::select! {
                        biased;

                        result = self.checkpoint_mailbox.recv() => {
                            SubscriptionServiceEvent::Checkpoint(result)
                        },
                        request = self.admission_mailbox.recv() => {
                            match request {
                                Some(request) => {
                                    SubscriptionServiceEvent::AdmissionRequest(request)
                                }
                                None => SubscriptionServiceEvent::AdmissionClosed,
                            }
                        },
                    }
                }
                AdmissionState::WaitingForShard(request) => {
                    let shards: Vec<_> = (0..self.shards.len())
                        .map(|offset| {
                            let shard = (self.next_shard + offset) % self.shards.len();
                            (shard, self.shards[shard].clone())
                        })
                        .collect();

                    tokio::select! {
                        biased;

                        result = self.checkpoint_mailbox.recv() => {
                            SubscriptionServiceEvent::Checkpoint(result)
                        },
                        _ = request.response_sender.closed() => {
                            SubscriptionServiceEvent::AdmissionCanceled
                        },
                        (shard, permit) = Self::reserve_first_available_shard(shards) => {
                            SubscriptionServiceEvent::ShardCapacity { shard, permit }
                        },
                    }
                }
                AdmissionState::Closed => {
                    SubscriptionServiceEvent::Checkpoint(self.checkpoint_mailbox.recv().await)
                }
            };

            match event {
                SubscriptionServiceEvent::Checkpoint(Ok(checkpoint)) => {
                    self.handle_checkpoint(checkpoint).await;
                }
                SubscriptionServiceEvent::Checkpoint(Err(broadcast::error::RecvError::Lagged(
                    skipped,
                ))) => {
                    self.handle_lag(skipped).await;
                }
                // Once the executor drops the sender this yields `Closed`
                // and we can terminate the event loop.
                SubscriptionServiceEvent::Checkpoint(Err(broadcast::error::RecvError::Closed)) => {
                    break;
                }
                SubscriptionServiceEvent::AdmissionRequest(request) => {
                    admission_state = self.admit_ready_requests(request);
                }
                SubscriptionServiceEvent::AdmissionClosed => {
                    // Established subscribers remain live until the checkpoint
                    // source closes.
                    admission_state = AdmissionState::Closed;
                }
                SubscriptionServiceEvent::ShardCapacity { shard, permit } => {
                    let AdmissionState::WaitingForShard(request) = admission_state else {
                        unreachable!("shard capacity requires a waiting admission");
                    };
                    self.complete_admission(shard, permit, request);
                    admission_state = AdmissionState::Accepting;
                }
                SubscriptionServiceEvent::AdmissionCanceled => {
                    let AdmissionState::WaitingForShard(_) = admission_state else {
                        unreachable!("admission cancellation requires a waiting admission");
                    };
                    admission_state = AdmissionState::Accepting;
                }
            }
        }

        for shard in &self.shards {
            shard
                .send(ShardMsg::Clear(
                    SubscriptionTerminationReason::ServiceShutdown,
                ))
                .await
                .expect("subscription shard terminated unexpectedly");
        }

        info!("RPC Subscription Services ended");
    }

    async fn handle_checkpoint(&mut self, checkpoint: Arc<Checkpoint>) {
        // Check that we recieved checkpoints in-order. The broadcast stream
        // preserves send order, and any gap surfaces separately as `Lagged`
        // (see `handle_lag`), so reaching here out-of-order indicates an
        // executor bug.
        {
            let last_sequence_number = self.metrics.last_recieved_checkpoint.get();
            let sequence_number = *checkpoint.summary.sequence_number() as i64;

            if last_sequence_number != 0 && (last_sequence_number + 1) != sequence_number {
                panic!(
                    "recieved checkpoint out-of-order. expected checkpoint {}, recieved {}",
                    last_sequence_number + 1,
                    sequence_number
                );
            }

            // Update the metric marking the latest checkpoint we've seen
            self.metrics.last_recieved_checkpoint.set(sequence_number);
        }

        // Hold the checkpoint back until the index has committed it, so a
        // client that observes this checkpoint can immediately read its
        // indexed state. No-op unless an index gate was configured.
        self.wait_until_indexed(*checkpoint.summary.sequence_number())
            .await;

        // No live subscriber or admitted registration pending in a shard:
        // skip fan-out. Requests merely queued in the admission lane are not
        // counted and may establish their stream boundary after this checkpoint.
        if self.counters.total.load(Ordering::Relaxed) == 0 {
            return;
        }

        // Extract the checkpoint's dimension keys once, only for the key
        // spaces with at least one filtered subscriber, then fan the
        // checkpoint out to every shard. The bounded sends give backpressure:
        // one slow shard stalls the dispatcher, which lags the broadcast
        // receiver and tears everything down (see `handle_lag`).
        let keys = Arc::new(matcher::extract_checkpoint_keys(
            &checkpoint,
            self.counters.filtered_tx.load(Ordering::Relaxed) > 0,
            self.counters.filtered_event.load(Ordering::Relaxed) > 0,
        ));
        for shard in &self.shards {
            shard
                .send(ShardMsg::Checkpoint(
                    Arc::clone(&checkpoint),
                    Arc::clone(&keys),
                ))
                .await
                .expect("subscription shard terminated unexpectedly");
        }
    }

    /// Block until the index has committed `sequence_number`, polling the
    /// configured [`IndexedCheckpointFn`]. Returns immediately when no gate is
    /// configured or the index is already caught up. Gives up after
    /// [`INDEX_WAIT_TIMEOUT`] -- a stalled indexer should not wedge delivery
    /// forever -- delivering a (possibly not-yet-indexed) checkpoint rather
    /// than stalling the stream.
    async fn wait_until_indexed(&self, sequence_number: u64) {
        let Some(indexed) = &self.indexed_checkpoint else {
            return;
        };

        if indexed().is_some_and(|hi| hi >= sequence_number) {
            return;
        }

        let wait_started = Instant::now();
        let deadline = wait_started + INDEX_WAIT_TIMEOUT;
        loop {
            sleep(INDEX_WAIT_POLL_INTERVAL).await;
            if indexed().is_some_and(|hi| hi >= sequence_number) {
                self.metrics
                    .index_wait_seconds
                    .observe(wait_started.elapsed().as_secs_f64());
                return;
            }
            if Instant::now() >= deadline {
                self.metrics.index_wait_timeouts_total.inc();
                self.metrics
                    .index_wait_seconds
                    .observe(wait_started.elapsed().as_secs_f64());
                warn!(
                    checkpoint = sequence_number,
                    "index did not catch up within {INDEX_WAIT_TIMEOUT:?}; \
                     delivering checkpoint anyway"
                );
                return;
            }
        }
    }

    /// Drop every in-progress subscription after the service fell behind the
    /// broadcast stream. Having missed `skipped` checkpoints we can no longer
    /// deliver an in-order, gap-free stream to any subscriber, and clients
    /// already tolerate connection breaks and reconnect, so tearing them all
    /// down is cheaper than trying to resynchronize each one.
    async fn handle_lag(&mut self, skipped: u64) {
        warn!(
            skipped,
            "subscription service lagged behind the checkpoint stream; \
             dropping all in-progress subscriptions"
        );
        // Per-shard FIFO ordering guarantees no shard delivers a post-gap
        // checkpoint to a pre-gap subscriber: Clear is enqueued behind all
        // pre-gap checkpoints and ahead of all post-gap ones.
        for shard in &self.shards {
            shard
                .send(ShardMsg::Clear(SubscriptionTerminationReason::SourceLag))
                .await
                .expect("subscription shard terminated unexpectedly");
        }
        // The next delivered checkpoint jumps ahead by `skipped`; reset the
        // in-order tracker so it is not mistaken for an out-of-order delivery.
        self.metrics.last_recieved_checkpoint.set(0);
    }

    fn try_reserve_least_backlogged_shard(&self) -> Option<(usize, mpsc::OwnedPermit<ShardMsg>)> {
        let mut selected_shard = None;
        let mut greatest_capacity = 0;

        for offset in 0..self.shards.len() {
            let shard = (self.next_shard + offset) % self.shards.len();
            let sender = &self.shards[shard];
            if sender.is_closed() {
                panic!("subscription shard terminated unexpectedly");
            }

            let capacity = sender.capacity();
            if selected_shard.is_none() || capacity > greatest_capacity {
                selected_shard = Some(shard);
                greatest_capacity = capacity;
            }
        }

        let shard = selected_shard.expect("subscription service requires at least one shard");
        match self.shards[shard].clone().try_reserve_owned() {
            Ok(permit) => Some((shard, permit)),
            Err(mpsc::error::TrySendError::Full(_)) => None,
            Err(mpsc::error::TrySendError::Closed(_)) => {
                panic!("subscription shard terminated unexpectedly")
            }
        }
    }

    async fn reserve_first_available_shard(
        shards: Vec<(usize, mpsc::Sender<ShardMsg>)>,
    ) -> (usize, mpsc::OwnedPermit<ShardMsg>) {
        assert!(
            !shards.is_empty(),
            "subscription service requires at least one shard"
        );
        for (_, sender) in &shards {
            if sender.is_closed() {
                panic!("subscription shard terminated unexpectedly");
            }
        }

        let mut reservations = FuturesUnordered::new();
        for (shard, sender) in shards {
            reservations.push(async move { (shard, sender.reserve_owned().await) });
        }

        match reservations
            .next()
            .await
            .expect("subscription service requires at least one shard")
        {
            (shard, Ok(permit)) => (shard, permit),
            (_, Err(_)) => panic!("subscription shard terminated unexpectedly"),
        }
    }

    fn complete_admission(
        &mut self,
        shard: usize,
        permit: mpsc::OwnedPermit<ShardMsg>,
        request: SubscriptionRequest,
    ) {
        if request.response_sender.is_closed() {
            trace!("failed to register new subscriber: request was cancelled");
            return;
        }

        let (sender, receiver) = mpsc::channel(SUBSCRIPTION_CHANNEL_SIZE);
        if request.response_sender.send(receiver).is_err() {
            trace!("failed to register new subscriber: request was cancelled");
            return;
        }

        trace!("successfully registered new subscriber");
        let kind = request.spec.kind;
        let filtered = request.spec.query.is_some();
        let guard =
            SubscriptionLifecycleGuard::new(kind, filtered, request.reservation, &self.metrics);
        permit.send(ShardMsg::Register {
            spec: request.spec,
            sender,
            guard,
        });
        self.next_shard = (shard + 1) % self.shards.len();
    }

    fn try_admit(&mut self, request: SubscriptionRequest) -> AdmissionState {
        if request.response_sender.is_closed() {
            trace!("failed to register new subscriber: request was cancelled");
            return AdmissionState::Accepting;
        }

        let Some((shard, permit)) = self.try_reserve_least_backlogged_shard() else {
            trace!("waiting for a subscription shard to have capacity");
            return AdmissionState::WaitingForShard(request);
        };

        self.complete_admission(shard, permit, request);
        AdmissionState::Accepting
    }

    fn admit_ready_requests(&mut self, first_request: SubscriptionRequest) -> AdmissionState {
        let mut request = first_request;
        let mut remaining_attempts = ADMISSION_TURN_LIMIT;

        loop {
            match self.try_admit(request) {
                AdmissionState::Accepting => {}
                state => return state,
            }

            remaining_attempts -= 1;
            if remaining_attempts == 0 {
                return AdmissionState::Accepting;
            }

            request = match self.admission_mailbox.try_recv() {
                Ok(request) => request,
                Err(mpsc::error::TryRecvError::Empty) => {
                    return AdmissionState::Accepting;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return AdmissionState::Closed;
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use move_core_types::account_address::AccountAddress;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use sui_rpc::proto::sui::rpc::v2 as proto;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::event::Event;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use crate::ledger_history::filter::event_filter_to_query;
    use crate::ledger_history::filter::transaction_filter_to_query;

    use super::*;

    /// An unspawned dispatcher plus its shards, driven synchronously via
    /// [`drain`] so the actor tests are fully deterministic.
    fn test_service(shard_count: usize) -> (SubscriptionService, Vec<SubscriptionShard>) {
        test_service_with(shard_count, 25, None)
    }

    fn test_service_with(
        shard_count: usize,
        watermark_interval: u32,
        indexed_checkpoint: Option<IndexedCheckpointFn>,
    ) -> (SubscriptionService, Vec<SubscriptionShard>) {
        let (service, _checkpoint_sender, _request_sender, shards) = actor_service_with(
            shard_count,
            watermark_interval,
            indexed_checkpoint,
            16,
            DEFAULT_MAX_SUBSCRIBERS,
        );
        (service, shards)
    }

    fn actor_service_with(
        shard_count: usize,
        watermark_interval: u32,
        indexed_checkpoint: Option<IndexedCheckpointFn>,
        checkpoint_capacity: usize,
        max_subscribers: usize,
    ) -> (
        SubscriptionService,
        broadcast::Sender<Arc<Checkpoint>>,
        mpsc::UnboundedSender<SubscriptionRequest>,
        Vec<SubscriptionShard>,
    ) {
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(checkpoint_capacity);
        let (request_sender, admission_mailbox) = subscription_admission_channel();
        let metrics = SubscriptionMetrics::new(&prometheus::Registry::new());
        let counters = Arc::new(SubscriberCounts::new(max_subscribers));

        let mut shard_senders = Vec::with_capacity(shard_count);
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            let (sender, shard_mailbox) = mpsc::channel(SHARD_MAILBOX_SIZE);
            shard_senders.push(sender);
            shards.push(SubscriptionShard {
                mailbox: shard_mailbox,
                matcher: matcher::SubscriptionMatcher::default(),
                watermark_interval,
            });
        }

        let service = SubscriptionService {
            checkpoint_mailbox,
            admission_mailbox,
            shards: shard_senders,
            next_shard: 0,
            counters,
            indexed_checkpoint,
            metrics,
        };
        (service, checkpoint_sender, request_sender, shards)
    }

    fn checkpoint(sequence_number: u64) -> Arc<Checkpoint> {
        Arc::new(TestCheckpointBuilder::new(sequence_number).build_checkpoint())
    }

    /// One checkpoint with one transaction per sender index, in order.
    fn checkpoint_with_senders(seq: u64, senders: &[u8]) -> Arc<Checkpoint> {
        let mut builder = TestCheckpointBuilder::new(seq);
        for &sender in senders {
            builder = builder.start_transaction(sender).finish_transaction();
        }
        Arc::new(builder.build_checkpoint())
    }

    /// One checkpoint where tx 0 carries no events and tx 1 carries two.
    fn checkpoint_with_events(seq: u64) -> Arc<Checkpoint> {
        let package = AccountAddress::random();
        let event = |name: &str| Event {
            package_id: ObjectID::from(package),
            transaction_module: Identifier::new("emitter").unwrap(),
            sender: addr(1),
            type_: StructTag {
                address: package,
                module: Identifier::new("mod_t").unwrap(),
                name: Identifier::new(name).unwrap(),
                type_params: vec![],
            },
            contents: vec![],
        };
        let mut builder = TestCheckpointBuilder::new(seq);
        builder = builder.start_transaction(0).finish_transaction();
        builder = builder
            .start_transaction(1)
            .with_events(vec![event("EventA"), event("EventB")])
            .finish_transaction();
        Arc::new(builder.build_checkpoint())
    }

    fn addr(idx: u8) -> SuiAddress {
        TestCheckpointBuilder::derive_address(idx)
    }

    fn sender_query(address: SuiAddress, negated: bool) -> BitmapQuery {
        let mut sender = proto::SenderFilter::default();
        sender.address = Some(address.to_string());
        let mut literal = proto::TransactionLiteral::default();
        literal.predicate = Some(proto::transaction_literal::Predicate::Sender(sender));
        literal.negated = negated;
        let mut term = proto::TransactionTerm::default();
        term.literals = vec![literal];
        let mut filter = proto::TransactionFilter::default();
        filter.terms = vec![term];
        transaction_filter_to_query(&filter, 16).unwrap()
    }

    fn event_type_query(type_str: &str) -> BitmapQuery {
        let mut event_type = proto::EventTypeFilter::default();
        event_type.event_type = Some(type_str.to_owned());
        let mut literal = proto::EventLiteral::default();
        literal.predicate = Some(proto::event_literal::Predicate::EventType(event_type));
        let mut term = proto::EventTerm::default();
        term.literals = vec![literal];
        let mut filter = proto::EventFilter::default();
        filter.terms = vec![term];
        event_filter_to_query(&filter, 16).unwrap()
    }

    fn unfiltered() -> SubscriptionSpec {
        SubscriptionSpec {
            kind: SubscriptionKind::Checkpoints,
            query: None,
        }
    }

    fn subscription_request(
        counters: &Arc<SubscriberCounts>,
        spec: SubscriptionSpec,
        response_sender: oneshot::Sender<mpsc::Receiver<SubscriptionUpdate>>,
    ) -> SubscriptionRequest {
        let reservation = counters
            .try_reserve()
            .expect("test request requires subscriber capacity");
        SubscriptionRequest {
            spec,
            response_sender,
            reservation,
        }
    }

    /// Register a subscriber through the real admission path (cap check,
    /// gauge/space-counter increments, shard selection, and Register enqueue),
    /// returning the receiving half a client would hold, or `None` when the
    /// dispatcher rejected the registration.
    async fn register(
        service: &mut SubscriptionService,
        spec: SubscriptionSpec,
    ) -> Option<mpsc::Receiver<SubscriptionUpdate>> {
        let reservation = service.counters.try_reserve()?;
        let (response_sender, response_receiver) = oneshot::channel();
        assert!(matches!(
            service.try_admit(SubscriptionRequest {
                spec,
                response_sender,
                reservation,
            }),
            AdmissionState::Accepting
        ));
        response_receiver.await.ok()
    }
    fn inflight_subscribers(metrics: &SubscriptionMetrics) -> i64 {
        ["checkpoint", "transaction", "event"]
            .into_iter()
            .flat_map(|kind| {
                ["true", "false"].into_iter().map(move |filtered| {
                    metrics
                        .inflight_subscribers
                        .with_label_values(&[kind, filtered])
                        .get()
                })
            })
            .sum()
    }

    fn terminations(
        metrics: &SubscriptionMetrics,
        kind: &'static str,
        reason: &'static str,
    ) -> u64 {
        metrics
            .terminations_total
            .with_label_values(&[kind, reason])
            .get()
    }

    /// Synchronously run every shard's pending mailbox messages.
    fn drain(shards: &mut [SubscriptionShard]) {
        for shard in shards {
            shard.drain();
        }
    }

    fn matched_sequence_number(update: SubscriptionUpdate) -> u64 {
        match update {
            SubscriptionUpdate::Matched(matched) => {
                assert!(matches!(matched.matches, SubscriptionMatches::Checkpoint));
                *matched.checkpoint.summary.sequence_number()
            }
            SubscriptionUpdate::WatermarkTick { .. } => {
                panic!("expected a matched checkpoint, got a watermark tick")
            }
        }
    }

    fn matched_transactions(update: SubscriptionUpdate) -> Vec<u32> {
        match update {
            SubscriptionUpdate::Matched(matched) => match matched.matches {
                SubscriptionMatches::Transactions(ranges) => ranges.into_iter().flatten().collect(),
                _ => panic!("expected transaction matches"),
            },
            SubscriptionUpdate::WatermarkTick { .. } => {
                panic!("expected a matched checkpoint, got a watermark tick")
            }
        }
    }

    #[test]
    fn subscriber_reservations_are_bounded_and_released() {
        let counters = Arc::new(SubscriberCounts::new(8));
        let attempted = Arc::new(std::sync::Barrier::new(65));
        let release = Arc::new(std::sync::Barrier::new(65));

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(64);
            for _ in 0..64 {
                let counters = Arc::clone(&counters);
                let attempted = Arc::clone(&attempted);
                let release = Arc::clone(&release);
                handles.push(scope.spawn(move || {
                    let reservation = counters.try_reserve();
                    attempted.wait();
                    release.wait();
                    reservation.is_some()
                }));
            }

            attempted.wait();
            assert_eq!(counters.reserved(), 8);
            release.wait();

            let successful_reservations = handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|success| *success)
                .count();
            assert_eq!(successful_reservations, 8);
        });

        assert_eq!(counters.reserved(), 0);
    }

    #[tokio::test]
    async fn pending_request_reserves_final_subscriber_slot() {
        let counters = Arc::new(SubscriberCounts::new(3));
        let active_reservations = [
            counters.try_reserve().unwrap(),
            counters.try_reserve().unwrap(),
        ];
        let (admission_sender, mailbox) = subscription_admission_channel();
        let handle = SubscriptionServiceHandle {
            admission_sender,
            counters: Arc::clone(&counters),
            metrics: SubscriptionMetrics::new(&prometheus::Registry::new()),
        };

        let pending_handle = handle.clone();
        let pending_registration =
            tokio::spawn(async move { pending_handle.register_subscription(unfiltered()).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while mailbox.len() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending registration did not enter admission");

        assert_eq!(counters.reserved(), 3);
        let rejected = tokio::time::timeout(
            Duration::from_secs(1),
            handle.register_subscription(unfiltered()),
        )
        .await
        .expect("the final subscriber slot is already reserved");
        assert!(rejected.is_none());
        assert_eq!(mailbox.len(), 1);

        drop(mailbox);
        assert!(pending_registration.await.unwrap().is_none());
        assert_eq!(counters.reserved(), 2);

        drop(active_reservations);
        assert_eq!(counters.reserved(), 0);
    }

    #[tokio::test]
    async fn zero_subscriber_limit_rejects_before_ingress() {
        let (_checkpoint_sender, handle) =
            SubscriptionService::build(&prometheus::Registry::new(), None, None, Some(0), Some(1));
        assert!(handle.counters.try_reserve().is_none());
        assert!(handle.register_subscription(unfiltered()).await.is_none());
        assert_eq!(handle.counters.reserved(), 0);
    }

    #[tokio::test]
    async fn public_admission_rejects_before_queue_at_limit() {
        let counters = Arc::new(SubscriberCounts::new(1));
        let held_reservation = counters.try_reserve().unwrap();
        let (admission_sender, mut mailbox) = subscription_admission_channel();
        let handle = SubscriptionServiceHandle {
            admission_sender,
            counters: Arc::clone(&counters),
            metrics: SubscriptionMetrics::new(&prometheus::Registry::new()),
        };

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            handle.register_subscription(unfiltered()),
        )
        .await
        .expect("a saturated service must reject before queueing");
        assert!(result.is_none());
        assert!(matches!(
            mailbox.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        drop(held_reservation);
        assert_eq!(counters.reserved(), 0);
    }

    #[tokio::test]
    async fn closed_public_admission_queue_releases_reservation() {
        let counters = Arc::new(SubscriberCounts::new(1));
        let (admission_sender, mailbox) = subscription_admission_channel();
        drop(mailbox);
        let handle = SubscriptionServiceHandle {
            admission_sender,
            counters: Arc::clone(&counters),
            metrics: SubscriptionMetrics::new(&prometheus::Registry::new()),
        };

        let result = handle.register_subscription(unfiltered()).await;
        assert!(result.is_none());
        assert_eq!(counters.reserved(), 0);
    }

    #[test]
    fn admission_turn_is_bounded_and_drains_ready_requests() {
        let (mut service, _checkpoint_sender, request_sender, _shards) =
            actor_service_with(3, 25, None, 4, DEFAULT_MAX_SUBSCRIBERS);
        let mut response_receivers = Vec::with_capacity(ADMISSION_TURN_LIMIT + 1);

        for _ in 0..=ADMISSION_TURN_LIMIT {
            let (response_sender, response_receiver) = oneshot::channel();
            assert!(
                request_sender
                    .send(subscription_request(
                        &service.counters,
                        unfiltered(),
                        response_sender,
                    ))
                    .is_ok()
            );
            response_receivers.push(response_receiver);
        }

        let first_request = service.admission_mailbox.try_recv().unwrap();
        assert!(matches!(
            service.admit_ready_requests(first_request),
            AdmissionState::Accepting
        ));
        assert_eq!(service.admission_mailbox.len(), 1);

        for response_receiver in response_receivers.iter_mut().take(ADMISSION_TURN_LIMIT) {
            response_receiver
                .try_recv()
                .expect("request should be admitted");
        }
        assert!(matches!(
            response_receivers.last_mut().unwrap().try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));

        let final_request = service.admission_mailbox.try_recv().unwrap();
        assert!(matches!(
            service.admit_ready_requests(final_request),
            AdmissionState::Accepting
        ));
        response_receivers
            .last_mut()
            .unwrap()
            .try_recv()
            .expect("final request should be admitted");
    }

    #[test]
    fn canceled_ready_admission_releases_reservation() {
        let (mut service, _checkpoint_sender, _request_sender, _shards) =
            actor_service_with(1, 25, None, 4, 1);
        let (response_sender, response_receiver) = oneshot::channel();
        let request = subscription_request(&service.counters, unfiltered(), response_sender);
        assert_eq!(service.counters.reserved(), 1);

        drop(response_receiver);
        assert!(matches!(
            service.try_admit(request),
            AdmissionState::Accepting
        ));
        assert_eq!(service.counters.reserved(), 0);
    }

    #[tokio::test]
    async fn checkpoint_backlog_drains_before_registration() {
        // Queue one admission and two checkpoints before starting the actor,
        // making both select branches ready on its first poll.
        let (service, checkpoint_sender, request_sender, mut shards) =
            actor_service_with(1, 25, None, 4, DEFAULT_MAX_SUBSCRIBERS);
        let metrics = service.metrics.clone();
        let (reply_sender, reply_receiver) = oneshot::channel();
        assert!(
            request_sender
                .send(subscription_request(
                    &service.counters,
                    unfiltered(),
                    reply_sender,
                ))
                .is_ok()
        );
        assert_eq!(checkpoint_sender.send(checkpoint(1)).unwrap(), 1);
        assert_eq!(checkpoint_sender.send(checkpoint(2)).unwrap(), 1);

        // The biased select must process both checkpoints before acknowledging
        // the registration.
        let actor = tokio::spawn(service.start());
        let mut receiver = tokio::time::timeout(Duration::from_secs(1), reply_receiver)
            .await
            .expect("admission remained blocked after the checkpoint backlog drained")
            .unwrap();
        assert_eq!(metrics.last_recieved_checkpoint.get(), 2);

        // The subscriber was admitted after checkpoints 1 and 2, so its first
        // deliverable checkpoint is 3.
        assert_eq!(checkpoint_sender.send(checkpoint(3)).unwrap(), 1);
        drop(checkpoint_sender);
        actor.await.unwrap();

        drain(&mut shards);
        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 3);
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn queued_admission_releases_reservation_when_actor_shuts_down() {
        let (service, checkpoint_sender, request_sender, _shards) =
            actor_service_with(1, 25, None, 4, 1);
        let counters = Arc::clone(&service.counters);
        let (response_sender, response_receiver) = oneshot::channel();
        assert!(
            request_sender
                .send(subscription_request(
                    &counters,
                    unfiltered(),
                    response_sender,
                ))
                .is_ok()
        );
        assert_eq!(counters.reserved(), 1);

        drop(checkpoint_sender);
        service.start().await;

        assert!(response_receiver.await.is_err());
        assert_eq!(counters.reserved(), 0);
    }

    #[tokio::test]
    async fn admission_prefers_least_backlogged_shard() {
        // Seed mailbox depths of two, one, and zero. Immediate admission should
        // choose shard 2 because it has the greatest remaining capacity.
        let (mut service, mut shards) = test_service(3);
        for _ in 0..2 {
            assert!(
                service.shards[0]
                    .try_send(ShardMsg::Clear(
                        SubscriptionTerminationReason::ServiceShutdown
                    ))
                    .is_ok()
            );
        }
        assert!(
            service.shards[1]
                .try_send(ShardMsg::Clear(
                    SubscriptionTerminationReason::ServiceShutdown
                ))
                .is_ok()
        );

        // Processing the selected registration also verifies that admission
        // installed its lifecycle accounting.
        let receiver = register(&mut service, unfiltered()).await.unwrap();
        let registration = shards[2].mailbox.try_recv().unwrap();
        assert!(matches!(registration, ShardMsg::Register { .. }));
        shards[2].handle_msg(registration);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 1);
        assert_eq!(inflight_subscribers(&service.metrics), 1);
        drop(receiver);
    }

    #[tokio::test]
    async fn saturated_admission_waits_for_any_shard() {
        // Fill every shard mailbox so the request enters WaitingForShard rather
        // than taking the immediate admission path.
        let (service, _checkpoint_sender, request_sender, mut shards) =
            actor_service_with(2, 25, None, 4, DEFAULT_MAX_SUBSCRIBERS);
        for shard in &service.shards {
            for _ in 0..SHARD_MAILBOX_SIZE {
                assert!(
                    shard
                        .try_send(ShardMsg::Clear(
                            SubscriptionTerminationReason::ServiceShutdown
                        ))
                        .is_ok()
                );
            }
        }

        // Polling once consumes the request and reaches the pending
        // shard-capacity wait before a mailbox slot is released.
        let (reply_sender, reply_receiver) = oneshot::channel();
        assert!(
            request_sender
                .send(subscription_request(
                    &service.counters,
                    unfiltered(),
                    reply_sender,
                ))
                .is_ok()
        );
        let mut actor = Box::pin(service.start());
        assert!(futures::poll!(&mut actor).is_pending());
        let actor = tokio::spawn(actor);

        // Free only shard 1. Waiting on all shard permits must wake and route
        // the registration there even though the cursor starts at shard 0.
        assert!(matches!(
            shards[1].mailbox.try_recv(),
            Ok(ShardMsg::Clear(_))
        ));
        let _receiver = tokio::time::timeout(Duration::from_secs(1), reply_receiver)
            .await
            .expect("admission did not wake when shard capacity became available")
            .unwrap();

        // The registration follows the existing shard 1 messages, while shard
        // 0 remains full and untouched.
        for _ in 1..SHARD_MAILBOX_SIZE {
            assert!(matches!(
                shards[1].mailbox.try_recv(),
                Ok(ShardMsg::Clear(_))
            ));
        }
        assert!(matches!(
            shards[1].mailbox.try_recv(),
            Ok(ShardMsg::Register { .. })
        ));
        assert_eq!(shards[0].mailbox.len(), SHARD_MAILBOX_SIZE);

        actor.abort();
        let _ = actor.await;
    }

    #[tokio::test]
    async fn saturated_head_blocks_newer_request_until_canceled() {
        // Fill every shard, then queue a head request followed by a newer one.
        let (service, _checkpoint_sender, request_sender, mut shards) =
            actor_service_with(2, 25, None, 4, DEFAULT_MAX_SUBSCRIBERS);
        let counters = Arc::clone(&service.counters);
        for shard in &service.shards {
            for _ in 0..SHARD_MAILBOX_SIZE {
                assert!(
                    shard
                        .try_send(ShardMsg::Clear(
                            SubscriptionTerminationReason::ServiceShutdown
                        ))
                        .is_ok()
                );
            }
        }

        let (first_sender, first_reply) = oneshot::channel();
        let (second_sender, second_reply) = oneshot::channel();
        assert!(
            request_sender
                .send(subscription_request(
                    &service.counters,
                    unfiltered(),
                    first_sender,
                ))
                .is_ok()
        );
        assert!(
            request_sender
                .send(subscription_request(
                    &service.counters,
                    unfiltered(),
                    second_sender,
                ))
                .is_ok()
        );
        assert_eq!(counters.reserved(), 2);

        // One poll consumes only the FIFO head and reaches the pending
        // shard-capacity wait, leaving the newer request queued.
        let mut actor = Box::pin(service.start());
        assert!(futures::poll!(&mut actor).is_pending());

        drop(first_reply);
        // The next poll drops the canceled head and moves the newer request
        // into the shard-capacity wait.
        assert!(futures::poll!(&mut actor).is_pending());
        assert_eq!(counters.reserved(), 1);
        let actor = tokio::spawn(actor);

        // Free one shard slot so the newer request can complete admission.
        assert!(matches!(
            shards[0].mailbox.try_recv(),
            Ok(ShardMsg::Clear(_))
        ));
        let _receiver = tokio::time::timeout(Duration::from_secs(1), second_reply)
            .await
            .expect("newer admission remained blocked after canceling the waiting request")
            .unwrap();
        assert_eq!(counters.reserved(), 1);

        actor.abort();
        let _ = actor.await;
        drop(shards);
        assert_eq!(counters.reserved(), 0);
    }

    #[tokio::test]
    async fn run_loop_lag_clears_subscribers_and_resets_sequence_tracker() {
        // Register a subscriber, then overflow a one-slot checkpoint broadcast.
        // The actor first observes Lagged and then the retained checkpoint 2.
        let (mut service, checkpoint_sender, _request_sender, mut shards) =
            actor_service_with(1, 25, None, 1, DEFAULT_MAX_SUBSCRIBERS);
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();
        let counters = Arc::clone(&service.counters);
        let metrics = service.metrics.clone();
        // A deliberately incompatible prior sequence proves lag handling resets
        // the tracker before checkpoint 2 is processed.
        metrics.last_recieved_checkpoint.set(99);

        assert_eq!(checkpoint_sender.send(checkpoint(1)).unwrap(), 1);
        assert_eq!(checkpoint_sender.send(checkpoint(2)).unwrap(), 1);
        let actor = tokio::spawn(service.start());
        drop(checkpoint_sender);
        actor.await.unwrap();

        // Draining applies the queued registration, source-lag clear, retained
        // checkpoint, and shutdown clear in shard FIFO order.
        drain(&mut shards);
        assert!(receiver.recv().await.is_none());
        assert_eq!(counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(inflight_subscribers(&metrics), 0);
        assert_eq!(terminations(&metrics, "checkpoint", "source_lag"), 1);
        // The retained checkpoint can be accepted only because the Lagged
        // branch reset the deliberately incompatible prior sequence number.
        assert_eq!(metrics.last_recieved_checkpoint.get(), 2);
    }

    #[tokio::test]
    async fn labeled_inflight_gauge_covers_all_types_and_filter_states() {
        let (mut service, mut shards) = test_service(1);
        let kinds = [
            SubscriptionKind::Checkpoints,
            SubscriptionKind::Transactions,
            SubscriptionKind::Events,
        ];
        let mut receivers = Vec::new();

        for kind in kinds {
            for filtered in [false, true] {
                let query = filtered.then(|| match kind {
                    SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                        sender_query(addr(0), false)
                    }
                    SubscriptionKind::Events => event_type_query(
                        "0x0000000000000000000000000000000000000000000000000000000000000002::coin::CoinEvent",
                    ),
                });
                receivers.push(
                    register(&mut service, SubscriptionSpec { kind, query })
                        .await
                        .unwrap(),
                );
            }
        }
        drain(&mut shards);

        for kind in kinds {
            for filtered in ["false", "true"] {
                assert_eq!(
                    service
                        .metrics
                        .inflight_subscribers
                        .with_label_values(&[kind.metric_label(), filtered])
                        .get(),
                    1
                );
            }
        }

        drop(receivers);
        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);
        assert_eq!(inflight_subscribers(&service.metrics), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);

        shards[0].handle_msg(ShardMsg::Clear(
            SubscriptionTerminationReason::ServiceShutdown,
        ));
        assert_eq!(inflight_subscribers(&service.metrics), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        for kind in ["checkpoint", "transaction", "event"] {
            assert_eq!(terminations(&service.metrics, kind, "client_closed"), 2);
        }
    }

    #[tokio::test]
    async fn handle_checkpoint_fans_out_in_order() {
        let (mut service, mut shards) = test_service(1);
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();

        service.handle_checkpoint(checkpoint(1)).await;
        service.handle_checkpoint(checkpoint(2)).await;
        drain(&mut shards);

        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 1);
        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 2);
        assert_eq!(shards[0].matcher.len(), 1);
    }

    #[tokio::test]
    async fn handle_checkpoint_drops_departed_subscriber() {
        let (mut service, mut shards) = test_service(1);
        let receiver = register(&mut service, unfiltered()).await.unwrap();
        drop(receiver);

        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);

        assert!(shards[0].matcher.is_empty());
        assert_eq!(inflight_subscribers(&service.metrics), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "client_closed"),
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn handle_checkpoint_waits_for_index_before_delivering() {
        // The first checkpoint is already indexed and does not count as a wait.
        let indexed = Arc::new(AtomicU64::new(4));
        let gate = indexed.clone();
        let (mut service, mut shards) = test_service_with(
            1,
            25,
            Some(Arc::new(move || Some(gate.load(Ordering::SeqCst)))),
        );
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();

        service.handle_checkpoint(checkpoint(4)).await;
        drain(&mut shards);
        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 4);
        assert_eq!(service.metrics.index_wait_seconds.get_sample_count(), 0);
        assert_eq!(service.metrics.index_wait_timeouts_total.get(), 0);

        // Delivery of checkpoint 5 blocks until the index catches up to it.
        {
            let mut deliver = std::pin::pin!(service.handle_checkpoint(checkpoint(5)));
            assert!(
                futures::poll!(&mut deliver).is_pending(),
                "delivery should block while checkpoint 5 is unindexed"
            );
            assert!(receiver.try_recv().is_err());

            indexed.store(5, Ordering::SeqCst);
            tokio::time::advance(INDEX_WAIT_POLL_INTERVAL).await;
            deliver.await;
        }
        drain(&mut shards);

        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 5);
        assert_eq!(service.metrics.index_wait_seconds.get_sample_count(), 1);
        assert!(
            service.metrics.index_wait_seconds.get_sample_sum()
                >= INDEX_WAIT_POLL_INTERVAL.as_secs_f64()
        );
        assert_eq!(service.metrics.index_wait_timeouts_total.get(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn index_wait_timeout_records_and_delivers_with_paused_time() {
        let indexed = Arc::new(AtomicU64::new(4));
        let gate = indexed.clone();
        let (mut service, mut shards) = test_service_with(
            1,
            25,
            Some(Arc::new(move || Some(gate.load(Ordering::SeqCst)))),
        );
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();

        {
            let mut deliver = std::pin::pin!(service.handle_checkpoint(checkpoint(5)));
            assert!(
                futures::poll!(&mut deliver).is_pending(),
                "delivery should block while checkpoint 5 is unindexed"
            );
            tokio::time::advance(INDEX_WAIT_TIMEOUT).await;
            deliver.await;
        }
        drain(&mut shards);

        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 5);
        assert_eq!(service.metrics.index_wait_seconds.get_sample_count(), 1);
        assert!(
            service.metrics.index_wait_seconds.get_sample_sum() >= INDEX_WAIT_TIMEOUT.as_secs_f64()
        );
        assert_eq!(service.metrics.index_wait_timeouts_total.get(), 1);
    }

    #[tokio::test]
    async fn handle_lag_drops_all_subscribers_and_resets_tracker() {
        let (mut service, mut shards) = test_service(2);
        let mut receiver_1 = register(&mut service, unfiltered()).await.unwrap();
        let mut receiver_2 = register(&mut service, unfiltered()).await.unwrap();

        service.handle_checkpoint(checkpoint(5)).await;
        drain(&mut shards);
        assert_eq!(service.metrics.last_recieved_checkpoint.get(), 5);

        service.handle_lag(10).await;
        drain(&mut shards);

        // Clear empties every shard.
        assert!(shards[0].matcher.is_empty());
        assert!(shards[1].matcher.is_empty());
        assert_eq!(inflight_subscribers(&service.metrics), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        // Both subscriptions are torn down, so the client streams close.
        assert!(receiver_1.recv().await.is_some()); // checkpoint 5, then closed
        assert!(receiver_1.recv().await.is_none());
        assert!(receiver_2.recv().await.is_some());
        assert!(receiver_2.recv().await.is_none());
        // The tracker is reset so the next, jumped-ahead checkpoint is not
        // mistaken for an out-of-order delivery (which would panic).
        assert_eq!(service.metrics.last_recieved_checkpoint.get(), 0);
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "source_lag"),
            2
        );
        service.handle_lag(1).await;
        drain(&mut shards);
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "source_lag"),
            2
        );

        let mut receiver_3 = register(&mut service, unfiltered()).await.unwrap();
        service.handle_checkpoint(checkpoint(100)).await;
        drain(&mut shards);
        assert_eq!(
            matched_sequence_number(receiver_3.recv().await.unwrap()),
            100
        );
    }

    #[tokio::test]
    async fn service_shutdown_records_each_subscription_once() {
        let (mut service, mut shards) = test_service(2);
        let receiver_explicit = register(&mut service, unfiltered()).await.unwrap();
        let receiver_fallback = register(&mut service, unfiltered()).await.unwrap();
        drain(&mut shards);
        let metrics = service.metrics.clone();
        let counters = Arc::clone(&service.counters);
        assert_eq!(counters.reserved(), 2);

        service.shards[0]
            .send(ShardMsg::Clear(
                SubscriptionTerminationReason::ServiceShutdown,
            ))
            .await
            .unwrap();
        shards[0].drain();
        assert_eq!(terminations(&metrics, "checkpoint", "service_shutdown"), 1);
        assert_eq!(counters.reserved(), 1);

        let fallback_shard = shards.pop().unwrap();
        drop(service);
        fallback_shard.run().await;
        assert!(receiver_explicit.is_closed());
        assert!(receiver_fallback.is_closed());
        assert_eq!(inflight_subscribers(&metrics), 0);
        assert_eq!(counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(counters.reserved(), 0);
        assert_eq!(counters.filtered_tx.load(Ordering::Relaxed), 0);
        assert_eq!(counters.filtered_event.load(Ordering::Relaxed), 0);
        assert_eq!(terminations(&metrics, "checkpoint", "service_shutdown"), 2);

        drop(shards);
        assert_eq!(terminations(&metrics, "checkpoint", "service_shutdown"), 2);
    }

    #[tokio::test]
    async fn subscribers_on_every_shard_receive_a_matched_checkpoint() {
        let (mut service, mut shards) = test_service(2);
        let mut receivers = Vec::new();
        for _ in 0..4 {
            receivers.push(register(&mut service, unfiltered()).await.unwrap());
        }
        drain(&mut shards);
        // Equal-capacity tie-breaking spreads subscribers across the shards.
        assert_eq!(shards[0].matcher.len(), 2);
        assert_eq!(shards[1].matcher.len(), 2);

        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);
        for receiver in &mut receivers {
            assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 1);
        }
    }

    #[tokio::test]
    async fn filtered_subscribers_match_via_dispatcher_extracted_keys() {
        let (mut service, mut shards) = test_service(2);
        // Include filter on sender 0...
        let mut include_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query: Some(sender_query(addr(0), false)),
            },
        )
        .await
        .unwrap();
        // ...and an exclude-only filter, anchored on the synthetic
        // `TxUniverse` key that dispatcher-side extraction must insert into
        // every transaction's key set.
        let mut exclude_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query: Some(sender_query(addr(0), true)),
            },
        )
        .await
        .unwrap();
        assert_eq!(service.counters.filtered_tx.load(Ordering::Relaxed), 2);

        let checkpoint = checkpoint_with_senders(1, &[0, 1]);
        let tx_lo = checkpoint.summary.data().network_total_transactions
            - checkpoint.transactions.len() as u64;
        service.handle_checkpoint(checkpoint).await;
        drain(&mut shards);

        assert!(matches!(
            include_rx.recv().await.unwrap(),
            SubscriptionUpdate::WatermarkTick {
                checkpoint: 0,
                tx_hi
            } if tx_hi == tx_lo
        ));
        assert!(matches!(
            exclude_rx.recv().await.unwrap(),
            SubscriptionUpdate::WatermarkTick {
                checkpoint: 0,
                tx_hi
            } if tx_hi == tx_lo
        ));

        assert_eq!(
            matched_transactions(include_rx.recv().await.unwrap()),
            vec![0]
        );
        assert_eq!(
            matched_transactions(exclude_rx.recv().await.unwrap()),
            vec![1]
        );
    }

    #[tokio::test]
    async fn configured_cap_is_enforced_globally_across_shards() {
        let max_subscribers = 3;
        let (mut service, _, _, mut shards) = actor_service_with(2, 25, None, 16, max_subscribers);
        let mut receivers = Vec::with_capacity(max_subscribers);
        for _ in 0..max_subscribers {
            receivers.push(register(&mut service, unfiltered()).await.unwrap());
        }
        drain(&mut shards);
        assert_eq!(
            service.counters.total.load(Ordering::Relaxed),
            max_subscribers
        );
        assert_eq!(service.counters.reserved(), max_subscribers);
        assert_eq!(
            inflight_subscribers(&service.metrics),
            max_subscribers as i64
        );
        assert!(!shards[0].matcher.is_empty());
        assert!(!shards[1].matcher.is_empty());

        // Reservation failure occurs before constructing an admission request,
        // so the cap cannot add queued work or lifecycle accounting.
        assert!(register(&mut service, unfiltered()).await.is_none());
        assert_eq!(
            service.counters.total.load(Ordering::Relaxed),
            max_subscribers
        );
        assert_eq!(service.counters.reserved(), max_subscribers);
        assert_eq!(
            inflight_subscribers(&service.metrics),
            max_subscribers as i64
        );
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "service_shutdown"),
            0
        );

        drop(receivers);
        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(service.counters.reserved(), 0);
        assert_eq!(inflight_subscribers(&service.metrics), 0);

        let replacement = register(&mut service, unfiltered()).await.unwrap();
        drain(&mut shards);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 1);
        assert_eq!(service.counters.reserved(), 1);

        drop(replacement);
        service.handle_checkpoint(checkpoint(2)).await;
        drain(&mut shards);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(service.counters.reserved(), 0);
    }

    #[tokio::test]
    async fn unprocessed_register_finalizes_guard_when_shard_drops() {
        let (mut service, shards) = test_service(1);
        let _receiver = register(&mut service, unfiltered()).await.unwrap();
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 1);
        assert_eq!(inflight_subscribers(&service.metrics), 1);
        assert_eq!(service.counters.reserved(), 1);

        // Drop the shard with the Register message still queued in its
        // mailbox: the guard travelling inside the message must finalize
        // with the default service_shutdown reason and rebalance the
        // counts and gauge exactly once.
        drop(shards);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(service.counters.reserved(), 0);
        assert_eq!(inflight_subscribers(&service.metrics), 0);
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "service_shutdown"),
            1
        );
    }

    #[tokio::test]
    async fn registration_racing_dispatch_lands_at_the_next_checkpoint() {
        let (mut service, mut shards) = test_service(1);
        let mut receiver_a = register(&mut service, unfiltered()).await.unwrap();
        service.handle_checkpoint(checkpoint(1)).await;
        let mut receiver_b = register(&mut service, unfiltered()).await.unwrap();
        service.handle_checkpoint(checkpoint(2)).await;
        drain(&mut shards);

        // Per-shard FIFO: B's Register is queued behind checkpoint 1, so B
        // sees only checkpoint 2.
        assert_eq!(matched_sequence_number(receiver_a.recv().await.unwrap()), 1);
        assert_eq!(matched_sequence_number(receiver_a.recv().await.unwrap()), 2);
        assert_eq!(matched_sequence_number(receiver_b.recv().await.unwrap()), 2);
        assert!(receiver_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn space_counters_track_filtered_subscribers() {
        let (mut service, mut shards) = test_service_with(2, 1, None);
        let tx_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query: Some(sender_query(addr(0), false)),
            },
        )
        .await
        .unwrap();
        assert_eq!(service.counters.filtered_tx.load(Ordering::Relaxed), 1);
        let event_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Events,
                query: Some(event_type_query(
                    "0x0000000000000000000000000000000000000000000000000000000000000002::coin::CoinEvent",
                )),
            },
        )
        .await
        .unwrap();
        assert_eq!(service.counters.filtered_event.load(Ordering::Relaxed), 1);

        // Departed clients are finalized by their shard-owned lifecycle
        // guards when the next dispatch observes the closed channels.
        drop(tx_rx);
        drop(event_rx);
        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);

        assert_eq!(service.counters.filtered_tx.load(Ordering::Relaxed), 0);
        assert_eq!(service.counters.filtered_event.load(Ordering::Relaxed), 0);
        assert_eq!(inflight_subscribers(&service.metrics), 0);
    }

    #[tokio::test]
    async fn unfiltered_subscribers_receive_all_matches() {
        let (mut service, mut shards) = test_service(1);
        let mut tx_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query: None,
            },
        )
        .await
        .unwrap();
        let mut ev_rx = register(
            &mut service,
            SubscriptionSpec {
                kind: SubscriptionKind::Events,
                query: None,
            },
        )
        .await
        .unwrap();

        service.handle_checkpoint(checkpoint_with_events(1)).await;
        drain(&mut shards);

        // Unfiltered matches arrive as O(1) "all" payloads that the index
        // accessors expand to every transaction / event.
        let matched = match tx_rx.recv().await.unwrap() {
            SubscriptionUpdate::Matched(matched) => matched,
            SubscriptionUpdate::WatermarkTick { .. } => panic!("expected a match, got a tick"),
        };
        assert!(matches!(
            matched.matches,
            SubscriptionMatches::AllTransactions
        ));
        let tx_count = matched.checkpoint.transactions.len() as u32;
        assert_eq!(
            matched
                .matches
                .transaction_indices(tx_count)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![0, 1]
        );

        let matched = match ev_rx.recv().await.unwrap() {
            SubscriptionUpdate::Matched(matched) => matched,
            SubscriptionUpdate::WatermarkTick { .. } => panic!("expected a match, got a tick"),
        };
        assert!(matches!(matched.matches, SubscriptionMatches::AllEvents));
        assert_eq!(
            matched
                .matches
                .event_indices(&matched.checkpoint)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![(1, 0), (1, 1)]
        );

        // A checkpoint with no transactions yields no frame for either
        // subscriber: empty match sets stay on the watermark-tick path.
        service.handle_checkpoint(checkpoint(2)).await;
        drain(&mut shards);
        assert!(tx_rx.try_recv().is_err());
        assert!(ev_rx.try_recv().is_err());
    }

    #[test]
    fn event_indices_flatten_sparse_matches_in_order() {
        let matches = SubscriptionMatches::Events(vec![(0, vec![1, 2]), (3, vec![0])]);
        let checkpoint = checkpoint(1);
        assert_eq!(
            matches
                .event_indices(&checkpoint)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![(0, 1), (0, 2), (3, 0)]
        );
    }
}
