// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pull-based submission of transactions to consensus, enabled by
//! `NodeConfig::consensus_transaction_pool`. Instead of the admission-queue drain
//! task pushing transactions through `ConsensusAdapter` into consensus, the
//! consensus proposer pulls them from a [`ConsensusTransactionPool`] at block
//! proposal time via the `consensus_core::TransactionPool` trait.
//!
//! Producers feed the pool from three directions: the validator gRPC service
//! inserts user transactions ([`TransactionPoolContext::try_insert`]), the
//! `ConsensusAdapter` submits system transactions and pings through
//! [`TransactionPoolClient`], and each submission is resolved when the proposer
//! includes it in a block (or with an error on eviction or epoch end).
//!
//! A pool instance is bound to a single epoch: `ConsensusManager` creates one at
//! consensus start, publishes it through the process-lifetime
//! [`TransactionPoolContext`], and closes it before stopping consensus.

use crate::admission_queue::{
    AdmissionQueueEntry, AdmissionQueueMetrics, PopAction, PriorityAdmissionQueue,
};
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::{
    BlockStatusReceiver, ConsensusAdapterMetrics, ConsensusClient, ProcessedMethod,
    processing_error,
};
use crate::consensus_handler::{SequencedConsensusTransactionKey, tx_type_label};
use async_trait::async_trait;
use consensus_core::{BlockStatus, ClientError, LimitReached, Transaction, TransactionPool};
use consensus_types::block::{
    BlockRef, NUM_RESERVED_TRANSACTION_INDICES, PING_TRANSACTION_INDEX, Round, TransactionIndex,
};
use itertools::Itertools;
use mysten_common::debug_fatal;
use mysten_common::sync::notify_read::OwnedRegistration;
use parking_lot::Mutex;
use prometheus::IntGauge;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_macros::fail_point_if;
use sui_types::base_types::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_consensus::{
    ConsensusPosition, ConsensusTransaction, ConsensusTransactionKey,
};
use tokio::sync::{oneshot, watch};
use tracing::warn;

// Pings originate from external RPC clients, so we give them a hard bound.
const MAX_PENDING_PINGS: usize = 1_024;

// System traffic is correctness-critical and cannot be rejected. A depth this
// large signals that proposal progress is stuck and needs operator attention.
const SYSTEM_LANE_WARN_THRESHOLD: usize = 10_000;

/// Resolves with the submission's final consensus positions once the proposer
/// includes it in a block, or with an error on eviction or epoch end.
pub type PositionReceiver = oneshot::Receiver<SuiResult<Vec<ConsensusPosition>>>;

/// Payload delivered when the proposer includes a `ConsensusAdapter`-path
/// submission in a block: the block, the submission's transaction indices in it,
/// and a subscription for the block's eventual status.
type BlockInclusion = (
    BlockRef,
    Vec<TransactionIndex>,
    oneshot::Receiver<BlockStatus>,
);

/// How a queued submission is resolved once its fate is known.
enum EntryAck {
    /// RPC user path: positions on inclusion, error on eviction / epoch end.
    User(oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>),
    /// `ConsensusAdapter` path (system transactions and pings): block inclusion
    /// ack plus a `BlockStatus` subscription.
    SystemOrPing(oneshot::Sender<BlockInclusion>),
}

/// Must be explicitly resolved; dropping it unresolved is a bug.
struct PendingAck {
    ack: Option<EntryAck>,
    keys: Vec<ConsensusTransactionKey>,
    created_at: &'static std::panic::Location<'static>,
    created: Instant,
}

impl PendingAck {
    #[track_caller]
    fn new(ack: EntryAck, keys: Vec<ConsensusTransactionKey>) -> Self {
        Self {
            ack: Some(ack),
            keys,
            created_at: std::panic::Location::caller(),
            created: Instant::now(),
        }
    }

    fn resolve_included(
        mut self,
        epoch: EpochId,
        block_ref: BlockRef,
        indices: Vec<TransactionIndex>,
        block: &mut ProposedBlock,
    ) {
        match self.ack.take().expect("ack must be pending") {
            EntryAck::User(sender) => {
                let positions = indices
                    .into_iter()
                    .map(|index| ConsensusPosition {
                        epoch,
                        block: block_ref,
                        index,
                    })
                    .collect();
                let _ = sender.send(Ok(positions));
            }
            EntryAck::SystemOrPing(sender) => {
                let (status_sender, status_receiver) = oneshot::channel();
                block.subscribers.push(status_sender);
                let _ = sender.send((block_ref, indices, status_receiver));
            }
        }
    }

    fn resolve_error(mut self, error: SuiError) {
        match self.ack.take().expect("ack must be pending") {
            EntryAck::User(sender) => {
                let _ = sender.send(Err(error));
            }
            EntryAck::SystemOrPing(sender) => {
                drop(sender);
            }
        }
    }

    fn drop_deliberately(mut self, _reason: &'static str) {
        drop(self.ack.take().expect("ack must be pending"));
    }

    fn is_abandoned_system(&self) -> bool {
        matches!(&self.ack, Some(EntryAck::SystemOrPing(sender)) if sender.is_closed())
    }
}

impl Drop for PendingAck {
    fn drop(&mut self) {
        let flavor = match self.ack {
            Some(EntryAck::User(_)) => "User",
            Some(EntryAck::SystemOrPing(_)) => "SystemOrPing",
            None => return,
        };
        if !std::thread::panicking() {
            debug_fatal!(
                "{} ack created at {} dropped without resolution after {:?}; keys={:?}",
                flavor,
                self.created_at,
                self.created.elapsed(),
                self.keys
            );
        }
    }
}

/// Watches one queued transaction key for the two ways it can become processed
/// without this validator proposing it: consensus output from a block another
/// validator proposed, and execution through a state-synced checkpoint.
struct ProcessedWatch {
    consensus: OwnedRegistration<SequencedConsensusTransactionKey, ()>,
    checkpoint: Option<OwnedRegistration<TransactionDigest, CheckpointSequenceNumber>>,
    /// Saves results of `try_recv` on the fields above.
    observed: Option<ProcessedMethod>,
}

impl ProcessedWatch {
    fn register(epoch_store: &Arc<AuthorityPerEpochStore>, key: ConsensusTransactionKey) -> Self {
        let key = SequencedConsensusTransactionKey::External(key);
        Self {
            checkpoint: key
                .user_transaction_digest()
                .map(|digest| epoch_store.register_executed_in_checkpoint_notify(&digest)),
            consensus: epoch_store.register_consensus_message_processed_notify(&key),
            observed: None,
        }
    }

    /// Return the path through which the key was observed as processed, if any.
    fn check_processed(&mut self) -> Option<ProcessedMethod> {
        if self.observed.is_none() {
            if self.consensus.try_recv().is_ok() {
                self.observed = Some(ProcessedMethod::ConsensusMessageProcessed);
            } else if let Some(checkpoint) = &mut self.checkpoint
                && checkpoint.try_recv().is_ok()
            {
                self.observed = Some(ProcessedMethod::CheckpointExecuted);
            }
        }
        self.observed
    }
}

/// One queued submission. Multiple transactions form a soft bundle, which is
/// included in a block atomically or not at all.
struct PoolEntry {
    transactions: Vec<Transaction>,
    total_bytes: usize,
    gas_price: u64,
    tx_type: &'static str, // tx label for metrics
    ack: PendingAck,
    metrics: Arc<AdmissionQueueMetrics>,
    /// Empty for system and ping submissions: the `ConsensusAdapter` already
    /// checks those before they reach the pool.
    processed: Vec<ProcessedWatch>,
}

/// `Some` once every watch has observed processing. Bundles are proposed
/// atomically, so a partially processed one is still proposed in full.
fn all_processed(watches: &mut [ProcessedWatch]) -> Option<ProcessedMethod> {
    let mut result: Option<ProcessedMethod> = None;
    for watch in watches {
        let observed = watch.check_processed()?;
        result = result.max(Some(observed));
    }
    result
}

impl AdmissionQueueEntry for PoolEntry {
    fn gas_price(&self) -> u64 {
        self.gas_price
    }

    fn transaction_keys(&self) -> impl Iterator<Item = ConsensusTransactionKey> {
        self.ack.keys.iter().cloned()
    }

    fn notify_evicted(self, min_gas_price: u64) {
        self.metrics.pool_depth.with_label_values(&["user"]).dec();
        self.metrics
            .pool_bytes
            .with_label_values(&["user"])
            .sub(self.total_bytes as i64);
        self.ack.resolve_error(
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price }
                .into(),
        );
    }

    fn notify_rejected(self, min_gas_price: u64) {
        self.ack.resolve_error(
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price }
                .into(),
        );
    }
}

/// Tracks the user lane's state, which is closed when user certs are no longer accepted.
enum UserLane {
    Open(PriorityAdmissionQueue<PoolEntry>),
    Closed,
}

impl UserLane {
    #[must_use]
    fn close(&mut self) -> Vec<PoolEntry> {
        match std::mem::replace(self, Self::Closed) {
            Self::Open(queue) => queue.into_entries(),
            Self::Closed => Vec::new(),
        }
    }
}

