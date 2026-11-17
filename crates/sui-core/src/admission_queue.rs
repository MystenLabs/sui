// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, earliest_expiry_deadline_ms,
};
use crate::consensus_adapter::ConsensusAdapter;
use crate::consensus_handler::SequencedConsensusTransactionKey;
use crate::staggered_submission::proposers_metric_label;
use arc_swap::ArcSwap;
use itertools::Itertools as _;
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
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::coin_reservation::ParsedDigest;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_consensus::{
    ConsensusPosition, ConsensusTransaction, ConsensusTransactionKey,
};
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

/// A transaction (or soft bundle) waiting in the admission queue for consensus submission.
pub struct QueueEntry {
    pub gas_price: u64,
    pub transactions: Vec<ConsensusTransaction>,
    pub position_sender: oneshot::Sender<Result<Vec<ConsensusPosition>, tonic::Status>>,
    pub submitter_client_addr: Option<IpAddr>,
    pub enqueue_time: Instant,
    dedup_keys: Vec<DedupKey>,
}

/// What `PriorityAdmissionQueue::pop_batch_while` does with an examined entry.
pub enum PopAction {
    /// Pop the entry into the included partition.
    Include,
    /// Pop the entry into the excluded partition.
    Exclude,
    /// Leave the entry queued and continue with the next entry.
    Skip,
    /// Leave the entry queued and stop iterating.
    Stop,
}

/// Identifies an owned object for deduplication by its id and version. Two transactions
/// that consume the same object version conflict whatever digest they name.
pub type DedupKey = (ObjectID, SequenceNumber);

