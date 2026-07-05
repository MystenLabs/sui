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
const MAILBOX_SIZE: usize = 128;
const SUBSCRIPTION_CHANNEL_SIZE: usize = 256;
const MAX_SUBSCRIBERS: usize = 1024;
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
}

impl SubscriptionServiceHandle {
    pub async fn register_subscription(
        &self,
        spec: SubscriptionSpec,
    ) -> Option<mpsc::Receiver<SubscriptionUpdate>> {
        let (sender, receiver) = oneshot::channel();
        let request = SubscriptionRequest { spec, sender };
        self.sender.send(request).await.ok()?;

        receiver.await.ok()
    }
}

/// Live subscriber counts shared dispatcher-to-shards, and the source of
/// truth for admission control (`total` vs [`MAX_SUBSCRIBERS`]) and for
/// gating expensive per-checkpoint key extraction (the per-space filtered
/// counts).
#[derive(Default)]
struct SubscriberCounts {
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
    },
    /// Lag teardown: drop every subscriber on this shard.
    Clear,
}

/// One worker task owning a partition of the subscribers: it evaluates their
/// filters against pre-extracted checkpoint keys and delivers their updates.
struct SubscriptionShard {
    mailbox: mpsc::Receiver<ShardMsg>,
    matcher: matcher::SubscriptionMatcher,
    /// Checkpoints a subscriber may go without any frame before a standalone
    /// watermark tick is delivered (see `RpcConfig::subscription_watermark_interval`).
    watermark_interval: u32,
    filtered_subscriber_counts: Arc<SubscriberCounts>,
    metrics: SubscriptionMetrics,
}

impl SubscriptionShard {
    async fn run(mut self) {
        // Once the dispatcher drops our sender this yields `None` and the
        // shard exits, dropping its matcher and closing its client streams.
        while let Some(msg) = self.mailbox.recv().await {
            self.handle_msg(msg);
        }
    }

    fn handle_msg(&mut self, msg: ShardMsg) {
        match msg {
            ShardMsg::Register { spec, sender } => {
                self.matcher.insert(spec, sender);
            }
            ShardMsg::Checkpoint(checkpoint, keys) => {
                let (before_tx, before_ev) = (
                    self.matcher.filtered_tx_subs(),
                    self.matcher.filtered_event_subs(),
                );
                let departed = self.matcher.dispatch_with_keys(
                    &checkpoint,
                    &keys,
                    self.watermark_interval,
                    &self.metrics,
                );
                // Mirror departures into the shared counts.
                self.filtered_subscriber_counts
                    .total
                    .fetch_sub(departed, Ordering::Relaxed);
                self.filtered_subscriber_counts.filtered_tx.fetch_sub(
                    before_tx - self.matcher.filtered_tx_subs(),
                    Ordering::Relaxed,
                );
                self.filtered_subscriber_counts.filtered_event.fetch_sub(
                    before_ev - self.matcher.filtered_event_subs(),
                    Ordering::Relaxed,
                );
            }
            ShardMsg::Clear => {
                self.filtered_subscriber_counts
                    .filtered_tx
                    .fetch_sub(self.matcher.filtered_tx_subs(), Ordering::Relaxed);
                self.filtered_subscriber_counts
                    .filtered_event
                    .fetch_sub(self.matcher.filtered_event_subs(), Ordering::Relaxed);
                let dropped = self.matcher.clear();
                self.filtered_subscriber_counts
                    .total
                    .fetch_sub(dropped, Ordering::Relaxed);
                self.metrics.inflight_subscribers.sub(dropped as i64);
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
    /// Round-robin registration targets: one mailbox per shard task, each
    /// owning a partition of the subscribers.
    shards: Vec<mpsc::Sender<ShardMsg>>,
    /// Cursor for round-robin routing of new registrations.
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
    /// `watermark_interval` and `shards` are tuning knobs; `None` picks the
    /// service defaults ([`DEFAULT_WATERMARK_INTERVAL`], [`default_shards`]).
    pub fn build(
        registry: &prometheus::Registry,
        indexed_checkpoint: Option<IndexedCheckpointFn>,
        watermark_interval: Option<u32>,
        shards: Option<u32>,
    ) -> (
        broadcast::Sender<Arc<Checkpoint>>,
        SubscriptionServiceHandle,
    ) {
        let metrics = SubscriptionMetrics::new(registry);
        let (checkpoint_sender, checkpoint_mailbox) = broadcast::channel(CHECKPOINT_MAILBOX_SIZE);
        let (subscription_request_sender, mailbox) = mpsc::channel(MAILBOX_SIZE);

        let counters = Arc::new(SubscriberCounts::default());
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
                    filtered_subscriber_counts: counters.clone(),
                    metrics: metrics.clone(),
                }
                .run(),
            );
            shard_senders.push(sender);
        }

        tokio::spawn(
            Self {
                checkpoint_mailbox,
                mailbox,
                shards: shard_senders,
                next_shard: 0,
                counters,
                indexed_checkpoint,
                metrics,
            }
            .start(),
        );

        (
            checkpoint_sender,
            SubscriptionServiceHandle {
                sender: subscription_request_sender,
            },
        )
    }

