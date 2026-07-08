// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

use consensus_core::BlockStatus;
use futures::FutureExt;
use futures::StreamExt;
use futures::future::{self, Either, join_all, select};
use futures::stream::FuturesUnordered;
use mysten_common::debug_fatal;
use mysten_metrics::{
    GaugeGuard, InflightGuardFutureExt, LATENCY_SEC_BUCKETS, spawn_monitored_task,
};
use parking_lot::RwLockReadGuard;
use prometheus::Histogram;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::IntGauge;
use prometheus::IntGaugeVec;
use prometheus::Registry;
use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use sui_types::base_types::AuthorityName;
use sui_types::error::{SuiError, SuiErrorKind, SuiResult};
use sui_types::fp_ensure;
use sui_types::messages_consensus::ConsensusPosition;
use sui_types::messages_consensus::ConsensusTransactionKind;
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKey};
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::{Notify, Semaphore, SemaphorePermit, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::{self};
use tracing::{Instrument, debug, debug_span, info, instrument, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::consensus_tx_status_cache::{
    ConsensusTxStatus, NotifyReadConsensusTxStatusResult,
};
use crate::checkpoints::CheckpointStore;
use crate::consensus_handler::{SequencedConsensusTransactionKey, classify};
use crate::epoch::reconfiguration::{ReconfigState, ReconfigurationInitiator};

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

pub struct ConsensusAdapterMetrics {
    // Certificate sequencing metrics
    pub sequencing_certificate_attempt: IntCounterVec,
    pub sequencing_certificate_success: IntCounterVec,
    pub sequencing_certificate_failures: IntCounterVec,
    pub sequencing_certificate_status: IntCounterVec,
    pub sequencing_certificate_settled_status: IntCounterVec,
    pub sequencing_certificate_inflight: IntGaugeVec,
    pub sequencing_acknowledge_latency: HistogramVec,
    pub sequencing_certificate_latency: HistogramVec,
    pub sequencing_certificate_stage_latency: HistogramVec,
    pub sequencing_certificate_submit_to_ack_latency: HistogramVec,
    pub sequencing_certificate_processed: IntCounterVec,
    pub sequencing_in_flight_semaphore_wait: IntGauge,
    pub sequencing_in_flight_submissions: IntGauge,
    pub sequencing_best_effort_timeout: IntCounterVec,
    pub consensus_latency: Histogram,
    pub num_rejected_cert_in_epoch_boundary: IntCounter,
}

impl ConsensusAdapterMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            sequencing_certificate_attempt: register_int_counter_vec_with_registry!(
                "sequencing_certificate_attempt",
                "Counts the number of certificates the validator attempts to sequence.",
                &["tx_type"],
                registry,
            )
                .unwrap(),
            sequencing_certificate_success: register_int_counter_vec_with_registry!(
                "sequencing_certificate_success",
                "Counts the number of successfully sequenced certificates.",
                &["tx_type"],
                registry,
            )
                .unwrap(),
            sequencing_certificate_failures: register_int_counter_vec_with_registry!(
                "sequencing_certificate_failures",
                "Counts the number of sequenced certificates that failed other than by timeout.",
                &["tx_type"],
                registry,
            )
                .unwrap(),
                sequencing_certificate_status: register_int_counter_vec_with_registry!(
                "sequencing_certificate_status",
                "The status of the certificate sequencing as reported by consensus. The status can be either sequenced or garbage collected.",
                &["tx_type", "status"],
                registry,
            )
                .unwrap(),
            sequencing_certificate_settled_status: register_int_counter_vec_with_registry!(
                "sequencing_certificate_settled_status",
                "The terminal per-position consensus status (finalized, rejected or dropped) of transactions whose submission settled via position status.",
                &["tx_type", "status"],
                registry,
            )
                .unwrap(),
            sequencing_certificate_inflight: register_int_gauge_vec_with_registry!(
                "sequencing_certificate_inflight",
                "The inflight requests to sequence certificates.",
                &["tx_type"],
                registry,
            )
                .unwrap(),
            sequencing_acknowledge_latency: register_histogram_vec_with_registry!(
                "sequencing_acknowledge_latency",
                "The latency for acknowledgement from sequencing engine. The overall sequencing latency is measured by the sequencing_certificate_latency metric",
                &["retry", "tx_type"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_latency: register_histogram_vec_with_registry!(
                "sequencing_certificate_latency",
                "The latency for sequencing a certificate.",
                &["submitted", "tx_type", "processed_method"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_stage_latency: register_histogram_vec_with_registry!(
                "sequencing_certificate_stage_latency",
                "Cumulative latency from inflight slot acquisition until a submission reaches each stage of the submit path: position_received (consensus positions returned by the first successful submit), block_sequenced (the submitted block committed), status_received / status_expired (all positions reached a terminal status, or one expired from the status cache first).",
                &["tx_type", "stage"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_submit_to_ack_latency: register_histogram_vec_with_registry!(
                "sequencing_certificate_submit_to_ack_latency",
                "Net latency from acquiring the in-flight submission permit (just before submitting to consensus) until the submission is acknowledged: the local submission settles (statuses received or expired, or a sequenced system message observed processed) or the transaction is observed processed via another path.",
                &["tx_type"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_processed: register_int_counter_vec_with_registry!(
                "sequencing_certificate_processed",
                "The number of certificates that have been processed either by consensus or checkpoint.",
                &["source"],
                registry
            ).unwrap(),
            sequencing_in_flight_semaphore_wait: register_int_gauge_with_registry!(
                "sequencing_in_flight_semaphore_wait",
                "How many requests are blocked on submit_permit.",
                registry,
            )
                .unwrap(),
            sequencing_in_flight_submissions: register_int_gauge_with_registry!(
                "sequencing_in_flight_submissions",
                "Number of transactions submitted to local consensus instance and not yet sequenced",
                registry,
            )
                .unwrap(),
            sequencing_best_effort_timeout: register_int_counter_vec_with_registry!(
                "sequencing_best_effort_timeout",
                "The number of times the best effort submission has timed out.",
                &["tx_type"],
                registry,
            ).unwrap(),
            // These two metrics originally lived in ValidatorServiceMetrics (authority_server.rs)
            // and keep their legacy names for dashboard compatibility.
            consensus_latency: register_histogram_with_registry!(
                "validator_service_consensus_latency",
                "Time spent between submitting a txn to consensus and getting back local acknowledgement. Execution and finalization time are not included.",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            num_rejected_cert_in_epoch_boundary: register_int_counter_with_registry!(
                "validator_service_num_rejected_cert_in_epoch_boundary",
                "Number of rejected transaction certificate during epoch transitioning",
                registry,
            ).unwrap(),
        }
    }

    pub fn new_test() -> Self {
        Self::new(&Registry::default())
    }
}

/// An object that can be used to check if the consensus is overloaded.
pub trait ConsensusOverloadChecker: Sync + Send + 'static {
    fn check_consensus_overload(&self) -> SuiResult;
}

pub type BlockStatusReceiver = oneshot::Receiver<BlockStatus>;

#[mockall::automock]
pub trait SubmitToConsensus: Sync + Send + 'static {
    fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult;

    /// Submits a system transaction to consensus once, without waiting for it to
    /// be sequenced and without retrying if it is garbage collected, bounded by
    /// `timeout`. Suits periodic, self-superseding messages (e.g. execution time
    /// observations) where a missed submission is replaced by the next one.
    ///
    /// For system transactions only. User transactions are rejected:
    /// this fire-and-forget, no-retry, backpressure-free path would
    /// silently mishandle them.
    fn submit_best_effort(
        &self,
        transaction: &ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        timeout: Duration,
    ) -> SuiResult;
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait ConsensusClient: Sync + Send + 'static {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)>;
}

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: Arc<dyn ConsensusClient>,
    /// The checkpoint store for the validator
    checkpoint_store: Arc<CheckpointStore>,
    /// Authority pubkey.
    authority: AuthorityName,
    /// The limit to number of inflight transactions at this node.
    max_pending_transactions: usize,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to register metrics
    metrics: ConsensusAdapterMetrics,
    /// Semaphore limiting parallel submissions to consensus
    submit_semaphore: Arc<Semaphore>,
    /// Notified when an inflight slot is freed (`InflightDropGuard` dropped).
    /// Used by the admission queue drainer to wake up and submit more
    /// transactions.
    inflight_slot_freed_notify: Arc<Notify>,
}

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_client: Arc<dyn ConsensusClient>,
        checkpoint_store: Arc<CheckpointStore>,
        authority: AuthorityName,
        max_pending_transactions: usize,
        max_pending_local_submissions: usize,
        metrics: ConsensusAdapterMetrics,
        inflight_slot_freed_notify: Arc<Notify>,
    ) -> Self {
        let num_inflight_transactions = Default::default();
        Self {
            consensus_client,
            checkpoint_store,
            authority,
            max_pending_transactions,
            num_inflight_transactions,
            metrics,
            submit_semaphore: Arc::new(Semaphore::new(max_pending_local_submissions)),
            inflight_slot_freed_notify,
        }
    }

    /// Get the current number of in-flight transactions
    pub fn num_inflight_transactions(&self) -> u64 {
        self.num_inflight_transactions.load(Ordering::Relaxed)
    }

    /// Get the maximum number of pending transactions (consensus capacity limit).
    pub fn max_pending_transactions(&self) -> usize {
        self.max_pending_transactions
    }

    /// Submits transactions to consensus within the reconfiguration lock and
    /// returns their consensus positions.
    pub async fn submit_and_get_positions(
        self: &Arc<Self>,
        consensus_transactions: Vec<ConsensusTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        submitter_client_addr: Option<IpAddr>,
    ) -> Result<Vec<ConsensusPosition>, SuiError> {
        let (tx_consensus_positions, rx_consensus_positions) = oneshot::channel();

        {
            // code block within reconfiguration lock
            let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
            if !reconfiguration_lock.should_accept_user_certs() {
                self.metrics.num_rejected_cert_in_epoch_boundary.inc();
                return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
            }

            // Submit to consensus and wait for the position. If the transaction has
            // already been processed via consensus output or a checkpoint, the adapter
            // skips submission and reports `TransactionProcessing` instead of a position.
            let _metrics_guard = self.metrics.consensus_latency.start_timer();

            self.submit_batch(
                &consensus_transactions,
                Some(&reconfiguration_lock),
                epoch_store,
                Some(tx_consensus_positions),
                submitter_client_addr,
            )?;
        }

        rx_consensus_positions.await.map_err(|e| {
            SuiError::from(SuiErrorKind::FailedToSubmitToConsensus(format!(
                "Failed to get consensus position: {e}"
            )))
        })?
    }

    pub fn recover_end_of_publish(self: &Arc<Self>, epoch_store: &Arc<AuthorityPerEpochStore>) {
        // This handles the case where the node crashed after setting reconfig lock state
        // but before the EndOfPublish message was sent to consensus.
        if epoch_store.should_send_end_of_publish() {
            let transaction = ConsensusTransaction::new_end_of_publish(self.authority);
            info!(epoch=?epoch_store.epoch(), "Submitting EndOfPublish message to consensus");
            self.submit_unchecked(&[transaction], epoch_store, None, None);
        }
    }

    /// This method blocks until transaction is persisted in local database
    /// It then returns handle to async task, user can join this handle to await while transaction is processed by consensus
    ///
    /// This method guarantees that once submit(but not returned async handle) returns,
    /// transaction is persisted and will eventually be sent to consensus even after restart
    ///
    /// When submitting a certificate caller **must** provide a ReconfigState lock guard
    pub fn submit(
        self: &Arc<Self>,
        transaction: ConsensusTransaction,
        lock: Option<&RwLockReadGuard<ReconfigState>>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_consensus_position: Option<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<JoinHandle<()>> {
        self.submit_batch(
            &[transaction],
            lock,
            epoch_store,
            tx_consensus_position,
            submitter_client_addr,
        )
    }

    // Submits the provided transactions to consensus in a batched fashion. The `transactions` vector can be also empty in case of a ping check.
    // In this case the system will simulate a transaction submission to consensus and return the consensus position.
    pub fn submit_batch(
        self: &Arc<Self>,
        transactions: &[ConsensusTransaction],
        _lock: Option<&RwLockReadGuard<ReconfigState>>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_consensus_position: Option<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<JoinHandle<()>> {
        if transactions.len() > 1 {
            // Soft bundles must contain only UserTransactionV2 transactions.
            for transaction in transactions {
                fp_ensure!(
                    transaction.is_user_transaction(),
                    SuiErrorKind::InvalidTxKindInSoftBundle.into()
                );
            }
        }

        Ok(self.submit_unchecked(
            transactions,
            epoch_store,
            tx_consensus_position,
            submitter_client_addr,
        ))
    }

    /// Performs weakly consistent checks on internal buffers to quickly
    /// discard transactions if we are overloaded
    fn check_limits(&self) -> bool {
        // First check total transactions (waiting and in submission)
        if self.num_inflight_transactions.load(Ordering::Relaxed) as usize
            >= self.max_pending_transactions
        {
            return false;
        }
        // Then check if submit_semaphore has permits
        self.submit_semaphore.available_permits() > 0
    }

    fn submit_unchecked(
        self: &Arc<Self>,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx_consensus_position: Option<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
        submitter_client_addr: Option<IpAddr>,
    ) -> JoinHandle<()> {
        // Reconfiguration lock is dropped when pending_consensus_transactions is persisted, before it is handled by consensus
        let async_stage = self
            .clone()
            .submit_and_wait(
                transactions.to_vec(),
                epoch_store.clone(),
                tx_consensus_position,
                submitter_client_addr,
            )
            .in_current_span();
        // Number of these tasks is weakly limited based on `num_inflight_transactions`.
        // (Limit is not applied atomically, and only to user transactions.)
        let join_handle = spawn_monitored_task!(async_stage);
        join_handle
    }

    async fn submit_and_wait(
        self: Arc<Self>,
        transactions: Vec<ConsensusTransaction>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        tx_consensus_position: Option<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
        submitter_client_addr: Option<IpAddr>,
    ) {
        // When epoch_terminated signal is received all pending submit_and_wait_inner are dropped.
        //
        // This is needed because submit_and_wait_inner waits on read_notify for consensus message to be processed,
        // which may never happen on epoch boundary.
        //
        // In addition to that, within_alive_epoch ensures that all pending consensus
        // adapter tasks are stopped before reconfiguration can proceed.
        //
        // This is essential because after epoch change, this validator may exit the committee and become a full node.
        // So it is no longer able to submit to consensus.
        //
        // Also, submission to consensus is not gated on epoch. Although it is ok to submit user transactions
        // to the new epoch, we want to cancel system transaction submissions from the current epoch to the new epoch.
        epoch_store
            .within_alive_epoch(self.submit_and_wait_inner(
                transactions,
                &epoch_store,
                tx_consensus_position,
                submitter_client_addr,
            ))
            .await
            .ok(); // result here indicates if epoch ended earlier, we don't care about it
    }

    #[allow(clippy::option_map_unit_fn)]
    #[instrument(name="ConsensusAdapter::submit_and_wait_inner", level="trace", skip_all, fields(tx_count = ?transactions.len(), tx_type = tracing::field::Empty, tx_keys = tracing::field::Empty, submit_status = tracing::field::Empty, consensus_positions = tracing::field::Empty))]
    async fn submit_and_wait_inner(
        self: Arc<Self>,
        transactions: Vec<ConsensusTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        mut tx_consensus_positions: Option<oneshot::Sender<SuiResult<Vec<ConsensusPosition>>>>,
        submitter_client_addr: Option<IpAddr>,
    ) {
        if transactions.is_empty() {
            // If transactions are empty, then we attempt to ping consensus and simulate a transaction submission to consensus.
            // We intentionally do not wait for the block status, as we are only interested in the consensus position and return it immediately.
            debug!(
                "Performing a ping check, pinging consensus to get a consensus position in next block"
            );
            let (consensus_positions, _status_waiter) = self
                .submit_inner(&transactions, epoch_store, &[], "ping")
                .await;

            if let Some(tx_consensus_positions) = tx_consensus_positions.take() {
                let _ = tx_consensus_positions.send(Ok(consensus_positions));
            } else {
                debug_fatal!("Ping check must have a consensus position channel");
            }
            return;
        }

        // Record submitted transactions early for DoS protection
        for transaction in &transactions {
            if let Some(tx) = transaction.kind.as_user_transaction() {
                let amplification_factor = (tx.data().transaction_data().gas_price()
                    / epoch_store.reference_gas_price().max(1))
                .max(1);
                epoch_store.submitted_transaction_cache.record_submitted_tx(
                    tx.digest(),
                    amplification_factor as u32,
                    submitter_client_addr,
                );
            }
        }

        // Current code path ensures:
        // - If transactions.len() > 1, it is a soft bundle. System transactions should have been submitted individually.
        // - If is_soft_bundle, then all transactions are of CertifiedTransaction or UserTransaction kind.
        // - If not is_soft_bundle, then transactions must contain exactly 1 tx, and transactions[0] can be of any kind.
        let is_soft_bundle = transactions.len() > 1;
        let is_system_message = !transactions[0].is_user_transaction();

        let mut transaction_keys = Vec::new();
        let mut tx_consensus_positions = tx_consensus_positions;

        for transaction in &transactions {
            if matches!(transaction.kind, ConsensusTransactionKind::EndOfPublish(..)) {
                info!(epoch=?epoch_store.epoch(), "Submitting EndOfPublish message to consensus");
                epoch_store.record_epoch_pending_certs_process_time_metric();
            }

            let transaction_key = SequencedConsensusTransactionKey::External(transaction.key());
            transaction_keys.push(transaction_key);
        }
        let tx_type = if is_soft_bundle {
            "soft_bundle"
        } else {
            classify(&transactions[0])
        };
        tracing::Span::current().record("tx_type", tx_type);
        tracing::Span::current().record("tx_keys", tracing::field::debug(&transaction_keys));

        let mut guard = InflightDropGuard::acquire(&self, tx_type, transactions.len() as u64);

        // Builds the error reported to a position-waiting caller (mfp) when the
        // transaction is already being processed and we therefore skip (re)submission.
        // The caller surfaces this as a retriable error so the client waits for
        // effects / retries instead of receiving a meaningless consensus position.
        let make_processing_error = |method: ProcessedMethod| -> SuiError {
            let digest = transactions
                .iter()
                .find_map(|t| t.kind.as_user_transaction().map(|tx| *tx.digest()))
                .unwrap_or_default();
            SuiErrorKind::TransactionProcessing {
                digest,
                status: format!("processed via {}", method.method_name()),
            }
            .into()
        };

        // Skip submission if the tx is already processed via consensus output or
        // checkpoint state sync.
        let already_processed =
            self.check_processed_via_consensus_or_checkpoint(&transaction_keys, epoch_store);
        if let Some(method) = already_processed {
            guard.processed_method = method;
            if let Some(tx_consensus_positions) = tx_consensus_positions.take() {
                let _ = tx_consensus_positions.send(Err(make_processing_error(method)));
            }
        }

        // Log warnings for administrative transactions that fail to get sequenced
        let _monitor = if matches!(
            transactions[0].kind,
            ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::CapabilityNotification(_)
                | ConsensusTransactionKind::CapabilityNotificationV2(_)
                | ConsensusTransactionKind::RandomnessDkgMessage(_, _)
                | ConsensusTransactionKind::RandomnessDkgConfirmation(_, _)
        ) {
            assert!(
                !is_soft_bundle,
                "System transactions should have been submitted individually"
            );
            let transaction_keys = transaction_keys.clone();
            Some(CancelOnDrop(spawn_monitored_task!(async {
                let mut i = 0u64;
                loop {
                    i += 1;
                    const WARN_DELAY_S: u64 = 30;
                    tokio::time::sleep(Duration::from_secs(WARN_DELAY_S)).await;
                    let total_wait = i * WARN_DELAY_S;
                    warn!(
                        "Still waiting {} seconds for transactions {:?} to commit in consensus",
                        total_wait, transaction_keys
                    );
                }
            })))
        } else {
            None
        };

        if already_processed.is_none() {
            guard.submitted = true;

            // Submit the transaction to consensus, racing against the processed waiter in
            // case another validator sequences the transaction first.
            let submit_start = guard.start;
            let observe_stage = |stage: &str| {
                self.metrics
                    .sequencing_certificate_stage_latency
                    .with_label_values(&[tx_type, stage])
                    .observe(submit_start.elapsed().as_secs_f64());
            };

            observe_stage("passed_processing");

            // System messages (checkpoint signatures, EndOfPublish, capability
            // notifications, randomness DKG, etc.) are not buffered behind user
            // tx; they are excluded from the semaphore.
            let _permit: Option<SemaphorePermit> = if is_system_message {
                None
            } else {
                Some(
                    self.submit_semaphore
                        .acquire()
                        .count_in_flight(self.metrics.sequencing_in_flight_semaphore_wait.clone())
                        .await
                        .expect("Consensus adapter does not close semaphore"),
                )
            };

            debug!("Submitting {:?} to consensus", transaction_keys);

            observe_stage("acquired_semaphore");

            let submit_to_ack_start = Instant::now();

            let _in_flight_submission_guard =
                GaugeGuard::acquire(&self.metrics.sequencing_in_flight_submissions);

            let submit_fut = async {
                const RETRY_DELAY_STEP: Duration = Duration::from_secs(1);
                let mut position_received_recorded = false;

                loop {
                    // Submit the transaction to consensus and return the submit result with a status waiter
                    let (consensus_positions, status_waiter) = self
                        .submit_inner(&transactions, epoch_store, &transaction_keys, tx_type)
                        .await;

                    // Only record the first receipt, so GC-triggered resubmissions don't
                    // re-count; later stages then include any resubmission time.
                    if !position_received_recorded {
                        position_received_recorded = true;
                        observe_stage("position_received");
                    }

                    if let Some(tx_consensus_positions) = tx_consensus_positions.take() {
                        tracing::Span::current().record(
                            "consensus_positions",
                            tracing::field::debug(&consensus_positions),
                        );
                        // We send the first consensus position returned by consensus
                        // to the submitting client even if it is retried internally within
                        // consensus adapter due to an error or GC. They can handle retries
                        // as needed if the consensus position does not return the desired
                        // results (e.g. not sequenced due to garbage collection).
                        let _ = tx_consensus_positions.send(Ok(consensus_positions.clone()));
                    }

                    match status_waiter.await {
                        Ok(status @ BlockStatus::Sequenced(_)) => {
                            tracing::Span::current()
                                .record("status", tracing::field::debug(&status));
                            self.metrics
                                .sequencing_certificate_status
                                .with_label_values(&[tx_type, "sequenced"])
                                .inc();
                            observe_stage("block_sequenced");
                            debug!(
                                "Transaction {transaction_keys:?} has been sequenced by consensus."
                            );
                            if is_system_message {
                                // System messages have consensus positions too, but the
                                // commit handler only assigns per-position statuses to
                                // user transactions, so their completion is signaled by
                                // the processed flag instead.
                                break SequencingOutcome::BlockSequenced;
                            }
                            if consensus_positions.len() != transactions.len() {
                                debug_fatal!(
                                    "Consensus client returned {} positions for {} transactions",
                                    consensus_positions.len(),
                                    transactions.len()
                                );
                                debug!(
                                    "Transaction {transaction_keys:?} client returned {} positions for {} transactions",
                                    consensus_positions.len(),
                                    transactions.len()
                                );
                                break SequencingOutcome::BlockSequenced;
                            }
                            // The block is committed, and the commit handler assigns every
                            // user transaction position a terminal status.
                            match self
                                .wait_for_position_statuses(&consensus_positions, epoch_store)
                                .await
                            {
                                Some(statuses) => {
                                    observe_stage("status_received");
                                    break SequencingOutcome::Sequenced(statuses);
                                }
                                None => {
                                    observe_stage("status_expired");
                                    // A position expired from the status cache before it
                                    // was read: the block was committed and its commit
                                    // processed more than the retention window ago, so a
                                    // terminal status existed and was merely missed. End
                                    // the submission instead of resubmitting — a missed
                                    // Finalized outcome needs nothing further from this
                                    // task (the digest is durably recorded as processed
                                    // and will execute), the other outcomes are terminal,
                                    // and transaction-level retries belong to the client.
                                    debug!(
                                        "Transaction {transaction_keys:?} status expired before being read. Ending submission."
                                    );
                                    self.metrics
                                        .sequencing_certificate_status
                                        .with_label_values(&[tx_type, "status_expired"])
                                        .inc();
                                    break SequencingOutcome::StatusExpired;
                                }
                            }
                        }
                        Ok(status @ BlockStatus::GarbageCollected(_)) => {
                            tracing::Span::current()
                                .record("status", tracing::field::debug(&status));
                            self.metrics
                                .sequencing_certificate_status
                                .with_label_values(&[tx_type, "garbage_collected"])
                                .inc();
                            // Block has been garbage collected and we have no guarantees that the transaction will appear in consensus output. We'll
                            // resubmit the transaction to consensus. If the transaction has been already "processed", then probably someone else has submitted
                            // the transaction and managed to get sequenced. Then this future will have been cancelled anyways so no need to check here on the processed output.
                            debug!(
                                "Transaction {transaction_keys:?} was garbage collected before being sequenced. Will be retried."
                            );
                            time::sleep(RETRY_DELAY_STEP).await;
                            continue;
                        }
                        Err(err) => {
                            warn!(
                                "Error while waiting for status from consensus for transactions {transaction_keys:?}, with error {:?}. Will be retried.",
                                err
                            );
                            time::sleep(RETRY_DELAY_STEP).await;
                            continue;
                        }
                    }
                }
            };

            // Race `processed_notify` against the submit loop. If the tx is
            // processed via another path (consensus output from another
            // validator's submission, or checkpoint state sync) while we're
            // inside the submit loop, the submission future is dropped and
            // the retry loop is cancelled cleanly.
            let processed_waiter = self
                .processed_notify(transaction_keys.clone(), epoch_store)
                .boxed();
            let processed_via_notify;
            guard.processed_method = match select(processed_waiter, submit_fut.boxed()).await {
                Either::Left((observed, _submit_fut)) => {
                    debug!("Transaction {transaction_keys:?} processed via notify");
                    processed_via_notify = true;
                    observed
                }
                Either::Right((SequencingOutcome::Sequenced(statuses), _processed_waiter)) => {
                    debug!("Transaction {transaction_keys:?} processed via statuses");
                    processed_via_notify = false;
                    for status in statuses {
                        self.metrics
                            .sequencing_certificate_settled_status
                            .with_label_values(&[tx_type, status_label(status)])
                            .inc();
                    }
                    ProcessedMethod::ConsensusStatusReceived
                }
                Either::Right((SequencingOutcome::StatusExpired, _processed_waiter)) => {
                    processed_via_notify = false;
                    ProcessedMethod::ConsensusStatusExpired
                }
                Either::Right((SequencingOutcome::BlockSequenced, processed_waiter)) => {
                    debug!("Submitted {transaction_keys:?} to consensus");
                    processed_via_notify = false;
                    processed_waiter.await
                }
            };
            self.metrics
                .sequencing_certificate_submit_to_ack_latency
                .with_label_values(&[tx_type])
                .observe(submit_to_ack_start.elapsed().as_secs_f64());
            // If processing was observed before a position was sent to a waiting caller,
            // report that the transaction is already processing so the caller returns a
            // retriable error. If a position was already sent, the channel is taken and this
            // is a no-op.
            if processed_via_notify
                && let Some(tx_consensus_positions) = tx_consensus_positions.take()
            {
                let _ =
                    tx_consensus_positions.send(Err(make_processing_error(guard.processed_method)));
            }
        }
        debug!(
            "{transaction_keys:?} processed via {}",
            guard.processed_method.method_name()
        );

        // After a user transaction or soft bundle submission,
        // send EndOfPublish if the epoch is closing.
        // EndOfPublish can also be sent during consensus commit handling, checkpoint execution and recovery.
        if transactions[0].is_user_transaction()
            && epoch_store.should_send_end_of_publish()
            && !epoch_store.protocol_config().timestamp_based_epoch_close()
        {
            // sending message outside of any locks scope
            if let Err(err) = self.submit(
                ConsensusTransaction::new_end_of_publish(self.authority),
                None,
                epoch_store,
                None,
                None,
            ) {
                warn!("Error when sending end of publish message: {:?}", err);
            } else {
                info!(epoch=?epoch_store.epoch(), "Sending EndOfPublish message to consensus");
            }
        }
        self.metrics
            .sequencing_certificate_success
            .with_label_values(&[tx_type])
            .inc();
    }

    #[instrument(name = "ConsensusAdapter::submit_inner", level = "trace", skip_all)]
    async fn submit_inner(
        self: &Arc<Self>,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
        transaction_keys: &[SequencedConsensusTransactionKey],
        tx_type: &str,
    ) -> (Vec<ConsensusPosition>, BlockStatusReceiver) {
        let ack_start = Instant::now();
        let mut retries: u32 = 0;
        let mut backoff = mysten_common::backoff::ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        );

        let (consensus_positions, status_waiter) = loop {
            let span = debug_span!("client_submit");
            match self
                .consensus_client
                .submit(transactions, epoch_store)
                .instrument(span)
                .await
            {
                Err(err) => {
                    // This can happen during reconfig, so keep retrying until succeed.
                    if cfg!(msim) || retries > 3 {
                        warn!(
                            "Failed to submit transactions {transaction_keys:?} to consensus: {err}. Retry #{retries}"
                        );
                    }
                    self.metrics
                        .sequencing_certificate_failures
                        .with_label_values(&[tx_type])
                        .inc();
                    retries += 1;

                    time::sleep(backoff.next().unwrap()).await;
                }
                Ok((consensus_positions, status_waiter)) => {
                    break (consensus_positions, status_waiter);
                }
            }
        };

        // we want to record the num of retries when reporting latency but to avoid label
        // cardinality we do some simple bucketing to give us a good enough idea of how
        // many retries happened associated with the latency.
        let bucket = match retries {
            0..=10 => retries.to_string(), // just report the retry count as is
            11..=20 => "between_10_and_20".to_string(),
            21..=50 => "between_20_and_50".to_string(),
            51..=100 => "between_50_and_100".to_string(),
            _ => "over_100".to_string(),
        };

        self.metrics
            .sequencing_acknowledge_latency
            .with_label_values(&[bucket.as_str(), tx_type])
            .observe(ack_start.elapsed().as_secs_f64());

        (consensus_positions, status_waiter)
    }

    /// Sync check for whether `transaction_keys` are already processed via
    /// consensus output or checkpoint state sync. Returns `Some(method)` if
    /// every key is already processed (Checkpoint dominates when any key was
    /// processed via checkpoint or synced-checkpoint), else `None`.
    ///
    /// Also increments `sequencing_certificate_processed` with the matching
    /// label for each key found processed, mirroring what `processed_notify`
    /// emits for its async wake-ups.
    fn check_processed_via_consensus_or_checkpoint(
        self: &Arc<Self>,
        transaction_keys: &[SequencedConsensusTransactionKey],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<ProcessedMethod> {
        let mut seen_checkpoint = false;
        for transaction_key in transaction_keys {
            // Check consensus-processed first; if already visible in consensus
            // output we don't need to submit again.
            if epoch_store
                .is_consensus_message_processed(transaction_key)
                .expect("Storage error when checking consensus message processed")
            {
                self.metrics
                    .sequencing_certificate_processed
                    .with_label_values(&["consensus"])
                    .inc();
                continue;
            }

            // For a cert-shaped key, check whether state sync executed the tx
            // via a checkpoint.
            if let SequencedConsensusTransactionKey::External(ConsensusTransactionKey::Certificate(
                digest,
            )) = transaction_key
                && epoch_store
                    .is_transaction_executed_in_checkpoint(digest)
                    .expect("Storage error when checking transaction executed in checkpoint")
            {
                self.metrics
                    .sequencing_certificate_processed
                    .with_label_values(&["checkpoint"])
                    .inc();
                seen_checkpoint = true;
                continue;
            }

            // For a checkpoint-signature key, check whether a checkpoint at
            // or above the target sequence number has already been synced —
            // in which case the signature is redundant.
            if let SequencedConsensusTransactionKey::External(
                ConsensusTransactionKey::CheckpointSignature(_, seq)
                | ConsensusTransactionKey::CheckpointSignatureV2(_, seq, _),
            ) = transaction_key
                && let Some(synced_seq) = self
                    .checkpoint_store
                    .get_highest_synced_checkpoint_seq_number()
                    .expect("Storage error when reading highest synced checkpoint")
                && synced_seq >= *seq
            {
                self.metrics
                    .sequencing_certificate_processed
                    .with_label_values(&["synced_checkpoint"])
                    .inc();
                seen_checkpoint = true;
                continue;
            }

            // Not processed via any path — caller must submit.
            return None;
        }

        if seen_checkpoint {
            Some(ProcessedMethod::CheckpointExecuted)
        } else {
            Some(ProcessedMethod::ConsensusMessageProcessed)
        }
    }

    /// Async wait for any of `transaction_keys` to become processed via
    /// consensus output or a checkpoint (either state-synced or executed
    /// locally). Used in the in-flight race against submission: cancelling
    /// the submit future when we learn the tx is processed by another path.
    /// Returns `Checkpoint` if any key resolves via a checkpoint path, else
    /// `Consensus`.
    async fn processed_notify(
        self: &Arc<Self>,
        transaction_keys: Vec<SequencedConsensusTransactionKey>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> ProcessedMethod {
        let notifications = FuturesUnordered::new();
        for transaction_key in transaction_keys {
            let transaction_digests = match transaction_key {
                SequencedConsensusTransactionKey::External(
                    ConsensusTransactionKey::Certificate(digest),
                ) => vec![digest],
                _ => vec![],
            };

            let checkpoint_synced_future = if let SequencedConsensusTransactionKey::External(
                ConsensusTransactionKey::CheckpointSignature(_, checkpoint_sequence_number)
                | ConsensusTransactionKey::CheckpointSignatureV2(_, checkpoint_sequence_number, _),
            ) = transaction_key
            {
                // If the transaction is a checkpoint signature, we can also wait to get notified when a checkpoint with equal or higher sequence
                // number has been already synced. This way we don't try to unnecessarily sequence the signature for an already verified checkpoint.
                Either::Left(
                    self.checkpoint_store
                        .notify_read_synced_checkpoint(checkpoint_sequence_number),
                )
            } else {
                Either::Right(future::pending())
            };

            // Wait for each key individually so soft bundles can complete even
            // when different transactions are observed through different paths.
            notifications.push(async move {
                tokio::select! {
                    processed = epoch_store.consensus_messages_processed_notify(vec![transaction_key]) => {
                        processed.expect("Storage error when waiting for consensus message processed");
                        self.metrics.sequencing_certificate_processed.with_label_values(&["consensus"]).inc();
                        return ProcessedMethod::ConsensusMessageProcessed;
                    },
                    processed = epoch_store.transactions_executed_in_checkpoint_notify(transaction_digests), if !transaction_digests.is_empty() => {
                        processed.expect("Storage error when waiting for transaction executed in checkpoint");
                        self.metrics.sequencing_certificate_processed.with_label_values(&["checkpoint"]).inc();
                    }
                    _ = checkpoint_synced_future => {
                        self.metrics.sequencing_certificate_processed.with_label_values(&["synced_checkpoint"]).inc();
                    }
                }
                ProcessedMethod::CheckpointExecuted
            });
        }

        let processed_methods = notifications.collect::<Vec<ProcessedMethod>>().await;
        for method in processed_methods {
            if method == ProcessedMethod::CheckpointExecuted {
                return ProcessedMethod::CheckpointExecuted;
            }
        }
        ProcessedMethod::ConsensusMessageProcessed
    }

    /// Waits until every consensus position reaches a terminal status (Finalized, Rejected or Dropped).
    /// Returns `None` if any position expired from the status cache before its status was read,
    /// in which case the submit loop treats the sequenced block as already handled and settles with
    /// `StatusExpired`.
    async fn wait_for_position_statuses(
        &self,
        consensus_positions: &[ConsensusPosition],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> Option<Vec<ConsensusTxStatus>> {
        join_all(consensus_positions.iter().map(|position| {
            epoch_store
                .consensus_tx_status_cache
                .notify_read_transaction_status(*position)
        }))
        .await
        .into_iter()
        .map(|result| match result {
            NotifyReadConsensusTxStatusResult::Status(status) => Some(status),
            NotifyReadConsensusTxStatusResult::Expired(_) => None,
        })
        .collect()
    }
}

impl ConsensusOverloadChecker for ConsensusAdapter {
    fn check_consensus_overload(&self) -> SuiResult {
        fp_ensure!(
            self.check_limits(),
            SuiErrorKind::TooManyTransactionsPendingConsensus.into()
        );
        Ok(())
    }
}

pub struct NoopConsensusOverloadChecker {}

impl ConsensusOverloadChecker for NoopConsensusOverloadChecker {
    fn check_consensus_overload(&self) -> SuiResult {
        Ok(())
    }
}

impl ReconfigurationInitiator for Arc<ConsensusAdapter> {
    /// This method is called externally to begin reconfiguration
    /// It sets reconfig state to reject new certificates from user.
    /// ConsensusAdapter will send EndOfPublish message once pending certificate queue is drained.
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
            if let Err(err) = self.submit(
                ConsensusTransaction::new_end_of_publish(self.authority),
                None,
                epoch_store,
                None,
                None,
            ) {
                warn!("Error when sending end of publish message: {:?}", err);
            } else {
                info!(epoch=?epoch_store.epoch(), "Sending EndOfPublish message to consensus");
            }
        }
    }
}

impl SubmitToConsensus for Arc<ConsensusAdapter> {
    fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.submit_batch(transactions, None, epoch_store, None, None)
            .map(|_| ())
    }

    fn submit_best_effort(
        &self,
        transaction: &ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        // timeout is required, or the spawned task can run forever
        timeout: Duration,
    ) -> SuiResult {
        if transaction.is_user_transaction() {
            debug_fatal!("submit_best_effort called with a user transaction");
            return Err(SuiErrorKind::GenericAuthorityError {
                error: "submit_best_effort does not accept user transactions".to_string(),
            }
            .into());
        }

        // There is no submit semaphone on this path as it services system msgs only.
        let _in_flight_submission_guard =
            GaugeGuard::acquire(&self.metrics.sequencing_in_flight_submissions);

        let key = SequencedConsensusTransactionKey::External(transaction.key());
        let tx_type = classify(transaction);

        let async_stage = {
            let transaction = transaction.clone();
            let epoch_store = epoch_store.clone();
            let this = self.clone();

            async move {
                let result = tokio::time::timeout(
                    timeout,
                    this.submit_inner(&[transaction], &epoch_store, &[key], tx_type),
                )
                .await;

                if let Err(e) = result {
                    warn!("Consensus submission timed out: {e:?}");
                    this.metrics
                        .sequencing_best_effort_timeout
                        .with_label_values(&[tx_type])
                        .inc();
                }
            }
        };

        let epoch_store = epoch_store.clone();
        spawn_monitored_task!(epoch_store.within_alive_epoch(async_stage));
        Ok(())
    }
}

struct CancelOnDrop<T>(JoinHandle<T>);

impl<T> Deref for CancelOnDrop<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Drop for CancelOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Tracks number of inflight consensus requests and relevant metrics
struct InflightDropGuard<'a> {
    adapter: &'a ConsensusAdapter,
    start: Instant,
    submitted: bool,
    tx_type: &'static str,
    processed_method: ProcessedMethod,
    /// Number of transactions this guard accounts for.
    /// > 1 for soft bundles.
    inflight_count: u64,
}

impl<'a> InflightDropGuard<'a> {
    pub fn acquire(
        adapter: &'a ConsensusAdapter,
        tx_type: &'static str,
        inflight_count: u64,
    ) -> Self {
        adapter
            .num_inflight_transactions
            .fetch_add(inflight_count, Ordering::SeqCst);
        adapter
            .metrics
            .sequencing_certificate_inflight
            .with_label_values(&[tx_type])
            .inc();
        adapter
            .metrics
            .sequencing_certificate_attempt
            .with_label_values(&[tx_type])
            .inc();
        Self {
            adapter,
            start: Instant::now(),
            submitted: false,
            tx_type,
            processed_method: ProcessedMethod::ConsensusMessageProcessed,
            inflight_count,
        }
    }
}

impl Drop for InflightDropGuard<'_> {
    fn drop(&mut self) {
        self.adapter
            .num_inflight_transactions
            .fetch_sub(self.inflight_count, Ordering::SeqCst);
        self.adapter
            .metrics
            .sequencing_certificate_inflight
            .with_label_values(&[self.tx_type])
            .dec();
        // Wake the admission queue drainer so it can submit more transactions.
        self.adapter.inflight_slot_freed_notify.notify_one();

        let latency = self.start.elapsed();
        let submitted = if self.submitted {
            "submitted"
        } else {
            "skipped"
        };

        self.adapter
            .metrics
            .sequencing_certificate_latency
            .with_label_values(&[
                submitted,
                self.tx_type,
                self.processed_method.metric_label(),
            ])
            .observe(latency.as_secs_f64());
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ProcessedMethod {
    ConsensusMessageProcessed,
    ConsensusStatusReceived,
    ConsensusStatusExpired,
    CheckpointExecuted,
}

impl ProcessedMethod {
    fn method_name(self) -> &'static str {
        match self {
            ProcessedMethod::ConsensusMessageProcessed => "consensus (processed message)",
            ProcessedMethod::ConsensusStatusReceived => "consensus (transaction status)",
            ProcessedMethod::ConsensusStatusExpired => "consensus (status expired)",
            ProcessedMethod::CheckpointExecuted => "checkpoint execution",
        }
    }

    fn metric_label(self) -> &'static str {
        match self {
            ProcessedMethod::ConsensusMessageProcessed => "consensus_message",
            ProcessedMethod::ConsensusStatusReceived => "consensus_status",
            ProcessedMethod::ConsensusStatusExpired => "consensus_status_expired",
            ProcessedMethod::CheckpointExecuted => "checkpoint_execution",
        }
    }
}

/// Outcome of the submit loop in `submit_and_wait_inner`.
enum SequencingOutcome {
    /// A user-transaction submission that was sequenced and whose positions all
    /// reached a terminal consensus status; nothing further to wait for.
    Sequenced(Vec<ConsensusTxStatus>),
    /// A system-message submission whose block was sequenced.
    BlockSequenced,
    /// A user-transaction submission whose block was sequenced, but at least one
    /// position expired from the status cache before its status was read. The
    /// terminal outcome existed and was missed; nothing further to wait for.
    StatusExpired,
}

fn status_label(status: ConsensusTxStatus) -> &'static str {
    match status {
        ConsensusTxStatus::Finalized => "finalized",
        ConsensusTxStatus::Rejected => "rejected",
        ConsensusTxStatus::Dropped => "dropped",
    }
}
