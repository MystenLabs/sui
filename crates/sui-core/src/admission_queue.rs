// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::ConsensusAdapter;
use arc_swap::ArcSwap;
use mysten_common::debug_fatal;
use mysten_metrics::{COUNT_BUCKETS, spawn_monitored_task};
use prometheus::{
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry,
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sui_macros::handle_fail_point_if;
use sui_network::tonic;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_consensus::{
    ConsensusPosition, ConsensusTransaction, ConsensusTransactionKey,
};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

/// A transaction (or soft bundle) waiting in the admission queue for consensus submission.
pub struct QueueEntry {
    pub gas_price: u64,
    pub transactions: Vec<ConsensusTransaction>,
    pub position_sender: oneshot::Sender<Result<Vec<ConsensusPosition>, tonic::Status>>,
    pub submitter_client_addr: Option<IpAddr>,
    pub enqueue_time: Instant,
}

/// What `PriorityAdmissionQueue::pop_batch_while` does with an examined entry.
pub enum PopAction {
    /// Pop the entry into the included partition.
    Include,
    /// Pop the entry into the excluded partition.
    Exclude,
    /// Leave the entry queued and stop iterating.
    Stop,
}

pub trait AdmissionQueueEntry {
    fn gas_price(&self) -> u64;
    fn transaction_keys(&self) -> impl Iterator<Item = ConsensusTransactionKey>;
    fn notify_evicted(self, min_gas_price: u64);
    fn notify_rejected(self, min_gas_price: u64);
}

/// Outcome of [`PriorityAdmissionQueue::try_insert`]. Any displaced entry is
/// carried here unnotified so that the caller can deliver the notification
/// outside of any held locks.
#[must_use = "call `notify` to deliver the outcome to the displaced entry"]
pub struct InsertOutcome<E> {
    outcome: Option<Outcome<E>>,
    created_at: &'static std::panic::Location<'static>,
}

enum Outcome<E> {
    Inserted {
        /// False if an entry sharing one of the transaction keys was already queued.
        newly_inserted: bool,
        /// The lowest-priced entry evicted to make room, if any.
        evicted: Option<E>,
        /// The inserted entry's gas price.
        gas_price: u64,
    },
    /// Rejected: the queue is full and `min_gas_price` was not met.
    Rejected { entry: E, min_gas_price: u64 },
}

impl<E: AdmissionQueueEntry> InsertOutcome<E> {
    #[track_caller]
    fn new(outcome: Outcome<E>) -> Self {
        Self {
            outcome: Some(outcome),
            created_at: std::panic::Location::caller(),
        }
    }

    /// Notifies the displaced entry, if any, then reports the insert result.
    /// `try_insert(e).notify()` is equivalent to `insert(e)`.
    pub fn notify(mut self) -> SuiResult<bool> {
        match self
            .outcome
            .take()
            .expect("outcome is pending until notified")
        {
            Outcome::Inserted {
                newly_inserted,
                evicted,
                gas_price,
            } => {
                if let Some(evicted) = evicted {
                    evicted.notify_evicted(gas_price);
                }
                Ok(newly_inserted)
            }
            Outcome::Rejected {
                entry,
                min_gas_price,
            } => {
                entry.notify_rejected(min_gas_price);
                Err(
                    SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion {
                        min_gas_price,
                    }
                    .into(),
                )
            }
        }
    }
}

impl<E> Drop for InsertOutcome<E> {
    fn drop(&mut self) {
        if self.outcome.is_some() && !std::thread::panicking() {
            debug_fatal!(
                "InsertOutcome from try_insert at {} dropped without notify",
                self.created_at
            );
        }
    }
}

impl AdmissionQueueEntry for QueueEntry {
    fn gas_price(&self) -> u64 {
        self.gas_price
    }

    fn transaction_keys(&self) -> impl Iterator<Item = ConsensusTransactionKey> {
        self.transactions.iter().map(ConsensusTransaction::key)
    }

    fn notify_evicted(self, min_gas_price: u64) {
        let _ = self
            .position_sender
            .send(Err(tonic::Status::from(SuiError::from(
                SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price },
            ))));
    }

    fn notify_rejected(self, _min_gas_price: u64) {
        // Nothing to do here in push mode. The rejected caller receives the
        // outbid error via the insert result instead.
    }
}

impl QueueEntry {
    #[cfg(test)]
    pub fn new_for_test(
        gas_price: u64,
        position_sender: oneshot::Sender<Result<Vec<ConsensusPosition>, tonic::Status>>,
    ) -> Self {
        Self {
            gas_price,
            transactions: vec![],
            position_sender,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        }
    }
}

/// Prometheus metrics for the admission queue.
pub struct AdmissionQueueMetrics {
    // Pull mode intentionally reuses these admission metrics for its user lane because
    // both modes implement the same admission policy and are mutually exclusive on a
    // validator. This preserves dashboard continuity when switching modes.
    pub queue_depth: IntGauge,
    pub queue_wait_latency: HistogramVec,
    pub evictions: IntCounter,
    pub rejections: IntCounter,
    pub duplicate_inserts: IntCounter,