struct Pool {
    user: UserLane,
    // Unbounded: system transactions are correctness-critical, produced by trusted
    // internal components at bounded rates, and must never be evicted or outbid.
    system: VecDeque<PoolEntry>,
    pings: VecDeque<PendingAck>,
    // This validator's proposed blocks, resolved by `notify_committed` once each
    // commits or is garbage collected.
    blocks: BTreeMap<BlockRef, ProposedBlock>,
}

#[derive(Default)]
struct ProposedBlock {
    /// `ConsensusAdapter`-path acks awaiting the block's status.
    subscribers: Vec<oneshot::Sender<BlockStatus>>,
    /// Every entry in the block, for reporting metrics.
    entries: Vec<ProposedEntry>,
}

struct ProposedEntry {
    lane: TakenLane,
    tx_type: &'static str,
    created: Instant,
}

enum Inner {
    Open(Pool),
    Closed,
}

/// A single epoch's transaction pool, drained by the consensus proposer through the
/// `consensus_core::TransactionPool` impl below. `take()` drains pings (zero block
/// space), then the system lane FIFO, then the user lane in gas-price order —
/// user transactions never delay system ones.
///
/// Backpressure for user RPCs is provided solely by the capacity limit of the
/// `user` priority queue.
pub struct ConsensusTransactionPool {
    epoch_store: Arc<AuthorityPerEpochStore>,
    metrics: Arc<AdmissionQueueMetrics>,
    adapter_metrics: ConsensusAdapterMetrics,
    inner: Arc<Mutex<Inner>>,
}

impl ConsensusTransactionPool {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        max_pending_transactions: usize,
        metrics: Arc<AdmissionQueueMetrics>,
        adapter_metrics: ConsensusAdapterMetrics,
    ) -> Self {
        assert!(
            max_pending_transactions > 0,
            "consensus transaction pool max_pending_transactions must be > 0"
        );
        assert!(
            epoch_store
                .protocol_config()
                .max_num_transactions_in_block()
                .saturating_sub(1)
                < u64::from(TransactionIndex::MAX.saturating_sub(NUM_RESERVED_TRANSACTION_INDICES)),
            "Unsupported max_num_transactions_in_block: {}",
            epoch_store
                .protocol_config()
                .max_num_transactions_in_block()
        );

        Self {
            epoch_store,
            metrics: metrics.clone(),
            adapter_metrics,
            inner: Arc::new(Mutex::new(Inner::Open(Pool {
                user: UserLane::Open(PriorityAdmissionQueue::new(
                    max_pending_transactions,
                    metrics,
                )),
                system: VecDeque::new(),
                pings: VecDeque::new(),
                blocks: BTreeMap::new(),
            }))),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(
        epoch_store: Arc<AuthorityPerEpochStore>,
        max_pending_transactions: usize,
        metrics: Arc<AdmissionQueueMetrics>,
    ) -> Self {
        Self::new(
            epoch_store,
            max_pending_transactions,
            metrics,
            ConsensusAdapterMetrics::new_test(),
        )
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch_store.epoch()
    }

    /// Inserts user transactions to the pool. Returns the receiver for their
    /// eventual positions, and bool indicating whether all transactions in the
    /// group were newly inserted.
    /// Fails fast on epoch mismatch, a closed pool/user lane, block-limit
    /// violations, or when outbid by the eviction policy.
    /// `gas_price` is the entry's priority in the user lane (proposed first, and
    /// outbids lower prices when the lane is full).
    pub fn try_insert(
        &self,
        caller_epoch: EpochId,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
    ) -> SuiResult<(PositionReceiver, bool)> {
        // Check whether the transactions were already processed by consensus or
        // checkpoint execution.
        let keys = transactions
            .iter()
            .map(ConsensusTransaction::key)
            .collect::<Vec<_>>();
        let mut processed = keys
            .iter()
            .map(|key| ProcessedWatch::register(&self.epoch_store, key.clone()))
            .collect::<Vec<_>>();
        if let Some(method) = self.check_already_processed(&mut processed)? {
            self.metrics
                .pool_already_processed
                .with_label_values(&["insert", method.metric_label()])
                .inc();
            return Err(processing_error(
                processed.iter().map(|watch| watch.consensus.key()),
                method,
            ));
        }

        let tx_type = tx_type_label(&transactions);
        let (transactions, total_bytes) = self.serialize_and_validate(&transactions)?;
        let (sender, receiver) = oneshot::channel();
        let entry = PoolEntry {
            transactions,
            total_bytes,
            gas_price,
            tx_type,
            ack: PendingAck::new(EntryAck::User(sender), keys),
            metrics: self.metrics.clone(),
            processed,
        };

        let mut inner = self.inner.lock();
        let user = match self.check_user_lane_open_locked(caller_epoch, &mut inner) {
            Ok(user) => user,
            Err(error) => {
                drop(inner);
                entry.ack.resolve_error(error.clone());
                return Err(error);
            }
        };
        let outcome = user.try_insert(entry);
        drop(inner);

        let newly_inserted = outcome.notify()?;
        self.metrics.pool_depth.with_label_values(&["user"]).inc();
        self.metrics
            .pool_bytes
            .with_label_values(&["user"])
            .add(total_bytes as i64);
        Ok((receiver, newly_inserted))
    }

    /// Queues a submission from the `ConsensusAdapter` path. An empty slice is a
    /// ping: it consumes no block space and is acked with `PING_TRANSACTION_INDEX`
    /// on the next proposal.
    fn submit(
        &self,
        caller_epoch: EpochId,
        transactions: &[ConsensusTransaction],
    ) -> SuiResult<oneshot::Receiver<BlockInclusion>> {
        let (sender, receiver) = oneshot::channel();
        if transactions.is_empty() {
            let mut inner = self.inner.lock();
            let pool = self.check_epoch_and_open_locked(caller_epoch, &mut inner)?;
            if pool.pings.len() >= MAX_PENDING_PINGS {
                return Err(SuiErrorKind::TooManyTransactionsPendingConsensus.into());
            }
            pool.pings
                .push_back(PendingAck::new(EntryAck::SystemOrPing(sender), Vec::new()));
            self.metrics.pool_depth.with_label_values(&["ping"]).inc();
            return Ok(receiver);
        }

        // User transactions must enter the pool through `try_insert`.
        if transactions
            .iter()
            .any(ConsensusTransaction::is_user_transaction)
        {
            debug_fatal!(
                "user transaction submitted through the consensus transaction pool client"
            );
            return Err(SuiErrorKind::GenericAuthorityError {
                error: "user transactions cannot be submitted through the transaction pool client"
                    .to_string(),
            }
            .into());
        }
        // Size limits cannot be relaxed for system transactions: peers enforce the
        // same limits on every transaction in a received block, and an over-budget
        // entry at the head of the FIFO system lane would wedge the lane.
        let (serialized, total_bytes) = match self.serialize_and_validate(transactions) {
            Ok(validated) => validated,
            Err(error) => {
                debug_fatal!("system transaction failed validation: {error}");
                return Err(error);
            }
        };
        let entry = PoolEntry {
            transactions: serialized,
            total_bytes,
            gas_price: 0,
            tx_type: tx_type_label(transactions),
            ack: PendingAck::new(
                EntryAck::SystemOrPing(sender),
                transactions.iter().map(ConsensusTransaction::key).collect(),
            ),
            metrics: self.metrics.clone(),
            processed: Vec::new(),
        };

        {
            let mut inner = self.inner.lock();
            let pool = match self.check_epoch_and_open_locked(caller_epoch, &mut inner) {
                Ok(pool) => pool,
                Err(error) => {
                    drop(inner);
                    entry.ack.resolve_error(error.clone());
                    return Err(error);
                }
            };
            pool.system.push_back(entry);
            let depth = pool.system.len();
            self.metrics
                .pool_depth
                .with_label_values(&["system"])
                .set(depth as i64);
            self.metrics
                .pool_bytes
                .with_label_values(&["system"])
                .add(total_bytes as i64);
            if depth == SYSTEM_LANE_WARN_THRESHOLD {
                warn!(
                    depth,
                    "Consensus transaction pool system lane exceeded warning threshold"
                );
            }
        }
        Ok(receiver)
    }

    fn check_already_processed(
        &self,
        watches: &mut [ProcessedWatch],
    ) -> SuiResult<Option<ProcessedMethod>> {
        // Check for tx already processed by consensus.
        let consensus_processed = self.epoch_store.check_consensus_messages_processed(
            watches.iter().map(|watch| watch.consensus.key().clone()),
        )?;
        for (watch, consensus_processed) in watches.iter_mut().zip_eq(consensus_processed) {
            if consensus_processed {
                watch.observed = Some(ProcessedMethod::ConsensusMessageProcessed);
            }
        }

        // Check for tx processed by checkpoint execution.
        let (unobserved, digests): (Vec<_>, Vec<_>) = watches
            .iter_mut()
            .filter_map(|watch| {
                if watch.observed.is_some() {
                    return None;
                }
                let digest = *watch.checkpoint.as_ref()?.key();
                Some((watch, digest))
            })
            .unzip();
        if !digests.is_empty() {
            let executed = self
                .epoch_store
                .multi_get_transaction_checkpoint(&digests)?;
            for (watch, executed) in unobserved.into_iter().zip_eq(executed) {
                if executed.is_some() {
                    watch.observed = Some(ProcessedMethod::CheckpointExecuted);
                }
            }
        }
        Ok(all_processed(watches))
    }

    fn check_epoch_and_open_locked<'a>(
        &self,
        caller_epoch: EpochId,
        inner: &'a mut Inner,
    ) -> SuiResult<&'a mut Pool> {
        if caller_epoch != self.epoch() {
            return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
        match inner {
            Inner::Open(pool) => Ok(pool),
            Inner::Closed => Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into()),
        }
    }

    fn check_user_lane_open_locked<'a>(
        &self,
        caller_epoch: EpochId,
        inner: &'a mut Inner,
    ) -> SuiResult<&'a mut PriorityAdmissionQueue<PoolEntry>> {
        let pool = self.check_epoch_and_open_locked(caller_epoch, inner)?;
        match &mut pool.user {
            UserLane::Open(user) => Ok(user),
            UserLane::Closed => Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into()),
        }
    }

    /// Enforces per-transaction and per-block size limits at insert time, which
    /// guarantees every admitted entry fits an empty proposal — `take()` can
    /// therefore never stall on an unfittable head-of-queue entry.
    ///
    /// Mirrors the checks in `consensus_core::TransactionClient::submit`, which
    /// pool submissions bypass (the proposer drains the pool directly).
    fn serialize_and_validate(
        &self,
        transactions: &[ConsensusTransaction],
    ) -> SuiResult<(Vec<Transaction>, usize)> {
        let protocol_config = self.epoch_store.protocol_config();
        let bundle_count = u64::try_from(transactions.len()).expect("bundle count fits into u64");
        if bundle_count > protocol_config.max_num_transactions_in_block() {
            return Err(consensus_client_error(
                ClientError::OversizedTransactionBundleCount(
                    bundle_count,
                    protocol_config.max_num_transactions_in_block(),
                ),
            ));
        }

        let mut total_bytes = 0usize;
        let mut serialized = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            let bytes =
                bcs::to_bytes(transaction).expect("Serializing consensus transaction cannot fail");
            let transaction_bytes =
                u64::try_from(bytes.len()).expect("transaction size fits into u64");
            if transaction_bytes > protocol_config.max_transaction_size_bytes() {
                return Err(consensus_client_error(ClientError::OversizedTransaction(
                    transaction_bytes,
                    protocol_config.max_transaction_size_bytes(),
                )));
            }
            total_bytes += bytes.len();
            let bundle_bytes = u64::try_from(total_bytes).expect("bundle size fits into u64");
            if bundle_bytes > protocol_config.max_transactions_in_block_bytes() {
                return Err(consensus_client_error(
                    ClientError::OversizedTransactionBundleBytes(
                        bundle_bytes,
                        protocol_config.max_transactions_in_block_bytes(),
                    ),
                ));
            }
            serialized.push(Transaction::new(bytes));
        }
        Ok((serialized, total_bytes))
    }

    /// Permanently shuts the pool down, called before stopping the consensus
    /// authority. Pending user waiters are resolved with `ValidatorHaltedAtEpochEnd`
    /// so the RPC retry loop resubmits them in the next epoch; system and ping acks
    /// are dropped, which their submitters treat as consensus shutdown. Idempotent.
    pub fn close(&self) {
        let mut pool = {
            let mut inner = self.inner.lock();
            match std::mem::replace(&mut *inner, Inner::Closed) {
                Inner::Open(pool) => pool,
                Inner::Closed => return,
            }
        };

        // The teardown below runs on the extracted pool with the mutex released.
        self.resolve_flushed_user_entries(pool.user.close());
        for entry in pool.system {
            entry.ack.drop_deliberately("pool closed");
        }
        for ping in pool.pings {
            ping.drop_deliberately("pool closed");
        }
        drop(pool.blocks);
        self.metrics
            .pool_depth
            .with_label_values(&["system"])
            .set(0);
        self.metrics
            .pool_bytes
            .with_label_values(&["system"])
            .set(0);
        self.metrics.pool_depth.with_label_values(&["ping"]).set(0);
    }

    /// Resolves user entries flushed by `UserLane::close` with the retriable halted
    /// error, so they retry in the next epoch. Dropping the entries deregisters
    /// their processed watches — call with the pool mutex released.
    fn resolve_flushed_user_entries(&self, entries: Vec<PoolEntry>) {
        for entry in entries {
            self.decrement_lane_metrics("user", &entry);
            entry
                .ack
                .resolve_error(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
    }

    fn decrement_lane_metrics(&self, lane: &str, entry: &PoolEntry) {
        self.metrics.pool_depth.with_label_values(&[lane]).dec();
        self.metrics
            .pool_bytes
            .with_label_values(&[lane])
            .sub(entry.total_bytes as i64);
    }

    #[cfg(test)]
    pub fn queue_depth(&self, lane: &str) -> i64 {
        self.metrics.pool_depth.with_label_values(&[lane]).get()
    }

    #[cfg(test)]
    pub fn already_processed_count(&self, stage: &str, method: &str) -> u64 {
        self.metrics
            .pool_already_processed
            .with_label_values(&[stage, method])
            .get()
    }

    #[cfg(test)]
    pub fn abandoned_count(&self, lane: &str) -> u64 {
        self.metrics.pool_abandoned.with_label_values(&[lane]).get()
    }
}

#[derive(Clone, Copy)]
enum TakenLane {
    System,
    User,
}

impl TakenLane {
    fn label(self) -> &'static str {
        match self {
            TakenLane::System => "system",
            TakenLane::User => "user",
        }
    }
}

