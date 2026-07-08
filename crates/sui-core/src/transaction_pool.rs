// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pull-based transaction pool feeding consensus block proposals.
//!
//! Transactions wait in a prioritized pool (system entries first, then user entries by
//! gas price descending) until the consensus proposer takes them into a block, and are
//! settled by push callbacks from the commit handler and checkpoint executor. This is an
//! alternative to the admission queue + consensus adapter submission pipeline.
//!
//! See `transaction_pool.md` (same directory) for the design; keep it in sync with
//! behavior changes here.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use consensus_core::LimitReached;
// Re-exported so downstream crates (e.g. sui-node) can name the consensus-facing trait
// without a direct consensus-core dependency.
pub use consensus_core::TransactionPool;
use consensus_types::block::{BlockRef, Round, TransactionIndex};
use mysten_common::debug_fatal;
use parking_lot::Mutex;
use prometheus::{
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Registry,
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};
use sui_types::base_types::{AuthorityName, TransactionDigest};
use sui_types::committee::EpochId;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_consensus::{
    ConsensusPosition, ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
};
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::consensus_tx_status_cache::ConsensusTxStatus;
use crate::checkpoints::CheckpointStore;
use crate::consensus_adapter::{ConsensusAdapterMetrics, SubmitToConsensus};
use crate::consensus_handler::{SequencedConsensusTransactionKey, classify};
use crate::epoch::reconfiguration::ReconfigurationInitiator;

/// A sequenced own block whose entries have not fully settled after this many rounds
/// past its round indicates missed settle coverage; the defensive sweep reaps it.
const SWEEP_GRACE_ROUNDS: Round = 100;

/// Metrics for the transaction pool. Reuses the already-registered
/// `ConsensusAdapterMetrics` series (prometheus handles are cheap clones sharing the
/// registered state) where the semantics carry over, so dashboards keep working across
/// the migration, and registers pool-specific metrics for pool state.
pub struct TransactionPoolMetrics {
    // Reused ConsensusAdapterMetrics series.
    sequencing_certificate_attempt: IntCounterVec,
    sequencing_certificate_success: IntCounterVec,
    sequencing_certificate_failures: IntCounterVec,
    sequencing_certificate_status: IntCounterVec,
    sequencing_certificate_settled_status: IntCounterVec,
    sequencing_certificate_inflight: IntGaugeVec,
    sequencing_acknowledge_latency: HistogramVec,
    sequencing_certificate_latency: HistogramVec,
    sequencing_certificate_processed: IntCounterVec,
    sequencing_best_effort_timeout: IntCounterVec,
    consensus_latency: Histogram,
    num_rejected_cert_in_epoch_boundary: IntCounter,
    // Pool-specific metrics.
    pool_pending: IntGauge,
    pool_inflight: IntGauge,
    pool_wait_latency: Histogram,
    pool_evictions: IntCounter,
    pool_rejections: IntCounter,
    pool_coalesced_inserts: IntCounter,
    pool_gc_requeues: IntCounter,
}