    pub pool_depth: IntGaugeVec,
    pub pool_bytes: IntGaugeVec,
    pub pool_taken_per_proposal: Histogram,
    pub pool_requeued_on_dropped_ack: IntCounter,
    pub pool_gc_notified: IntCounter,
    pub pool_waiting_inserts: IntGauge,
    pub pool_already_processed: IntCounterVec,
    pub pool_commit_latency: HistogramVec,
    pub pool_abandoned: IntCounterVec,
}

impl AdmissionQueueMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            queue_depth: register_int_gauge_with_registry!(
                "admission_queue_depth",
                "Current number of entries in the admission priority queue",
                registry,
            )
            .unwrap(),
            queue_wait_latency: register_histogram_vec_with_registry!(
                "admission_queue_wait_latency",
                "Time a submission spends waiting in the admission queue or transaction pool before being drained or proposed",
                &["lane"],
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            evictions: register_int_counter_with_registry!(
                "admission_queue_evictions",
                "Number of entries evicted from the admission queue by higher gas price transactions",
                registry,
            )
            .unwrap(),
            rejections: register_int_counter_with_registry!(
                "admission_queue_rejections",
                "Number of transactions rejected because the queue was full and their gas price was too low",
                registry,
            )
            .unwrap(),
            duplicate_inserts: register_int_counter_with_registry!(
                "admission_queue_duplicate_inserts",
                "Transactions admitted to the queue whose ConsensusTransactionKey duplicated an entry already present. Tallied as spam for DoS protection.",
                registry,
            )
            .unwrap(),
            pool_depth: register_int_gauge_vec_with_registry!(
                "consensus_transaction_pool_depth",
                "Current number of entries in each consensus transaction pool lane",
                &["lane"],
                registry,
            )
            .unwrap(),
            pool_bytes: register_int_gauge_vec_with_registry!(
                "consensus_transaction_pool_bytes",
                "Current serialized transaction bytes in each consensus transaction pool lane",
                &["lane"],
                registry,
            )
            .unwrap(),
            pool_taken_per_proposal: register_histogram_with_registry!(
                "consensus_transaction_pool_taken_per_proposal",
                "Transactions taken from the consensus transaction pool per proposal",
                COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            pool_requeued_on_dropped_ack: register_int_counter_with_registry!(
                "consensus_transaction_pool_requeued_on_dropped_ack",
                "Entries requeued because a proposal acknowledgement was dropped",
                registry,
            )
            .unwrap(),
            pool_gc_notified: register_int_counter_with_registry!(
                "consensus_transaction_pool_gc_notified",
                "Block-status subscribers notified that their block was garbage collected",
                registry,
            )
            .unwrap(),
            pool_waiting_inserts: register_int_gauge_with_registry!(
                "consensus_transaction_pool_waiting_inserts",
                "Pool submissions waiting for the matching epoch pool to become available",
                registry,
            )
            .unwrap(),
            pool_already_processed: register_int_counter_vec_with_registry!(
                "consensus_transaction_pool_already_processed",
                "User submissions not proposed because they were already processed elsewhere, by the stage that detected it and the path that processed them",
                &["stage", "method"],
                registry,
            )
            .unwrap(),
            pool_commit_latency: register_histogram_vec_with_registry!(
                "consensus_transaction_pool_commit_latency",
                "Time from insert into the transaction pool to commit of the block that proposed the entry",
                &["lane"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            pool_abandoned: register_int_counter_vec_with_registry!(
                "consensus_transaction_pool_abandoned",
                "Pool entries dropped at proposal time because their submitter stopped waiting",
                &["lane"],
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        Self::new(&Registry::new())
    }
}

/// Bounded priority queue that orders transactions by gas price. Uses a BTreeMap
/// for efficient access at both ends: lowest gas price (for eviction) and highest
/// gas price (for draining to consensus). Entries at the same gas price are FIFO.
pub struct PriorityAdmissionQueue<E: AdmissionQueueEntry> {
    capacity: usize,
    map: BTreeMap<u64, VecDeque<E>>,
    /// Number of queue entries per transaction key, for duplicate detection.
    queued_keys: HashMap<ConsensusTransactionKey, u32>,
    total_len: usize,
    metrics: Arc<AdmissionQueueMetrics>,
}

impl<E: AdmissionQueueEntry> PriorityAdmissionQueue<E> {
    pub fn new(capacity: usize, metrics: Arc<AdmissionQueueMetrics>) -> Self {
        Self {
            capacity,
            map: BTreeMap::new(),
            queued_keys: HashMap::new(),
            total_len: 0,
            metrics,
        }
    }

    pub fn len(&self) -> usize {
        self.total_len
    }

    pub fn min_gas_price(&self) -> Option<u64> {
        self.map.first_key_value().map(|(&k, _)| k)
    }

    /// On success, returns `Ok(true)` or `Ok(false)` to indicate whether the
    /// value was newly inserted. Returns `Err` if the queue was full and the
    /// tx's gas price was not high enough to evict an existing entry.
    pub fn insert(&mut self, entry: E) -> SuiResult<bool> {
        self.try_insert(entry).notify()
    }

    /// Same as `insert`, without notifying displaced entries. The evicted entry (on
    /// success) or the refused entry itself (on rejection) is handed back in the
    /// outcome for the caller to notify manually.
    #[track_caller]
    pub fn try_insert(&mut self, entry: E) -> InsertOutcome<E> {
        let keys: Vec<_> = entry.transaction_keys().collect();
        let newly_inserted = !keys.iter().any(|k| self.queued_keys.contains_key(k));
        if !newly_inserted {
            self.metrics.duplicate_inserts.inc();
        }

        let gas_price = entry.gas_price();
        if self.total_len < self.capacity {
            self.push_entry(entry, keys, false);
            return InsertOutcome::new(Outcome::Inserted {
                newly_inserted,
                evicted: None,
                gas_price,
            });
        }

        let min_gas_price = self.min_gas_price().unwrap();
        if gas_price > min_gas_price {
            let evicted = self.evict_lowest();
            self.push_entry(entry, keys, false);
            self.metrics.evictions.inc();
            return InsertOutcome::new(Outcome::Inserted {
                newly_inserted,
                evicted: Some(evicted),
                gas_price,
            });
        }

        self.metrics.rejections.inc();
        InsertOutcome::new(Outcome::Rejected {
            entry,
            min_gas_price,
        })
    }

    /// Pop up to `count` entries, highest gas price first.
    /// Within the same gas price, entries are returned in FIFO order.
    pub fn pop_batch(&mut self, count: usize) -> Vec<E> {
        let mut remaining = count;
        self.pop_batch_while(|_| {
            if remaining == 0 {
                PopAction::Stop
            } else {
                remaining -= 1;
                PopAction::Include
            }
        })
        .0
    }

    pub fn into_entries(mut self) -> Vec<E> {
        let len = self.len();
        self.pop_batch(len)
    }

    /// Pop entries highest gas price first (FIFO within a price level) until
    /// `action` returns `Stop` or the queue is empty. The entry that stopped
    /// iteration stays queued. Popped entries are returned partitioned into
    /// those the callback chose to `Include` and those to `Exclude`.
    pub fn pop_batch_while(
        &mut self,
        mut action: impl FnMut(&mut E) -> PopAction,
    ) -> (Vec<E>, Vec<E>) {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        'levels: while let Some(mut last) = self.map.last_entry() {
            let deque = last.get_mut();
            while let Some(entry) = deque.front_mut() {
                let action = action(entry);
                if matches!(action, PopAction::Stop) {
                    break 'levels;
                }
                let entry = deque.pop_front().expect("front entry must exist");
                self.total_len -= 1;
                match action {
                    PopAction::Include => included.push(entry),
                    PopAction::Exclude => excluded.push(entry),
                    PopAction::Stop => unreachable!("Stop breaks out above"),
                }
            }
            last.remove();
        }
        for entry in included.iter().chain(&excluded) {
            self.remove_keys(entry);
        }
        self.metrics.queue_depth.set(self.total_len as i64);
        (included, excluded)
    }

    pub fn is_empty(&self) -> bool {
        self.total_len == 0
    }

    pub fn reinsert_front(&mut self, entry: E) {
        let keys: Vec<_> = entry.transaction_keys().collect();
        self.push_entry(entry, keys, true);
    }

    fn push_entry(&mut self, entry: E, keys: Vec<ConsensusTransactionKey>, front: bool) {
        for key in keys {
            *self.queued_keys.entry(key).or_insert(0) += 1;
        }
        let level = self.map.entry(entry.gas_price()).or_default();
        if front {
            level.push_front(entry);
        } else {
            level.push_back(entry);
        }
        self.total_len += 1;
        self.metrics.queue_depth.set(self.total_len as i64);
    }

    fn evict_lowest(&mut self) -> E {
        let evicted = {
            let mut first = self
                .map
                .first_entry()
                .expect("evict_lowest called on empty queue");
            let deque = first.get_mut();
            let evicted = deque.pop_front().unwrap();
            if deque.is_empty() {
                first.remove();
            }
            evicted
        };
        self.remove_keys(&evicted);
        self.total_len -= 1;
        evicted
    }

    fn remove_keys(&mut self, entry: &E) {
        for key in entry.transaction_keys() {
            let std::collections::hash_map::Entry::Occupied(mut slot) = self.queued_keys.entry(key)
            else {
                debug_fatal!("remove_keys on absent key");
                continue;
            };
            *slot.get_mut() -= 1;
            if *slot.get() == 0 {
                slot.remove();
            }
        }
    }
}

/// Command sent from RPC handlers to the admission queue actor via mpsc channel.
struct InsertCommand {
    entry: QueueEntry,
    response: oneshot::Sender<SuiResult<bool>>,
}

/// Cloneable handle for submitting transactions to the admission queue actor.
/// Held by RPC handlers; the actor runs in a separate spawned task.
#[derive(Clone)]
pub struct AdmissionQueueHandle {
    sender: mpsc::Sender<InsertCommand>,
    /// The moment the queue last submitted an entry to consensus.
    last_drain: Arc<Mutex<Instant>>,
    queue_depth: Arc<AtomicUsize>,
    failover_timeout: Duration,
}

impl AdmissionQueueHandle {
    /// Returns true if the queue has been non-empty for longer than
    /// `failover_timeout` without any drain to consensus. Callers should
    /// bypass the queue entirely when this is true.
    pub fn failover_tripped(&self) -> bool {
        if self.queue_depth.load(Ordering::Relaxed) == 0 {
            return false;
        }
        self.last_drain.lock().unwrap().elapsed() > self.failover_timeout
    }

    /// Returns `(position_receiver, newly_inserted)` on admission. Returns `Err` on outbid
    /// rejection.
    pub async fn try_insert(
        &self,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<(
        oneshot::Receiver<Result<Vec<ConsensusPosition>, tonic::Status>>,
        bool,
    )> {
        let (position_tx, position_rx) = oneshot::channel();
        let entry = QueueEntry {
            gas_price,
            transactions,
            position_sender: position_tx,
            submitter_client_addr,
            enqueue_time: Instant::now(),
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = InsertCommand {
            entry,
            response: resp_tx,
        };

        self.sender
            .send(cmd)
            .await
            .map_err(|_| SuiError::from(SuiErrorKind::TooManyTransactionsPendingConsensus))?;

        let newly_inserted = resp_rx
            .await
            .map_err(|_| SuiError::from(SuiErrorKind::TooManyTransactionsPendingConsensus))??;

        Ok((position_rx, newly_inserted))
    }
}

/// Manages the lifecycle of per-epoch admission queue actors.
/// Holds immutable config and shared metrics; call `spawn()` each epoch
/// with the new epoch store to create a fresh actor and handle.
pub struct AdmissionQueueManager {
    capacity: usize,
    failover_timeout: Duration,
    metrics: Arc<AdmissionQueueMetrics>,
    consensus_adapter: Arc<ConsensusAdapter>,
    slot_freed_notify: Arc<tokio::sync::Notify>,
}

impl AdmissionQueueManager {
    pub fn new(
        consensus_adapter: Arc<ConsensusAdapter>,
        metrics: Arc<AdmissionQueueMetrics>,
        capacity_fraction: f64,
        failover_timeout: Duration,
        slot_freed_notify: Arc<tokio::sync::Notify>,
    ) -> Self {
        let max_pending = consensus_adapter.max_pending_transactions();
        let capacity = (max_pending as f64 * capacity_fraction) as usize;
        assert!(
            capacity > 0,
            "admission_queue_capacity_fraction ({capacity_fraction}) * max_pending_transactions ({max_pending}) must be > 0"
        );
        Self {
            capacity,
            failover_timeout,
            metrics,
            consensus_adapter,
            slot_freed_notify,
        }
    }

    pub fn new_for_tests(
        consensus_adapter: Arc<ConsensusAdapter>,
        slot_freed_notify: Arc<tokio::sync::Notify>,
    ) -> Self {
        Self {
            capacity: 10_000,
            failover_timeout: Duration::from_secs(30),
            metrics: Arc::new(AdmissionQueueMetrics::new_for_tests()),
            consensus_adapter,
            slot_freed_notify,
        }
    }

    pub fn metrics(&self) -> &Arc<AdmissionQueueMetrics> {
        &self.metrics
    }

    /// Spawns a new per-epoch admission queue actor and returns a handle.
    /// The previous actor shuts down when its handle is dropped.
    pub fn spawn(&self, epoch_store: Arc<AuthorityPerEpochStore>) -> AdmissionQueueHandle {
        let last_drain = Arc::new(Mutex::new(Instant::now()));
        let queue_depth = Arc::new(AtomicUsize::new(0));

        let (sender, receiver) = mpsc::channel(self.capacity.max(1024));

        let event_loop = AdmissionQueueEventLoop {
            receiver,
            queue: PriorityAdmissionQueue::new(self.capacity, self.metrics.clone()),
            consensus_adapter: self.consensus_adapter.clone(),
            slot_freed_notify: self.slot_freed_notify.clone(),
            epoch_store,
            last_drain: last_drain.clone(),
            queue_depth: queue_depth.clone(),
            last_published_depth: 0,
        };
        spawn_monitored_task!(event_loop.run());

        AdmissionQueueHandle {
            sender,
            last_drain,
            queue_depth,
            failover_timeout: self.failover_timeout,
        }
    }
}

/// Shared handle to a live admission queue. Holds the manager (for spawning a
/// fresh per-epoch actor on reconfig), the per-epoch `ArcSwap` handle, and the
/// cached (config-derived) bypass threshold. Cloned cheaply by `Arc`; passed
/// both to `ValidatorService` (for hot-path routing) and through
/// `ValidatorComponents` (for epoch rotation).
#[derive(Clone)]
pub struct AdmissionQueueContext {
    manager: Arc<AdmissionQueueManager>,
    swap: Arc<ArcSwap<AdmissionQueueHandle>>,
}

impl AdmissionQueueContext {
    pub fn spawn(
        manager: Arc<AdmissionQueueManager>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Self {
        let initial_handle = manager.spawn(epoch_store);
        let swap = Arc::new(ArcSwap::new(Arc::new(initial_handle)));
        Self { manager, swap }
    }

    /// Spawns a new per-epoch actor and atomically replaces the current handle.
    /// The old actor shuts down when its handle is dropped.
    pub fn rotate_for_epoch(&self, epoch_store: Arc<AuthorityPerEpochStore>) {
        self.swap.store(Arc::new(self.manager.spawn(epoch_store)));
    }

    pub(crate) fn load(&self) -> arc_swap::Guard<Arc<AdmissionQueueHandle>> {
        self.swap.load()
    }
}

/// Per-epoch event loop that owns the priority queue and drains entries
/// to consensus as capacity becomes available.
struct AdmissionQueueEventLoop {
    receiver: mpsc::Receiver<InsertCommand>,
    queue: PriorityAdmissionQueue<QueueEntry>,
    consensus_adapter: Arc<ConsensusAdapter>,
    slot_freed_notify: Arc<tokio::sync::Notify>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    last_drain: Arc<Mutex<Instant>>,
    queue_depth: Arc<AtomicUsize>,
    last_published_depth: usize,
}

impl AdmissionQueueEventLoop {
    pub async fn run(mut self) {
        loop {
            self.process_pending_inserts();
            self.publish_queue_depth();

            if !handle_fail_point_if("admission_queue_disable_drain")
                && !self.queue.is_empty()
                && self.has_consensus_capacity()
            {
                self.drain_batch();
                self.publish_queue_depth();
                continue;
            }

            if self.queue.is_empty() {
                // Nothing to drain — just wait for a new insert.
                match self.receiver.recv().await {
                    Some(cmd) => self.handle_insert(cmd),
                    None => {
                        debug!("Admission queue actor shutting down");
                        break;
                    }
                }
                continue;
            }

            // Queue has entries but consensus is at capacity. Wait for either
            // a new insert or a freed inflight slot.
            // Register the notified future BEFORE re-checking capacity to avoid
            // missing notifications.
            let notify = self.slot_freed_notify.clone();
            let slot_freed = notify.notified();
            tokio::pin!(slot_freed);

            self.process_pending_inserts();
            if !handle_fail_point_if("admission_queue_disable_drain")
                && !self.queue.is_empty()
                && self.has_consensus_capacity()
            {
                continue;
            }

            tokio::select! {
                biased;

                result = self.receiver.recv() => {
                    match result {
                        Some(cmd) => self.handle_insert(cmd),
                        None => {
                            debug!("Admission queue actor shutting down");
                            break;
                        }
                    }
                }

                _ = &mut slot_freed => {}
            }
        }
    }

    fn publish_queue_depth(&mut self) {
        let len = self.queue.len();
        if len != self.last_published_depth {
            self.queue_depth.store(len, Ordering::Relaxed);
            self.last_published_depth = len;
        }
    }

    fn process_pending_inserts(&mut self) {
        while let Ok(cmd) = self.receiver.try_recv() {
            self.handle_insert(cmd);
        }
    }

    fn has_consensus_capacity(&self) -> bool {
        self.consensus_adapter.num_inflight_transactions()
            < u64::try_from(self.consensus_adapter.max_pending_transactions()).unwrap()
    }

    fn drain_batch(&mut self) {
        let max_pending = u64::try_from(self.consensus_adapter.max_pending_transactions()).unwrap();
        let available =
            max_pending.saturating_sub(self.consensus_adapter.num_inflight_transactions());
        let entries = self.queue.pop_batch(usize::try_from(available).unwrap());
        if entries.is_empty() {
            return;
        }
        for entry in entries {
            self.queue
                .metrics
                .queue_wait_latency
                .with_label_values(&["user"])
                .observe(entry.enqueue_time.elapsed().as_secs_f64());
            let adapter = self.consensus_adapter.clone();
            let es = self.epoch_store.clone();
            spawn_monitored_task!(submit_queue_entry(entry, adapter, es));
        }
        *self.last_drain.lock().unwrap() = Instant::now();
    }

    fn handle_insert(&mut self, cmd: InsertCommand) {
        let _ = cmd.response.send(self.queue.insert(cmd.entry));
    }
}

async fn submit_queue_entry(
    entry: QueueEntry,
    consensus_adapter: Arc<ConsensusAdapter>,
    epoch_store: Arc<AuthorityPerEpochStore>,
) {
    let _ = entry.position_sender.send(
        consensus_adapter
            .submit_and_get_positions(
                entry.transactions,
                &epoch_store,
                entry.submitter_client_addr,
            )
            .await
            .map_err(tonic::Status::from),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_entry(
        gas_price: u64,
    ) -> (
        QueueEntry,
        oneshot::Receiver<Result<Vec<ConsensusPosition>, tonic::Status>>,
    ) {
        let (tx, rx) = oneshot::channel();
        (QueueEntry::new_for_test(gas_price, tx), rx)
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "dropped without notify")]
    fn dropped_insert_outcome_without_notify_panics() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(1, metrics);
        let (entry, _rx) = make_test_entry(100);
        let _ = q.try_insert(entry);
    }

    #[test]
    fn try_insert_notify_matches_insert() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(1, metrics);
        let (low, mut low_rx) = make_test_entry(100);
        let (high, _) = make_test_entry(200);
        let (rejected, mut rejected_rx) = make_test_entry(50);

        assert!(q.try_insert(low).notify().unwrap());
        // Eviction is delivered only by `notify`.
        let outcome = q.try_insert(high);
        assert!(low_rx.try_recv().is_err());
        assert!(outcome.notify().unwrap());
        assert!(matches!(
            low_rx.try_recv(),
            Ok(Err(status)) if status.message().contains("minimum gas price required: 200")
        ));

        assert!(matches!(
            q.try_insert(rejected).notify().unwrap_err().as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 200 }
        ));
        assert_eq!(q.len(), 1);
        assert!(rejected_rx.try_recv().is_err());
    }

    fn build_queue(capacity: usize, gas_prices: &[u64]) -> PriorityAdmissionQueue<QueueEntry> {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(capacity, metrics);
        for &gp in gas_prices {
            let (entry, _) = make_test_entry(gp);
            q.insert(entry).unwrap();
        }
        q
    }

    #[test]
    fn test_insert_within_capacity() {
        let q = build_queue(3, &[100, 200, 50]);
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn test_pop_batch_while_partitions_and_stops() {
        let mut q = build_queue(10, &[100, 200, 200, 50, 75]);

        let (included, excluded) = q.pop_batch_while(|entry| match entry.gas_price {
            price if price < 80 => PopAction::Stop,
            price if price < 150 => PopAction::Exclude,
            _ => PopAction::Include,
        });
        assert_eq!(
            included.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![200, 200]
        );
        assert_eq!(
            excluded.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![100]
        );
        // The entry that stopped iteration stays queued, as does everything behind it.
        assert_eq!(q.len(), 2);
        assert_eq!(q.min_gas_price(), Some(50));
        let (rest, _) = q.pop_batch_while(|_| PopAction::Include);
        assert_eq!(rest.len(), 2);
        assert!(q.is_empty());
    }

    #[test]
    fn test_eviction_when_full() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(2, metrics);

        let (e1, mut r1) = make_test_entry(100);
        let (e2, _) = make_test_entry(200);
        let (e3, _) = make_test_entry(300);

        q.insert(e1).unwrap();
        q.insert(e2).unwrap();
        assert_eq!(q.len(), 2);

        assert!(q.insert(e3).is_ok());
        assert_eq!(q.len(), 2);
        // Evicted entry's caller receives an explicit outbid error.
        let r1_result = r1.try_recv().expect("evicted entry must be signalled");
        assert!(matches!(r1_result, Err(ref status) if status.message().contains("outbid")));
    }

    #[test]
    fn test_rejection_when_full_and_low_price() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(2, metrics);

        let (e1, _) = make_test_entry(100);
        let (e2, _) = make_test_entry(200);
        let (e3, mut r3) = make_test_entry(50);

        q.insert(e1).unwrap();
        q.insert(e2).unwrap();

        assert!(matches!(
            q.insert(e3).unwrap_err().as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 100 }
        ));
        assert_eq!(q.len(), 2);
        assert!(r3.try_recv().is_err());
    }

    #[test]
    fn test_pop_batch() {
        let mut q = build_queue(5, &[100, 300, 200]);
        let batch = q.pop_batch(2);
        assert_eq!(batch.len(), 2);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_min_gas_price() {
        let q = build_queue(5, &[200, 100, 300]);
        assert_eq!(q.min_gas_price(), Some(100));
    }

    #[test]
    fn test_gasless_tx_evicted_first() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(2, metrics);

        let (gasless, mut r_gasless) = make_test_entry(0);
        let (normal, _) = make_test_entry(1000);
        let (high, _) = make_test_entry(2000);

        q.insert(gasless).unwrap();
        q.insert(normal).unwrap();

        assert!(q.insert(high).is_ok());
        let gasless_result = r_gasless
            .try_recv()
            .expect("evicted gasless entry must be signalled");
        assert!(matches!(gasless_result, Err(ref status) if status.message().contains("outbid")));
        assert_eq!(q.min_gas_price(), Some(1000));
    }

    #[test]
    fn test_pop_batch_returns_highest_gas_price_first() {
        let mut q = build_queue(5, &[100, 500, 200, 400, 300]);
        let batch = q.pop_batch(5);
        let gas_prices: Vec<u64> = batch.iter().map(|e| e.gas_price).collect();
        assert_eq!(gas_prices, vec![500, 400, 300, 200, 100]);
    }

    #[test]
    fn test_equal_gas_price_rejected_when_full() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(1, metrics);

        let (e1, _) = make_test_entry(100);
        let (e2, _) = make_test_entry(100);

        q.insert(e1).unwrap();
        assert!(matches!(
            q.insert(e2).unwrap_err().as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 100 }
        ));
    }

    fn make_dup_entry(
        gas_price: u64,
        tx: ConsensusTransaction,
    ) -> (
        QueueEntry,
        oneshot::Receiver<Result<Vec<ConsensusPosition>, tonic::Status>>,
    ) {
        let (position_tx, position_rx) = oneshot::channel();
        let entry = QueueEntry {
            gas_price,
            transactions: vec![tx],
            position_sender: position_tx,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        };
        (entry, position_rx)
    }

    #[test]
    fn test_duplicate_transaction_admitted_and_flagged() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        let tx = ConsensusTransaction::new_end_of_publish(AuthorityName::ZERO);

        let (entry1, _rx1) = make_dup_entry(100, tx.clone());
        assert!(q.insert(entry1).unwrap());
        assert_eq!(q.len(), 1);

        // Same transaction again — admitted, but flagged as not-fresh so the
        // RPC layer can tally it as spam for DoS protection.
        let (entry2, _rx2) = make_dup_entry(100, tx.clone());
        assert!(!q.insert(entry2).unwrap());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn test_duplicate_key_counter_decrements_on_pop() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        let tx = ConsensusTransaction::new_end_of_publish(AuthorityName::ZERO);

        // Insert two copies of the same tx.
        let (entry1, _rx1) = make_dup_entry(100, tx.clone());
        q.insert(entry1).unwrap();
        let (entry2, _rx2) = make_dup_entry(100, tx.clone());
        assert!(!q.insert(entry2).unwrap());

        // Pop one copy. The key's counter should drop to 1 — a fresh insert
        // should still be flagged as not-fresh against the remaining copy.
        let batch = q.pop_batch(1);
        assert_eq!(batch.len(), 1);
        let (entry3, _rx3) = make_dup_entry(100, tx.clone());
        assert!(!q.insert(entry3).unwrap());
        assert_eq!(q.len(), 2);

        // Drain both remaining entries. The counter should hit 0 and the key
        // should be removed — a subsequent insert is fresh again.
        let _ = q.pop_batch(q.len());
        assert!(q.is_empty());
        let (entry4, _rx4) = make_dup_entry(100, tx);
        assert!(q.insert(entry4).unwrap());
    }

    #[test]
    fn test_duplicate_key_counter_decrements_on_evict() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(2, metrics);

        let tx = ConsensusTransaction::new_end_of_publish(AuthorityName::ZERO);

        // Fill queue with two copies of `tx` at price 100.
        let (entry1, _rx1) = make_dup_entry(100, tx.clone());
        q.insert(entry1).unwrap();
        let (entry2, _rx2) = make_dup_entry(100, tx.clone());
        q.insert(entry2).unwrap();
        assert_eq!(q.len(), 2);

        // Evict one dup with a higher-priced non-dup.
        let (filler, _) = make_test_entry(200);
        q.insert(filler).unwrap();
        assert_eq!(q.len(), 2);

        // Evict the remaining dup with another non-dup. After both dups are
        // evicted, the counter should hit 0 and re-inserting `tx` is not a
        // duplicate.
        let (filler2, _) = make_test_entry(300);
        q.insert(filler2).unwrap();

        let (entry3, _rx3) = make_dup_entry(500, tx);
        assert!(q.insert(entry3).unwrap());
    }

    #[test]
    fn test_duplicate_key_counter_restored_on_reinsert_front() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        let tx = ConsensusTransaction::new_end_of_publish(AuthorityName::ZERO);

        let (entry1, _rx1) = make_dup_entry(100, tx.clone());
        q.insert(entry1).unwrap();

        // Popping removes the key; reinserting the popped entry (the dropped-proposal
        // requeue path) must restore it, so a copy of `tx` is flagged as a duplicate.
        let popped = q.pop_batch(1).pop().unwrap();
        q.reinsert_front(popped);
        assert_eq!(q.len(), 1);
        let (entry2, _rx2) = make_dup_entry(100, tx.clone());
        assert!(!q.insert(entry2).unwrap());

        // Draining everything zeroes the counter and the same tx is fresh again.
        let _ = q.pop_batch(q.len());
        assert!(q.is_empty());
        let (entry3, _rx3) = make_dup_entry(100, tx);
        assert!(q.insert(entry3).unwrap());
    }

    #[tokio::test]
    async fn test_actor_shuts_down_when_handle_dropped() {
        use crate::authority::test_authority_builder::TestAuthorityBuilder;
        use crate::checkpoints::CheckpointStore;
        use crate::consensus_adapter::ConsensusAdapterMetrics;
        use crate::mysticeti_adapter::LazyMysticetiClient;
        use sui_types::base_types::AuthorityName;

        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(LazyMysticetiClient::new()),
            CheckpointStore::new_for_tests(),
            AuthorityName::ZERO,
            100_000,
            100_000,
            ConsensusAdapterMetrics::new_test(),
            Arc::new(tokio::sync::Notify::new()),
        ));

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let (sender, receiver) = mpsc::channel(100);
        let slot_freed_notify = Arc::new(tokio::sync::Notify::new());

        let event_loop = AdmissionQueueEventLoop {
            receiver,
            queue: PriorityAdmissionQueue::new(100, metrics),
            consensus_adapter,
            slot_freed_notify,
            epoch_store,
            last_drain: Arc::new(Mutex::new(Instant::now())),
            queue_depth: Arc::new(AtomicUsize::new(0)),
            last_published_depth: 0,
        };

        let handle = tokio::spawn(event_loop.run());

        // Drop the sender — this closes the channel.
        drop(sender);

        // The actor should exit promptly.
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("actor did not shut down within timeout")
            .expect("actor task panicked");
    }

    async fn build_consensus_adapter(
        max_pending_transactions: usize,
    ) -> (
        Arc<ConsensusAdapter>,
        Arc<AuthorityPerEpochStore>,
        Arc<tokio::sync::Notify>,
    ) {
        use crate::authority::test_authority_builder::TestAuthorityBuilder;
        use crate::checkpoints::CheckpointStore;
        use crate::consensus_adapter::ConsensusAdapterMetrics;
        use crate::mysticeti_adapter::LazyMysticetiClient;
        use sui_types::base_types::AuthorityName;

        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let slot_freed_notify = Arc::new(tokio::sync::Notify::new());
        let adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(LazyMysticetiClient::new()),
            CheckpointStore::new_for_tests(),
            AuthorityName::ZERO,
            max_pending_transactions,
            100_000,
            ConsensusAdapterMetrics::new_test(),
            slot_freed_notify.clone(),
        ));
        (adapter, epoch_store, slot_freed_notify)
    }

    #[tokio::test]
    async fn test_failover_tripped_when_actor_stalls() {
        // Construct a handle with a tiny failover window and no running actor.
        // Failover requires queue_depth > 0, so simulate a non-empty queue.
        let handle = AdmissionQueueHandle {
            sender: mpsc::channel(1).0,
            last_drain: Arc::new(Mutex::new(Instant::now())),
            queue_depth: Arc::new(AtomicUsize::new(1)),
            failover_timeout: Duration::from_millis(10),
        };
        assert!(!handle.failover_tripped());
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(handle.failover_tripped());

        // An empty queue is never a failover, even if last_drain is stale.
        handle.queue_depth.store(0, Ordering::Relaxed);
        assert!(!handle.failover_tripped());
    }

    #[tokio::test]
    async fn test_idle_actor_does_not_trip_failover() {
        // A healthy actor with an empty queue must never trip failover, even
        // after long idle periods while blocked on `receiver.recv()`.
        let (adapter, epoch_store, notify) = build_consensus_adapter(100_000).await;
        let manager = AdmissionQueueManager::new(
            adapter,
            Arc::new(AdmissionQueueMetrics::new_for_tests()),
            0.5,
            Duration::from_millis(10),
            notify,
        );
        let handle = manager.spawn(epoch_store);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.failover_tripped(),
            "idle actor with empty queue must not trip failover"
        );
    }

    /// If `drain_batch` is entered but consensus has zero slots available
    /// (the inflight count raced past `max_pending_transactions` between the
    /// `has_consensus_capacity` check and the read inside `drain_batch`), no
    /// entries are popped and `last_drain` must NOT advance — otherwise a
    /// truly stuck drainer would be hidden from the failover check.
    #[tokio::test]
    async fn test_drain_batch_does_not_bump_last_drain_when_no_slots() {
        let (adapter, epoch_store, notify) = build_consensus_adapter(0).await;
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let (_sender, receiver) = mpsc::channel(10);

        let mut queue = PriorityAdmissionQueue::new(10, metrics.clone());
        let (entry, _rx) = make_test_entry(100);
        assert!(queue.insert(entry).is_ok());
        assert_eq!(queue.len(), 1);

        let last_drain = Arc::new(Mutex::new(Instant::now()));
        let before = *last_drain.lock().unwrap();

        let mut event_loop = AdmissionQueueEventLoop {
            receiver,
            queue,
            consensus_adapter: adapter,
            slot_freed_notify: notify,
            epoch_store,
            last_drain: last_drain.clone(),
            queue_depth: Arc::new(AtomicUsize::new(0)),
            last_published_depth: 0,
        };

        // Sleep so that if drain_batch erroneously stamps Instant::now() the
        // stored value would differ from `before`.
        tokio::time::sleep(Duration::from_millis(20)).await;

        event_loop.drain_batch();

        assert_eq!(event_loop.queue.len(), 1, "no entries should be drained");
        assert_eq!(
            *last_drain.lock().unwrap(),
            before,
            "last_drain must not advance when drain_batch drained nothing"
        );
    }
}