/// Dedup keys of the gas payment objects of the user transactions in `transactions`, sorted
/// with duplicates removed. Coin reservation refs are excluded: they name a reusable address
/// balance rather than an exclusive object, so distinct transactions may share one. A soft
/// bundle is one entry keyed on all its members' gas, so it is refused or displaced whole.
pub fn gas_dedup_keys(transactions: &[ConsensusTransaction]) -> Vec<DedupKey> {
    let mut keys: Vec<DedupKey> = transactions
        .iter()
        .filter_map(|transaction| transaction.kind.as_user_transaction())
        .flat_map(|tx| tx.transaction_data().gas_data().payment.iter())
        .filter(|object_ref| !ParsedDigest::is_coin_reservation_digest(&object_ref.2))
        .map(|object_ref| (object_ref.0, object_ref.1))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

pub fn owned_object_conflict_error(object_id: ObjectID) -> SuiError {
    SuiErrorKind::OwnedObjectConflict { object_id }.into()
}

pub trait AdmissionQueueEntry {
    fn gas_price(&self) -> u64;
    fn transaction_keys(&self) -> impl Iterator<Item = ConsensusTransactionKey>;
    /// Keys this entry is deduped on, sorted with duplicates removed: two queued entries
    /// sharing one can never both execute, so only the highest-priced is admitted. Both lanes
    /// supply [`gas_dedup_keys`], the gas payment objects being the only inputs that are
    /// unambiguously owned without a state read.
    fn dedup_keys(&self) -> &[DedupKey];
    fn notify_evicted(self, min_gas_price: u64);
    fn notify_rejected(self, min_gas_price: u64);
    /// A queued entry lost `object_id` to an entry that took priority over it.
    fn notify_replaced(self, object_id: ObjectID);
    /// An entry was refused because a queued entry already holds `object_id`.
    fn notify_owned_object_conflict(self, object_id: ObjectID);
    /// Returns the bytes that the queue counts against its byte capacity. The value must
    /// not change while the entry is queued.
    fn byte_size(&self) -> usize;
}

/// Outcome of [`PriorityAdmissionQueue::try_insert`]. Displaced entries are
/// carried here unnotified so that the caller can deliver the notifications
/// outside of any held locks.
#[must_use = "call `notify` to deliver the outcome to the displaced entries"]
pub struct InsertOutcome<E> {
    outcome: Option<Outcome<E>>,
    created_at: &'static std::panic::Location<'static>,
}

enum Outcome<E> {
    Inserted {
        /// False if an entry sharing one of the transaction keys was already queued.
        newly_inserted: bool,
        /// The lowest-priced entries evicted to make room, cheapest first.
        evicted: Vec<E>,
        /// Queued entries this entry displaced from an owned object, with that object.
        replaced: Vec<(E, ObjectID)>,
        /// The inserted entry's gas price.
        gas_price: u64,
    },
    /// Rejected: evicting the entries priced below this one cannot make room. A new bid
    /// must exceed `min_gas_price`.
    Rejected { entry: E, min_gas_price: u64 },
    /// Rejected: an owned gas object this entry consumes is already pending.
    OwnedObjectConflict { entry: E, object_id: ObjectID },
}

impl<E: AdmissionQueueEntry> InsertOutcome<E> {
    #[track_caller]
    fn new(outcome: Outcome<E>) -> Self {
        Self {
            outcome: Some(outcome),
            created_at: std::panic::Location::caller(),
        }
    }

    pub fn was_inserted(&self) -> bool {
        matches!(self.outcome, Some(Outcome::Inserted { .. }))
    }

    /// Notifies the displaced entries, if any, then reports the insert result.
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
                replaced,
                gas_price,
            } => {
                for (entry, object_id) in replaced {
                    entry.notify_replaced(object_id);
                }
                for entry in evicted {
                    entry.notify_evicted(gas_price);
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
            Outcome::OwnedObjectConflict { entry, object_id } => {
                entry.notify_owned_object_conflict(object_id);
                Err(owned_object_conflict_error(object_id))
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

    fn dedup_keys(&self) -> &[DedupKey] {
        &self.dedup_keys
    }

    /// The push-mode queue has no byte capacity, so its entries skip serializing
    /// themselves to measure their size.
    fn byte_size(&self) -> usize {
        0
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

    fn notify_replaced(self, object_id: ObjectID) {
        self.notify_owned_object_conflict(object_id);
    }

    fn notify_owned_object_conflict(self, object_id: ObjectID) {
        let _ = self
            .position_sender
            .send(Err(tonic::Status::from(owned_object_conflict_error(
                object_id,
            ))));
    }
}

impl QueueEntry {
    pub fn new(
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        position_sender: oneshot::Sender<Result<Vec<ConsensusPosition>, tonic::Status>>,
        submitter_client_addr: Option<IpAddr>,
    ) -> Self {
        Self {
            gas_price,
            dedup_keys: gas_dedup_keys(&transactions),
            transactions,
            position_sender,
            submitter_client_addr,
            enqueue_time: Instant::now(),
        }
    }

    #[cfg(test)]
    pub fn new_for_test(
        gas_price: u64,
        position_sender: oneshot::Sender<Result<Vec<ConsensusPosition>, tonic::Status>>,
    ) -> Self {
        Self::new(gas_price, vec![], position_sender, None)
    }
}

/// Prometheus metrics for the admission queue.
pub struct AdmissionQueueMetrics {
    // Pull mode intentionally reuses these admission metrics for its user lane because
    // the two modes are mutually exclusive on a validator. This preserves dashboard
    // continuity when switching modes.
    pub queue_depth: IntGauge,
    pub queue_wait_latency: HistogramVec,
    pub evictions: IntCounter,
    pub rejections: IntCounter,
    pub byte_capacity_bound: IntCounter,
    pub expired: IntCounter,
    pub duplicate_inserts: IntCounter,
    pub expired_at_drain: IntCounter,
    pub owned_object_conflicts: IntCounter,
    pub owned_object_replacements: IntCounter,

    pub pool_depth: IntGaugeVec,
    pub pool_bytes: IntGaugeVec,
    pub pool_staggered_held: IntGauge,
    pub pool_staggered_held_bytes: IntGauge,
    pub pool_staggered_quota_rejected: IntCounterVec,
    pub pool_staggered_rejected: IntCounter,
    pub pool_staggered_delay: Histogram,
    pub pool_stagger_shadow_decisions: IntCounterVec,
    pub pool_stagger_shadow_delay: Histogram,
    pub pool_taken_per_proposal: Histogram,
    pub pool_requeued_on_dropped_ack: IntCounter,
    pub pool_gc_notified: IntCounter,
    pub pool_waiting_inserts: IntGauge,
    pub pool_already_processed: IntCounterVec,
    pub pool_rejected_at_proposal: IntCounterVec,
    pub pool_commit_latency: HistogramVec,
    pub pool_abandoned: IntCounterVec,
    pub pool_byte_budget_bytes: IntGauge,
    pub pool_byte_budget_exhausted: IntCounter,
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
                &["lane", "proposers"],
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
                "Number of transactions rejected because the queue was full, by entry count or by bytes, and evicting cheaper entries could not make room",
                registry,
            )
            .unwrap(),
            byte_capacity_bound: register_int_counter_with_registry!(
                "admission_queue_byte_capacity_bound",
                "Inserts that did not fit within the byte capacity without evicting, whether they then evicted entries or were rejected",
                registry,
            )
            .unwrap(),
            expired: register_int_counter_with_registry!(
                "admission_queue_expired",
                "Number of queued transactions dropped for exceeding their time to live",
                registry,
            )
            .unwrap(),
            duplicate_inserts: register_int_counter_with_registry!(
                "admission_queue_duplicate_inserts",
                "Transactions admitted to the queue whose ConsensusTransactionKey duplicated an entry already present. Tallied as spam for DoS protection.",
                registry,
            )
            .unwrap(),
            expired_at_drain: register_int_counter_with_registry!(
                "admission_queue_expired_at_drain",
                "Queue entries rejected when drained because a transaction was past its max_timestamp by more than the grace period",
                registry,
            )
            .unwrap(),
            owned_object_conflicts: register_int_counter_with_registry!(
                "admission_queue_owned_object_conflicts",
                "Transactions refused at admission because an owned gas object they consume is already pending. Tallied as spam for DoS protection.",
                registry,
            )
            .unwrap(),
            owned_object_replacements: register_int_counter_with_registry!(
                "admission_queue_owned_object_replacements",
                "Queued entries removed because a higher-priced transaction consumes one of their owned gas objects",
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
            pool_staggered_held: register_int_gauge_with_registry!(
                "consensus_transaction_pool_staggered_held",
                "User-lane entries in the pool stamped with a staggered-submission hold, whether or not the hold has elapsed",
                registry,
            )
            .unwrap(),
            pool_staggered_held_bytes: register_int_gauge_with_registry!(
                "consensus_transaction_pool_staggered_held_bytes",
                "Serialized bytes of the user-lane entries counted against the staggered-submission held-byte quota",
                registry,
            )
            .unwrap(),
            pool_staggered_quota_rejected: register_int_counter_vec_with_registry!(
                "consensus_transaction_pool_staggered_quota_rejected",
                "Staggered submissions refused because the held entries reached a quota, by quota (entries or bytes)",
                &["quota"],
                registry,
            )
            .unwrap(),
            pool_staggered_rejected: register_int_counter_with_registry!(
                "consensus_transaction_pool_staggered_rejected",
                "User submissions rejected because this validator is outside the transaction's staggered-submission proposer set",
                registry,
            )
            .unwrap(),
            pool_staggered_delay: register_histogram_with_registry!(
                "consensus_transaction_pool_staggered_delay",
                "The staggered-submission hold applied to a user-lane entry without allowed proposers before it may be proposed",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            pool_stagger_shadow_decisions: register_int_counter_vec_with_registry!(
                "consensus_transaction_pool_stagger_shadow_decisions",
                "Decision (free, hold or reject) staggering would have made per user-lane submission while inactive",
                &["decision"],
                registry,
            )
            .unwrap(),
            pool_stagger_shadow_delay: register_histogram_with_registry!(
                "consensus_transaction_pool_stagger_shadow_delay",
                "Hold staggering would have applied to a user-lane submission while inactive",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
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
            pool_rejected_at_proposal: register_int_counter_vec_with_registry!(
                "consensus_transaction_pool_rejected_at_proposal",
                "User submissions dropped at proposal time because their owned input objects no longer match live state or a transaction expired, by error",
                &["error"],
                registry,
            )
            .unwrap(),
            pool_commit_latency: register_histogram_vec_with_registry!(
                "consensus_transaction_pool_commit_latency",
                "Time from insert into the transaction pool to commit of the block that proposed the entry",
                &["lane", "proposers"],
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
            pool_byte_budget_bytes: register_int_gauge_with_registry!(
                "consensus_transaction_pool_byte_budget_bytes",
                "User-lane bytes the rate limiter allows the next proposal to take",
                registry,
            )
            .unwrap(),
            pool_byte_budget_exhausted: register_int_counter_with_registry!(
                "consensus_transaction_pool_byte_budget_exhausted",
                "Proposals in which the user-lane byte rate limit stopped the take",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        Self::new(&Registry::new())
    }
}

/// `Totals` holds the entry count and serialized bytes of a set of queued entries.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Totals {
    len: usize,
    bytes: u64,
}

impl Totals {
    fn add(&mut self, bytes: u64) {
        self.len += 1;
        self.bytes += bytes;
    }

    fn remove(&mut self, bytes: u64) {
        self.len -= 1;
        self.bytes -= bytes;
    }
}

/// Bounded priority queue that orders transactions by gas price. Uses a BTreeMap
/// for efficient access at both ends: lowest gas price (for eviction) and highest
/// gas price (for draining to consensus). Entries at the same gas price are FIFO.
pub struct PriorityAdmissionQueue<E: AdmissionQueueEntry> {
    capacity: usize,
    /// When set, this caps the serialized bytes of queued entries, which is the
    /// measure the block byte limit uses. It bounds the backlog of bytes waiting
    /// to be proposed.
    byte_capacity: Option<u64>,
    map: BTreeMap<u64, VecDeque<E>>,
    /// Number of queue entries per transaction key, for duplicate detection.
    queued_keys: HashMap<ConsensusTransactionKey, u32>,
    /// Gas price of the single queued entry consuming each owned gas object, keyed by
    /// `(id, version)`. One object is consumable by one transaction, so there is at most
    /// one holder, and a higher bid replaces it rather than stacking; its price is all
    /// the holder lookups need.
    queued_dedup_objects: HashMap<DedupKey, u64>,
    totals: Totals,
    metrics: Arc<AdmissionQueueMetrics>,
}

impl<E: AdmissionQueueEntry> PriorityAdmissionQueue<E> {
    pub fn new(capacity: usize, metrics: Arc<AdmissionQueueMetrics>) -> Self {
        Self {
            capacity,
            byte_capacity: None,
            map: BTreeMap::new(),
            queued_keys: HashMap::new(),
            queued_dedup_objects: HashMap::new(),
            totals: Totals::default(),
            metrics,
        }
    }

    /// Adds a cap on total queued serialized bytes. Entries are evicted lowest
    /// gas price first to stay within it, on the same terms as the entry-count cap.
    pub fn with_byte_capacity(mut self, byte_capacity: Option<u64>) -> Self {
        self.byte_capacity = byte_capacity;
        self
    }

    pub fn len(&self) -> usize {
        self.totals.len
    }

    #[cfg(test)]
    pub fn byte_capacity(&self) -> Option<u64> {
        self.byte_capacity
    }

    #[cfg(test)]
    fn assert_consistent(&self) {
        let mut totals = Totals::default();
        let mut queued_keys: HashMap<ConsensusTransactionKey, u32> = HashMap::new();
        let mut queued_dedup_objects: HashMap<DedupKey, u64> = HashMap::new();
        for (&gas_price, deque) in &self.map {
            assert!(!deque.is_empty());
            for entry in deque {
                assert_eq!(entry.gas_price(), gas_price);
                totals.add(entry.byte_size() as u64);
                for key in entry.transaction_keys() {
                    *queued_keys.entry(key).or_default() += 1;
                }
                for key in entry.dedup_keys() {
                    assert!(queued_dedup_objects.insert(*key, gas_price).is_none());
                }
            }
        }
        assert_eq!(self.totals, totals);
        assert_eq!(self.queued_keys, queued_keys);
        assert_eq!(self.queued_dedup_objects, queued_dedup_objects);
    }

    pub fn min_gas_price(&self) -> Option<u64> {
        self.map.first_key_value().map(|(&k, _)| k)
    }

    /// On success, returns `Ok(true)` or `Ok(false)` to indicate whether the
    /// value was newly inserted. Returns `Err` if the entry does not fit and
    /// evicting the entries priced below it cannot make room, or if a queued
    /// entry consumes the same owned gas object at an equal or higher price.
    pub fn insert(&mut self, entry: E) -> SuiResult<bool> {
        self.try_insert(entry).notify()
    }

    /// Same as `insert`, without notifying displaced entries. The evicted entries and the
    /// holders replaced on an owned gas object (on success), or the refused entry itself
    /// (on rejection), are handed back in the outcome for the caller to notify manually.
    ///
    /// A newcomer must strictly outbid the queued holder of each owned gas object it
    /// consumes. The holders it outbids leave the queue if it is admitted, so their room
    /// counts toward the room it needs. If it still does not fit, it evicts the cheapest
    /// queued entries, FIFO within a gas price, until it fits both capacities. Only
    /// entries priced strictly below it can be evicted. Admission is decided before any
    /// entry is removed, so a refusal leaves the queue unchanged, including the holders,
    /// and reports the first queued gas price at or above the entry's.
    #[track_caller]
    pub fn try_insert(&mut self, entry: E) -> InsertOutcome<E> {
        let newly_inserted = !entry
            .transaction_keys()
            .any(|key| self.queued_keys.contains_key(&key));
        let gas_price = entry.gas_price();

        // A resubmission of a still-queued transaction ties against its own copy, so it is
        // refused and the queued copy keeps its position.
        let replaces_holders = match self.owned_conflict(entry.dedup_keys(), gas_price, false) {
            Ok(replaces_holders) => replaces_holders,
            Err(object_id) => {
                return InsertOutcome::new(Outcome::OwnedObjectConflict { entry, object_id });
            }
        };

        if !newly_inserted {
            self.metrics.duplicate_inserts.inc();
        }

        let entry_bytes = entry.byte_size() as u64;
        if !replaces_holders && self.fits_after(Totals::default(), entry_bytes) {
            self.push(entry, false);
            return InsertOutcome::new(Outcome::Inserted {
                newly_inserted,
                evicted: Vec::new(),
                replaced: Vec::new(),
                gas_price,
            });
        }

        let mut freed = if replaces_holders {
            self.owned_holder_totals(entry.dedup_keys())
        } else {
            Totals::default()
        };
        if !self.fits_bytes_after(freed, entry_bytes) {
            self.metrics.byte_capacity_bound.inc();
        }

        // The entries to evict are counted before any is removed, so that a refusal
        // leaves the queue unchanged. Holders are already counted in `freed` and are
        // skipped. They are priced strictly below the entry, so once they are removed
        // the walked entries are the cheapest ones left.
        let mut walked = 0;
        let mut blocking_price = None;
        'walk: for (&price, deque) in &self.map {
            for queued in deque {
                if self.fits_after(freed, entry_bytes) {
                    break 'walk;
                }
                if price >= gas_price {
                    blocking_price = Some(price);
                    break 'walk;
                }
                if replaces_holders && Self::holds_any(queued, entry.dedup_keys()) {
                    continue;
                }
                freed.add(queued.byte_size() as u64);
                walked += 1;
            }
        }
        if !self.fits_after(freed, entry_bytes) {
            self.metrics.rejections.inc();
            return InsertOutcome::new(Outcome::Rejected {
                entry,
                min_gas_price: blocking_price.unwrap_or(gas_price),
            });
        }

        let replaced = if replaces_holders {
            self.remove_owned_holders(entry.dedup_keys())
        } else {
            Vec::new()
        };
        let evicted: Vec<E> = (0..walked).map(|_| self.evict_lowest()).collect();
        self.metrics.evictions.inc_by(evicted.len() as u64);
        self.push(entry, false);
        InsertOutcome::new(Outcome::Inserted {
            newly_inserted,
            evicted,
            replaced,
            gas_price,
        })
    }

    /// Returns whether an entry of `entry_bytes` can be admitted after the queued
    /// entries counted in `freed` leave. A reinsert can leave the queue over a
    /// capacity. While it is over, an admission must not increase that total, so it
    /// frees at least one entry for the entry count and at least its own bytes for
    /// the byte capacity.
    fn fits_after(&self, freed: Totals, entry_bytes: u64) -> bool {
        self.totals.len - freed.len < self.capacity.max(self.totals.len)
            && self.fits_bytes_after(freed, entry_bytes)
    }

    fn fits_bytes_after(&self, freed: Totals, entry_bytes: u64) -> bool {
        self.byte_capacity.is_none_or(|cap| {
            self.totals.bytes - freed.bytes + entry_bytes <= cap.max(self.totals.bytes)
        })
    }

    /// Decides an incoming entry's claims on owned objects against their queued holders,
    /// without changing the queue. Returns whether it outbids any holder, or the contested
    /// object when it does not outbid them all. `tie_wins` decides an equal bid.
    fn owned_conflict(
        &self,
        owned: &[DedupKey],
        gas_price: u64,
        tie_wins: bool,
    ) -> Result<bool, ObjectID> {
        let incumbent = owned
            .iter()
            .filter_map(|key| {
                self.queued_dedup_objects
                    .get(key)
                    .map(|&price| (price, key.0))
            })
            .max_by_key(|&(price, _)| price);
        let Some((incumbent_gas, object_id)) = incumbent else {
            return Ok(false);
        };
        if gas_price > incumbent_gas || (tie_wins && gas_price == incumbent_gas) {
            Ok(true)
        } else {
            self.metrics.owned_object_conflicts.inc();
            Err(object_id)
        }
    }

    fn holds_any(entry: &E, owned: &[DedupKey]) -> bool {
        entry
            .dedup_keys()
            .iter()
            .any(|key| owned.binary_search(key).is_ok())
    }

    /// Price levels of the queued holders of `owned` (sorted), ascending.
    fn owned_holder_levels(&self, owned: &[DedupKey]) -> Vec<u64> {
        let mut levels: Vec<u64> = owned
            .iter()
            .filter_map(|key| self.queued_dedup_objects.get(key))
            .copied()
            .collect();
        levels.sort_unstable();
        levels.dedup();
        levels
    }

    /// Counts the queued entries that consume one of `owned` (sorted).
    fn owned_holder_totals(&self, owned: &[DedupKey]) -> Totals {
        let mut totals = Totals::default();
        for gas_price in self.owned_holder_levels(owned) {
            for entry in self.map.get(&gas_price).into_iter().flatten() {
                if Self::holds_any(entry, owned) {
                    totals.add(entry.byte_size() as u64);
                }
            }
        }
        totals
    }

    /// Removes every queued entry that consumes one of `owned` (sorted), returning each with
    /// the object it lost so the caller can notify it outside the lock. Only the price levels
    /// recorded for those objects are walked.
    fn remove_owned_holders(&mut self, owned: &[DedupKey]) -> Vec<(E, ObjectID)> {
        let mut removed = Vec::new();
        for gas_price in self.owned_holder_levels(owned) {
            // A recorded object always has its holder queued at the recorded price, so the
            // level exists while the record does; its absence means the bookkeeping drifted.
            let Some(deque) = self.map.get_mut(&gas_price) else {
                debug_fatal!(
                    "owned object recorded at gas price {gas_price} has no queued entries"
                );
                continue;
            };
            let mut source = std::mem::take(deque);
            while let Some(entry) = source.pop_front() {
                let lost = entry
                    .dedup_keys()
                    .iter()
                    .find(|key| owned.binary_search(key).is_ok())
                    .map(|key| key.0);
                match lost {
                    Some(object_id) => removed.push((entry, object_id)),
                    None => deque.push_back(entry),
                }
            }
            if deque.is_empty() {
                self.map.remove(&gas_price);
            }
        }
        for (entry, _) in &removed {
            self.untrack(entry);
        }
        self.metrics
            .owned_object_replacements
            .inc_by(removed.len() as u64);
        self.metrics.queue_depth.set(self.totals.len as i64);
        removed
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
    /// `action` returns `Stop` or the queue is exhausted. `Skip`ped entries and
    /// the entry that stopped iteration stay queued. Popped entries are returned
    /// partitioned into those the callback chose to `Include` and those to
    /// `Exclude`.
    pub fn pop_batch_while(
        &mut self,
        mut action: impl FnMut(&mut E) -> PopAction,
    ) -> (Vec<E>, Vec<E>) {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        let mut stopped = false;
        for deque in self.map.values_mut().rev() {
            if stopped {
                break;
            }
            // Drain the level once, writing retained entries back in order: a batch with
            // removals costs one pass over the level rather than a shift per removal.
            // This runs on every block proposal, so it must stay linear even when most
            // entries are skipped (e.g. held by staggered submission).
            let mut source = std::mem::take(deque);
            while let Some(mut entry) = source.pop_front() {
                if stopped {
                    deque.push_back(entry);
                    continue;
                }
                match action(&mut entry) {
                    PopAction::Stop => {
                        stopped = true;
                        deque.push_back(entry);
                    }
                    PopAction::Skip => deque.push_back(entry),
                    PopAction::Include => included.push(entry),
                    PopAction::Exclude => excluded.push(entry),
                }
            }
        }
        self.map.retain(|_, deque| !deque.is_empty());
        for entry in included.iter().chain(&excluded) {
            self.untrack(entry);
        }
        self.metrics.queue_depth.set(self.totals.len as i64);
        (included, excluded)
    }

    /// Pops, from the front of each price level, the entries for which `pred` holds,
    /// stopping each level at the first entry for which it does not. Within a level
    /// entries are ordered by admission time, so a predicate on age pops the aged prefix
    /// and a level with no match costs one call.
    pub fn pop_front_while(&mut self, mut pred: impl FnMut(&mut E) -> bool) -> Vec<E> {
        let mut popped = Vec::new();
        for deque in self.map.values_mut() {
            while let Some(front) = deque.front_mut()
                && pred(front)
            {
                popped.push(deque.pop_front().expect("front was just observed"));
            }
        }
        if popped.is_empty() {
            return popped;
        }
        for entry in &popped {
            self.untrack(entry);
        }
        self.map.retain(|_, deque| !deque.is_empty());
        self.metrics.queue_depth.set(self.totals.len as i64);
        popped
    }

    pub fn is_empty(&self) -> bool {
        self.totals.len == 0
    }

    /// Reinserts an entry that was taken for a proposal whose block was dropped, at the
    /// front of its price level so it reclaims the position it held before being drained.
    /// The entry-count and byte capacities are bypassed because the entry was already
    /// admitted. While it was out with the proposer a conflicting entry may have been
    /// queued on one of its owned objects: the higher gas price wins, and a tie is kept
    /// for this returning entry since it held the object first. The outcome carries any
    /// displaced entry, or this entry when it is outbid, for the caller to notify outside
    /// the lock.
    #[track_caller]
    pub fn reinsert_front(&mut self, entry: E) -> InsertOutcome<E> {
        let newly_inserted = !entry
            .transaction_keys()
            .any(|key| self.queued_keys.contains_key(&key));
        let gas_price = entry.gas_price();
        let replaced = match self.owned_conflict(entry.dedup_keys(), gas_price, true) {
            Ok(true) => self.remove_owned_holders(entry.dedup_keys()),
            Ok(false) => Vec::new(),
            Err(object_id) => {
                return InsertOutcome::new(Outcome::OwnedObjectConflict { entry, object_id });
            }
        };
        self.push(entry, true);
        InsertOutcome::new(Outcome::Inserted {
            newly_inserted,
            evicted: Vec::new(),
            replaced,
            gas_price,
        })
    }

    fn push(&mut self, entry: E, front: bool) {
        for key in entry.transaction_keys() {
            *self.queued_keys.entry(key).or_insert(0) += 1;
        }
        let gas_price = entry.gas_price();
        for key in entry.dedup_keys() {
            // A losing bid is refused and a winning one removes the holder before the
            // newcomer is pushed, so a key is never already present at a different price.
            let previous = self.queued_dedup_objects.insert(*key, gas_price);
            debug_assert!(previous.is_none_or(|price| price == gas_price));
        }
        self.totals.add(entry.byte_size() as u64);
        let deque = self.map.entry(gas_price).or_default();
        if front {
            deque.push_front(entry);
        } else {
            deque.push_back(entry);
        }
        self.metrics.queue_depth.set(self.totals.len as i64);
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
        self.untrack(&evicted);
        evicted
    }

    /// Removes an entry that has left the queue from the totals and the key indexes.
    /// Every path that removes an entry calls this, so the totals stay consistent.
    fn untrack(&mut self, entry: &E) {
        self.totals.remove(entry.byte_size() as u64);
        self.remove_keys(entry);
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
        for key in entry.dedup_keys() {
            if self.queued_dedup_objects.remove(key).is_none() {
                debug_fatal!("remove_keys on absent owned object");
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
    /// rejection or on an owned gas object conflict with a queued entry.
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
        let entry = QueueEntry::new(gas_price, transactions, position_tx, submitter_client_addr);

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
        let now_ms = AuthorityState::unixtime_now_ms();
        for entry in entries {
            // An entry that was already processed goes on to the adapter, which reports it as
            // processed.
            if earliest_expiry_deadline_ms(&entry.transactions).is_some_and(|d| now_ms >= d)
                && !self.all_processed(&entry.transactions)
            {
                debug!(
                    keys = ?entry.transactions.iter().map(|t| t.key()).collect::<Vec<_>>(),
                    "Admission queue entry rejected at drain: transaction expired"
                );
                self.queue.metrics.expired_at_drain.inc();
                let error: SuiError = SuiErrorKind::TransactionExpired.into();
                let _ = entry.position_sender.send(Err(error.into()));
                continue;
            }
            let proposers = proposers_metric_label(&entry.transactions, &self.epoch_store);
            self.queue
                .metrics
                .queue_wait_latency
                .with_label_values(&["user", proposers])
                .observe(entry.enqueue_time.elapsed().as_secs_f64());
            let adapter = self.consensus_adapter.clone();
            let es = self.epoch_store.clone();
            spawn_monitored_task!(submit_queue_entry(entry, adapter, es));
        }
        *self.last_drain.lock().unwrap() = Instant::now();
    }

    /// Whether every transaction in `transactions` was processed by consensus or executed from
    /// a checkpoint, as `ConsensusAdapter` checks before it submits.
    fn all_processed(&self, transactions: &[ConsensusTransaction]) -> bool {
        let consensus_processed = self.epoch_store.check_consensus_messages_processed(
            transactions
                .iter()
                .map(|transaction| SequencedConsensusTransactionKey::External(transaction.key())),
        );
        let unprocessed: Vec<_> = transactions
            .iter()
            .zip_eq(consensus_processed)
            .filter(|(_, processed)| !processed)
            .map(|(transaction, _)| transaction)
            .collect();
        if unprocessed.is_empty() {
            return true;
        }
        let Some(digests) = unprocessed
            .iter()
            .map(|transaction| {
                transaction
                    .kind
                    .as_user_transaction()
                    .map(|tx| *tx.digest())
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        self.epoch_store
            .multi_get_transaction_checkpoint(&digests)
            .expect("Storage error when checking transaction executed in checkpoint")
            .iter()
            .all(Option::is_some)
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
    use std::sync::Mutex;
    use sui_macros::sim_test;
    use sui_types::base_types::{AuthorityName, ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
    use sui_types::coin_reservation::encode_object_ref;
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::digests::{ChainIdentifier, TransactionDigest};
    use sui_types::transaction::PlainTransactionWithClaims;

    #[derive(Clone, Default)]
    struct MockOutcomeLog {
        evicted: Arc<Mutex<Vec<u64>>>,
        rejected: Arc<Mutex<Vec<u64>>>,
        replaced: Arc<Mutex<Vec<u64>>>,
    }

    /// Minimal entry for exercising the queue's ordering, capacities and owned-object
    /// dedup without building real transactions. Records which notification it received
    /// in `outcome`, and its id in `log`.
    struct MockEntry {
        id: u64,
        gas_price: u64,
        bytes: usize,
        keys: Vec<ConsensusTransactionKey>,
        owned: Vec<DedupKey>,
        outcome: Arc<Mutex<Option<&'static str>>>,
        log: MockOutcomeLog,
    }

    fn keys_of(owned: &[ObjectRef]) -> Vec<DedupKey> {
        let mut keys: Vec<DedupKey> = owned.iter().map(|r| (r.0, r.1)).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    fn mock_entry(
        gas_price: u64,
        owned: Vec<ObjectRef>,
    ) -> (MockEntry, Arc<Mutex<Option<&'static str>>>) {
        let outcome = Arc::new(Mutex::new(None));
        let entry = MockEntry {
            keys: vec![ConsensusTransactionKey::Certificate(
                TransactionDigest::random(),
            )],
            owned: keys_of(&owned),
            outcome: outcome.clone(),
            ..sized_entry(0, gas_price, 0, &MockOutcomeLog::default())
        };
        (entry, outcome)
    }

    fn byte_capped_queue(capacity: usize, byte_capacity: u64) -> PriorityAdmissionQueue<MockEntry> {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        PriorityAdmissionQueue::new(capacity, metrics).with_byte_capacity(Some(byte_capacity))
    }

    /// A mock entry of `bytes` bytes with no transaction keys and no owned objects.
    fn sized_entry(id: u64, gas_price: u64, bytes: usize, log: &MockOutcomeLog) -> MockEntry {
        MockEntry {
            id,
            gas_price,
            bytes,
            keys: Vec::new(),
            owned: Vec::new(),
            outcome: Arc::new(Mutex::new(None)),
            log: log.clone(),
        }
    }

    /// A mock entry holding one transaction key derived from `id`.
    fn keyed_entry(id: u64, gas_price: u64, bytes: usize, log: &MockOutcomeLog) -> MockEntry {
        let mut digest = [0; 32];
        digest[..8].copy_from_slice(&id.to_le_bytes());
        MockEntry {
            keys: vec![ConsensusTransactionKey::Certificate(
                TransactionDigest::new(digest),
            )],
            ..sized_entry(id, gas_price, bytes, log)
        }
    }

    /// A mock entry of `bytes` bytes that consumes the owned objects `owned`.
    fn owning_entry(
        id: u64,
        gas_price: u64,
        bytes: usize,
        owned: &[ObjectRef],
        log: &MockOutcomeLog,
    ) -> MockEntry {
        MockEntry {
            owned: keys_of(owned),
            ..sized_entry(id, gas_price, bytes, log)
        }
    }

    fn owned_ref(id: u8, version: u64) -> ObjectRef {
        (
            ObjectID::from_single_byte(id),
            SequenceNumber::from_u64(version),
            ObjectDigest::MIN,
        )
    }

    impl AdmissionQueueEntry for MockEntry {
        fn gas_price(&self) -> u64 {
            self.gas_price
        }
        fn transaction_keys(&self) -> impl Iterator<Item = ConsensusTransactionKey> {
            self.keys.iter().cloned()
        }
        fn dedup_keys(&self) -> &[DedupKey] {
            &self.owned
        }
        fn byte_size(&self) -> usize {
            self.bytes
        }
        fn notify_evicted(self, _min_gas_price: u64) {
            *self.outcome.lock().unwrap() = Some("evicted");
            self.log.evicted.lock().unwrap().push(self.id);
        }
        fn notify_rejected(self, _min_gas_price: u64) {
            *self.outcome.lock().unwrap() = Some("rejected");
            self.log.rejected.lock().unwrap().push(self.id);
        }
        fn notify_replaced(self, _object_id: ObjectID) {
            *self.outcome.lock().unwrap() = Some("replaced");
            self.log.replaced.lock().unwrap().push(self.id);
        }
        fn notify_owned_object_conflict(self, _object_id: ObjectID) {
            *self.outcome.lock().unwrap() = Some("conflict");
        }
    }

    #[test]
    fn byte_cap_admits_entries_within_budget() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        assert!(q.insert(sized_entry(1, 10, 400, &log)).is_ok());
        assert!(q.insert(sized_entry(2, 10, 400, &log)).is_ok());
        assert_eq!(q.len(), 2);
        assert!(log.evicted.lock().unwrap().is_empty());
        assert!(log.rejected.lock().unwrap().is_empty());
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_evicts_multiple_cheaper_entries_for_one_large() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 300, &log)).unwrap();
        q.insert(sized_entry(2, 10, 300, &log)).unwrap();
        q.insert(sized_entry(3, 10, 300, &log)).unwrap();
        q.insert(sized_entry(4, 20, 600, &log)).unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2]);
        assert!(log.rejected.lock().unwrap().is_empty());
        assert_eq!(q.metrics.evictions.get(), 2);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_rejects_when_cheaper_entries_cannot_free_enough() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 30, 700, &log)).unwrap();
        q.insert(sized_entry(2, 10, 200, &log)).unwrap();
        assert!(matches!(
            q.insert(sized_entry(3, 20, 500, &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 30 }
        ));
        assert_eq!(*log.rejected.lock().unwrap(), vec![3]);
        assert!(log.evicted.lock().unwrap().is_empty());
        assert_eq!(q.len(), 2);
        assert_eq!(q.metrics.evictions.get(), 0);
        q.insert(sized_entry(4, 31, 500, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![2, 1]);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_evicts_across_price_levels() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 300, &log)).unwrap();
        q.insert(sized_entry(2, 20, 200, &log)).unwrap();
        q.insert(sized_entry(3, 20, 200, &log)).unwrap();
        q.insert(sized_entry(4, 30, 300, &log)).unwrap();
        q.insert(sized_entry(5, 40, 600, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2, 3]);
        assert_eq!(q.len(), 2);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_rejects_entry_larger_than_budget_when_empty() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        assert!(q.insert(sized_entry(1, 10, 1001, &log)).is_err());
        assert_eq!(*log.rejected.lock().unwrap(), vec![1]);
        assert_eq!(q.len(), 0);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_never_evicts_equal_priced_entries() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 600, &log)).unwrap();
        q.insert(sized_entry(2, 10, 300, &log)).unwrap();
        assert!(q.insert(sized_entry(3, 10, 300, &log)).is_err());
        assert_eq!(*log.rejected.lock().unwrap(), vec![3]);
        assert!(log.evicted.lock().unwrap().is_empty());
        q.assert_consistent();
    }

    #[test]
    fn entry_count_cap_binds_under_byte_cap() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(2, 1000);
        q.insert(sized_entry(1, 10, 100, &log)).unwrap();
        q.insert(sized_entry(2, 10, 100, &log)).unwrap();
        q.insert(sized_entry(3, 20, 100, &log)).unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(*log.evicted.lock().unwrap(), vec![1]);
        assert!(q.insert(sized_entry(4, 5, 100, &log)).is_err());
        assert_eq!(*log.rejected.lock().unwrap(), vec![4]);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_frees_budget_when_entries_are_popped() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 800, &log)).unwrap();
        assert!(q.insert(sized_entry(2, 5, 800, &log)).is_err());
        assert_eq!(q.pop_batch(1).len(), 1);
        q.assert_consistent();
        assert!(q.insert(sized_entry(3, 5, 800, &log)).is_ok());
        assert_eq!(q.len(), 1);
        q.assert_consistent();
    }

    #[test]
    fn byte_cap_counts_reinserted_entries() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 800, &log)).unwrap();
        let popped = q.pop_batch(1).pop().unwrap();
        q.assert_consistent();
        q.reinsert_front(popped).notify().unwrap();
        assert!(q.insert(sized_entry(2, 5, 300, &log)).is_err());
        assert_eq!(*log.rejected.lock().unwrap(), vec![2]);
        assert_eq!(q.len(), 1);
        q.assert_consistent();
    }

    #[test]
    fn over_entry_capacity_admission_evicts_one_entry() {
        let log = MockOutcomeLog::default();
        let mut q =
            PriorityAdmissionQueue::new(3, Arc::new(AdmissionQueueMetrics::new_for_tests()));
        for (id, gas_price) in [(1, 10), (2, 10), (3, 20)] {
            q.insert(sized_entry(id, gas_price, 100, &log)).unwrap();
        }
        let popped = q.pop_batch(3);
        for (id, gas_price) in [(4, 15), (5, 15), (6, 30)] {
            q.insert(sized_entry(id, gas_price, 100, &log)).unwrap();
        }
        for entry in popped.into_iter().rev() {
            q.reinsert_front(entry).notify().unwrap();
        }
        assert_eq!(q.len(), 6);
        q.assert_consistent();

        q.insert(sized_entry(7, 40, 100, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1]);
        q.insert(sized_entry(8, 12, 100, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2]);
        assert_eq!(q.len(), 6);
        assert!(matches!(
            q.insert(sized_entry(9, 12, 100, &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 12 }
        ));
        q.assert_consistent();
    }

    #[test]
    fn over_byte_capacity_admission_evicts_at_least_its_own_bytes() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        q.insert(sized_entry(1, 10, 600, &log)).unwrap();
        let popped = q.pop_batch(1);
        q.insert(sized_entry(2, 10, 300, &log)).unwrap();
        q.insert(sized_entry(3, 10, 300, &log)).unwrap();
        q.insert(sized_entry(4, 20, 400, &log)).unwrap();
        q.reinsert_front(popped.into_iter().next().unwrap())
            .notify()
            .unwrap();
        assert_eq!(q.metrics.byte_capacity_bound.get(), 0);
        q.assert_consistent();

        // The queue holds 1600 bytes. Evicting entry 1 frees enough for entry 5.
        q.insert(sized_entry(5, 30, 500, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1]);
        // Entry 6 must free 700 bytes, which takes entries 2, 3 and 4.
        q.insert(sized_entry(6, 30, 700, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(q.metrics.byte_capacity_bound.get(), 2);
        q.assert_consistent();
    }

    #[test]
    fn rejection_reports_the_first_level_at_or_above_the_bid() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        for (id, gas_price) in [(1, 10), (2, 20), (3, 30), (4, 40)] {
            q.insert(sized_entry(id, gas_price, 250, &log)).unwrap();
        }
        assert!(matches!(
            q.insert(sized_entry(5, 15, 600, &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 20 }
        ));
        assert!(matches!(
            q.insert(sized_entry(6, 21, 600, &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 30 }
        ));
        q.insert(sized_entry(7, 31, 600, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2, 3]);
        q.assert_consistent();
    }

    #[test]
    fn over_both_capacities_admission_evicts_an_entry_and_its_own_bytes() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(2, 1000);
        q.insert(sized_entry(1, 10, 600, &log)).unwrap();
        let popped = q.pop_batch(1);
        q.insert(sized_entry(2, 10, 300, &log)).unwrap();
        q.insert(sized_entry(3, 20, 400, &log)).unwrap();
        q.reinsert_front(popped.into_iter().next().unwrap())
            .notify()
            .unwrap();
        // The queue holds 3 entries and 1300 bytes, over both capacities.
        assert_eq!(q.len(), 3);
        q.assert_consistent();

        // Evicting entry 1 frees one entry and more than the newcomer's bytes.
        q.insert(sized_entry(4, 30, 100, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1]);
        // The queue holds 3 entries and 800 bytes. Entry 5 must free one entry and 300
        // bytes, which entry 2 does.
        q.insert(sized_entry(5, 30, 500, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2]);
        assert_eq!(q.len(), 3);
        // The cheapest remaining entry is priced above entry 6, so entry 6 is refused
        // and nothing is evicted.
        assert!(matches!(
            q.insert(sized_entry(6, 15, 100, &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 20 }
        ));
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2]);
        assert_eq!(q.len(), 3);
        assert_eq!(q.metrics.byte_capacity_bound.get(), 3);
        q.assert_consistent();
    }

    #[test]
    fn multi_entry_eviction_releases_every_evicted_key() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(100, 1000);
        for id in 1..=4 {
            q.insert(keyed_entry(id, 10, 250, &log)).unwrap();
        }
        q.insert(keyed_entry(5, 20, 700, &log)).unwrap();
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2, 3]);
        q.assert_consistent();

        // A copy of an evicted transaction is not reported as a duplicate, and a copy of
        // a queued one is.
        assert!(q.insert(keyed_entry(1, 30, 20, &log)).unwrap());
        assert!(!q.insert(keyed_entry(4, 30, 20, &log)).unwrap());
        assert_eq!(*log.evicted.lock().unwrap(), vec![1, 2, 3]);
        q.assert_consistent();
    }

    #[test]
    fn replacement_counts_holder_bytes_as_freed() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(10, 100);
        let gas = owned_ref(1, 6);
        q.insert(owning_entry(1, 100, 60, &[gas], &log)).unwrap();
        q.insert(sized_entry(2, 50, 40, &log)).unwrap();

        // The holder's 60 bytes leave with it, and evicting the cheaper entry frees the rest.
        assert!(q.insert(owning_entry(3, 200, 70, &[gas], &log)).unwrap());
        assert_eq!(*log.replaced.lock().unwrap(), vec![1]);
        assert_eq!(*log.evicted.lock().unwrap(), vec![2]);
        assert_eq!(q.len(), 1);
        q.assert_consistent();
    }

    #[test]
    fn replacement_that_does_not_fit_keeps_the_holder() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(10, 100);
        let gas = owned_ref(1, 6);
        q.insert(owning_entry(1, 100, 60, &[gas], &log)).unwrap();
        q.insert(sized_entry(2, 300, 40, &log)).unwrap();

        // Removing the holder frees 60 bytes, which is not enough for 80, and the other
        // entry is priced above the newcomer.
        assert!(matches!(
            q.insert(owning_entry(3, 200, 80, &[gas], &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 300 }
        ));
        assert_eq!(*log.rejected.lock().unwrap(), vec![3]);
        assert!(log.replaced.lock().unwrap().is_empty());
        assert!(log.evicted.lock().unwrap().is_empty());
        assert_eq!(q.len(), 2);
        q.assert_consistent();

        // The holder still holds the object.
        assert!(matches!(
            q.insert(owning_entry(4, 100, 1, &[gas], &log))
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
    }

    #[test]
    fn over_byte_capacity_replacement_frees_at_least_its_own_bytes() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(10, 100);
        let gas = owned_ref(1, 6);
        q.insert(owning_entry(1, 100, 30, &[gas], &log)).unwrap();
        q.insert(sized_entry(2, 50, 60, &log)).unwrap();
        let popped = q.pop_batch(2);
        q.insert(sized_entry(3, 40, 50, &log)).unwrap();
        for entry in popped.into_iter().rev() {
            q.reinsert_front(entry).notify().unwrap();
        }
        assert_eq!(q.len(), 3);

        // The queue holds 140 bytes against a capacity of 100. The 40-byte newcomer frees
        // the holder's 30 bytes and evicts the cheapest entry to free its own bytes.
        assert!(q.insert(owning_entry(4, 200, 40, &[gas], &log)).unwrap());
        assert_eq!(*log.replaced.lock().unwrap(), vec![1]);
        assert_eq!(*log.evicted.lock().unwrap(), vec![3]);
        assert_eq!(q.len(), 2);
        q.assert_consistent();
    }

    #[test]
    fn expired_entries_leave_the_byte_total() {
        let log = MockOutcomeLog::default();
        let mut q = byte_capped_queue(10, 100);
        q.insert(sized_entry(1, 100, 60, &log)).unwrap();
        q.insert(sized_entry(2, 200, 30, &log)).unwrap();
        let popped = q.pop_front_while(|entry| entry.id == 1);
        assert_eq!(popped.len(), 1);
        q.assert_consistent();
        assert!(q.insert(sized_entry(3, 10, 70, &log)).unwrap());
        assert!(log.evicted.lock().unwrap().is_empty());
        q.assert_consistent();
    }

    #[test]
    fn owned_object_higher_gas_replaces_lower() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (low, low_outcome) = mock_entry(100, vec![gas]);
        let (high, _) = mock_entry(200, vec![gas]);

        assert!(q.insert(low).unwrap());
        assert_eq!(q.len(), 1);
        assert!(q.insert(high).unwrap());
        assert_eq!(q.len(), 1);
        assert_eq!(*low_outcome.lock().unwrap(), Some("replaced"));
        assert_eq!(q.pop_batch(10).first().map(|e| e.gas_price), Some(200));
    }

    #[test]
    fn owned_object_lower_gas_rejected() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (high, _) = mock_entry(200, vec![gas]);
        let (low, _) = mock_entry(100, vec![gas]);

        assert!(q.insert(high).unwrap());
        let outcome = q.try_insert(low);
        assert!(matches!(
            outcome.notify().unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn owned_object_equal_gas_distinct_rejected() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (first, _) = mock_entry(100, vec![gas]);
        let (second, _) = mock_entry(100, vec![gas]);

        assert!(q.insert(first).unwrap());
        assert!(matches!(
            q.try_insert(second).notify().unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn reinsert_front_reclaims_object_from_lower_priced_newcomer() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (first, _) = mock_entry(200, vec![gas]);

        assert!(q.insert(first).unwrap());
        let taken = q.pop_batch(1).pop().unwrap();
        assert_eq!(q.len(), 0);

        let (newcomer, newcomer_outcome) = mock_entry(100, vec![gas]);
        assert!(q.insert(newcomer).unwrap());

        let outcome = q.reinsert_front(taken);
        assert!(outcome.was_inserted());
        outcome.notify().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop_batch(10).first().map(|e| e.gas_price), Some(200));
        assert_eq!(*newcomer_outcome.lock().unwrap(), Some("replaced"));
    }

    #[test]
    fn reinsert_front_reclaims_object_on_tie() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (first, _) = mock_entry(100, vec![gas]);

        assert!(q.insert(first).unwrap());
        let taken = q.pop_batch(1).pop().unwrap();

        // The requeued entry held the object first, so it wins the tie.
        let (newcomer, newcomer_outcome) = mock_entry(100, vec![gas]);
        assert!(q.insert(newcomer).unwrap());

        let outcome = q.reinsert_front(taken);
        assert!(outcome.was_inserted());
        outcome.notify().unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(*newcomer_outcome.lock().unwrap(), Some("replaced"));
    }

    #[test]
    fn reinsert_front_outbid_is_rejected() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (first, first_outcome) = mock_entry(100, vec![gas]);

        assert!(q.insert(first).unwrap());
        let taken = q.pop_batch(1).pop().unwrap();

        let (newcomer, _) = mock_entry(200, vec![gas]);
        assert!(q.insert(newcomer).unwrap());

        let outcome = q.reinsert_front(taken);
        assert!(!outcome.was_inserted());
        assert!(matches!(
            outcome.notify().unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop_batch(10).first().map(|e| e.gas_price), Some(200));
        assert_eq!(*first_outcome.lock().unwrap(), Some("conflict"));
    }

    #[test]
    fn owned_object_same_id_version_different_digest_dedups() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let id = ObjectID::from_single_byte(1);
        let version = SequenceNumber::from_u64(6);
        let (first, _) = mock_entry(100, vec![(id, version, ObjectDigest::MIN)]);
        let (second, _) = mock_entry(100, vec![(id, version, ObjectDigest::MAX)]);

        assert!(q.insert(first).unwrap());
        assert!(matches!(
            q.try_insert(second).notify().unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn soft_bundle_is_displaced_whole_by_a_higher_bid_on_one_member() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let contested = owned_ref(1, 6);
        let uncontested = owned_ref(2, 6);
        let bundle_txs = vec![
            user_transaction_with_gas(contested),
            user_transaction_with_gas(uncontested),
        ];
        assert_eq!(
            gas_dedup_keys(&bundle_txs),
            keys_of(&[contested, uncontested])
        );

        let (position_tx, mut bundle_receiver) = oneshot::channel();
        let bundle = QueueEntry::new(100, bundle_txs, position_tx, None);
        assert!(q.insert(bundle).unwrap());

        let (tie, _tie_receiver) = make_dup_entry(100, user_transaction_with_gas(contested));
        assert!(matches!(
            q.insert(tie).unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);

        let (bid, _bid_receiver) = make_dup_entry(200, user_transaction_with_gas(contested));
        assert!(q.insert(bid).unwrap());
        assert_eq!(q.len(), 1);
        let status = bundle_receiver.try_recv().unwrap().unwrap_err();
        assert!(matches!(
            SuiError::from(status).as_inner(),
            SuiErrorKind::OwnedObjectConflict { object_id } if *object_id == contested.0
        ));

        let (other, _other_receiver) = make_dup_entry(100, user_transaction_with_gas(uncontested));
        assert!(q.insert(other).unwrap());
    }

    #[test]
    fn over_capacity_replacement_admits_and_returns_displaced() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(1, metrics);
        let obj_a = owned_ref(1, 6);
        let obj_b = owned_ref(2, 6);

        let (low, low_outcome) = mock_entry(50, vec![obj_a]);
        assert!(q.insert(low).unwrap());
        let taken = q.pop_batch(1).pop().unwrap();

        let (high, _) = mock_entry(200, vec![obj_b]);
        assert!(q.insert(high).unwrap());

        // The requeue transiently exceeds capacity.
        q.reinsert_front(taken).notify().unwrap();
        assert_eq!(q.len(), 2);

        // The newcomer outbids the incumbent on A but not the queue minimum. Displacing the
        // incumbent frees a slot, so the newcomer is admitted.
        let (newcomer, _) = mock_entry(100, vec![obj_a]);
        q.try_insert(newcomer).notify().unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(*low_outcome.lock().unwrap(), Some("replaced"));
    }

    #[test]
    fn owned_object_equal_gas_resubmission_rejected() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let key = ConsensusTransactionKey::Certificate(TransactionDigest::random());
        let first = MockEntry {
            keys: vec![key.clone()],
            owned: keys_of(&[gas]),
            ..sized_entry(0, 100, 0, &MockOutcomeLog::default())
        };
        assert!(q.insert(first).unwrap());

        let resubmit = MockEntry {
            keys: vec![key],
            owned: keys_of(&[gas]),
            ..sized_entry(0, 100, 0, &MockOutcomeLog::default())
        };
        assert!(matches!(
            q.try_insert(resubmit).notify().unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn multi_key_newcomer_must_outbid_every_holder() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics.clone());
        let gas_a = owned_ref(1, 6);
        let gas_b = owned_ref(2, 6);
        let (low_holder, low_outcome) = mock_entry(100, vec![gas_a]);
        let (high_holder, high_outcome) = mock_entry(300, vec![gas_b]);
        assert!(q.insert(low_holder).unwrap());
        assert!(q.insert(high_holder).unwrap());

        let (between, _) = mock_entry(200, vec![gas_a, gas_b]);
        assert!(matches!(
            q.insert(between).unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { object_id } if *object_id == gas_b.0
        ));
        assert_eq!(q.len(), 2);
        assert_eq!(*low_outcome.lock().unwrap(), None);

        let (above, _) = mock_entry(400, vec![gas_a, gas_b]);
        assert!(q.insert(above).unwrap());
        assert_eq!(q.len(), 1);
        assert_eq!(*low_outcome.lock().unwrap(), Some("replaced"));
        assert_eq!(*high_outcome.lock().unwrap(), Some("replaced"));
        assert_eq!(metrics.owned_object_replacements.get(), 2);
        assert_eq!(q.pop_batch(10).first().map(|e| e.gas_price), Some(400));
    }

    #[test]
    fn owned_object_replace_leaves_other_entries_at_incumbent_price() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let target = owned_ref(1, 6);
        let (incumbent, incumbent_outcome) = mock_entry(100, vec![target]);
        let (bystander, bystander_outcome) = mock_entry(100, vec![owned_ref(2, 6)]);
        let (bump, _) = mock_entry(200, vec![target]);

        assert!(q.insert(incumbent).unwrap());
        assert!(q.insert(bystander).unwrap());
        assert!(q.insert(bump).unwrap());
        assert_eq!(q.len(), 2);
        assert_eq!(*incumbent_outcome.lock().unwrap(), Some("replaced"));
        assert_eq!(*bystander_outcome.lock().unwrap(), None);
    }

    #[test]
    fn owned_object_different_versions_coexist() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let (v6, _) = mock_entry(100, vec![owned_ref(1, 6)]);
        let (v7, _) = mock_entry(100, vec![owned_ref(1, 7)]);

        assert!(q.insert(v6).unwrap());
        assert!(q.insert(v7).unwrap());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn queue_entry_dedups_gas_objects_but_not_coin_reservations() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);

        // Reservations on one address balance share (id, version) and differ only in the
        // amount carried in the digest.
        let balance_id = ObjectID::random();
        let version = SequenceNumber::from_u64(5);
        let mut receivers = Vec::new();
        for amount in [100, 200] {
            let gas = encode_object_ref(balance_id, version, 0, amount, ChainIdentifier::default());
            let (entry, receiver) = make_dup_entry(100, user_transaction_with_gas(gas));
            receivers.push(receiver);
            assert!(q.insert(entry).unwrap());
        }
        assert_eq!(q.len(), 2);

        let gas = owned_ref(1, 6);
        let (first, _first_receiver) = make_dup_entry(100, user_transaction_with_gas(gas));
        let (second, _second_receiver) = make_dup_entry(100, user_transaction_with_gas(gas));
        assert!(q.insert(first).unwrap());
        assert!(matches!(
            q.insert(second).unwrap_err().as_inner(),
            SuiErrorKind::OwnedObjectConflict { .. }
        ));
        assert_eq!(q.len(), 3);
    }

    fn user_transaction_with_gas(gas: ObjectRef) -> ConsensusTransaction {
        let (sender, keypair) = get_key_pair::<AccountKeyPair>();
        let transaction = crate::test_utils::make_transfer_sui_transaction(
            gas,
            sender,
            Some(1),
            sender,
            &keypair,
            1,
        );
        ConsensusTransaction::new_user_transaction_v2_message(
            &AuthorityName::ZERO,
            PlainTransactionWithClaims::no_aliases(transaction),
        )
    }

    #[test]
    fn owned_object_freed_after_pop() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let gas = owned_ref(1, 6);
        let (first, _) = mock_entry(100, vec![gas]);
        assert!(q.insert(first).unwrap());
        assert_eq!(q.pop_batch(10).len(), 1);
        let (again, _) = mock_entry(100, vec![gas]);
        assert!(q.insert(again).unwrap());
        assert_eq!(q.len(), 1);
    }

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

    fn entry_at(gas_price: u64, enqueue_time: Instant) -> QueueEntry {
        let mut entry = make_test_entry(gas_price).0;
        entry.enqueue_time = enqueue_time;
        entry
    }

    fn expired_at(now: Instant, ttl: Duration) -> impl FnMut(&mut QueueEntry) -> bool {
        move |entry| now.saturating_duration_since(entry.enqueue_time) >= ttl
    }

    fn remaining(q: &mut PriorityAdmissionQueue<QueueEntry>) -> Vec<(u64, Instant)> {
        let len = q.len();
        q.pop_batch(len)
            .into_iter()
            .map(|entry| (entry.gas_price, entry.enqueue_time))
            .collect()
    }

    #[test]
    fn pop_front_while_drops_aged_entries_and_their_keys() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics.clone());
        let now = Instant::now();
        let mut keyed = entry_at(100, now);
        keyed.transactions = vec![ConsensusTransaction::new_end_of_publish(
            sui_types::base_types::AuthorityName::ZERO,
        )];
        q.insert(keyed).unwrap();
        q.insert(entry_at(200, now)).unwrap();

        let popped = q.pop_front_while(expired_at(
            now + Duration::from_secs(120),
            Duration::from_secs(60),
        ));
        assert_eq!(popped.len(), 2);
        assert_eq!(q.len(), 0);
        assert_eq!(metrics.queue_depth.get(), 0);
        assert!(q.queued_keys.is_empty());
        assert!(q.map.is_empty());
    }

    #[test]
    fn pop_front_while_keeps_fresh_entries() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let now = Instant::now();
        q.insert(entry_at(100, now)).unwrap();
        q.insert(entry_at(200, now)).unwrap();

        let popped = q.pop_front_while(expired_at(now, Duration::from_secs(60)));
        assert!(popped.is_empty());
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn pop_front_while_pops_the_aged_prefix_of_every_level() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut q = PriorityAdmissionQueue::new(10, metrics);
        let now = Instant::now();
        let ttl = Duration::from_secs(60);
        let fresh = now + Duration::from_secs(120);
        // Level 100 is entirely fresh and sits below level 200, so a sweep that stops at
        // the first fresh level never reaches 200.
        q.insert(entry_at(100, fresh)).unwrap();
        // Level 200 holds two adjacent aged entries ahead of a fresh one.
        q.insert(entry_at(200, now)).unwrap();
        q.insert(entry_at(200, now + Duration::from_secs(1)))
            .unwrap();
        q.insert(entry_at(200, fresh)).unwrap();
        // Level 300 ends with an aged entry behind a fresh one, which stays queued.
        q.insert(entry_at(300, fresh)).unwrap();
        q.insert(entry_at(300, now)).unwrap();

        let popped = q.pop_front_while(expired_at(now + ttl + Duration::from_secs(1), ttl));
        let popped: Vec<_> = popped
            .into_iter()
            .map(|entry| (entry.gas_price, entry.enqueue_time))
            .collect();
        assert_eq!(
            popped,
            vec![(200, now), (200, now + Duration::from_secs(1))]
        );
        assert_eq!(
            remaining(&mut q),
            vec![(300, fresh), (300, now), (200, fresh), (100, fresh)]
        );
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
    fn test_pop_batch_while_skips_and_retains() {
        let mut q = build_queue(10, &[100, 200, 200, 50, 75]);

        // Skipping continues past the entry — both within a price level (only one of
        // the two 200s is skipped) and into lower levels.
        let mut seen_200 = false;
        let (included, excluded) = q.pop_batch_while(|entry| match entry.gas_price {
            200 if !seen_200 => {
                seen_200 = true;
                PopAction::Skip
            }
            100 => PopAction::Skip,
            75 => PopAction::Exclude,
            _ => PopAction::Include,
        });
        assert_eq!(
            included.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![200, 50]
        );
        assert_eq!(
            excluded.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![75]
        );
        // Skipped entries stay queued in order and are visible to later pops.
        assert_eq!(q.len(), 2);
        let (rest, _) = q.pop_batch_while(|_| PopAction::Include);
        assert_eq!(
            rest.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![200, 100]
        );
        assert!(q.is_empty());
    }

    #[test]
    fn test_pop_batch_while_skip_then_stop() {
        // A skipped entry ahead of the stopping one: both stay queued, in order,
        // with bookkeeping intact.
        let mut q = build_queue(10, &[300, 200, 100]);

        let (included, excluded) = q.pop_batch_while(|entry| match entry.gas_price {
            300 => PopAction::Skip,
            200 => PopAction::Include,
            _ => PopAction::Stop,
        });
        assert_eq!(
            included.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![200]
        );
        assert!(excluded.is_empty());
        assert_eq!(q.len(), 2);
        assert_eq!(q.min_gas_price(), Some(100));
        let (rest, _) = q.pop_batch_while(|_| PopAction::Include);
        assert_eq!(
            rest.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![300, 100]
        );
        assert!(q.is_empty());
    }

    #[test]
    fn test_pop_batch_while_fully_skipped_level_retained() {
        let mut q = build_queue(10, &[200, 200]);

        let (included, excluded) = q.pop_batch_while(|_| PopAction::Skip);
        assert!(included.is_empty() && excluded.is_empty());
        // The untouched level survives with its invariants: length, min price, and
        // eviction of a full queue still work on it.
        assert_eq!(q.len(), 2);
        assert_eq!(q.min_gas_price(), Some(200));
        let (rest, _) = q.pop_batch_while(|_| PopAction::Include);
        assert_eq!(
            rest.iter().map(|e| e.gas_price).collect::<Vec<_>>(),
            vec![200, 200]
        );
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
        (
            QueueEntry::new(gas_price, vec![tx], position_tx, None),
            position_rx,
        )
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
        q.assert_consistent();
        q.reinsert_front(popped).notify().unwrap();
        assert_eq!(q.len(), 1);
        let (entry2, _rx2) = make_dup_entry(100, tx.clone());
        assert!(!q.insert(entry2).unwrap());

        // Draining everything zeroes the counter and the same tx is fresh again.
        let _ = q.pop_batch(q.len());
        assert!(q.is_empty());
        let (entry3, _rx3) = make_dup_entry(100, tx);
        assert!(q.insert(entry3).unwrap());
    }

    #[sim_test]
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

    #[sim_test]
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
    #[sim_test]
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