impl TransactionPoolMetrics {
    pub fn new(registry: &Registry, adapter_metrics: &ConsensusAdapterMetrics) -> Self {
        Self {
            sequencing_certificate_attempt: adapter_metrics.sequencing_certificate_attempt.clone(),
            sequencing_certificate_success: adapter_metrics.sequencing_certificate_success.clone(),
            sequencing_certificate_failures: adapter_metrics
                .sequencing_certificate_failures
                .clone(),
            sequencing_certificate_status: adapter_metrics.sequencing_certificate_status.clone(),
            sequencing_certificate_settled_status: adapter_metrics
                .sequencing_certificate_settled_status
                .clone(),
            sequencing_certificate_inflight: adapter_metrics
                .sequencing_certificate_inflight
                .clone(),
            sequencing_acknowledge_latency: adapter_metrics.sequencing_acknowledge_latency.clone(),
            sequencing_certificate_latency: adapter_metrics.sequencing_certificate_latency.clone(),
            sequencing_certificate_processed: adapter_metrics
                .sequencing_certificate_processed
                .clone(),
            sequencing_best_effort_timeout: adapter_metrics.sequencing_best_effort_timeout.clone(),
            consensus_latency: adapter_metrics.consensus_latency.clone(),
            num_rejected_cert_in_epoch_boundary: adapter_metrics
                .num_rejected_cert_in_epoch_boundary
                .clone(),
            pool_pending: register_int_gauge_with_registry!(
                "transaction_pool_pending",
                "Number of user transactions pending in the pool, waiting to be taken into a block",
                registry,
            )
            .unwrap(),
            pool_inflight: register_int_gauge_with_registry!(
                "transaction_pool_inflight",
                "Number of user transactions taken into proposed blocks and not yet settled",
                registry,
            )
            .unwrap(),
            pool_wait_latency: register_histogram_with_registry!(
                "transaction_pool_wait_latency",
                "Time a transaction spends pending in the pool before being taken into a block",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            pool_evictions: register_int_counter_with_registry!(
                "transaction_pool_evictions",
                "Number of entries evicted from the pool by higher gas price transactions",
                registry,
            )
            .unwrap(),
            pool_rejections: register_int_counter_with_registry!(
                "transaction_pool_rejections",
                "Number of transactions rejected because the pool was full and their gas price was too low",
                registry,
            )
            .unwrap(),
            pool_coalesced_inserts: register_int_counter_with_registry!(
                "transaction_pool_coalesced_inserts",
                "Duplicate submissions coalesced onto (or admitted alongside) an existing pool entry. Tallied as spam for DoS protection.",
                registry,
            )
            .unwrap(),
            pool_gc_requeues: register_int_counter_with_registry!(
                "transaction_pool_gc_requeues",
                "System entries returned to pending after their block was garbage collected",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Arc<Self> {
        let registry = Registry::new();
        let adapter_metrics = ConsensusAdapterMetrics::new(&registry);
        Arc::new(Self::new(&registry, &adapter_metrics))
    }
}

/// Ranks pool entries for take order: system entries first, then user entries by gas
/// price descending; FIFO within a price level via the insertion sequence. `BTreeMap`
/// ascending iteration order is take order. Extensible: future ranking criteria slot in
/// as comparison fields.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct PoolKey {
    class: PriorityClass,
    price: Reverse<u64>,
    seq: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum PriorityClass {
    System,
    User,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryKind {
    User,
    System,
    /// One-shot system submission: dropped instead of taken past the deadline, and
    /// dropped instead of re-queued on garbage collection.
    BestEffort { deadline: Instant },
    /// Payload-less entry acknowledged with a ping position at the next proposed block.
    Ping,
}

impl EntryKind {
    fn priority_class(&self) -> PriorityClass {
        match self {
            EntryKind::User => PriorityClass::User,
            EntryKind::System | EntryKind::BestEffort { .. } | EntryKind::Ping => {
                PriorityClass::System
            }
        }
    }

    fn is_user(&self) -> bool {
        matches!(self, EntryKind::User)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryState {
    /// Waiting in the pending map to be taken into a block.
    Pending,
    /// Taken by the proposer, awaiting the block ack.
    Staged,
    /// In a proposed block, awaiting a terminal outcome.
    Proposed {
        block_ref: BlockRef,
        first_index: TransactionIndex,
    },
    Settled,
}

/// Why an entry left the pool. Used for waiter errors, metrics labels and logs.
#[derive(Clone, Copy, Debug)]
enum SettleReason {
    /// Consensus key observed processed, from any validator's committed block.
    Processed,
    /// Terminal per-position status for our own proposed block.
    Status(ConsensusTxStatus),
    /// Transaction executed via a (locally built or state-synced) checkpoint.
    CheckpointExecuted,
    /// Own block garbage collected; user and best-effort entries are dropped
    /// (system entries requeue instead of settling).
    GarbageCollected,
    /// Evicted from a full pool by a higher-priced submission carrying this price.
    Evicted { min_gas_price: u64 },
    /// Pool shut down at epoch end.
    Shutdown,
    /// Best-effort entry passed its deadline before being taken.
    BestEffortExpired,
    /// Defensive sweep of a sequenced block whose statuses never fully arrived.
    Sweep,
}

impl SettleReason {
    fn as_label(&self) -> &'static str {
        match self {
            SettleReason::Processed => "processed",
            SettleReason::Status(ConsensusTxStatus::Finalized) => "status_finalized",
            SettleReason::Status(ConsensusTxStatus::Rejected) => "status_rejected",
            SettleReason::Status(ConsensusTxStatus::Dropped) => "status_dropped",
            SettleReason::CheckpointExecuted => "checkpoint_executed",
            SettleReason::GarbageCollected => "garbage_collected",
            SettleReason::Evicted { .. } => "evicted",
            SettleReason::Shutdown => "shutdown",
            SettleReason::BestEffortExpired => "best_effort_expired",
            SettleReason::Sweep => "sweep",
        }
    }

    fn is_success(&self) -> bool {
        matches!(
            self,
            SettleReason::Processed | SettleReason::Status(_) | SettleReason::CheckpointExecuted
        )
    }

    /// The error delivered to waiters that have not received positions yet.
    fn waiter_error(&self, digest: TransactionDigest) -> SuiError {
        match self {
            SettleReason::Processed | SettleReason::Status(_) => {
                SuiErrorKind::TransactionProcessing {
                    digest,
                    status: "processed via consensus".to_string(),
                }
                .into()
            }
            SettleReason::CheckpointExecuted => SuiErrorKind::TransactionProcessing {
                digest,
                status: "processed via checkpoint".to_string(),
            }
            .into(),
            SettleReason::Evicted { min_gas_price } => {
                SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion {
                    min_gas_price: *min_gas_price,
                }
                .into()
            }
            SettleReason::Shutdown => SuiErrorKind::ValidatorHaltedAtEpochEnd.into(),
            SettleReason::GarbageCollected
            | SettleReason::BestEffortExpired
            | SettleReason::Sweep => SuiErrorKind::FailedToSubmitToConsensus(format!(
                "submission ended: {}",
                self.as_label()
            ))
            .into(),
        }
    }
}

fn status_label(status: &ConsensusTxStatus) -> &'static str {
    match status {
        ConsensusTxStatus::Finalized => "finalized",
        ConsensusTxStatus::Rejected => "rejected",
        ConsensusTxStatus::Dropped => "dropped",
    }
}

type PositionWaiter = oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>;
type Notifications = Vec<(PositionWaiter, SuiResult<Vec<ConsensusPosition>>)>;

fn deliver(notifications: Notifications) {
    for (waiter, result) in notifications {
        let _ = waiter.send(result);
    }
}

/// Mutable entry state; guarded by its own mutex, only locked while holding the pool
/// mutex (lock order: pool mutex, then entry mutex).
struct EntryMut {
    state: EntryState,
    waiters: Vec<PositionWaiter>,
    /// Per-transaction flags: true while the transaction lacks a terminal signal.
    unsettled: Vec<bool>,
    unsettled_count: usize,
    /// One element per (coalesced) submission not yet accounted for DoS, drained when
    /// the entry is taken.
    unrecorded_addrs: Vec<Option<IpAddr>>,
    /// Whether the entry has been taken at least once (DoS accounting ran).
    dos_recorded: bool,
}

/// One submission group (a single transaction, or a soft bundle that must land in one
/// block) resident in the pool.
pub struct PoolEntry {
    transactions: Vec<ConsensusTransaction>,
    /// BCS bytes cached at insert so `take()` never serializes on the core thread.
    serialized: Vec<Vec<u8>>,
    keys: Vec<ConsensusTransactionKey>,
    gas_price: u64,
    kind: EntryKind,
    tx_type: &'static str,
    pool_key: PoolKey,
    submit_time: Instant,
    mutable: Mutex<EntryMut>,
}

impl PoolEntry {
    fn total_bytes(&self) -> usize {
        self.serialized.iter().map(|t| t.len()).sum()
    }

    fn num_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn first_user_digest(&self) -> TransactionDigest {
        self.transactions
            .iter()
            .find_map(|t| t.kind.as_user_transaction().map(|tx| *tx.digest()))
            .unwrap_or_default()
    }
}

struct ProposedBlock {
    sequenced: bool,
    /// Entries in block order with the block index of their first transaction.
    entries: Vec<(TransactionIndex, Arc<PoolEntry>)>,
}

struct PoolInner {
    /// Transactions waiting to be proposed, in take order.
    pending: BTreeMap<PoolKey, Arc<PoolEntry>>,
    /// Transactions in our own proposed blocks, awaiting terminal outcome. BlockRef
    /// orders round-first, so GC processes a prefix.
    inflight: BTreeMap<BlockRef, ProposedBlock>,
    /// Settle/dedup index over both maps, first-writer-wins: an entry admitted while
    /// its key is already mapped (partial bundle overlap) settles via its own position
    /// statuses instead.
    by_key: HashMap<ConsensusTransactionKey, (Arc<PoolEntry>, usize)>,
    next_seq: u64,
    /// User transactions resident in `pending`, counted per transaction.
    pending_user_txs: usize,
    /// User transactions staged or proposed and not yet settled, counted per
    /// transaction.
    inflight_user_txs: usize,
    shutdown: bool,
}

impl PoolInner {
    /// Marks transaction `tx_idx` of `entry` settled. Returns true when this was the
    /// entry's last unsettled transaction; the caller must then `finish_entry`.
    fn mark_tx_settled(&self, entry: &PoolEntry, tx_idx: usize) -> bool {
        let mut m = entry.mutable.lock();
        if matches!(m.state, EntryState::Settled) || !m.unsettled[tx_idx] {
            return false;
        }
        m.unsettled[tx_idx] = false;
        m.unsettled_count -= 1;
        m.unsettled_count == 0
    }

    /// Removes the entry from all maps and bookkeeping, transitions it to `Settled`,
    /// and collects its waiters (paired with the error to deliver, when positions were
    /// never sent) into `notifications` for delivery after the pool lock is released.
    fn finish_entry(
        &mut self,
        entry: &Arc<PoolEntry>,
        reason: SettleReason,
        metrics: &TransactionPoolMetrics,
        notifications: &mut Notifications,
    ) {
        let mut m = entry.mutable.lock();
        if matches!(m.state, EntryState::Settled) {
            return;
        }
        let prev_state = m.state;
        m.state = EntryState::Settled;
        m.unsettled_count = 0;
        m.unsettled.fill(false);
        let waiters = std::mem::take(&mut m.waiters);
        drop(m);

        match prev_state {
            EntryState::Pending => {
                self.pending.remove(&entry.pool_key);
                if entry.kind.is_user() {
                    self.pending_user_txs -= entry.num_transactions();
                }
            }
            EntryState::Staged | EntryState::Proposed { .. } => {
                // Staged entries are owned by the in-flight take; proposed entries sit
                // in their `inflight` bucket, cleaned up in `notify_committed`.
                if entry.kind.is_user() {
                    self.inflight_user_txs -= entry.num_transactions();
                }
            }
            EntryState::Settled => unreachable!(),
        }
        for key in &entry.keys {
            if let std::collections::hash_map::Entry::Occupied(slot) =
                self.by_key.entry(key.clone())
                && Arc::ptr_eq(&slot.get().0, entry)
            {
                slot.remove();
            }
        }

        let error = reason.waiter_error(entry.first_user_digest());
        for waiter in waiters {
            notifications.push((waiter, Err(error.clone())));
        }

        let submitted = if matches!(prev_state, EntryState::Proposed { .. }) {
            "true"
        } else {
            "false"
        };
        metrics
            .sequencing_certificate_latency
            .with_label_values(&[submitted, entry.tx_type, reason.as_label()])
            .observe(entry.submit_time.elapsed().as_secs_f64());
        if reason.is_success() {
            metrics
                .sequencing_certificate_success
                .with_label_values(&[entry.tx_type])
                .inc();
        } else {
            metrics
                .sequencing_certificate_failures
                .with_label_values(&[entry.tx_type])
                .inc();
        }
        metrics
            .sequencing_certificate_inflight
            .with_label_values(&[entry.tx_type])
            .sub(entry.num_transactions().max(1) as i64);
        self.publish_gauges(metrics);

        debug!(
            keys = ?entry.keys,
            reason = reason.as_label(),
            residence = ?entry.submit_time.elapsed(),
            "Transaction pool entry settled",
        );
    }

    /// Returns a staged or proposed entry to `pending` (GC requeue and dropped-ack
    /// paths), retaining its original pool key and thus its seniority.
    fn requeue_entry(&mut self, entry: Arc<PoolEntry>) {
        let mut m = entry.mutable.lock();
        debug_assert!(matches!(
            m.state,
            EntryState::Staged | EntryState::Proposed { .. }
        ));
        m.state = EntryState::Pending;
        drop(m);
        if entry.kind.is_user() {
            self.inflight_user_txs -= entry.num_transactions();
            self.pending_user_txs += entry.num_transactions();
        }
        let key = entry.pool_key;
        self.pending.insert(key, entry);
    }

    /// The oldest pending user entry at the lowest gas price: the eviction victim.
    fn eviction_victim(&self) -> Option<Arc<PoolEntry>> {
        let (last_key, last_entry) = self.pending.last_key_value()?;
        if !last_entry.kind.is_user() {
            return None;
        }
        let band_start = PoolKey {
            class: PriorityClass::User,
            price: last_key.price,
            seq: 0,
        };
        self.pending
            .range(band_start..)
            .next()
            .map(|(_, e)| e.clone())
    }

    fn publish_gauges(&self, metrics: &TransactionPoolMetrics) {
        metrics.pool_pending.set(self.pending_user_txs as i64);
        metrics.pool_inflight.set(self.inflight_user_txs as i64);
    }
}

/// Per-epoch pull-based transaction pool. Implements the consensus-core
/// `TransactionPool` trait for the proposer, and receives settle callbacks from the
/// commit handler and checkpoint executor via the epoch store.
pub struct SuiTransactionPool {
    epoch: EpochId,
    epoch_store: Arc<AuthorityPerEpochStore>,
    /// `pending` user-transaction capacity: the gas auction bound.
    capacity: usize,
    /// Inflight user-transaction budget: `take` stops filling blocks with user
    /// transactions when this many are staged/proposed and unsettled.
    max_pending_transactions: usize,
    inner: Arc<Mutex<PoolInner>>,
    metrics: Arc<TransactionPoolMetrics>,
}

impl SuiTransactionPool {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        capacity: usize,
        max_pending_transactions: usize,
        metrics: Arc<TransactionPoolMetrics>,
    ) -> Arc<Self> {
        assert!(capacity > 0);
        Arc::new(Self {
            epoch: epoch_store.epoch(),
            epoch_store,
            capacity,
            max_pending_transactions,
            inner: Arc::new(Mutex::new(PoolInner {
                pending: BTreeMap::new(),
                inflight: BTreeMap::new(),
                by_key: HashMap::new(),
                next_seq: 0,
                pending_user_txs: 0,
                inflight_user_txs: 0,
                shutdown: false,
            })),
            metrics,
        })
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }

    /// Number of user transactions pending in the pool.
    pub fn pending_user_transactions(&self) -> usize {
        self.inner.lock().pending_user_txs
    }

    /// Number of user transactions staged or in proposed blocks, not yet settled.
    pub fn inflight_user_transactions(&self) -> usize {
        self.inner.lock().inflight_user_txs
    }

    /// Submits user transactions (or a ping, when `transactions` is empty) and returns
    /// a receiver for their consensus positions plus whether this was a new entry
    /// (false = duplicate submission; tallied as spam by the RPC layer).
    ///
    /// `gas_price` is the minimum across the group, 0 for gasless.
    pub fn submit_user_transactions(
        &self,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<(oneshot::Receiver<SuiResult<Vec<ConsensusPosition>>>, bool)> {
        let kind = if transactions.is_empty() {
            EntryKind::Ping
        } else {
            EntryKind::User
        };
        let (tx, rx) = oneshot::channel();
        let newly_inserted = self.submit_entry(
            kind,
            gas_price,
            transactions,
            submitter_client_addr,
            Some(tx),
        )?;
        Ok((rx, newly_inserted))
    }

    /// Submits a system transaction. System entries are never evicted, don't count
    /// against pool capacity, and are retained (re-queued on garbage collection) until
    /// observed processed or the epoch ends — unless `best_effort_deadline` is set, in
    /// which case the entry is dropped at the deadline or on garbage collection.
    pub fn submit_system_transaction(
        &self,
        transaction: ConsensusTransaction,
        best_effort_deadline: Option<Instant>,
    ) -> SuiResult {
        assert!(
            !transaction.is_user_transaction(),
            "user transactions must go through submit_user_transactions"
        );
        let kind = match best_effort_deadline {
            Some(deadline) => EntryKind::BestEffort { deadline },
            None => EntryKind::System,
        };
        if matches!(transaction.kind, ConsensusTransactionKind::EndOfPublish(..)) {
            info!(epoch = ?self.epoch, "Submitting EndOfPublish message to consensus");
            self.epoch_store
                .record_epoch_pending_certs_process_time_metric();
        }
        self.submit_entry(kind, 0, vec![transaction], None, None)?;
        Ok(())
    }

    /// Inserts an entry into the pool. Returns whether it was newly inserted (false =
    /// duplicate of an existing entry, coalesced or admitted alongside).
    fn submit_entry(
        &self,
        kind: EntryKind,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        submitter_client_addr: Option<IpAddr>,
        waiter: Option<PositionWaiter>,
    ) -> SuiResult<bool> {
        let keys: Vec<_> = transactions.iter().map(|t| t.key()).collect();
        let serialized: Vec<_> = transactions
            .iter()
            .map(|t| bcs::to_bytes(t).expect("Serializing consensus transaction cannot fail"))
            .collect();
        let tx_type = match kind {
            EntryKind::Ping => "ping",
            _ if transactions.len() > 1 => "soft_bundle",
            _ => classify(&transactions[0]),
        };
        let num_txs = transactions.len();

        let mut notifications = Notifications::new();
        let mut record_dos_for_coalesced: Option<Arc<PoolEntry>> = None;
        let result = 'inserted: {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
            }

            // Coalesce onto an existing entry when the key sets match exactly: the new
            // waiter shares the existing entry's positions. A partial overlap is
            // admitted as a separate entry (flagged not-new), like today's queue
            // duplicates; such an entry settles via its own position statuses.
            if !keys.is_empty()
                && let Some((existing, _)) = inner.by_key.get(&keys[0]).cloned()
                && existing.keys == keys
            {
                let mut m = existing.mutable.lock();
                if let EntryState::Proposed {
                    block_ref,
                    first_index,
                } = m.state
                {
                    // Positions are already known: answer immediately.
                    if let Some(waiter) = waiter {
                        let positions = (0..num_txs as TransactionIndex)
                            .map(|i| ConsensusPosition {
                                epoch: self.epoch,
                                block: block_ref,
                                index: first_index + i,
                            })
                            .collect();
                        notifications.push((waiter, Ok(positions)));
                    }
                } else if let Some(waiter) = waiter {
                    m.waiters.push(waiter);
                }
                if m.dos_recorded {
                    record_dos_for_coalesced = Some(existing.clone());
                } else {
                    m.unrecorded_addrs.push(submitter_client_addr);
                }
                drop(m);
                self.metrics.pool_coalesced_inserts.inc();
                debug!(?keys, "Coalesced duplicate submission onto pool entry");
                break 'inserted Ok(false);
            }

            let newly_inserted = !keys.iter().any(|k| inner.by_key.contains_key(k));
            if !newly_inserted {
                self.metrics.pool_coalesced_inserts.inc();
            }

            // Gas auction: evict strictly lower-priced user entries (oldest at the
            // lowest price first) until the newcomer fits; reject the newcomer if
            // that cannot free enough capacity.
            if kind.is_user() {
                while inner.pending_user_txs + num_txs > self.capacity {
                    let victim = inner.eviction_victim();
                    match victim {
                        Some(victim) if victim.gas_price < gas_price => {
                            inner.finish_entry(
                                &victim,
                                SettleReason::Evicted {
                                    min_gas_price: gas_price,
                                },
                                &self.metrics,
                                &mut notifications,
                            );
                            self.metrics.pool_evictions.inc();
                            info!(
                                evicted_keys = ?victim.keys,
                                evicted_gas_price = victim.gas_price,
                                evicting_gas_price = gas_price,
                                "Evicted pool entry outbid by higher gas price",
                            );
                        }
                        _ => {
                            self.metrics.pool_rejections.inc();
                            let min_gas_price =
                                victim.map(|v| v.gas_price).unwrap_or(gas_price);
                            info!(
                                ?keys,
                                gas_price,
                                min_gas_price,
                                "Rejected submission: pool full and gas price too low",
                            );
                            drop(inner);
                            deliver(notifications);
                            return Err(
                                SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion {
                                    min_gas_price,
                                }
                                .into(),
                            );
                        }
                    }
                }
            }

            let pool_key = PoolKey {
                class: kind.priority_class(),
                price: Reverse(gas_price),
                seq: inner.next_seq,
            };
            inner.next_seq += 1;

            let entry = Arc::new(PoolEntry {
                serialized,
                keys: keys.clone(),
                gas_price,
                kind,
                tx_type,
                pool_key,
                submit_time: Instant::now(),
                mutable: Mutex::new(EntryMut {
                    state: EntryState::Pending,
                    waiters: waiter.into_iter().collect(),
                    unsettled: vec![true; num_txs],
                    unsettled_count: num_txs,
                    unrecorded_addrs: vec![submitter_client_addr],
                    dos_recorded: false,
                }),
                transactions,
            });

            for (i, key) in keys.iter().enumerate() {
                inner
                    .by_key
                    .entry(key.clone())
                    .or_insert((entry.clone(), i));
            }
            if kind.is_user() {
                inner.pending_user_txs += num_txs;
            }
            inner.pending.insert(pool_key, entry);

            self.metrics
                .sequencing_certificate_attempt
                .with_label_values(&[tx_type])
                .inc();
            self.metrics
                .sequencing_certificate_inflight
                .with_label_values(&[tx_type])
                .add(num_txs.max(1) as i64);
            inner.publish_gauges(&self.metrics);
            debug!(?keys, gas_price, ?kind, "Transaction entered pool");
            Ok(newly_inserted)
        };
        deliver(notifications);
        if let Some(entry) = record_dos_for_coalesced {
            self.record_dos_accounting(&entry, &[submitter_client_addr]);
        }
        result
    }

    /// DoS/spam accounting, at the point transactions are actually submitted to
    /// consensus: caps how many times a digest may be observed in consensus output
    /// across validators before further appearances are tallied as spam.
    fn record_dos_accounting(&self, entry: &PoolEntry, addrs: &[Option<IpAddr>]) {
        if !entry.kind.is_user() {
            return;
        }
        let rgp = self.epoch_store.reference_gas_price().max(1);
        for tx in &entry.transactions {
            let Some(user_tx) = tx.kind.as_user_transaction() else {
                continue;
            };
            let amplification_factor = (user_tx.transaction_data().gas_price() / rgp).max(1);
            for addr in addrs {
                self.epoch_store
                    .submitted_transaction_cache
                    .record_submitted_tx(user_tx.digest(), amplification_factor as u32, *addr);
            }
        }
    }

    /// Settles entries whose consensus key was recorded processed by the commit
    /// handler, for commits from any validator's blocks. This is the sole settle
    /// signal for system messages, and reaches entries in any state — a pending entry
    /// covered by another validator's commit is answered without being proposed.
    pub fn note_processed<'a>(
        &self,
        keys: impl Iterator<Item = &'a SequencedConsensusTransactionKey>,
    ) {
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return;
            }
            for key in keys {
                let SequencedConsensusTransactionKey::External(key) = key else {
                    continue;
                };
                let Some((entry, idx)) = inner.by_key.get(key).cloned() else {
                    continue;
                };
                if inner.mark_tx_settled(&entry, idx) {
                    inner.finish_entry(
                        &entry,
                        SettleReason::Processed,
                        &self.metrics,
                        &mut notifications,
                    );
                    self.metrics
                        .sequencing_certificate_processed
                        .with_label_values(&["consensus"])
                        .inc();
                }
            }
        }
        deliver(notifications);
    }

