// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::ConsensusAdapter;
use arc_swap::ArcSwap;
use mysten_common::debug_fatal;
use mysten_metrics::spawn_monitored_task;
use prometheus::{
    Histogram, IntCounter, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry,
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
    pub queue_depth: IntGauge,
    pub queue_wait_latency: Histogram,
    pub evictions: IntCounter,
    pub rejections: IntCounter,
    pub duplicate_inserts: IntCounter,
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
            queue_wait_latency: register_histogram_with_registry!(
                "admission_queue_wait_latency",
                "Time a transaction spends waiting in the admission queue before being drained",
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
        }
    }

    pub fn new_for_tests() -> Self {
        Self::new(&Registry::new())
    }
}

/// Bounded priority queue that orders transactions by gas price. Uses a BTreeMap
/// for efficient access at both ends: lowest gas price (for eviction) and highest
/// gas price (for draining to consensus). Entries at the same gas price are FIFO.
pub struct PriorityAdmissionQueue {
    capacity: usize,
    map: BTreeMap<u64, VecDeque<QueueEntry>>,
    /// Number of queue entries per transaction key, for duplicate detection.
    queued_keys: HashMap<ConsensusTransactionKey, u32>,
    total_len: usize,
    metrics: Arc<AdmissionQueueMetrics>,
}

impl PriorityAdmissionQueue {
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
    pub fn insert(&mut self, entry: QueueEntry) -> SuiResult<bool> {
        let keys: Vec<_> = entry.transactions.iter().map(|t| t.key()).collect();
        let newly_inserted = !keys.iter().any(|k| self.queued_keys.contains_key(k));
        if !newly_inserted {
            self.metrics.duplicate_inserts.inc();
        }

        if self.total_len < self.capacity {
            self.push_entry(entry, keys);
            self.metrics.queue_depth.set(self.total_len as i64);
            return Ok(newly_inserted);
        }

        let min_price = self.min_gas_price().unwrap();
        if entry.gas_price > min_price {
            let evicter_price = entry.gas_price;
            let evicted = self.evict_lowest();
            self.push_entry(entry, keys);
            self.metrics.evictions.inc();
            // Signal the evicted entry's caller so `position_rx.await` returns
            // a distinct outbid error rather than a generic RecvError.
            let _ = evicted
                .position_sender
                .send(Err(tonic::Status::from(SuiError::from(
                    SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion {
                        min_gas_price: evicter_price,
                    },
                ))));
            return Ok(newly_inserted);
        }

        self.metrics.rejections.inc();
        Err(
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion {
                min_gas_price: min_price,
            }
            .into(),
        )
    }

    /// Pop up to `count` entries, highest gas price first.
    /// Within the same gas price, entries are returned in FIFO order.
    pub fn pop_batch(&mut self, count: usize) -> Vec<QueueEntry> {
        let mut remaining = count.min(self.total_len);
        let mut entries = Vec::with_capacity(remaining);
        while remaining > 0 {
            let Some(mut last) = self.map.last_entry() else {
                break;
            };
            let deque = last.get_mut();
            if deque.len() <= remaining {
                // Drain the entire price level at once.
                remaining -= deque.len();
                self.total_len -= deque.len();
                entries.extend(last.remove());
            } else {
                // Partial drain from this price level.
                self.total_len -= remaining;
                entries.extend(deque.drain(..remaining));
                remaining = 0;
            }
        }
        for entry in &entries {
            self.remove_keys(entry);
        }
        self.metrics.queue_depth.set(self.total_len as i64);
        entries
    }

    pub fn is_empty(&self) -> bool {
        self.total_len == 0
    }

    fn push_entry(&mut self, entry: QueueEntry, keys: Vec<ConsensusTransactionKey>) {
        for key in keys {
            *self.queued_keys.entry(key).or_insert(0) += 1;
        }
        self.map
            .entry(entry.gas_price)
            .or_default()
            .push_back(entry);
        self.total_len += 1;
    }

    fn evict_lowest(&mut self) -> QueueEntry {
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

    fn remove_keys(&mut self, entry: &QueueEntry) {
        for tx in &entry.transactions {
            let key = tx.key();
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
    bypass_threshold: usize,
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
        bypass_fraction: f64,
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
            bypass_threshold: (max_pending as f64 * bypass_fraction) as usize,
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
            bypass_threshold: usize::MAX,
            failover_timeout: Duration::from_secs(30),
            metrics: Arc::new(AdmissionQueueMetrics::new_for_tests()),
            consensus_adapter,
            slot_freed_notify,
        }
    }

    pub fn metrics(&self) -> &Arc<AdmissionQueueMetrics> {
        &self.metrics
    }

    pub(crate) fn bypass_threshold(&self) -> usize {
        self.bypass_threshold
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

    pub(crate) fn bypass_threshold(&self) -> usize {
        self.manager.bypass_threshold()
    }

    pub(crate) fn load(&self) -> arc_swap::Guard<Arc<AdmissionQueueHandle>> {
        self.swap.load()
    }
}

/// Per-epoch event loop that owns the priority queue and drains entries
/// to consensus as capacity becomes available.
struct AdmissionQueueEventLoop {
    receiver: mpsc::Receiver<InsertCommand>,
    queue: PriorityAdmissionQueue,
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

    fn build_queue(capacity: usize, gas_prices: &[u64]) -> PriorityAdmissionQueue {
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
            0.9,
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