struct TakenEntry {
    lane: TakenLane,
    entry: PoolEntry,
}

/// Holds the entries handed out by one `take()` until their fate is known:
/// `acknowledge` (the proposer created a block) resolves every waiter with its
/// position/inclusion ack, while dropping the guard uninvoked (failed proposal or
/// shutdown) returns the entries to the front of their lanes so nothing is lost or
/// reordered. All paths serialize on the pool mutex, so they compose correctly
/// with `close()` running in between.
struct TakenTransactionsGuard {
    epoch: EpochId,
    inner: Arc<Mutex<Inner>>,
    metrics: Arc<AdmissionQueueMetrics>,
    entries: Option<Vec<TakenEntry>>,
    pings: Option<Vec<PendingAck>>,
}

impl TakenTransactionsGuard {
    fn acknowledge(mut self, block_ref: BlockRef) {
        let entries = self.entries.take().expect("acknowledgement called once");
        let pings = self.pings.take().expect("acknowledgement called once");
        let mut inner = self.inner.lock();
        let Inner::Open(pool) = &mut *inner else {
            drop(inner);
            Self::resolve_after_close(entries, pings, "pool closed before acknowledgement");
            return;
        };

        // Transactions occupy the block in exactly the order take() returned them,
        // so indices are assigned contiguously across entries in that order.
        let mut next_index = 0usize;
        let mut watches_to_drop = Vec::with_capacity(entries.len());
        let mut block = ProposedBlock::default();
        for taken in entries {
            let lane = taken.lane;
            let mut entry = taken.entry;
            block.entries.push(ProposedEntry {
                lane,
                tx_type: entry.tx_type,
                created: entry.ack.created,
            });
            watches_to_drop.push(std::mem::take(&mut entry.processed));
            let start = next_index;
            next_index += entry.transactions.len();
            let indices = (start..next_index)
                .map(|index| {
                    TransactionIndex::try_from(index)
                        .expect("validated transaction index must fit TransactionIndex")
                })
                .collect::<Vec<_>>();
            self.metrics
                .queue_wait_latency
                .with_label_values(&[lane.label()])
                .observe(entry.ack.created.elapsed().as_secs_f64());
            entry
                .ack
                .resolve_included(self.epoch, block_ref, indices, &mut block);
        }
        for ping in pings {
            self.metrics
                .queue_wait_latency
                .with_label_values(&["ping"])
                .observe(ping.created.elapsed().as_secs_f64());
            ping.resolve_included(
                self.epoch,
                block_ref,
                vec![PING_TRANSACTION_INDEX],
                &mut block,
            );
        }
        if !block.subscribers.is_empty() || !block.entries.is_empty() {
            pool.blocks.insert(block_ref, block);
        }
        drop(inner);
        drop(watches_to_drop); // drop after mutex release
    }