    /// Settles user transactions in our own proposed blocks by their terminal
    /// per-position status. This is the sole settle signal for vote-rejected
    /// transactions and for certs-closed / post-EndOfPublish drops, which are never
    /// marked consensus-message-processed.
    pub fn note_statuses(&self, updates: &[(ConsensusPosition, ConsensusTxStatus)]) {
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return;
            }
            for (position, status) in updates {
                let Some(block) = inner.inflight.get(&position.block) else {
                    // Not our block (or already cleaned up).
                    continue;
                };
                // The entry covering this index: the last entry whose
                // first_index <= position.index.
                let i = match block
                    .entries
                    .binary_search_by_key(&position.index, |(first, _)| *first)
                {
                    Ok(i) => i,
                    Err(0) => continue,
                    Err(i) => i - 1,
                };
                let (first_index, entry) = &block.entries[i];
                let tx_idx = (position.index - first_index) as usize;
                if tx_idx >= entry.num_transactions() {
                    // Covers the block-level ping position as well.
                    continue;
                }
                let entry = entry.clone();
                self.metrics
                    .sequencing_certificate_settled_status
                    .with_label_values(&[entry.tx_type, status_label(status)])
                    .inc();
                if inner.mark_tx_settled(&entry, tx_idx) {
                    inner.finish_entry(
                        &entry,
                        SettleReason::Status(*status),
                        &self.metrics,
                        &mut notifications,
                    );
                }
            }
        }
        deliver(notifications);
    }

    /// Settles entries whose transaction was executed via a certified checkpoint
    /// (locally built or state-synced).
    pub fn note_executed_in_checkpoint(&self, digests: &[TransactionDigest]) {
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return;
            }
            for digest in digests {
                let key = ConsensusTransactionKey::Certificate(*digest);
                let Some((entry, idx)) = inner.by_key.get(&key).cloned() else {
                    continue;
                };
                if inner.mark_tx_settled(&entry, idx) {
                    inner.finish_entry(
                        &entry,
                        SettleReason::CheckpointExecuted,
                        &self.metrics,
                        &mut notifications,
                    );
                    self.metrics
                        .sequencing_certificate_processed
                        .with_label_values(&["checkpoint"])
                        .inc();
                }
            }
        }
        deliver(notifications);
    }

    /// Shuts the pool down at epoch end: fails every waiter with
    /// `ValidatorHaltedAtEpochEnd` and drains all maps. Subsequent submissions are
    /// rejected and callbacks become no-ops.
    pub fn shutdown(&self) {
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return;
            }
            inner.shutdown = true;
            let pending: Vec<_> = inner.pending.values().cloned().collect();
            let proposed: Vec<_> = inner
                .inflight
                .values()
                .flat_map(|b| b.entries.iter().map(|(_, e)| e.clone()))
                .collect();
            let (num_pending, num_proposed) = (pending.len(), proposed.len());
            for entry in pending.into_iter().chain(proposed) {
                inner.finish_entry(
                    &entry,
                    SettleReason::Shutdown,
                    &self.metrics,
                    &mut notifications,
                );
                self.metrics.num_rejected_cert_in_epoch_boundary.inc();
            }
            inner.pending.clear();
            inner.inflight.clear();
            inner.by_key.clear();
            inner.pending_user_txs = 0;
            inner.inflight_user_txs = 0;
            inner.publish_gauges(&self.metrics);
            info!(
                epoch = self.epoch,
                num_pending, num_proposed, "Transaction pool shut down",
            );
        }
        deliver(notifications);
    }
}

