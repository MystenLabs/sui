// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::ConsensusAdapter;
use mysten_metrics::spawn_monitored_task;
use prometheus::{
    Histogram, IntCounter, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry,
};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;
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
    /// Tracks transaction keys currently in the queue to reject duplicates.
    queued_keys: HashSet<ConsensusTransactionKey>,
    total_len: usize,
    metrics: Arc<AdmissionQueueMetrics>,
}

impl PriorityAdmissionQueue {
    pub fn new(capacity: usize, metrics: Arc<AdmissionQueueMetrics>) -> Self {
        Self {
            capacity,
            map: BTreeMap::new(),
            queued_keys: HashSet::new(),
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

    /// Returns true if the entry was accepted into the queue.
    pub fn insert(&mut self, entry: QueueEntry) -> bool {
        let keys: Vec<_> = entry.transactions.iter().map(|t| t.key()).collect();
        if keys.iter().any(|k| self.queued_keys.contains(k)) {
            return false; // duplicate insert
        }

        if self.total_len < self.capacity {
            self.push_entry(entry, keys);
            self.metrics.queue_depth.set(self.total_len as i64);
            return true; // directly inserted
        }

        let min_price = self.min_gas_price().unwrap();
        if entry.gas_price > min_price {
            self.evict_lowest();
            self.push_entry(entry, keys);
            self.metrics.evictions.inc();
            return true; // inserted after evicting lower-priority tx
        }

        self.metrics.rejections.inc();
        false
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
        self.queued_keys.extend(keys);
        self.map
            .entry(entry.gas_price)
            .or_default()
            .push_back(entry);
        self.total_len += 1;
    }

    fn evict_lowest(&mut self) {
        let evicted = {
            let Some(mut first) = self.map.first_entry() else {
                return;
            };
            let deque = first.get_mut();
            let evicted = deque.pop_front().unwrap();
            if deque.is_empty() {
                first.remove();
            }
            evicted
        };
        self.remove_keys(&evicted);
        self.total_len -= 1;
    }

    fn remove_keys(&mut self, entry: &QueueEntry) {
        for tx in &entry.transactions {
            self.queued_keys.remove(&tx.key());
        }
    }
}

/// Command sent from RPC handlers to the admission queue actor via mpsc channel.
struct InsertCommand {
    entry: QueueEntry,
    response: oneshot::Sender<SuiResult<()>>,
}

/// Cloneable handle for submitting transactions to the admission queue actor.
/// Held by RPC handlers; the actor runs in a separate spawned task.
#[derive(Clone)]
pub struct AdmissionQueueHandle {
    sender: mpsc::Sender<InsertCommand>,
    pub(crate) metrics: Arc<AdmissionQueueMetrics>,
    pub(crate) bypass_threshold: usize,
}

impl AdmissionQueueHandle {
    /// Returns Ok(receiver for consensus position) if accepted, Err if rejected.
    pub async fn try_insert(
        &self,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<oneshot::Receiver<Result<Vec<ConsensusPosition>, tonic::Status>>> {
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

        resp_rx
            .await
            .map_err(|_| SuiError::from(SuiErrorKind::TooManyTransactionsPendingConsensus))??;

        Ok(position_rx)
    }
}

/// Manages the lifecycle of per-epoch admission queue actors.
/// Holds immutable config and shared metrics; call `spawn()` each epoch
/// with the new epoch store to create a fresh actor and handle.
pub struct AdmissionQueueManager {
    capacity: usize,
    bypass_threshold: usize,
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
        slot_freed_notify: Arc<tokio::sync::Notify>,
    ) -> Self {
        let max_pending = consensus_adapter.max_pending_transactions();
        Self {
            capacity: (max_pending as f64 * capacity_fraction) as usize,
            bypass_threshold: (max_pending as f64 * bypass_fraction) as usize,
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
        let (sender, receiver) = mpsc::channel(self.capacity.max(1024));

        let event_loop = AdmissionQueueEventLoop {
            receiver,
            queue: PriorityAdmissionQueue::new(self.capacity, self.metrics.clone()),
            consensus_adapter: self.consensus_adapter.clone(),
            slot_freed_notify: self.slot_freed_notify.clone(),
            epoch_store,
        };
        spawn_monitored_task!(event_loop.run());

        AdmissionQueueHandle {
            sender,
            metrics: self.metrics.clone(),
            bypass_threshold: self.bypass_threshold,
        }
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
}

impl AdmissionQueueEventLoop {
    pub async fn run(mut self) {
        loop {
            // Process all pending inserts from the channel (non-blocking).
            self.process_pending_inserts();

            // Drain to consensus if the queue has entries and capacity is available.
            if !handle_fail_point_if("admission_queue_disable_drain")
                && !self.queue.is_empty()
                && self.has_consensus_capacity()
            {
                self.drain_batch();
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
        for entry in entries {
            self.queue
                .metrics
                .queue_wait_latency
                .observe(entry.enqueue_time.elapsed().as_secs_f64());
            let adapter = self.consensus_adapter.clone();
            let es = self.epoch_store.clone();
            spawn_monitored_task!(submit_queue_entry(entry, adapter, es));
        }
    }

    fn handle_insert(&mut self, cmd: InsertCommand) {
        if self.queue.insert(cmd.entry) {
            let _ = cmd.response.send(Ok(()));
        } else {
            let _ = cmd
                .response
                .send(Err(SuiErrorKind::TooManyTransactionsPendingConsensus.into()));
        }
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
            q.insert(entry);
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

        q.insert(e1);
        q.insert(e2);
        assert_eq!(q.len(), 2);

        assert!(q.insert(e3));
        assert_eq!(q.len(), 2);
        assert!(r1.try_recv().is_err());
    }

    #[test]
    fn test_rejection_when_full_and_low_price() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(2, metrics);

        let (e1, _) = make_test_entry(100);
        let (e2, _) = make_test_entry(200);
        let (e3, mut r3) = make_test_entry(50);

        q.insert(e1);
        q.insert(e2);

        assert!(!q.insert(e3));
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

        q.insert(gasless);
        q.insert(normal);

        assert!(q.insert(high));
        assert!(r_gasless.try_recv().is_err());
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

        q.insert(e1);
        assert!(!q.insert(e2));
    }

    #[test]
    fn test_duplicate_transaction_rejected() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        let authority = AuthorityName::ZERO;
        let tx = ConsensusTransaction::new_end_of_publish(authority);

        let (position_tx1, _rx1) = oneshot::channel();
        let entry1 = QueueEntry {
            gas_price: 100,
            transactions: vec![tx.clone()],
            position_sender: position_tx1,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        };
        assert!(q.insert(entry1));

        // Same transaction again — rejected as duplicate
        let (position_tx2, _rx2) = oneshot::channel();
        let entry2 = QueueEntry {
            gas_price: 100,
            transactions: vec![tx.clone()],
            position_sender: position_tx2,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        };
        assert!(!q.insert(entry2));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_duplicate_key_freed_after_pop() {
        use sui_types::base_types::AuthorityName;

        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        let authority = AuthorityName::ZERO;
        let tx = ConsensusTransaction::new_end_of_publish(authority);

        let (position_tx1, _rx1) = oneshot::channel();
        let entry1 = QueueEntry {
            gas_price: 100,
            transactions: vec![tx.clone()],
            position_sender: position_tx1,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        };
        q.insert(entry1);

        // Pop the entry — key should be freed
        let batch = q.pop_batch(1);
        assert_eq!(batch.len(), 1);

        // Now the same transaction can be re-inserted
        let (position_tx2, _rx2) = oneshot::channel();
        let entry2 = QueueEntry {
            gas_price: 100,
            transactions: vec![tx],
            position_sender: position_tx2,
            submitter_client_addr: None,
            enqueue_time: Instant::now(),
        };
        assert!(q.insert(entry2));
    }

    #[tokio::test]
    async fn test_actor_shuts_down_when_handle_dropped() {
        use crate::authority::test_authority_builder::TestAuthorityBuilder;
        use crate::checkpoints::CheckpointStore;
        use crate::consensus_adapter::{ConnectionMonitorStatusForTests, ConsensusAdapterMetrics};
        use crate::mysticeti_adapter::LazyMysticetiClient;
        use sui_types::base_types::AuthorityName;

        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(LazyMysticetiClient::new()),
            CheckpointStore::new_for_tests(),
            AuthorityName::ZERO,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
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
}