    // The pool closed while these entries were out with the proposer: resolve user
    // waiters with the retriable halted error and drop system/ping acks, exactly as
    // close() did for the entries still queued.
    fn resolve_after_close(entries: Vec<TakenEntry>, pings: Vec<PendingAck>, reason: &'static str) {
        for taken in entries {
            taken
                .entry
                .ack
                .resolve_error(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
        for ping in pings {
            ping.drop_deliberately(reason);
        }
    }
}

impl Drop for TakenTransactionsGuard {
    fn drop(&mut self) {
        let Some(entries) = self.entries.take() else {
            return;
        };
        let pings = self.pings.take().unwrap_or_default();
        let mut inner = self.inner.lock();
        let Inner::Open(pool) = &mut *inner else {
            drop(inner);
            Self::resolve_after_close(entries, pings, "pool closed before dropped acknowledgement");
            return;
        };

        let mut requeued = 0;
        let mut halted = Vec::new();
        // Capacity is bypassed because these entries were already admitted. Concurrent
        // inserts can put us over capacity, which we accept transiently.
        for taken in entries.into_iter().rev() {
            match taken.lane {
                TakenLane::System => {
                    requeued += 1;
                    self.metrics.pool_depth.with_label_values(&["system"]).inc();
                    self.metrics
                        .pool_bytes
                        .with_label_values(&["system"])
                        .add(taken.entry.total_bytes as i64);
                    pool.system.push_front(taken.entry);
                    let depth = pool.system.len();
                    if depth == SYSTEM_LANE_WARN_THRESHOLD {
                        warn!(
                            depth,
                            "Consensus transaction pool system lane exceeded warning threshold"
                        );
                    }
                }
                TakenLane::User => match &mut pool.user {
                    UserLane::Open(user) => {
                        requeued += 1;
                        self.metrics.pool_depth.with_label_values(&["user"]).inc();
                        self.metrics
                            .pool_bytes
                            .with_label_values(&["user"])
                            .add(taken.entry.total_bytes as i64);
                        user.reinsert_front(taken.entry);
                    }
                    UserLane::Closed => halted.push(taken.entry),
                },
            }
        }
        for ping in pings.into_iter().rev() {
            pool.pings.push_front(ping);
            self.metrics.pool_depth.with_label_values(&["ping"]).inc();
        }
        self.metrics
            .pool_requeued_on_dropped_ack
            .inc_by(requeued as u64);
        drop(inner);

        for entry in halted {
            entry
                .ack
                .resolve_error(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        }
    }
}

impl TransactionPool for ConsensusTransactionPool {
    fn take(
        &self,
        max_count: usize,
        max_bytes: usize,
    ) -> (
        Vec<Transaction>,
        Box<dyn FnOnce(BlockRef) + Send>,
        LimitReached,
    ) {
        // This acquires (and immediately releases) the epoch store's reconfiguration
        // read lock. It must happen before the pool mutex is taken — never read the
        // reconfig state while holding the pool mutex.
        let should_accept_user_certs = self
            .epoch_store
            .get_reconfig_state_read_lock_guard()
            .should_accept_user_certs();
        #[allow(unused_mut)]
        let mut disabled = false;
        fail_point_if!("consensus_transaction_pool_disable_take", || {
            disabled = true;
        });
        #[allow(unused_mut)]
        let mut user_take_disabled = false;
        fail_point_if!("consensus_transaction_pool_disable_user_take", || {
            user_take_disabled = true;
        });

        let mut inner = self.inner.lock();
        let pool = match &mut *inner {
            Inner::Open(pool) => pool,
            Inner::Closed => {
                return (
                    Vec::new(),
                    Box::new(|_| {}),
                    LimitReached::AllTransactionsIncluded,
                );
            }
        };
        let flushed = if !should_accept_user_certs {
            pool.user.close()
        } else {
            Vec::new()
        };
        if disabled {
            drop(inner);
            self.resolve_flushed_user_entries(flushed); // resolve after mutex release
            return (
                Vec::new(),
                Box::new(|_| {}),
                LimitReached::AllTransactionsIncluded,
            );
        }

        let (abandoned_pings, pings): (Vec<_>, Vec<_>) = pool
            .pings
            .drain(..)
            .partition(PendingAck::is_abandoned_system);
        self.metrics
            .pool_depth
            .with_label_values(&["ping"])
            .sub((pings.len() + abandoned_pings.len()) as i64);

        let mut transactions = Vec::new();
        let mut entries = Vec::new();
        let mut total_bytes = 0usize;
        let mut limit_reached = LimitReached::AllTransactionsIncluded;

        let mut abandoned = Vec::new();
        while let Some(entry) = pool.system.front() {
            // Skip entries whose submitter stopped waiting (e.g. a checkpoint
            // signature whose checkpoint was already synced).
            if entry.ack.is_abandoned_system() {
                let entry = pool.system.pop_front().expect("front entry must exist");
                self.decrement_lane_metrics("system", &entry);
                abandoned.push(entry);
                continue;
            }
            if let Some(limit) =
                entry_limit(entry, transactions.len(), total_bytes, max_count, max_bytes)
            {
                limit_reached = limit;
                break;
            }
            let entry = pool.system.pop_front().expect("front entry must exist");
            total_bytes += entry.total_bytes;
            transactions.extend(entry.transactions.iter().cloned());
            self.decrement_lane_metrics("system", &entry);
            entries.push(TakenEntry {
                lane: TakenLane::System,
                entry,
            });
        }

        let mut already_processed = Vec::new();
        if !user_take_disabled
            && matches!(limit_reached, LimitReached::AllTransactionsIncluded)
            && let UserLane::Open(user) = &mut pool.user
        {
            let mut pending_count = transactions.len();
            let mut pending_bytes = total_bytes;
            let popped;
            (popped, already_processed) = user.pop_batch_while(|entry| {
                // An already-processed entry is excluded without consuming block budget.
                if all_processed(&mut entry.processed).is_some() {
                    return PopAction::Exclude;
                }
                match entry_limit(entry, pending_count, pending_bytes, max_count, max_bytes) {
                    Some(limit) => {
                        limit_reached = limit;
                        PopAction::Stop
                    }
                    None => {
                        pending_count += entry.transactions.len();
                        pending_bytes += entry.total_bytes;
                        PopAction::Include
                    }
                }
            });
            for entry in popped {
                self.decrement_lane_metrics("user", &entry);
                transactions.extend(entry.transactions.iter().cloned());
                entries.push(TakenEntry {
                    lane: TakenLane::User,
                    entry,
                });
            }
        }
        drop(inner);

        // Resolve flushed/processed items outside the mutex.
        self.resolve_flushed_user_entries(flushed);
        for entry in abandoned {
            self.metrics
                .pool_abandoned
                .with_label_values(&["system"])
                .inc();
            entry
                .ack
                .drop_deliberately("submitter stopped waiting before proposal");
        }
        for ping in abandoned_pings {
            self.metrics
                .pool_abandoned
                .with_label_values(&["ping"])
                .inc();
            ping.drop_deliberately("submitter stopped waiting before proposal");
        }
        for mut entry in already_processed {
            self.decrement_lane_metrics("user", &entry);
            let method = all_processed(&mut entry.processed)
                .expect("excluded entries are already processed");
            self.metrics
                .pool_already_processed
                .with_label_values(&["proposal", method.metric_label()])
                .inc();
            let error = processing_error(
                entry.processed.iter().map(|watch| watch.consensus.key()),
                method,
            );
            entry.ack.resolve_error(error);
        }

        self.metrics
            .pool_taken_per_proposal
            .observe(transactions.len() as f64);
        let guard = TakenTransactionsGuard {
            epoch: self.epoch(),
            inner: self.inner.clone(),
            metrics: self.metrics.clone(),
            entries: Some(entries),
            pings: Some(pings),
        };
        (
            transactions,
            Box::new(move |block_ref| guard.acknowledge(block_ref)),
            limit_reached,
        )
    }

    // Same semantics as consensus-core's TransactionConsumer::notify_own_blocks_status:
    // committed blocks resolve Sequenced first, then every remaining subscription at
    // round <= gc_round resolves GarbageCollected (those blocks can never commit).
    fn notify_committed(&self, own_committed_blocks: Vec<BlockRef>, gc_round: Round) {
        let mut sequenced = Vec::new();
        let mut garbage_collected = Vec::new();
        {
            let mut inner = self.inner.lock();
            let Inner::Open(pool) = &mut *inner else {
                return;
            };
            for block_ref in own_committed_blocks {
                if let Some(block) = pool.blocks.remove(&block_ref) {
                    for subscriber in block.subscribers {
                        let _ = subscriber.send(BlockStatus::Sequenced(block_ref));
                    }
                    sequenced.extend(block.entries);
                }
            }
            while let Some((block_ref, block)) = pool.blocks.pop_first() {
                if block_ref.round > gc_round {
                    pool.blocks.insert(block_ref, block);
                    break;
                }
                self.metrics
                    .pool_gc_notified
                    .inc_by(block.subscribers.len() as u64);
                for subscriber in block.subscribers {
                    let _ = subscriber.send(BlockStatus::GarbageCollected(block_ref));
                }
                garbage_collected.extend(block.entries);
            }
        }

        // Metrics are updated with the mutex released.
        self.report_proposed_status(&sequenced, "sequenced");
        self.report_proposed_status(&garbage_collected, "garbage_collected");
        self.report_commit_latency(&sequenced);
    }
}

impl ConsensusTransactionPool {
    /// Reports user entries to `sequencing_certificate_status*` metrics.
    /// System entries are reported by the adapter itself, which waits on their block
    /// status.
    fn report_proposed_status(&self, entries: &[ProposedEntry], status: &'static str) {
        let mut counts: Vec<(&'static str, u64)> = Vec::new();
        for entry in entries
            .iter()
            .filter(|entry| matches!(entry.lane, TakenLane::User))
        {
            match counts
                .iter_mut()
                .find(|(tx_type, _)| *tx_type == entry.tx_type)
            {
                Some((_, count)) => *count += 1,
                None => counts.push((entry.tx_type, 1)),
            }
        }
        for (tx_type, count) in counts {
            self.adapter_metrics
                .sequencing_certificate_status
                .with_label_values(&[tx_type, status])
                .inc_by(count);
        }
    }

    fn report_commit_latency(&self, entries: &[ProposedEntry]) {
        let now = Instant::now();
        let user = self
            .metrics
            .pool_commit_latency
            .with_label_values(&["user"]);
        let system = self
            .metrics
            .pool_commit_latency
            .with_label_values(&["system"]);
        for entry in entries {
            let histogram = match entry.lane {
                TakenLane::User => &user,
                TakenLane::System => &system,
            };
            histogram.observe(now.saturating_duration_since(entry.created).as_secs_f64());
        }
    }
}

/// Which block limit the entry would exceed, if any. Entries are all-or-nothing:
/// a bundle that does not fit stays queued in full for the next proposal.
fn entry_limit(
    entry: &PoolEntry,
    current_count: usize,
    current_bytes: usize,
    max_count: usize,
    max_bytes: usize,
) -> Option<LimitReached> {
    if current_bytes.saturating_add(entry.total_bytes) > max_bytes {
        return Some(LimitReached::MaxBytes);
    }
    if current_count.saturating_add(entry.transactions.len()) > max_count {
        return Some(LimitReached::MaxNumOfTransactions);
    }
    None
}

fn consensus_client_error(error: ClientError) -> SuiError {
    SuiErrorKind::FailedToSubmitToConsensus(error.to_string()).into()
}

#[derive(Clone)]
enum PoolState {
    /// Before the first consensus start.
    Absent,
    Active(EpochId, Arc<ConsensusTransactionPool>),
    /// No pool will be installed for this epoch (the node is not a validator, or is
    /// shutting down); waiters must fail rather than wait for it.
    Unavailable(EpochId),
}

/// Process-lifetime handle connecting submitters to the current epoch's pool.
/// The watch channel is the single authoritative state, always updated with
/// `send_replace` so an installation is never lost while no receiver is subscribed.
///
/// This closes the reconfiguration window in which the new epoch store is already
/// published but the new epoch's consensus (and pool) has not started yet: callers
/// wait on the watch instead of failing or, worse, inserting into the previous
/// epoch's still-installed pool.
pub struct TransactionPoolContext {
    state: watch::Sender<PoolState>,
    metrics: Arc<AdmissionQueueMetrics>,
    adapter_metrics: ConsensusAdapterMetrics,
}

impl TransactionPoolContext {
    pub fn new(
        metrics: Arc<AdmissionQueueMetrics>,
        adapter_metrics: ConsensusAdapterMetrics,
    ) -> Self {
        let (state, _) = watch::channel(PoolState::Absent);
        Self {
            state,
            metrics,
            adapter_metrics,
        }
    }

    pub fn set_active(&self, epoch: EpochId, pool: Arc<ConsensusTransactionPool>) {
        assert_eq!(epoch, pool.epoch());
        drop(self.state.send_replace(PoolState::Active(epoch, pool)));
    }

    /// Marks `epoch` as one that will never get a pool, promptly failing current and
    /// future waiters that would otherwise hang until their transport deadline.
    pub fn set_unavailable(&self, epoch: EpochId) {
        drop(self.state.send_replace(PoolState::Unavailable(epoch)));
    }

    pub async fn try_insert(
        &self,
        epoch: EpochId,
        gas_price: u64,
        transactions: Vec<ConsensusTransaction>,
    ) -> SuiResult<(PositionReceiver, bool)> {
        self.wait_for_pool(epoch)
            .await?
            .try_insert(epoch, gas_price, transactions)
    }

    /// Resolves the pool for `caller_epoch`. Waits while the state is absent or older
    /// than the caller — e.g. during the gap between epoch-store publication and consensus
    /// start. Fails immediately with `ValidatorHaltedAtEpochEnd` when the state is *newer*
    /// (the caller raced with a stale epoch store and should re-validate), and with a
    /// retriable overload error when this epoch is `Unavailable` (client retries
    /// against another validator). Bounded only by the caller's own deadline or
    /// cancellation.
    pub async fn wait_for_pool(
        &self,
        caller_epoch: EpochId,
    ) -> SuiResult<Arc<ConsensusTransactionPool>> {
        self.wait_for_pool_after_subscribe(caller_epoch, || {})
            .await
    }

    async fn wait_for_pool_after_subscribe(
        &self,
        caller_epoch: EpochId,
        after_subscribe: impl FnOnce(),
    ) -> SuiResult<Arc<ConsensusTransactionPool>> {
        let mut receiver = self.state.subscribe();
        after_subscribe();
        let start = Instant::now();
        loop {
            let state = receiver.borrow_and_update().clone();
            match state {
                PoolState::Active(epoch, pool) if epoch == caller_epoch => {
                    return Ok(pool);
                }
                PoolState::Active(epoch, _) if epoch > caller_epoch => {
                    return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
                }
                PoolState::Unavailable(epoch) if epoch > caller_epoch => {
                    return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
                }
                PoolState::Unavailable(epoch) if epoch == caller_epoch => {
                    return Err(SuiErrorKind::TooManyTransactionsPendingConsensus.into());
                }
                PoolState::Absent | PoolState::Active(_, _) | PoolState::Unavailable(_) => {}
            }

            let _waiting = WaitingInsertGuard::new(self.metrics.pool_waiting_inserts.clone());
            match tokio::time::timeout(Duration::from_secs(10), receiver.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    // Unreachable: `changed()` errors only when the watch sender is dropped,
                    // but the sender is a field of this context and `&self` is borrowed
                    // across this await. All real teardown paths publish a state change
                    // (`set_active` / `set_unavailable`) instead of dropping the sender.
                    debug_fatal!("transaction pool watch sender dropped while context is alive");
                    return Err(SuiErrorKind::TooManyTransactionsPendingConsensus.into());
                }
                Err(_) => {
                    warn!(
                        caller_epoch,
                        elapsed = ?start.elapsed(),
                        "Waiting for consensus transaction pool to initialize"
                    );
                }
            }
        }
    }

    pub fn metrics(&self) -> &Arc<AdmissionQueueMetrics> {
        &self.metrics
    }

    pub fn adapter_metrics(&self) -> &ConsensusAdapterMetrics {
        &self.adapter_metrics
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(metrics: Arc<AdmissionQueueMetrics>) -> Self {
        Self::new(metrics, ConsensusAdapterMetrics::new_test())
    }
}

struct WaitingInsertGuard {
    gauge: IntGauge,
}

impl WaitingInsertGuard {
    fn new(gauge: IntGauge) -> Self {
        gauge.inc();
        Self { gauge }
    }
}

impl Drop for WaitingInsertGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

/// The `ConsensusClient` installed in pull mode in place of `LazyMysticetiClient`,
/// so the unchanged `ConsensusAdapter` (system transactions, pings, GC retries)
/// submits into the pool. Wraps the context rather than one pool, resolving the
/// right epoch's pool per submission.
pub struct TransactionPoolClient {
    context: Arc<TransactionPoolContext>,
}

impl TransactionPoolClient {
    pub fn new(context: Arc<TransactionPoolContext>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl ConsensusClient for TransactionPoolClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
        let epoch = epoch_store.epoch();
        let pool = self.context.wait_for_pool(epoch).await?;
        let receiver = pool.submit(epoch, transactions)?;
        let (block_ref, indices, status_receiver) = receiver.await.map_err(|error| {
            consensus_client_error(ClientError::ConsensusShuttingDown(error.to_string()))
        })?;
        let positions = indices
            .into_iter()
            .map(|index| ConsensusPosition {
                epoch,
                block: block_ref,
                index,
            })
            .collect();
        Ok((positions, status_receiver))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::AuthorityState;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use consensus_config::AuthorityIndex;
    use consensus_types::block::BlockDigest;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{AuthorityName, random_object_ref};
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::transaction::PlainTransactionWithClaims;

    async fn test_state_and_pool(
        capacity: usize,
    ) -> (Arc<AuthorityState>, Arc<ConsensusTransactionPool>) {
        let state = TestAuthorityBuilder::new().build().await;
        let pool = pool_for_current_epoch(&state, capacity);
        (state, pool)
    }

    async fn test_state_and_pool_with_protocol_config(
        capacity: usize,
        protocol_config: ProtocolConfig,
    ) -> (Arc<AuthorityState>, Arc<ConsensusTransactionPool>) {
        let state = TestAuthorityBuilder::new()
            .with_protocol_config(protocol_config)
            .build()
            .await;
        let pool = pool_for_current_epoch(&state, capacity);
        (state, pool)
    }

    fn pool_for_current_epoch(
        state: &Arc<AuthorityState>,
        capacity: usize,
    ) -> Arc<ConsensusTransactionPool> {
        Arc::new(ConsensusTransactionPool::new_for_tests(
            state.epoch_store_for_testing().clone(),
            capacity,
            Arc::new(AdmissionQueueMetrics::new_for_tests()),
        ))
    }

    fn transaction() -> ConsensusTransaction {
        ConsensusTransaction::new_end_of_publish(AuthorityName::ZERO)
    }

    fn user_transaction() -> ConsensusTransaction {
        let (sender, keypair) = get_key_pair::<AccountKeyPair>();
        let transaction = crate::test_utils::make_transfer_sui_transaction(
            random_object_ref(),
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

    fn block(round: Round) -> BlockRef {
        BlockRef::new(round, AuthorityIndex::new_for_test(0), BlockDigest::MIN)
    }

    fn consensus_key(transaction: &ConsensusTransaction) -> SequencedConsensusTransactionKey {
        SequencedConsensusTransactionKey::External(transaction.key())
    }

    fn digest_of(transaction: &ConsensusTransaction) -> TransactionDigest {
        consensus_key(transaction)
            .user_transaction_digest()
            .expect("expected a user transaction key")
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "dropped without resolution")]
    async fn unresolved_pending_ack_panics() {
        let (sender, _receiver) = oneshot::channel();
        drop(PendingAck::new(
            EntryAck::User(sender),
            vec![transaction().key()],
        ));
    }

    #[tokio::test]
    async fn user_lane_close_returns_pending_entries_and_closes_lane() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let mut lane = UserLane::Open(PriorityAdmissionQueue::new(1, metrics.clone()));
        let consensus_transaction = transaction();
        let serialized = bcs::to_bytes(&consensus_transaction).unwrap();
        let (sender, receiver) = oneshot::channel();
        let entry = PoolEntry {
            transactions: vec![Transaction::new(serialized.clone())],
            total_bytes: serialized.len(),
            gas_price: 1,
            tx_type: tx_type_label(std::slice::from_ref(&consensus_transaction)),
            ack: PendingAck::new(EntryAck::User(sender), vec![consensus_transaction.key()]),
            metrics,
            processed: Vec::new(),
        };
        let UserLane::Open(user) = &mut lane else {
            unreachable!("new user lane must be open");
        };
        user.insert(entry).unwrap();

        let mut entries = lane.close();
        assert!(matches!(lane, UserLane::Closed));
        assert_eq!(entries.len(), 1);
        entries
            .pop()
            .unwrap()
            .ack
            .resolve_error(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
        assert!(matches!(
            receiver.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
    }

    #[tokio::test]
    async fn locked_user_recheck_defuses_ack_after_pool_close() {
        let (_state, pool) = test_state_and_pool(1).await;
        pool.close();
        // The armed ack constructed inside try_insert must be defused when the locked
        // check fails — a regression detonates the drop bomb and panics this test.
        assert!(matches!(
            pool.try_insert(pool.epoch(), 1, vec![transaction()])
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
    }

    #[tokio::test]
    async fn take_respects_priority_budgets_and_acknowledges_positions() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let system_receiver = pool.submit(epoch, &[transaction()]).unwrap();
        assert_eq!(pool.queue_depth("system"), 1);
        let (low_receiver, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let (high_receiver, _) = pool.try_insert(epoch, 20, vec![transaction()]).unwrap();

        let (transactions, ack, limit) = pool.take(2, usize::MAX);
        assert_eq!(transactions.len(), 2);
        assert_eq!(limit, LimitReached::MaxNumOfTransactions);
        assert_eq!(pool.queue_depth("system"), 0);
        let block_ref = block(5);
        ack(block_ref);

        let (system_block, system_indices, system_status) = system_receiver.await.unwrap();
        assert_eq!(system_block, block_ref);
        assert_eq!(system_indices, vec![0]);
        let positions = high_receiver.await.unwrap().unwrap();
        assert_eq!(
            positions,
            vec![ConsensusPosition {
                epoch,
                block: block_ref,
                index: 1,
            }]
        );
        pool.notify_committed(vec![block_ref], 0);
        assert_eq!(
            system_status.await.unwrap(),
            BlockStatus::Sequenced(block_ref)
        );

        let serialized_len = bcs::to_bytes(&transaction()).unwrap().len();
        let (transactions, ack, limit) = pool.take(1, serialized_len - 1);
        assert!(transactions.is_empty());
        assert_eq!(limit, LimitReached::MaxBytes);
        drop(ack);

        let (transactions, ack, limit) = pool.take(1, serialized_len);
        assert_eq!(transactions.len(), 1);
        assert_eq!(limit, LimitReached::AllTransactionsIncluded);
        drop(ack);
        assert_eq!(pool.queue_depth("user"), 1);

        let (transactions, ack, _) = pool.take(1, serialized_len);
        assert_eq!(transactions.len(), 1);
        ack(block(6));
        assert_eq!(low_receiver.await.unwrap().unwrap()[0].index, 0);
    }

    #[tokio::test]
    async fn bundles_are_atomic() {
        let (_state, pool) = test_state_and_pool(10).await;
        let (_receiver, _) = pool
            .try_insert(pool.epoch(), 10, vec![transaction(), transaction()])
            .unwrap();
        let (transactions, ack, limit) = pool.take(1, usize::MAX);
        assert!(transactions.is_empty());
        assert_eq!(limit, LimitReached::MaxNumOfTransactions);
        drop(ack);

        let bundle_bytes = 2 * bcs::to_bytes(&transaction()).unwrap().len();
        let (transactions, ack, limit) = pool.take(2, bundle_bytes - 1);
        assert!(transactions.is_empty());
        assert_eq!(limit, LimitReached::MaxBytes);
        drop(ack);

        let (transactions, ack, _) = pool.take(2, bundle_bytes);
        assert_eq!(transactions.len(), 2);
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn dropped_ack_preserves_front_order() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let (first, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let (mut second, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let (_, ack, _) = pool.take(2, usize::MAX);
        drop(ack);

        let (_, ack, _) = pool.take(1, usize::MAX);
        ack(block(1));
        assert_eq!(first.await.unwrap().unwrap()[0].index, 0);
        assert!(second.try_recv().is_err());
        pool.close();
    }

    #[tokio::test]
    async fn ping_capacity_and_reserved_index() {
        let (_state, pool) = test_state_and_pool(1).await;
        let epoch = pool.epoch();
        let mut receivers = Vec::with_capacity(MAX_PENDING_PINGS);
        for _ in 0..MAX_PENDING_PINGS {
            receivers.push(pool.submit(epoch, &[]).unwrap());
        }
        assert!(matches!(
            pool.submit(epoch, &[]).err().unwrap().as_inner(),
            SuiErrorKind::TooManyTransactionsPendingConsensus
        ));

        let (transactions, ack, _) = pool.take(0, 0);
        assert!(transactions.is_empty());
        let block_ref = block(1);
        ack(block_ref);
        let (ping_block, indices, status) = receivers.remove(0).await.unwrap();
        assert_eq!(ping_block, block_ref);
        assert_eq!(indices, vec![PING_TRANSACTION_INDEX]);
        pool.notify_committed(vec![block_ref], 0);
        assert_eq!(status.await.unwrap(), BlockStatus::Sequenced(block_ref));
    }

    #[tokio::test]
    async fn system_lane_is_not_bounded_by_user_capacity() {
        let (_state, pool) = test_state_and_pool(1).await;
        let receivers = (0..3)
            .map(|_| pool.submit(pool.epoch(), &[transaction()]).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(pool.queue_depth("system"), 3);
        let (transactions, ack, _) = pool.take(3, usize::MAX);
        assert_eq!(transactions.len(), 3);
        ack(block(1));
        for receiver in receivers {
            assert_eq!(receiver.await.unwrap().1.len(), 1);
        }
    }

    #[tokio::test]
    async fn take_skips_abandoned_system_entries_and_pings() {
        let (_state, pool) = test_state_and_pool(1).await;
        let epoch = pool.epoch();

        // Abandoned entry first, so skipping it must not consume the count budget
        // of take(1, ..) below.
        let stale = pool.submit(epoch, &[transaction()]).unwrap();
        drop(stale);
        let live = pool.submit(epoch, &[transaction()]).unwrap();
        let stale_ping = pool.submit(epoch, &[]).unwrap();
        drop(stale_ping);
        let live_ping = pool.submit(epoch, &[]).unwrap();

        let (transactions, ack, _) = pool.take(1, usize::MAX);
        assert_eq!(transactions.len(), 1);
        assert_eq!(pool.queue_depth("system"), 0);
        assert_eq!(pool.queue_depth("ping"), 0);
        assert_eq!(pool.abandoned_count("system"), 1);
        assert_eq!(pool.abandoned_count("ping"), 1);

        ack(block(1));
        assert_eq!(live.await.unwrap().1.len(), 1);
        assert_eq!(live_ping.await.unwrap().1, vec![PING_TRANSACTION_INDEX]);
    }

    #[tokio::test]
    async fn eviction_rejection_and_duplicate_detection_match_admission_queue() {
        let (_state, pool) = test_state_and_pool(2).await;
        let epoch = pool.epoch();
        let (first, newly_inserted) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        assert!(newly_inserted);
        let (_second, newly_inserted) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        assert!(!newly_inserted);

        let (_high, _) = pool.try_insert(epoch, 20, vec![transaction()]).unwrap();
        assert!(matches!(
            first.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 20 }
        ));
        assert!(matches!(
            pool.try_insert(epoch, 10, vec![transaction()])
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { min_gas_price: 10 }
        ));
        pool.close();
    }

    #[tokio::test]
    async fn close_resolves_pending_and_taken_user_entries() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        for wrong_epoch in [epoch + 1, epoch + 2] {
            assert!(matches!(
                pool.try_insert(wrong_epoch, 1, vec![transaction()])
                    .err()
                    .unwrap()
                    .as_inner(),
                SuiErrorKind::ValidatorHaltedAtEpochEnd
            ));
        }
        let (pending, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let (taken, _) = pool.try_insert(epoch, 20, vec![transaction()]).unwrap();
        let (_, ack, _) = pool.take(1, usize::MAX);
        pool.close();

        assert!(matches!(
            pending.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        ack(block(1));
        assert!(matches!(
            taken.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        assert!(matches!(
            pool.try_insert(epoch, 30, vec![transaction()])
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
    }

    #[tokio::test]
    async fn dropped_ack_after_close_does_not_requeue() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let (receiver, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let system_receiver = pool.submit(epoch, &[transaction()]).unwrap();
        let (_, ack, _) = pool.take(2, usize::MAX);
        pool.close();
        drop(ack);
        assert!(matches!(
            receiver.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        assert!(system_receiver.await.is_err());
        assert_eq!(pool.queue_depth("user"), 0);
    }

    #[tokio::test]
    async fn take_closes_user_lane_when_epoch_stops_accepting_user_certs() {
        let (state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let (user_receiver, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let system_receiver = pool.submit(epoch, &[transaction()]).unwrap();

        let epoch_store = state.epoch_store_for_testing();
        epoch_store.close_user_certs_for_manual_epoch_close(
            epoch_store.get_reconfig_state_write_lock_guard(),
        );
        let (transactions, ack, limit) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1);
        assert_eq!(limit, LimitReached::AllTransactionsIncluded);
        assert!(matches!(
            user_receiver.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        assert!(matches!(
            pool.try_insert(epoch, 20, vec![transaction()])
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));

        let block_ref = block(1);
        ack(block_ref);
        assert_eq!(system_receiver.await.unwrap().0, block_ref);
    }

    #[tokio::test]
    async fn dropped_ack_after_user_lane_close_resolves_taken_entries() {
        let (state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let (taken, _) = pool.try_insert(epoch, 10, vec![transaction()]).unwrap();
        let (transactions, ack, _) = pool.take(1, usize::MAX);
        assert_eq!(transactions.len(), 1);

        // Close the user lane while the taken entry is still out with the proposer.
        let epoch_store = state.epoch_store_for_testing();
        epoch_store.close_user_certs_for_manual_epoch_close(
            epoch_store.get_reconfig_state_write_lock_guard(),
        );
        let (flushed, flush_ack, _) = pool.take(10, usize::MAX);
        assert!(flushed.is_empty());
        drop(flush_ack);

        // The dropped ack must resolve the taken entry with the halted error instead
        // of re-queueing it into the closed lane.
        drop(ack);
        assert!(matches!(
            taken.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        assert_eq!(pool.queue_depth("user"), 0);
    }

    #[tokio::test]
    async fn notify_committed_sequences_commits_before_garbage_collection() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let first = pool.submit(epoch, &[transaction()]).unwrap();
        let (_, ack, _) = pool.take(1, usize::MAX);
        let first_block = block(1);
        ack(first_block);
        let (_, _, first_status) = first.await.unwrap();

        let second = pool.submit(epoch, &[transaction()]).unwrap();
        let (_, ack, _) = pool.take(1, usize::MAX);
        let second_block = block(2);
        ack(second_block);
        let (_, _, second_status) = second.await.unwrap();

        pool.notify_committed(vec![second_block], 1);
        assert_eq!(
            second_status.await.unwrap(),
            BlockStatus::Sequenced(second_block)
        );
        assert_eq!(
            first_status.await.unwrap(),
            BlockStatus::GarbageCollected(first_block)
        );
    }

    #[tokio::test]
    async fn notify_committed_reports_proposed_user_outcomes() {
        let (_state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let status = |status: &str| {
            pool.adapter_metrics
                .sequencing_certificate_status
                .with_label_values(&["owned_user_transaction_v2", status])
                .get()
        };
        let commit_latency_count = |lane: &str| {
            pool.metrics
                .pool_commit_latency
                .with_label_values(&[lane])
                .get_sample_count()
        };

        // Block 1: one user transaction, later garbage collected.
        let (_first, _) = pool.try_insert(epoch, 1, vec![user_transaction()]).unwrap();
        let (_, ack, _) = pool.take(1, usize::MAX);
        ack(block(1));
        // Block 2: one user transaction and one system transaction, committed.
        let (_second, _) = pool.try_insert(epoch, 1, vec![user_transaction()]).unwrap();
        let _system = pool.submit(epoch, &[transaction()]).unwrap();
        let (_, ack, _) = pool.take(2, usize::MAX);
        ack(block(2));
        assert_eq!(status("sequenced"), 0);

        pool.notify_committed(vec![block(2)], 1);
        assert_eq!(status("sequenced"), 1);
        assert_eq!(status("garbage_collected"), 1);
        // System entries report through the adapter; only their latency is recorded here.
        assert_eq!(commit_latency_count("user"), 1);
        assert_eq!(commit_latency_count("system"), 1);
    }

    #[tokio::test]
    async fn insert_validation_rejects_oversized_transactions_and_bundles() {
        let serialized_len = u64::try_from(bcs::to_bytes(&transaction()).unwrap().len()).unwrap();

        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_consensus_max_transaction_size_bytes_for_testing(serialized_len - 1);
        let (_state, pool) = test_state_and_pool_with_protocol_config(10, config).await;
        assert!(
            pool.try_insert(pool.epoch(), 1, vec![transaction()])
                .err()
                .unwrap()
                .to_string()
                .contains("Transaction size")
        );

        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_consensus_max_num_transactions_in_block_for_testing(1);
        let (_state, pool) = test_state_and_pool_with_protocol_config(10, config).await;
        assert!(
            pool.try_insert(pool.epoch(), 1, vec![transaction(), transaction()])
                .err()
                .unwrap()
                .to_string()
                .contains("bundle count")
        );

        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_consensus_max_transactions_in_block_bytes_for_testing(serialized_len);
        let (_state, pool) = test_state_and_pool_with_protocol_config(10, config).await;
        assert!(
            pool.try_insert(pool.epoch(), 1, vec![transaction(), transaction()])
                .err()
                .unwrap()
                .to_string()
                .contains("bundle size")
        );
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "system transaction failed validation")]
    async fn oversized_system_transaction_is_an_invariant_violation() {
        let serialized_len = u64::try_from(bcs::to_bytes(&transaction()).unwrap().len()).unwrap();
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_consensus_max_transaction_size_bytes_for_testing(serialized_len - 1);
        let (_state, pool) = test_state_and_pool_with_protocol_config(10, config).await;
        let _ = pool.submit(pool.epoch(), &[transaction()]);
    }

    #[tokio::test]
    async fn take_skips_already_processed_entries_without_shrinking_the_block() {
        let (state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let epoch_store = state.epoch_store_for_testing();

        // Both bid highest, so they head the pop order and would consume the whole
        // budget below if they were not skipped — one observed through consensus
        // output, one through checkpoint execution.
        let processed = user_transaction();
        let (processed_consensus, _) = pool
            .try_insert(epoch, 100, vec![processed.clone()])
            .unwrap();
        let executed = user_transaction();
        let (processed_checkpoint, _) = pool.try_insert(epoch, 90, vec![executed.clone()]).unwrap();
        let live = (0..3)
            .map(|_| {
                pool.try_insert(epoch, 10, vec![user_transaction()])
                    .unwrap()
                    .0
            })
            .collect::<Vec<_>>();

        epoch_store.process_notifications(std::iter::once(&consensus_key(&processed)));
        epoch_store
            .insert_finalized_transactions(&[digest_of(&executed)], 1)
            .unwrap();

        let (transactions, ack, limit) = pool.take(3, usize::MAX);
        assert_eq!(transactions.len(), 3);
        assert_eq!(limit, LimitReached::AllTransactionsIncluded);
        assert_eq!(pool.queue_depth("user"), 0);
        assert_eq!(
            pool.already_processed_count("proposal", "consensus_message"),
            1
        );
        assert_eq!(
            pool.already_processed_count("proposal", "checkpoint_execution"),
            1
        );
        for receiver in [processed_consensus, processed_checkpoint] {
            assert!(matches!(
                receiver.await.unwrap().unwrap_err().as_inner(),
                SuiErrorKind::TransactionProcessing { .. }
            ));
        }

        ack(block(1));
        for receiver in live {
            assert_eq!(receiver.await.unwrap().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn bundles_are_skipped_only_when_every_transaction_is_already_processed() {
        let (state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let epoch_store = state.epoch_store_for_testing();

        let first = user_transaction();
        let second = user_transaction();
        let (receiver, _) = pool
            .try_insert(epoch, 10, vec![first.clone(), second.clone()])
            .unwrap();

        epoch_store.process_notifications(std::iter::once(&consensus_key(&first)));
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 2);
        assert_eq!(
            pool.already_processed_count("proposal", "consensus_message"),
            0
        );
        // Requeue instead of acknowledging: the next proposal must still remember that
        // `first` was observed as processed, since its notification is delivered once.
        drop(ack);

        epoch_store.process_notifications(std::iter::once(&consensus_key(&second)));
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty());
        assert_eq!(
            pool.already_processed_count("proposal", "consensus_message"),
            1
        );
        assert!(matches!(
            receiver.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::TransactionProcessing { .. }
        ));
        drop(ack);
    }

    #[tokio::test]
    async fn insert_rejects_already_processed_transactions() {
        let (state, pool) = test_state_and_pool(10).await;
        let epoch = pool.epoch();
        let epoch_store = state.epoch_store_for_testing();

        // An already-processed insert must fail before the ack is armed — a regression detonates
        // the drop bomb and panics this test.
        let via_consensus = user_transaction();
        epoch_store.test_insert_user_signature(digest_of(&via_consensus), vec![]);
        assert!(matches!(
            pool.try_insert(epoch, 10, vec![via_consensus])
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionProcessing { .. }
        ));
        assert_eq!(
            pool.already_processed_count("insert", "consensus_message"),
            1
        );

        let via_checkpoint = user_transaction();
        epoch_store
            .insert_finalized_transactions(&[digest_of(&via_checkpoint)], 1)
            .unwrap();
        assert!(matches!(
            pool.try_insert(epoch, 10, vec![via_checkpoint])
                .unwrap_err()
                .as_inner(),
            SuiErrorKind::TransactionProcessing { .. }
        ));
        assert_eq!(
            pool.already_processed_count("insert", "checkpoint_execution"),
            1
        );

        assert_eq!(pool.queue_depth("user"), 0);
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty());
        drop(ack);
    }

    #[tokio::test]
    async fn departing_entries_deregister_their_processed_watches() {
        let (state, pool) = test_state_and_pool(1).await;
        let epoch = pool.epoch();
        let epoch_store = state.epoch_store_for_testing();
        // Each user key registers with both the consensus and the checkpoint registry.
        let baseline = epoch_store.num_pending_processed_notifications();

        let evicted_tx = user_transaction();
        let (evicted, _) = pool
            .try_insert(epoch, 10, vec![evicted_tx.clone()])
            .unwrap();
        assert_eq!(
            epoch_store.num_pending_processed_notifications(),
            baseline + 2
        );

        let taken_tx = user_transaction();
        let (taken, _) = pool.try_insert(epoch, 20, vec![taken_tx.clone()]).unwrap();
        assert!(matches!(
            evicted.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::TransactionRejectedDueToOutbiddingDuringCongestion { .. }
        ));
        assert_eq!(
            epoch_store.num_pending_processed_notifications(),
            baseline + 2
        );

        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1);
        ack(block(1));
        taken.await.unwrap().unwrap();
        assert_eq!(epoch_store.num_pending_processed_notifications(), baseline);

        let closed_tx = user_transaction();
        let (closed, _) = pool.try_insert(epoch, 10, vec![closed_tx.clone()]).unwrap();
        assert_eq!(
            epoch_store.num_pending_processed_notifications(),
            baseline + 2
        );
        pool.close();
        assert!(matches!(
            closed.await.unwrap().unwrap_err().as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        assert_eq!(epoch_store.num_pending_processed_notifications(), baseline);

        for transaction in [&evicted_tx, &taken_tx, &closed_tx] {
            epoch_store.process_notifications(std::iter::once(&consensus_key(transaction)));
            epoch_store
                .insert_finalized_transactions(&[digest_of(transaction)], 1)
                .unwrap();
        }
        assert_eq!(epoch_store.num_pending_processed_notifications(), baseline);
    }

    #[tokio::test]
    async fn zero_user_capacity_is_rejected_at_startup() {
        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ConsensusTransactionPool::new_for_tests(
                epoch_store,
                0,
                Arc::new(AdmissionQueueMetrics::new_for_tests()),
            )
        }));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn context_observes_installed_pool_and_waits_for_matching_epoch() {
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let context = Arc::new(TransactionPoolContext::new_for_tests(metrics));
        let state = TestAuthorityBuilder::new().build().await;
        let pool = pool_for_current_epoch(&state, 10);
        let epoch = pool.epoch();
        let absent_waiter = tokio::spawn({
            let context = context.clone();
            async move { context.wait_for_pool(epoch).await }
        });
        while context.metrics().pool_waiting_inserts.get() == 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(context.metrics().pool_waiting_inserts.get(), 1);
        context.set_active(epoch, pool.clone());
        assert_eq!(absent_waiter.await.unwrap().unwrap().epoch(), epoch);

        // send_replace retains the active state after the only receiver is gone.
        assert_eq!(context.wait_for_pool(epoch).await.unwrap().epoch(), epoch);

        let race_context = Arc::new(TransactionPoolContext::new_for_tests(Arc::new(
            AdmissionQueueMetrics::new_for_tests(),
        )));
        let setter = race_context.clone();
        let race_pool = pool.clone();
        assert_eq!(
            race_context
                .wait_for_pool_after_subscribe(epoch, move || {
                    setter.set_active(epoch, race_pool);
                })
                .await
                .unwrap()
                .epoch(),
            epoch
        );

        let next_epoch = epoch + 1;
        let context_waiter = context.clone();
        let waiter = tokio::spawn(async move { context_waiter.wait_for_pool(next_epoch).await });
        while context.metrics().pool_waiting_inserts.get() == 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(context.metrics().pool_waiting_inserts.get(), 1);
        state.reconfigure_for_testing().await;
        let next_pool = pool_for_current_epoch(&state, 10);
        assert_eq!(next_pool.epoch(), next_epoch);
        context.set_active(next_epoch, next_pool);
        assert_eq!(waiter.await.unwrap().unwrap().epoch(), next_epoch);
    }

    #[tokio::test]
    async fn context_fails_stale_and_unavailable_epochs_promptly() {
        let context =
            TransactionPoolContext::new_for_tests(Arc::new(AdmissionQueueMetrics::new_for_tests()));
        // Advance to epoch 1 so a stale (older-epoch) caller can exist.
        let state = TestAuthorityBuilder::new().build().await;
        state.reconfigure_for_testing().await;
        let pool = pool_for_current_epoch(&state, 10);
        let active_epoch = pool.epoch();
        context.set_active(active_epoch, pool);
        assert!(matches!(
            context
                .wait_for_pool(active_epoch - 1)
                .await
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
        context.set_unavailable(active_epoch + 1);
        assert!(matches!(
            context
                .wait_for_pool(active_epoch + 1)
                .await
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::TooManyTransactionsPendingConsensus
        ));
        assert!(matches!(
            context
                .wait_for_pool(active_epoch)
                .await
                .err()
                .unwrap()
                .as_inner(),
            SuiErrorKind::ValidatorHaltedAtEpochEnd
        ));
    }

    #[tokio::test]
    async fn transaction_pool_client_maps_ping_ack_and_status() {
        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let context = Arc::new(TransactionPoolContext::new_for_tests(metrics.clone()));
        let pool = Arc::new(ConsensusTransactionPool::new_for_tests(
            epoch_store.clone(),
            10,
            metrics,
        ));
        context.set_active(epoch_store.epoch(), pool.clone());
        let client = Arc::new(TransactionPoolClient::new(context));

        let task = tokio::spawn({
            let client = client.clone();
            let epoch_store = epoch_store.clone();
            async move { client.submit(&[], &epoch_store).await }
        });
        while pool.queue_depth("ping") == 0 {
            tokio::task::yield_now().await;
        }
        let (_, ack, _) = pool.take(0, 0);
        let block_ref = block(3);
        ack(block_ref);
        let (positions, status) = task.await.unwrap().unwrap();
        assert_eq!(positions[0].block, block_ref);
        assert_eq!(positions[0].index, PING_TRANSACTION_INDEX);
        pool.notify_committed(vec![block_ref], 0);
        assert_eq!(status.await.unwrap(), BlockStatus::Sequenced(block_ref));

        let user_transaction = user_transaction();
        let task = tokio::spawn({
            let client = client.clone();
            let epoch_store = epoch_store.clone();
            async move { client.submit(&[user_transaction], &epoch_store).await }
        });
        match task.await {
            // debug_fatal panics in debug/sim builds.
            Err(join_error) => assert!(join_error.is_panic()),
            // Release builds log and return the rejection instead.
            Ok(result) => assert!(result.is_err()),
        }
        assert_eq!(pool.queue_depth("user"), 0);
        assert_eq!(pool.queue_depth("system"), 0);
    }
}