impl TransactionPool for SuiTransactionPool {
    fn take(
        &self,
        max_count: usize,
        max_bytes: usize,
    ) -> (
        Vec<consensus_core::Transaction>,
        Box<dyn FnOnce(BlockRef) + Send>,
        LimitReached,
    ) {
        let mut taken: Vec<Arc<PoolEntry>> = Vec::new();
        let mut transactions = Vec::new();
        let mut limit = LimitReached::AllTransactionsIncluded;
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if !inner.shutdown {
                let user_budget = self
                    .max_pending_transactions
                    .saturating_sub(inner.inflight_user_txs);
                let now = Instant::now();
                let mut count = 0usize;
                let mut bytes = 0usize;
                let mut user_count = 0usize;
                let mut expired = Vec::new();
                for entry in inner.pending.values() {
                    if let EntryKind::BestEffort { deadline } = entry.kind
                        && deadline < now
                    {
                        expired.push(entry.clone());
                        continue;
                    }
                    let n = entry.num_transactions();
                    let entry_bytes = entry.total_bytes();
                    if bytes + entry_bytes > max_bytes {
                        limit = LimitReached::MaxBytes;
                        break;
                    }
                    if count + n > max_count {
                        limit = LimitReached::MaxNumOfTransactions;
                        break;
                    }
                    if entry.kind.is_user() && user_count + n > user_budget {
                        // Pool-side inflight throttle, not a block limit: stop filling
                        // blocks with user transactions until in-flight ones settle.
                        // All remaining entries are user entries (system sorts first).
                        limit = LimitReached::MaxNumOfTransactions;
                        break;
                    }
                    count += n;
                    bytes += entry_bytes;
                    if entry.kind.is_user() {
                        user_count += n;
                    }
                    taken.push(entry.clone());
                }
                for entry in expired {
                    inner.finish_entry(
                        &entry,
                        SettleReason::BestEffortExpired,
                        &self.metrics,
                        &mut notifications,
                    );
                    self.metrics
                        .sequencing_best_effort_timeout
                        .with_label_values(&[entry.tx_type])
                        .inc();
                }
                for entry in &taken {
                    inner.pending.remove(&entry.pool_key);
                    entry.mutable.lock().state = EntryState::Staged;
                    if entry.kind.is_user() {
                        inner.pending_user_txs -= entry.num_transactions();
                        inner.inflight_user_txs += entry.num_transactions();
                    }
                    self.metrics
                        .pool_wait_latency
                        .observe(entry.submit_time.elapsed().as_secs_f64());
                    for bytes in &entry.serialized {
                        transactions.push(consensus_core::Transaction::new(bytes.clone()));
                    }
                }
                inner.publish_gauges(&self.metrics);
            }
        }
        deliver(notifications);

