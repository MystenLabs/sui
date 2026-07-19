// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::SubscriptionMetrics;
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
/// Pending admissions are only a [`SubscriptionSpec`] and a oneshot sender,
/// roughly hundreds of bytes each, and have no stream or delivery semantics
/// until admitted. This deep lane absorbs cold-join bursts for approximately
/// 1 MiB at capacity. A full lane remains explicit, retryable backpressure to
/// the caller.
const ADMISSION_MAILBOX_SIZE: usize = 4096;
/// Each admission costs microseconds: a cap check, bounded shard-capacity
/// probes, and non-awaiting sends. This batch therefore stays well below one
/// millisecond between checkpoint polls.
const ADMISSION_BATCH_SIZE: usize = 128;
/// Pending admissions retry periodically without hot-spinning when every
/// shard remains saturated. Checkpoint arrival can preempt this timer.
const RETAINED_ADMISSION_RETRY_INTERVAL: Duration = Duration::from_millis(1);
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
    counters: Arc<SubscriberCounts>,
    inflight_subscribers: prometheus::IntGauge,
    terminations_total: prometheus::IntCounterVec,
    termination_reason: SubscriptionTerminationReason,
}

impl SubscriptionLifecycleGuard {
    pub(crate) fn new(
        kind: SubscriptionKind,
        filtered: bool,
        counters: Arc<SubscriberCounts>,
        metrics: &SubscriptionMetrics,
    ) -> Self {
        counters.total.fetch_add(1, Ordering::Relaxed);
        if filtered {
            match kind {
                SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                    counters.filtered_tx.fetch_add(1, Ordering::Relaxed);
                }
                SubscriptionKind::Events => {
                    counters.filtered_event.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        let inflight_subscribers = metrics
            .inflight_subscribers
            .with_label_values(&[kind.metric_label(), if filtered { "true" } else { "false" }]);
        inflight_subscribers.inc();

        Self {
            kind,
            filtered,
            counters,
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
        self.counters.total.fetch_sub(1, Ordering::Relaxed);
        if self.filtered {
            match self.kind {
                SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                    self.counters.filtered_tx.fetch_sub(1, Ordering::Relaxed);
                }
                SubscriptionKind::Events => {
                    self.counters.filtered_event.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
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
    sender: oneshot::Sender<mpsc::Receiver<SubscriptionUpdate>>,
}

#[derive(Clone)]
pub struct SubscriptionServiceHandle {
    sender: mpsc::Sender<SubscriptionRequest>,
    metrics: SubscriptionMetrics,
}

impl SubscriptionServiceHandle {
    pub async fn register_subscription(
        &self,
        spec: SubscriptionSpec,
    ) -> Option<mpsc::Receiver<SubscriptionUpdate>> {
        let (sender, receiver) = oneshot::channel();
        let request = SubscriptionRequest { spec, sender };
        self.sender.try_send(request).ok()?;

        receiver.await.ok()
    }

    pub(crate) fn stream_metrics(
        &self,
        kind: SubscriptionKind,
    ) -> crate::metrics::SubscriptionStreamMetrics {
        self.metrics.stream_metrics(kind.metric_label())
    }
}

/// Live subscriber counts shared dispatcher-to-shards, and the source of
/// truth for admission control (`total` vs [`MAX_SUBSCRIBERS`]) and for
/// gating expensive per-checkpoint key extraction (the per-space filtered
/// counts).
#[derive(Default)]
pub(crate) struct SubscriberCounts {
    total: AtomicUsize,
    filtered_tx: AtomicUsize,
    filtered_event: AtomicUsize,
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
    mailbox: mpsc::Receiver<SubscriptionRequest>,
    /// An admission removed from `mailbox` but waiting for shard capacity.
    /// This slot is always serviced before receiving a newer request.
    pending_admission: Option<SubscriptionRequest>,
    /// Round-robin registration targets: one mailbox per shard task, each
    /// owning a partition of the subscribers.
    shards: Vec<mpsc::Sender<ShardMsg>>,
    /// Cursor for round-robin routing of new registrations.
    next_shard: usize,
    /// Filtered-subscriber counts per key space, shared with the shards;
    /// gates per-checkpoint key extraction.
    counters: Arc<SubscriberCounts>,
    /// Global admission limit across all shards.
    max_subscribers: usize,

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
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(CHECKPOINT_MAILBOX_SIZE);
        let (subscription_request_sender, mailbox) = mpsc::channel(ADMISSION_MAILBOX_SIZE);
        let handle = SubscriptionServiceHandle {
            sender: subscription_request_sender,
            metrics: metrics.clone(),
        };

        let counters = Arc::new(SubscriberCounts::default());
        let max_subscribers = max_subscribers.unwrap_or(DEFAULT_MAX_SUBSCRIBERS);
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
                mailbox,
                pending_admission: None,
                shards: shard_senders,
                next_shard: 0,
                counters,
                max_subscribers,
                indexed_checkpoint,
                metrics,
            }
            .start(),
        );

        (checkpoint_sender, handle)
    }

    async fn start(mut self) {
        let mut admission_open = true;
        loop {
            tokio::select! {
                biased;

                result = self.checkpoint_mailbox.recv() => {
                    match result {
                        Ok(checkpoint) => {
                            self.handle_checkpoint(checkpoint).await;
                            self.handle_admission_turn(None, &mut admission_open);
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            self.handle_lag(skipped).await;
                        }
                        // Once the executor drops the sender this yields `Closed`
                        // and we can terminate the event loop.
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                },
                // A short timer prevents hot-spinning while every shard stays
                // full. Checkpoints remain above this branch and can retry the
                // pending request immediately after fan-out.
                _ = sleep(RETAINED_ADMISSION_RETRY_INTERVAL),
                    if self.pending_admission.is_some() =>
                {
                    self.handle_admission_turn(None, &mut admission_open);
                },
                maybe_message = self.mailbox.recv(),
                    if admission_open && self.pending_admission.is_none() =>
                {
                    if let Some(message) = maybe_message {
                        self.handle_admission_turn(Some(message), &mut admission_open);
                    } else {
                        // No more admissions can arrive, but established
                        // subscribers remain live until the checkpoint source
                        // closes.
                        admission_open = false;
                    }
                },
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

    /// Runs one bounded admission turn. `first` is a request already received
    /// by the select loop; a retained request, when present, always precedes it.
    fn handle_admission_turn(
        &mut self,
        mut first: Option<SubscriptionRequest>,
        admission_open: &mut bool,
    ) {
        for _ in 0..ADMISSION_BATCH_SIZE {
            let request = if let Some(request) = self.pending_admission.take() {
                request
            } else if let Some(request) = first.take() {
                request
            } else if *admission_open {
                match self.mailbox.try_recv() {
                    Ok(request) => request,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        *admission_open = false;
                        break;
                    }
                }
            } else {
                break;
            };

            if let Err(request) = self.handle_message(request) {
                self.pending_admission = Some(request);
                break;
            }
        }
    }

    /// Returns the request only when every shard mailbox is full. The caller
    /// retains that request and retries it before receiving a newer admission.
    fn handle_message(&mut self, request: SubscriptionRequest) -> Result<(), SubscriptionRequest> {
        // The guard increments `counters.total` after the client accepts the
        // receiver and owns that admission until the shard finalizes it.
        if self.counters.total.load(Ordering::Relaxed) >= self.max_subscribers {
            trace!(
                "failed to register new subscriber: hit maximum number of subscribers {}",
                self.max_subscribers
            );
            // Dropping the oneshot makes `register_subscription` return
            // `None` -> `Status::unavailable`.
            return Ok(());
        }

        for offset in 0..self.shards.len() {
            let shard = (self.next_shard + offset) % self.shards.len();
            match self.shards[shard].try_reserve() {
                Ok(reservation) => {
                    let (sender, receiver) = mpsc::channel(SUBSCRIPTION_CHANNEL_SIZE);
                    if request.sender.send(receiver).is_err() {
                        trace!("failed to register new subscriber: request was cancelled");
                        return Ok(());
                    }

                    trace!("successfully registered new subscriber");
                    let kind = request.spec.kind;
                    let filtered = request.spec.query.is_some();
                    let guard = SubscriptionLifecycleGuard::new(
                        kind,
                        filtered,
                        Arc::clone(&self.counters),
                        &self.metrics,
                    );
                    reservation.send(ShardMsg::Register {
                        spec: request.spec,
                        sender,
                        guard,
                    });
                    self.next_shard = (shard + 1) % self.shards.len();
                    return Ok(());
                }
                Err(mpsc::error::TrySendError::Full(())) => {}
                Err(mpsc::error::TrySendError::Closed(())) => {
                    panic!("subscription shard terminated unexpectedly");
                }
            }
        }

        trace!("retaining new subscriber until a subscription shard has capacity");
        Err(request)
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
        let (service, _checkpoint_sender, _request_sender, shards) =
            actor_service_with(shard_count, watermark_interval, indexed_checkpoint, 16, 16);
        (service, shards)
    }

    fn actor_service_with(
        shard_count: usize,
        watermark_interval: u32,
        indexed_checkpoint: Option<IndexedCheckpointFn>,
        checkpoint_capacity: usize,
        admission_capacity: usize,
    ) -> (
        SubscriptionService,
        broadcast::Sender<Arc<Checkpoint>>,
        mpsc::Sender<SubscriptionRequest>,
        Vec<SubscriptionShard>,
    ) {
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(checkpoint_capacity);
        let (request_sender, mailbox) = mpsc::channel(admission_capacity);
        let metrics = SubscriptionMetrics::new(&prometheus::Registry::new());
        let counters = Arc::new(SubscriberCounts::default());

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
            mailbox,
            pending_admission: None,
            shards: shard_senders,
            next_shard: 0,
            counters,
            max_subscribers: DEFAULT_MAX_SUBSCRIBERS,
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

    /// Register a subscriber through the real admission path (cap check,
    /// gauge/space-counter increments, round-robin Register enqueue),
    /// returning the receiving half a client would hold, or `None` when the
    /// dispatcher rejected the registration.
    async fn register(
        service: &mut SubscriptionService,
        spec: SubscriptionSpec,
    ) -> Option<mpsc::Receiver<SubscriptionUpdate>> {
        let (sender, receiver) = oneshot::channel();
        assert!(
            service
                .handle_message(SubscriptionRequest { spec, sender })
                .is_ok()
        );
        receiver.await.ok()
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

    #[tokio::test]
    async fn full_public_admission_queue_returns_none_promptly() {
        let (sender, _mailbox) = mpsc::channel(1);
        let (occupied_sender, _occupied_receiver) = oneshot::channel();
        assert!(
            sender
                .try_send(SubscriptionRequest {
                    spec: unfiltered(),
                    sender: occupied_sender,
                })
                .is_ok()
        );
        let handle = SubscriptionServiceHandle {
            sender,
            metrics: SubscriptionMetrics::new(&prometheus::Registry::new()),
        };

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            handle.register_subscription(unfiltered()),
        )
        .await
        .expect("a full admission queue must not make the caller wait");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn prequeued_checkpoint_wins_and_registration_starts_at_next_checkpoint() {
        let (service, checkpoint_sender, request_sender, mut shards) =
            actor_service_with(1, 25, None, 4, 4);
        let (reply_sender, reply_receiver) = oneshot::channel();
        assert!(
            request_sender
                .try_send(SubscriptionRequest {
                    spec: unfiltered(),
                    sender: reply_sender,
                })
                .is_ok()
        );
        assert_eq!(checkpoint_sender.send(checkpoint(1)).unwrap(), 1);
        assert_eq!(checkpoint_sender.send(checkpoint(2)).unwrap(), 1);

        let actor = tokio::spawn(service.start());
        drop(checkpoint_sender);
        actor.await.unwrap();

        let mut receiver = reply_receiver.await.unwrap();
        drain(&mut shards);
        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 2);
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn post_checkpoint_turn_batches_admissions_without_starving_ready_checkpoints() {
        const REQUESTS: usize = 5;

        let (service, checkpoint_sender, request_sender, mut shards) =
            actor_service_with(1, 25, None, 8, REQUESTS);
        let metrics = service.metrics.clone();
        let mut replies = Vec::new();
        for _ in 0..REQUESTS {
            let (reply_sender, reply_receiver) = oneshot::channel();
            assert!(
                request_sender
                    .try_send(SubscriptionRequest {
                        spec: unfiltered(),
                        sender: reply_sender,
                    })
                    .is_ok()
            );
            replies.push(reply_receiver);
        }
        for sequence_number in 1..=3 {
            assert_eq!(
                checkpoint_sender.send(checkpoint(sequence_number)).unwrap(),
                1
            );
        }

        let actor = tokio::spawn(service.start());
        drop(checkpoint_sender);
        actor.await.unwrap();

        let mut receivers = Vec::new();
        for reply in replies {
            receivers.push(reply.await.unwrap());
        }
        drain(&mut shards);
        for mut receiver in receivers {
            assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 2);
            assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 3);
            assert!(receiver.recv().await.is_none());
        }
        assert_eq!(metrics.last_recieved_checkpoint.get(), 3);
    }

    #[tokio::test]
    async fn shard_admission_spills_over_when_preferred_shard_is_full() {
        let (mut service, mut shards) = test_service(2);
        for _ in 0..SHARD_MAILBOX_SIZE {
            assert!(
                service.shards[0]
                    .try_send(ShardMsg::Clear(
                        SubscriptionTerminationReason::ServiceShutdown
                    ))
                    .is_ok()
            );
        }

        let (reply_sender, reply_receiver) = oneshot::channel();
        assert!(
            service
                .handle_message(SubscriptionRequest {
                    spec: unfiltered(),
                    sender: reply_sender,
                })
                .is_ok()
        );
        let receiver = reply_receiver.await.unwrap();
        let registration = shards[1].mailbox.try_recv().unwrap();
        assert!(matches!(registration, ShardMsg::Register { .. }));
        shards[1].handle_msg(registration);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 1);
        assert_eq!(inflight_subscribers(&service.metrics), 1);
        drop(receiver);
    }

    #[tokio::test]
    async fn saturated_admission_is_retained_ahead_of_newer_requests() {
        let (mut service, _checkpoint_sender, request_sender, mut shards) =
            actor_service_with(1, 25, None, 4, 4);
        for _ in 0..SHARD_MAILBOX_SIZE {
            assert!(
                service.shards[0]
                    .try_send(ShardMsg::Clear(
                        SubscriptionTerminationReason::ServiceShutdown
                    ))
                    .is_ok()
            );
        }

        let (first_sender, mut first_reply) = oneshot::channel();
        let (second_sender, mut second_reply) = oneshot::channel();
        assert!(
            request_sender
                .try_send(SubscriptionRequest {
                    spec: unfiltered(),
                    sender: first_sender,
                })
                .is_ok()
        );
        assert!(
            request_sender
                .try_send(SubscriptionRequest {
                    spec: SubscriptionSpec {
                        kind: SubscriptionKind::Events,
                        query: None,
                    },
                    sender: second_sender,
                })
                .is_ok()
        );

        let mut admission_open = true;
        service.handle_admission_turn(None, &mut admission_open);
        assert!(service.pending_admission.is_some());
        assert_eq!(service.mailbox.len(), 1);
        assert!(matches!(
            first_reply.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            second_reply.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));

        assert!(matches!(
            shards[0].mailbox.try_recv(),
            Ok(ShardMsg::Clear(_))
        ));
        service.handle_admission_turn(None, &mut admission_open);
        let _first_receiver = first_reply.try_recv().unwrap();
        assert!(matches!(
            second_reply.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(service.pending_admission.is_some());
        assert_eq!(service.mailbox.len(), 0);

        for _ in 1..SHARD_MAILBOX_SIZE {
            assert!(matches!(
                shards[0].mailbox.try_recv(),
                Ok(ShardMsg::Clear(_))
            ));
        }
        match shards[0].mailbox.try_recv().unwrap() {
            ShardMsg::Register { spec, .. } => {
                assert_eq!(spec.kind, SubscriptionKind::Checkpoints);
            }
            _ => panic!("expected the retained registration first"),
        }

        service.handle_admission_turn(None, &mut admission_open);
        let _second_receiver = second_reply.try_recv().unwrap();
        assert!(service.pending_admission.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn retained_admission_retry_parks_between_capacity_probes() {
        let (service, _checkpoint_sender, request_sender, mut shards) =
            actor_service_with(1, 25, None, 4, 4);
        for _ in 0..SHARD_MAILBOX_SIZE {
            assert!(
                service.shards[0]
                    .try_send(ShardMsg::Clear(
                        SubscriptionTerminationReason::ServiceShutdown
                    ))
                    .is_ok()
            );
        }

        let (reply_sender, mut reply_receiver) = oneshot::channel();
        assert!(
            request_sender
                .try_send(SubscriptionRequest {
                    spec: unfiltered(),
                    sender: reply_sender,
                })
                .is_ok()
        );
        let actor = tokio::spawn(service.start());

        // Paused time auto-advances only while every task is parked. This
        // sleep would hang if the pending-admission retry loop hot-spun.
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(matches!(
            reply_receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));

        assert!(matches!(
            shards[0].mailbox.try_recv(),
            Ok(ShardMsg::Clear(_))
        ));
        tokio::time::advance(RETAINED_ADMISSION_RETRY_INTERVAL).await;
        let _receiver = reply_receiver.await.unwrap();

        actor.abort();
        let _ = actor.await;
    }

    #[tokio::test]
    async fn run_loop_lag_clears_subscribers_and_resets_sequence_tracker() {
        let (mut service, checkpoint_sender, _request_sender, mut shards) =
            actor_service_with(1, 25, None, 1, 4);
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();
        let counters = Arc::clone(&service.counters);
        let metrics = service.metrics.clone();
        metrics.last_recieved_checkpoint.set(99);

        assert_eq!(checkpoint_sender.send(checkpoint(1)).unwrap(), 1);
        assert_eq!(checkpoint_sender.send(checkpoint(2)).unwrap(), 1);
        let actor = tokio::spawn(service.start());
        drop(checkpoint_sender);
        actor.await.unwrap();

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

        service.shards[0]
            .send(ShardMsg::Clear(
                SubscriptionTerminationReason::ServiceShutdown,
            ))
            .await
            .unwrap();
        shards[0].drain();
        assert_eq!(terminations(&metrics, "checkpoint", "service_shutdown"), 1);

        let fallback_shard = shards.pop().unwrap();
        drop(service);
        fallback_shard.run().await;
        assert!(receiver_explicit.is_closed());
        assert!(receiver_fallback.is_closed());
        assert_eq!(inflight_subscribers(&metrics), 0);
        assert_eq!(counters.total.load(Ordering::Relaxed), 0);
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
        // Round-robin registration spreads subscribers across the shards.
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
        let (mut service, mut shards) = test_service(2);
        let max_subscribers = 3;
        service.max_subscribers = max_subscribers;
        let mut receivers = Vec::with_capacity(max_subscribers);
        for _ in 0..max_subscribers {
            receivers.push(register(&mut service, unfiltered()).await.unwrap());
        }
        drain(&mut shards);
        assert_eq!(
            service.counters.total.load(Ordering::Relaxed),
            max_subscribers
        );
        // The gauge mirrors the admission count for observability.
        assert_eq!(
            inflight_subscribers(&service.metrics),
            max_subscribers as i64
        );
        assert!(!shards[0].matcher.is_empty());
        assert!(!shards[1].matcher.is_empty());

        // The cap is global across shards: the next registration is rejected
        // (the dispatcher drops the reply oneshot). Rejection creates no
        // guard, so lifecycle accounting is untouched.
        assert!(register(&mut service, unfiltered()).await.is_none());
        assert_eq!(
            service.counters.total.load(Ordering::Relaxed),
            max_subscribers
        );
        assert_eq!(
            inflight_subscribers(&service.metrics),
            max_subscribers as i64
        );
        assert_eq!(
            terminations(&service.metrics, "checkpoint", "service_shutdown"),
            0
        );
    }

    #[tokio::test]
    async fn unprocessed_register_finalizes_guard_when_shard_drops() {
        let (mut service, shards) = test_service(1);
        let _receiver = register(&mut service, unfiltered()).await.unwrap();
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 1);
        assert_eq!(inflight_subscribers(&service.metrics), 1);

        // Drop the shard with the Register message still queued in its
        // mailbox: the guard travelling inside the message must finalize
        // with the default service_shutdown reason and rebalance the
        // counts and gauge exactly once.
        drop(shards);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
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