    async fn start(mut self) {
        // Start main loop.
        loop {
            tokio::select! {
                result = self.checkpoint_mailbox.recv() => {
                    match result {
                        Ok(checkpoint) => self.handle_checkpoint(checkpoint).await,
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            self.handle_lag(skipped).await;
                        }
                        // Once the executor drops the sender this yields `Closed`
                        // and we can terminate the event loop.
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                },
                maybe_message = self.mailbox.recv() => {
                    // Once all handles to our mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop
                    if let Some(message) = maybe_message {
                        self.handle_message(message).await;
                    } else {
                        break;
                    }
                },
            }
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

        // No live or in-flight subscribers anywhere: skip fan-out. (`total`
        // is incremented at admission, before the Register is enqueued, so
        // total == 0 proves no pending registration either.)
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

        let deadline = Instant::now() + INDEX_WAIT_TIMEOUT;
        loop {
            sleep(INDEX_WAIT_POLL_INTERVAL).await;
            if indexed().is_some_and(|hi| hi >= sequence_number) {
                return;
            }
            if Instant::now() >= deadline {
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
        // pre-gap checkpoints and ahead of all post-gap ones. The shards
        // decrement the inflight gauge for the subscribers they drop.
        for shard in &self.shards {
            shard
                .send(ShardMsg::Clear)
                .await
                .expect("subscription shard terminated unexpectedly");
        }
        // The next delivered checkpoint jumps ahead by `skipped`; reset the
        // in-order tracker so it is not mistaken for an out-of-order delivery.
        self.metrics.last_recieved_checkpoint.set(0);
    }

    async fn handle_message(&mut self, request: SubscriptionRequest) {
        // Check if we've reached the limit to the number of subscribers we
        // can have at one time. `counters.total` is incremented here at
        // admission and decremented by the shards on departure/clear, so it
        // counts live + in-flight subscribers across every shard.
        if self.counters.total.load(Ordering::Relaxed) >= MAX_SUBSCRIBERS {
            trace!(
                "failed to register new subscriber: hit maximum number of subscribers {}",
                MAX_SUBSCRIBERS
            );
            // Dropping the oneshot makes `register_subscription` return
            // `None` -> `Status::unavailable`.
            return;
        }

        let (sender, reciever) = mpsc::channel(SUBSCRIPTION_CHANNEL_SIZE);
        match request.sender.send(reciever) {
            Ok(()) => {
                trace!("successfully registered new subscriber");
                self.counters.total.fetch_add(1, Ordering::Relaxed);
                self.metrics.inflight_subscribers.inc();
                if request.spec.query.is_some() {
                    match request.spec.kind {
                        SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                            self.counters.filtered_tx.fetch_add(1, Ordering::Relaxed)
                        }
                        SubscriptionKind::Events => {
                            self.counters.filtered_event.fetch_add(1, Ordering::Relaxed)
                        }
                    };
                }
                let shard = self.next_shard;
                self.next_shard = (self.next_shard + 1) % self.shards.len();
                self.shards[shard]
                    .send(ShardMsg::Register {
                        spec: request.spec,
                        sender,
                    })
                    .await
                    .expect("subscription shard terminated unexpectedly");
            }
            Err(e) => {
                trace!("failed to register new subscriber: {e:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use move_core_types::account_address::AccountAddress;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use sui_rpc::proto::sui::rpc::v2alpha as proto;
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
        let (_checkpoint_sender, checkpoint_mailbox) = broadcast::channel(16);
        let (_request_sender, mailbox) = mpsc::channel(16);
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
                filtered_subscriber_counts: counters.clone(),
                metrics: metrics.clone(),
            });
        }

        let service = SubscriptionService {
            checkpoint_mailbox,
            mailbox,
            shards: shard_senders,
            next_shard: 0,
            counters,
            indexed_checkpoint,
            metrics,
        };
        (service, shards)
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
        service
            .handle_message(SubscriptionRequest { spec, sender })
            .await;
        receiver.await.ok()
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
        assert_eq!(service.metrics.inflight_subscribers.get(), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn handle_checkpoint_waits_for_index_before_delivering() {
        // The index reports it has committed through checkpoint 4; checkpoint 5
        // is not yet indexed.
        let indexed = Arc::new(AtomicU64::new(4));
        let gate = indexed.clone();
        let (mut service, mut shards) = test_service_with(
            1,
            25,
            Some(Arc::new(move || Some(gate.load(Ordering::SeqCst)))),
        );
        let mut receiver = register(&mut service, unfiltered()).await.unwrap();

        // Delivery of checkpoint 5 blocks until the index catches up to it.
        let mut deliver = std::pin::pin!(service.handle_checkpoint(checkpoint(5)));
        assert!(
            futures::poll!(&mut deliver).is_pending(),
            "delivery should block while checkpoint 5 is unindexed"
        );
        assert!(receiver.try_recv().is_err());

        // Once the index reaches 5, delivery completes.
        indexed.store(5, Ordering::SeqCst);
        deliver.await;
        drain(&mut shards);
        assert_eq!(matched_sequence_number(receiver.recv().await.unwrap()), 5);
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
        assert_eq!(service.metrics.inflight_subscribers.get(), 0);
        assert_eq!(service.counters.total.load(Ordering::Relaxed), 0);
        // Both subscriptions are torn down, so the client streams close.
        assert!(receiver_1.recv().await.is_some()); // checkpoint 5, then closed
        assert!(receiver_1.recv().await.is_none());
        assert!(receiver_2.recv().await.is_some());
        assert!(receiver_2.recv().await.is_none());
        // The tracker is reset so the next, jumped-ahead checkpoint is not
        // mistaken for an out-of-order delivery (which would panic).
        assert_eq!(service.metrics.last_recieved_checkpoint.get(), 0);

        let mut receiver_3 = register(&mut service, unfiltered()).await.unwrap();
        service.handle_checkpoint(checkpoint(100)).await;
        drain(&mut shards);
        assert_eq!(
            matched_sequence_number(receiver_3.recv().await.unwrap()),
            100
        );
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
    async fn cap_is_enforced_globally_across_shards() {
        let (mut service, mut shards) = test_service(2);
        let mut receivers = Vec::with_capacity(MAX_SUBSCRIBERS);
        for i in 0..MAX_SUBSCRIBERS {
            // Keep each 64-slot shard mailbox from filling: the dispatcher's
            // bounded send would otherwise block with no spawned shard task.
            if i % 32 == 0 {
                drain(&mut shards);
            }
            receivers.push(register(&mut service, unfiltered()).await.unwrap());
        }
        drain(&mut shards);
        assert_eq!(
            service.counters.total.load(Ordering::Relaxed),
            MAX_SUBSCRIBERS
        );
        // The gauge mirrors the admission count for observability.
        assert_eq!(
            service.metrics.inflight_subscribers.get(),
            MAX_SUBSCRIBERS as i64
        );
        assert!(!shards[0].matcher.is_empty());
        assert!(!shards[1].matcher.is_empty());

        // The cap is global across shards: the next registration is rejected
        // (the dispatcher drops the reply oneshot).
        assert!(register(&mut service, unfiltered()).await.is_none());
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

        // Departed clients: the next frame (a tick, at interval 1) fails to
        // send, the shards drop both subscribers and mirror the decrements
        // back into the shared counters.
        drop(tx_rx);
        drop(event_rx);
        service.handle_checkpoint(checkpoint(1)).await;
        drain(&mut shards);

        assert_eq!(service.counters.filtered_tx.load(Ordering::Relaxed), 0);
        assert_eq!(service.counters.filtered_event.load(Ordering::Relaxed), 0);
        assert_eq!(service.metrics.inflight_subscribers.get(), 0);
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