        // DoS accounting for every (coalesced) submission of each taken user
        // transaction, at the point of actual submission to consensus.
        for entry in &taken {
            let addrs = {
                let mut m = entry.mutable.lock();
                m.dos_recorded = true;
                std::mem::take(&mut m.unrecorded_addrs)
            };
            self.record_dos_accounting(entry, &addrs);
        }

        let batch = StagedBatch {
            inner: self.inner.clone(),
            metrics: self.metrics.clone(),
            epoch: self.epoch,
            entries: Some(taken),
        };
        (
            transactions,
            Box::new(move |block_ref| batch.ack(block_ref)),
            limit,
        )
    }

    fn notify_committed(&self, own_committed_blocks: Vec<BlockRef>, gc_round: Round) {
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            if inner.shutdown {
                return;
            }
            for block_ref in own_committed_blocks {
                if let Some(block) = inner.inflight.get_mut(&block_ref) {
                    block.sequenced = true;
                    for (_, entry) in &block.entries {
                        self.metrics
                            .sequencing_certificate_status
                            .with_label_values(&[entry.tx_type, "sequenced"])
                            .inc();
                    }
                }
            }

            let expired_refs: Vec<BlockRef> = inner
                .inflight
                .keys()
                .take_while(|b| b.round <= gc_round)
                .copied()
                .collect();
            for block_ref in expired_refs {
                let block = inner.inflight.remove(&block_ref).unwrap();
                let unsettled: Vec<_> = block
                    .entries
                    .iter()
                    .filter(|(_, e)| !matches!(e.mutable.lock().state, EntryState::Settled))
                    .map(|(_, e)| e.clone())
                    .collect();
                if block.sequenced {
                    if unsettled.is_empty() {
                        continue;
                    }
                    if block_ref.round + SWEEP_GRACE_ROUNDS > gc_round {
                        // Statuses from the sequencing commit may still be in flight;
                        // keep the bucket for now.
                        inner.inflight.insert(block_ref, block);
                        continue;
                    }
                    // A sequenced block's transactions must all have received status
                    // or processed signals by now; a leftover indicates missed settle
                    // coverage. Degrade to a metric and a warning instead of leaking.
                    debug_fatal!(
                        "Unsettled entries in sequenced block {} far below gc round {}",
                        block_ref,
                        gc_round
                    );
                    for entry in unsettled {
                        warn!(
                            keys = ?entry.keys,
                            %block_ref,
                            "Sweeping unsettled entry in sequenced block",
                        );
                        inner.finish_entry(
                            &entry,
                            SettleReason::Sweep,
                            &self.metrics,
                            &mut notifications,
                        );
                    }
                    continue;
                }
                // The block was garbage collected and will never commit. System
                // entries requeue (the protocol depends on them landing); user and
                // best-effort entries settle — the client observes position expiry
                // via WaitForEffects and resubmits.
                for entry in unsettled {
                    self.metrics
                        .sequencing_certificate_status
                        .with_label_values(&[entry.tx_type, "garbage_collected"])
                        .inc();
                    if matches!(entry.kind, EntryKind::System) {
                        self.metrics.pool_gc_requeues.inc();
                        debug!(
                            keys = ?entry.keys,
                            %block_ref,
                            "Requeuing system entry from garbage collected block",
                        );
                        inner.requeue_entry(entry);
                    } else {
                        inner.finish_entry(
                            &entry,
                            SettleReason::GarbageCollected,
                            &self.metrics,
                            &mut notifications,
                        );
                    }
                }
            }
            inner.publish_gauges(&self.metrics);
        }
        deliver(notifications);
    }
}

/// Entries taken by the proposer, awaiting the block ack. Dropping the batch without
/// acking (proposer error path, consensus shutdown) returns the entries to the pool.
struct StagedBatch {
    inner: Arc<Mutex<PoolInner>>,
    metrics: Arc<TransactionPoolMetrics>,
    epoch: EpochId,
    entries: Option<Vec<Arc<PoolEntry>>>,
}

impl StagedBatch {
    fn ack(mut self, block_ref: BlockRef) {
        let entries = self.entries.take().unwrap();
        let mut notifications = Notifications::new();
        {
            let mut inner = self.inner.lock();
            let mut offset: TransactionIndex = 0;
            let mut proposed = Vec::new();
            let mut num_txs = 0usize;
            for entry in entries {
                let first_index = offset;
                // The block contains the entry's transactions regardless of entry
                // state, so the offset always advances.
                offset += entry.num_transactions() as TransactionIndex;
                num_txs += entry.num_transactions();

                let mut m = entry.mutable.lock();
                match m.state {
                    EntryState::Staged if matches!(entry.kind, EntryKind::Ping) => {
                        m.state = EntryState::Settled;
                        let waiters = std::mem::take(&mut m.waiters);
                        drop(m);
                        for waiter in waiters {
                            notifications.push((
                                waiter,
                                Ok(vec![ConsensusPosition::ping(self.epoch, block_ref)]),
                            ));
                        }
                        self.metrics
                            .sequencing_certificate_inflight
                            .with_label_values(&[entry.tx_type])
                            .sub(1);
                    }
                    EntryState::Staged => {
                        m.state = EntryState::Proposed {
                            block_ref,
                            first_index,
                        };
                        let waiters = std::mem::take(&mut m.waiters);
                        drop(m);
                        let positions: Vec<_> = (0..entry.num_transactions()
                            as TransactionIndex)
                            .map(|i| ConsensusPosition {
                                epoch: self.epoch,
                                block: block_ref,
                                index: first_index + i,
                            })
                            .collect();
                        for waiter in waiters {
                            notifications.push((waiter, Ok(positions.clone())));
                        }
                        self.metrics
                            .sequencing_acknowledge_latency
                            .with_label_values(&["false", entry.tx_type])
                            .observe(entry.submit_time.elapsed().as_secs_f64());
                        proposed.push((first_index, entry.clone()));
                    }
                    // Settled while staged (e.g. observed processed from another
                    // validator's commit, or pool shutdown): its bytes are in the
                    // block, but there is nothing left to track.
                    EntryState::Settled => {}
                    state => {
                        debug_fatal!("Unexpected entry state at block ack: {:?}", state);
                    }
                }
            }
            if num_txs > 0 || !proposed.is_empty() {
                debug!(
                    %block_ref,
                    entries = proposed.len(),
                    transactions = num_txs,
                    "Transactions submitted to consensus in proposed block",
                );
            }
            if !proposed.is_empty() {
                inner.inflight.insert(
                    block_ref,
                    ProposedBlock {
                        sequenced: false,
                        entries: proposed,
                    },
                );
            }
        }
        deliver(notifications);
    }
}

impl Drop for StagedBatch {
    fn drop(&mut self) {
        let Some(entries) = self.entries.take() else {
            return;
        };
        let mut inner = self.inner.lock();
        for entry in entries {
            if matches!(entry.mutable.lock().state, EntryState::Staged) {
                inner.requeue_entry(entry);
            }
        }
        inner.publish_gauges(&self.metrics);
    }
}

/// Long-lived handle to the current epoch's pool. Carries the sui-core-facing entry
/// points; rotated at reconfiguration.
#[derive(Clone)]
pub struct TransactionPoolContext {
    inner: Arc<TransactionPoolContextInner>,
}

struct TransactionPoolContextInner {
    checkpoint_store: Arc<CheckpointStore>,
    authority: AuthorityName,
    capacity: usize,
    max_pending_transactions: usize,
    metrics: Arc<TransactionPoolMetrics>,
    current: ArcSwap<SuiTransactionPool>,
}

impl TransactionPoolContext {
    pub fn new(
        checkpoint_store: Arc<CheckpointStore>,
        authority: AuthorityName,
        max_pending_transactions: usize,
        metrics: Arc<TransactionPoolMetrics>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Self {
        // Pending capacity absorbs the full stream (there is no bypass), so it
        // defaults to the full inflight budget; total resident transactions are
        // bounded by 2x max_pending_transactions.
        let capacity = max_pending_transactions;
        let pool = SuiTransactionPool::new(
            epoch_store,
            capacity,
            max_pending_transactions,
            metrics.clone(),
        );
        Self {
            inner: Arc::new(TransactionPoolContextInner {
                checkpoint_store,
                authority,
                capacity,
                max_pending_transactions,
                metrics,
                current: ArcSwap::new(pool),
            }),
        }
    }

    /// The current epoch's pool; handed to consensus at epoch start and registered on
    /// the epoch store for settle callbacks.
    pub fn current_pool(&self) -> Arc<SuiTransactionPool> {
        self.inner.current.load_full()
    }

    /// Shuts down the old epoch's pool (failing all waiters with
    /// `ValidatorHaltedAtEpochEnd`) and swaps in a fresh pool bound to the new epoch.
    /// Returns the new pool for handing to consensus.
    pub fn rotate_for_epoch(
        &self,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Arc<SuiTransactionPool> {
        let new_pool = SuiTransactionPool::new(
            epoch_store,
            self.inner.capacity,
            self.inner.max_pending_transactions,
            self.inner.metrics.clone(),
        );
        let old_pool = self.inner.current.swap(new_pool.clone());
        if !Arc::ptr_eq(&old_pool, &new_pool) {
            old_pool.shutdown();
        }
        new_pool
    }

    /// Submits user transactions (or a ping when `transactions` is empty) and returns
    /// a receiver for their consensus positions, plus whether the submission created a
    /// new pool entry (false is tallied as spam weight by the RPC layer).
    ///
    /// A `TransactionProcessing` error means the outcome is already known via
    /// consensus output or checkpoint state; it is retriable per transaction.
    pub fn submit_for_positions(
        &self,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<(oneshot::Receiver<SuiResult<Vec<ConsensusPosition>>>, bool)> {
        let pool = self.current_pool();
        if pool.epoch() != epoch_store.epoch() {
            return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }

        // Hold the reconfiguration read lock across the insert so no user transaction
        // enters the pool after user certs close.
        let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
        if !reconfiguration_lock.should_accept_user_certs() {
            self.inner
                .metrics
                .num_rejected_cert_in_epoch_boundary
                .inc();
            return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }

        // Answer early when the outcome is already known via consensus output or
        // checkpoint state, instead of (re)submitting.
        if !transactions.is_empty()
            && let Some(status) = check_already_processed(
                &transactions,
                epoch_store,
                &self.inner.checkpoint_store,
                &self.inner.metrics,
            )
        {
            let digest = transactions
                .iter()
                .find_map(|t| t.kind.as_user_transaction().map(|tx| *tx.digest()))
                .unwrap_or_default();
            return Err(SuiErrorKind::TransactionProcessing {
                digest,
                status: status.to_string(),
            }
            .into());
        }

        pool.submit_user_transactions(gas_price, transactions, submitter_client_addr)
    }

    /// Submits user transactions and awaits their consensus positions. Same contract
    /// as `ConsensusAdapter::submit_and_get_positions`.
    pub async fn submit_and_get_positions(
        &self,
        transactions: Vec<ConsensusTransaction>,
        gas_price: u64,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<Vec<ConsensusPosition>> {
        let _timer = self.inner.metrics.consensus_latency.start_timer();
        let (rx, _newly_inserted) =
            self.submit_for_positions(gas_price, transactions, epoch_store, submitter_client_addr)?;
        rx.await.map_err(|e| {
            SuiError::from(SuiErrorKind::FailedToSubmitToConsensus(format!(
                "Failed to get consensus position: {e}"
            )))
        })?
    }

    /// Crash recovery: resubmit EndOfPublish if the node restarted after closing user
    /// certs but before the message landed.
    pub fn recover_end_of_publish(&self, epoch_store: &Arc<AuthorityPerEpochStore>) {
        if epoch_store.should_send_end_of_publish() {
            let transaction = ConsensusTransaction::new_end_of_publish(self.inner.authority);
            if let Err(e) = self.submit_to_consensus(&[transaction], epoch_store) {
                warn!("Failed to recover EndOfPublish submission: {e}");
            }
        }
    }
}

impl ReconfigurationInitiator for TransactionPoolContext {
    /// Begins reconfiguration: sets reconfig state to reject new user transactions,
    /// then submits EndOfPublish.
    fn close_epoch(&self, epoch_store: &Arc<AuthorityPerEpochStore>) {
        {
            let reconfig_guard = epoch_store.get_reconfig_state_write_lock_guard();
            if !reconfig_guard.should_accept_user_certs() {
                // Allow caller to call this method multiple times
                return;
            }
            epoch_store.close_user_certs(reconfig_guard);
        }
        if epoch_store.should_send_end_of_publish() {
            let transaction = ConsensusTransaction::new_end_of_publish(self.inner.authority);
            if let Err(err) = self.submit_to_consensus(&[transaction], epoch_store) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
    }
}

impl SubmitToConsensus for TransactionPoolContext {
    fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let pool = self.current_pool();
        if pool.epoch() != epoch_store.epoch() {
            return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
        for transaction in transactions {
            // Answer early: system messages get no per-position statuses and their
            // processed key is only recorded once, so an already-processed message
            // must not enter the pool (it would never settle).
            if check_already_processed(
                std::slice::from_ref(transaction),
                epoch_store,
                &self.inner.checkpoint_store,
                &self.inner.metrics,
            )
            .is_some()
            {
                continue;
            }
            pool.submit_system_transaction(transaction.clone(), None)?;
        }
        Ok(())
    }

    fn submit_best_effort(
        &self,
        transaction: &ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        timeout: Duration,
    ) -> SuiResult {
        if transaction.is_user_transaction() {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: "submit_best_effort does not accept user transactions".to_string(),
            }
            .into());
        }
        let pool = self.current_pool();
        if pool.epoch() != epoch_store.epoch() {
            return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
        if check_already_processed(
            std::slice::from_ref(transaction),
            epoch_store,
            &self.inner.checkpoint_store,
            &self.inner.metrics,
        )
        .is_some()
        {
            return Ok(());
        }
        pool.submit_system_transaction(transaction.clone(), Some(Instant::now() + timeout))
    }
}

/// Synchronous already-processed check against consensus-processed keys,
/// checkpoint-executed digests and synced checkpoint sequence numbers. Returns the
/// processed method when every transaction of the group is covered.
fn check_already_processed(
    transactions: &[ConsensusTransaction],
    epoch_store: &Arc<AuthorityPerEpochStore>,
    checkpoint_store: &Arc<CheckpointStore>,
    metrics: &TransactionPoolMetrics,
) -> Option<&'static str> {
    let mut seen_checkpoint = false;
    for transaction in transactions {
        let key = transaction.key();
        if epoch_store
            .is_consensus_message_processed(&SequencedConsensusTransactionKey::External(
                key.clone(),
            ))
            .expect("Storage error when checking consensus message processed")
        {
            metrics
                .sequencing_certificate_processed
                .with_label_values(&["consensus"])
                .inc();
            continue;
        }
        if let ConsensusTransactionKey::Certificate(digest) = &key
            && epoch_store
                .is_transaction_executed_in_checkpoint(digest)
                .expect("Storage error when checking transaction executed in checkpoint")
        {
            metrics
                .sequencing_certificate_processed
                .with_label_values(&["checkpoint"])
                .inc();
            seen_checkpoint = true;
            continue;
        }
        if let ConsensusTransactionKey::CheckpointSignature(_, seq)
        | ConsensusTransactionKey::CheckpointSignatureV2(_, seq, _) = &key
            && let Some(synced_seq) = checkpoint_store
                .get_highest_synced_checkpoint_seq_number()
                .expect("Storage error when reading highest synced checkpoint")
            && synced_seq >= *seq
        {
            metrics
                .sequencing_certificate_processed
                .with_label_values(&["synced_checkpoint"])
                .inc();
            seen_checkpoint = true;
            continue;
        }
        return None;
    }
    if seen_checkpoint {
        Some("processed via checkpoint")
    } else {
        Some("processed via consensus")
    }
}
