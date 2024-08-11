// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::{ArcSwap, ArcSwapOption};
use bytes::Bytes;
use dashmap::try_result::TryResult;
use dashmap::DashMap;
use futures::future::{select, Either};
use futures::pin_mut;
use futures::FutureExt;
use itertools::Itertools;
use narwhal_types::{TransactionProto, TransactionsClient};
use narwhal_worker::LazyNarwhalClient;
use parking_lot::RwLockReadGuard;
use prometheus::Histogram;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::IntGauge;
use prometheus::IntGaugeVec;
use prometheus::Registry;
use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry,
};
use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::error::{SuiError, SuiResult};

use tap::prelude::*;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::task::JoinHandle;
use tokio::time::{self};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::{classify, SequencedConsensusTransactionKey};
use crate::consensus_throughput_calculator::{ConsensusThroughputProfiler, Level};
use crate::epoch::reconfiguration::{ReconfigState, ReconfigurationInitiator};
use crate::metrics::LatencyObserver;
use mysten_metrics::{spawn_monitored_task, GaugeGuard, GaugeGuardFutureExt};
use sui_protocol_config::ProtocolConfig;
use sui_simulator::anemo::PeerId;
use sui_simulator::narwhal_network::connectivity::ConnectionStatus;
use sui_types::base_types::AuthorityName;
use sui_types::fp_ensure;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::messages_consensus::ConsensusTransactionKind;
use tokio::time::Duration;
use tracing::{debug, info, warn};

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

const SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 1.75, 2., 2.25, 2.5, 2.75, 3., 4., 5., 6., 7., 10., 15.,
    20., 25., 30., 60., 90., 120., 150., 180., 210., 240., 270., 300.,
];

const SEQUENCING_CERTIFICATE_POSITION_BUCKETS: &[f64] = &[
    0., 1., 2., 3., 5., 10., 15., 20., 25., 30., 50., 100., 150., 200.,
];

pub struct ConsensusAdapterMetrics {
    // Certificate sequencing metrics
    pub sequencing_certificate_attempt: IntCounterVec,
    pub sequencing_certificate_success: IntCounterVec,
    pub sequencing_certificate_failures: IntCounterVec,
    pub sequencing_certificate_inflight: IntGaugeVec,
    pub sequencing_acknowledge_latency: mysten_metrics::histogram::HistogramVec,
    pub sequencing_certificate_latency: HistogramVec,
    pub sequencing_certificate_authority_position: Histogram,
    pub sequencing_certificate_positions_moved: Histogram,
    pub sequencing_certificate_preceding_disconnected: Histogram,
    pub sequencing_in_flight_semaphore_wait: IntGauge,
    pub sequencing_in_flight_submissions: IntGauge,
    pub sequencing_estimated_latency: IntGauge,
    pub sequencing_resubmission_interval_ms: IntGauge,
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
            sequencing_certificate_inflight: register_int_gauge_vec_with_registry!(
                "sequencing_certificate_inflight",
                "The inflight requests to sequence certificates.",
                &["tx_type"],
                registry,
            )
                .unwrap(),
            sequencing_acknowledge_latency: mysten_metrics::histogram::HistogramVec::new_in_registry(
                "sequencing_acknowledge_latency",
                "The latency for acknowledgement from sequencing engine. The overall sequencing latency is measured by the sequencing_certificate_latency metric",
                &["retry", "tx_type"],
                registry,
            ),
            sequencing_certificate_latency: register_histogram_vec_with_registry!(
                "sequencing_certificate_latency",
                "The latency for sequencing a certificate.",
                &["position", "tx_type"],
                SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_authority_position: register_histogram_with_registry!(
                "sequencing_certificate_authority_position",
                "The position of the authority when submitted a certificate to consensus.",
                SEQUENCING_CERTIFICATE_POSITION_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_positions_moved: register_histogram_with_registry!(
                "sequencing_certificate_positions_moved",
                "The number of authorities ahead of ourselves that were filtered out when submitting a certificate to consensus.",
                SEQUENCING_CERTIFICATE_POSITION_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            sequencing_certificate_preceding_disconnected: register_histogram_with_registry!(
                "sequencing_certificate_preceding_disconnected",
                "The number of authorities that were hashed to an earlier position that were filtered out due to being disconnected when submitting to consensus.",
                SEQUENCING_CERTIFICATE_POSITION_BUCKETS.to_vec(),
                registry,
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
            sequencing_estimated_latency: register_int_gauge_with_registry!(
                "sequencing_estimated_latency",
                "Consensus latency estimated by consensus adapter in milliseconds",
                registry,
            )
                .unwrap(),
            sequencing_resubmission_interval_ms: register_int_gauge_with_registry!(
                "sequencing_resubmission_interval_ms",
                "Resubmission interval used by consensus adapter in milliseconds",
                registry,
            )
                .unwrap(),
        }
    }

    pub fn new_test() -> Self {
        Self::new(&Registry::default())
    }
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait SubmitToConsensus: Sync + Send + 'static {
    async fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult;
}

#[async_trait::async_trait]
impl SubmitToConsensus for TransactionsClient<sui_network::tonic::transport::Channel> {
    async fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let transactions_bytes = transactions
            .iter()
            .map(|t| {
                let serialized =
                    bcs::to_bytes(t).expect("Serializing consensus transaction cannot fail");
                Bytes::from(serialized)
            })
            .collect::<Vec<_>>();
        self.clone()
            .submit_transaction(TransactionProto {
                transactions: transactions_bytes,
            })
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for LazyNarwhalClient {
    async fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let transactions = transactions
            .iter()
            .map(|t| bcs::to_bytes(t).expect("Serializing consensus transaction cannot fail"))
            .collect::<Vec<_>>();
        // The retrieved LocalNarwhalClient can be from the past epoch. Submit would fail after
        // Narwhal shuts down, so there should be no correctness issue.
        let client = {
            let c = self.client.load();
            if c.is_some() {
                c
            } else {
                self.client.store(Some(self.get().await));
                self.client.load()
            }
        };
        let client = client.as_ref().unwrap().load();
        client
            .submit_transactions(transactions)
            .await
            .map_err(|e| SuiError::FailedToSubmitToConsensus(format!("{:?}", e)))
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: Arc<dyn SubmitToConsensus>,
    /// Authority pubkey.
    authority: AuthorityName,
    /// The limit to number of inflight transactions at this node.
    max_pending_transactions: usize,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// Dictates the maximum position  from which will submit to consensus. Even if the is elected to
    /// submit from a higher position than this, it will "reset" to the max_submit_position.
    max_submit_position: Option<usize>,
    /// When provided it will override the current back off logic and will use this value instead
    /// as delay step.
    submit_delay_step_override: Option<Duration>,
    /// A structure to check the connection statuses populated by the Connection Monitor Listener
    connection_monitor_status: Arc<dyn CheckConnection>,
    /// A structure to check the reputation scores populated by Consensus
    low_scoring_authorities: ArcSwap<Arc<ArcSwap<HashMap<AuthorityName, u64>>>>,
    /// The throughput profiler to be used when making decisions to submit to consensus
    consensus_throughput_profiler: ArcSwapOption<ConsensusThroughputProfiler>,
    /// A structure to register metrics
    metrics: ConsensusAdapterMetrics,
    /// Semaphore limiting parallel submissions to narwhal
    submit_semaphore: Semaphore,
    latency_observer: LatencyObserver,
    protocol_config: ProtocolConfig,
}

pub trait CheckConnection: Send + Sync {
    fn check_connection(
        &self,
        ourself: &AuthorityName,
        authority: &AuthorityName,
    ) -> Option<ConnectionStatus>;
    fn update_mapping_for_epoch(&self, authority_names_to_peer_ids: HashMap<AuthorityName, PeerId>);
}

pub struct ConnectionMonitorStatus {
    /// Current connection statuses forwarded from the connection monitor
    pub connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
    /// A map from authority name to peer id
    pub authority_names_to_peer_ids: ArcSwap<HashMap<AuthorityName, PeerId>>,
}

pub struct ConnectionMonitorStatusForTests {}

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_client: Arc<dyn SubmitToConsensus>,
        authority: AuthorityName,
        connection_monitor_status: Arc<dyn CheckConnection>,
        max_pending_transactions: usize,
        max_pending_local_submissions: usize,
        max_submit_position: Option<usize>,
        submit_delay_step_override: Option<Duration>,
        metrics: ConsensusAdapterMetrics,
        protocol_config: ProtocolConfig,
    ) -> Self {
        let num_inflight_transactions = Default::default();
        let low_scoring_authorities =
            ArcSwap::from_pointee(Arc::new(ArcSwap::from_pointee(HashMap::new())));
        Self {
            consensus_client,
            authority,
            max_pending_transactions,
            max_submit_position,
            submit_delay_step_override,
            num_inflight_transactions,
            connection_monitor_status,
            low_scoring_authorities,
            metrics,
            submit_semaphore: Semaphore::new(max_pending_local_submissions),
            latency_observer: LatencyObserver::new(),
            consensus_throughput_profiler: ArcSwapOption::empty(),
            protocol_config,
        }
    }

    pub fn swap_low_scoring_authorities(
        &self,
        new_low_scoring: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    ) {
        self.low_scoring_authorities.swap(Arc::new(new_low_scoring));
    }

    pub fn swap_throughput_profiler(&self, profiler: Arc<ConsensusThroughputProfiler>) {
        self.consensus_throughput_profiler.store(Some(profiler))
    }

    // todo - this probably need to hold some kind of lock to make sure epoch does not change while we are recovering
    pub fn submit_recovered(self: &Arc<Self>, epoch_store: &Arc<AuthorityPerEpochStore>) {
        // Currently narwhal worker might lose transactions on restart, so we need to resend them
        // todo - get_all_pending_consensus_transactions is called twice when
        // initializing AuthorityPerEpochStore and here, should not be a big deal but can be optimized
        let mut recovered = epoch_store.get_all_pending_consensus_transactions();

        #[allow(clippy::collapsible_if)] // This if can be collapsed but it will be ugly
        if epoch_store
            .get_reconfig_state_read_lock_guard()
            .is_reject_user_certs()
            && epoch_store.pending_consensus_certificates_empty()
        {
            if recovered
                .iter()
                .any(ConsensusTransaction::is_end_of_publish)
            {
                // There are two cases when this is needed
                // (1) We send EndOfPublish message after removing pending certificates in submit_and_wait_inner
                // It is possible that node will crash between those two steps, in which case we might need to
                // re-introduce EndOfPublish message on restart
                // (2) If node crashed inside ConsensusAdapter::close_epoch,
                // after reconfig lock state was written to DB and before we persisted EndOfPublish message
                recovered.push(ConsensusTransaction::new_end_of_publish(self.authority));
            }
        }
        debug!(
            "Submitting {:?} recovered pending consensus transactions to Narwhal",
            recovered.len()
        );
        for transaction in recovered {
            if transaction.is_end_of_publish() {
                info!(epoch=?epoch_store.epoch(), "Submitting EndOfPublish message to consensus");
            }
            self.submit_unchecked(&[transaction], epoch_store);
        }
    }

    fn await_submit_delay(
        &self,
        committee: &Committee,
        transactions: &[ConsensusTransaction],
    ) -> (impl Future<Output = ()>, usize, usize, usize) {
        // Use the minimum digest to compute submit delay.
        let min_digest = transactions
            .iter()
            .filter_map(|tx| match &tx.kind {
                ConsensusTransactionKind::UserTransaction(certificate) => {
                    Some(certificate.digest())
                }
                _ => None,
            })
            .min();

        let (duration, position, positions_moved, preceding_disconnected) = match min_digest {
            Some(digest) => self.await_submit_delay_user_transaction(committee, digest),
            _ => (Duration::ZERO, 0, 0, 0),
        };
        (
            tokio::time::sleep(duration),
            position,
            positions_moved,
            preceding_disconnected,
        )
    }

    fn await_submit_delay_user_transaction(
        &self,
        committee: &Committee,
        tx_digest: &TransactionDigest,
    ) -> (Duration, usize, usize, usize) {
        let (position, positions_moved, preceding_disconnected) =
            self.submission_position(committee, tx_digest);

        const DEFAULT_LATENCY: Duration = Duration::from_secs(1); // > p50 consensus latency with global deployment
        const MIN_LATENCY: Duration = Duration::from_millis(150);
        const MAX_LATENCY: Duration = Duration::from_secs(3);

        let latency = self.latency_observer.latency().unwrap_or(DEFAULT_LATENCY);
        self.metrics
            .sequencing_estimated_latency
            .set(latency.as_millis() as i64);

        let latency = std::cmp::max(latency, MIN_LATENCY);
        let latency = std::cmp::min(latency, MAX_LATENCY);
        let latency = latency * 2;
        let latency = self.override_by_throughput_profiler(position, latency);
        let (delay_step, position) =
            self.override_by_max_submit_position_settings(latency, position);

        self.metrics
            .sequencing_resubmission_interval_ms
            .set(delay_step.as_millis() as i64);

        (
            delay_step * position as u32,
            position,
            positions_moved,
            preceding_disconnected,
        )
    }

    // According to the throughput profile we want to either allow some transaction duplication or not)
    // When throughput profile is Low and the validator is in position = 1, then it will submit to consensus with much lower latency.
    // When throughput profile is High then we go back to default operation and no-one co-submits.
    fn override_by_throughput_profiler(&self, position: usize, latency: Duration) -> Duration {
        const LOW_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS: u64 = 0;
        const MEDIUM_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS: u64 = 2_500;
        const HIGH_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS: u64 = 3_500;

        let p = self.consensus_throughput_profiler.load();

        if let Some(profiler) = p.as_ref() {
            let (level, _) = profiler.throughput_level();

            // we only run this for the position = 1 validator to co-submit with the validator of
            // position = 0. We also enable this only when the feature is enabled on the protocol config.
            if self.protocol_config.throughput_aware_consensus_submission() && position == 1 {
                return match level {
                    Level::Low => Duration::from_millis(LOW_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS),
                    Level::Medium => {
                        Duration::from_millis(MEDIUM_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS)
                    }
                    Level::High => {
                        let l = Duration::from_millis(HIGH_THROUGHPUT_DELAY_BEFORE_SUBMIT_MS);

                        // back off according to recorded latency if it's significantly higher
                        if latency >= 2 * l {
                            latency
                        } else {
                            l
                        }
                    }
                };
            }
        }
        latency
    }

    /// Overrides the latency and the position if there are defined settings for `max_submit_position` and
    /// `submit_delay_step_override`. If the `max_submit_position` has defined, then that will always be used
    /// irrespective of any so far decision. Same for the `submit_delay_step_override`.
    fn override_by_max_submit_position_settings(
        &self,
        latency: Duration,
        mut position: usize,
    ) -> (Duration, usize) {
        // Respect any manual override for position and latency from the settings
        if let Some(max_submit_position) = self.max_submit_position {
            position = std::cmp::min(position, max_submit_position);
        }

        let delay_step = self.submit_delay_step_override.unwrap_or(latency);
        (delay_step, position)
    }

    /// Check when this authority should submit the certificate to consensus.
    /// This sorts all authorities based on pseudo-random distribution derived from transaction hash.
    ///
    /// The function targets having 1 consensus transaction submitted per user transaction
    /// when system operates normally.
    ///
    /// The function returns the position of this authority when it is their turn to submit the transaction to consensus.
    fn submission_position(
        &self,
        committee: &Committee,
        tx_digest: &TransactionDigest,
    ) -> (usize, usize, usize) {
        let positions = committee.shuffle_by_stake_from_tx_digest(tx_digest);

        self.check_submission_wrt_connectivity_and_scores(positions)
    }

    /// This function runs the following algorithm to decide whether or not to submit a transaction
    /// to consensus.
    ///
    /// It takes in a deterministic list that represents positions of all the authorities.
    /// The authority in the first position will be responsible for submitting to consensus, and
    /// so we check if we are this validator, and if so, return true.
    ///
    /// If we are not in that position, we check our connectivity to the authority in that position.
    /// If we are connected to them, we can assume that they are operational and will submit the transaction.
    /// If we are not connected to them, we assume that they are not operational and we will not rely
    /// on that authority to submit the transaction. So we shift them out of the first position, and
    /// run this algorithm again on the new set of positions.
    ///
    /// This can possibly result in a transaction being submitted twice if an authority sees a false
    /// negative in connectivity to another, such as in the case of a network partition.
    ///
    /// Recursively, if the authority further ahead of us in the positions is a low performing authority, we will
    /// move our positions up one, and submit the transaction. This allows maintaining performance
    /// overall. We will only do this part for authorities that are not low performers themselves to
    /// prevent extra amplification in the case that the positions look like [low_scoring_a1, low_scoring_a2, a3]
    fn check_submission_wrt_connectivity_and_scores(
        &self,
        positions: Vec<AuthorityName>,
    ) -> (usize, usize, usize) {
        let low_scoring_authorities = self.low_scoring_authorities.load().load_full();
        if low_scoring_authorities.get(&self.authority).is_some() {
            return (positions.len(), 0, 0);
        }
        let initial_position = get_position_in_list(self.authority, positions.clone());
        let mut preceding_disconnected = 0;
        let mut before_our_position = true;

        let filtered_positions: Vec<_> = positions
            .into_iter()
            .filter(|authority| {
                let keep = self.authority == *authority; // don't filter ourself out
                if keep {
                    before_our_position = false;
                }

                // filter out any nodes that appear disconnected
                let connected = self
                    .connection_monitor_status
                    .check_connection(&self.authority, authority)
                    .unwrap_or(ConnectionStatus::Disconnected)
                    == ConnectionStatus::Connected;
                if !connected && before_our_position {
                    preceding_disconnected += 1; // used for metrics
                }

                // Filter out low scoring nodes
                let high_scoring = low_scoring_authorities.get(authority).is_none();

                keep || (connected && high_scoring)
            })
            .collect();

        let position = get_position_in_list(self.authority, filtered_positions);

        (
            position,
            initial_position - position,
            preceding_disconnected,
        )
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
    ) -> SuiResult<JoinHandle<()>> {
        self.submit_batch(&[transaction], lock, epoch_store)
    }

    pub fn submit_batch(
        self: &Arc<Self>,
        transactions: &[ConsensusTransaction],
        lock: Option<&RwLockReadGuard<ReconfigState>>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<JoinHandle<()>> {
        if transactions.len() > 1 {
            // In soft bundle, we need to check if all transactions are of UserTransaction kind.
            // The check is required because we assume this in submit_and_wait_inner.
            for transaction in transactions {
                fp_ensure!(
                    matches!(
                        transaction.kind,
                        ConsensusTransactionKind::UserTransaction(_)
                    ),
                    SuiError::InvalidTxKindInSoftBundle
                );
            }
        }

        epoch_store.insert_pending_consensus_transactions(transactions, lock)?;
        Ok(self.submit_unchecked(transactions, epoch_store))
    }

    /// Performs weakly consistent checks on internal buffers to quickly
    /// discard transactions if we are overloaded
    pub fn check_limits(&self) -> bool {
        // First check total transactions (waiting and in submission)
        if self.num_inflight_transactions.load(Ordering::Relaxed) as usize
            > self.max_pending_transactions
        {
            return false;
        }
        // Then check if submit_semaphore has permits
        self.submit_semaphore.available_permits() > 0
    }

    pub(crate) fn check_consensus_overload(&self) -> SuiResult {
        fp_ensure!(
            self.check_limits(),
            SuiError::TooManyTransactionsPendingConsensus
        );
        Ok(())
    }

    fn submit_unchecked(
        self: &Arc<Self>,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> JoinHandle<()> {
        // Reconfiguration lock is dropped when pending_consensus_transactions is persisted, before it is handled by consensus
        let async_stage = self
            .clone()
            .submit_and_wait(transactions.to_vec(), epoch_store.clone());
        // Number of these tasks is weakly limited based on `num_inflight_transactions`.
        // (Limit is not applied atomically, and only to user transactions.)
        let join_handle = spawn_monitored_task!(async_stage);
        join_handle
    }

    async fn submit_and_wait(
        self: Arc<Self>,
        transactions: Vec<ConsensusTransaction>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        // When epoch_terminated signal is received all pending submit_and_wait_inner are dropped.
        //
        // This is needed because submit_and_wait_inner waits on read_notify for consensus message to be processed,
        // which may never happen on epoch boundary.
        //
        // In addition to that, within_alive_epoch ensures that all pending consensus
        // adapter tasks are stopped before reconfiguration can proceed.
        //
        // This is essential because narwhal workers reuse same ports when narwhal restarts,
        // this means we might be sending transactions from previous epochs to narwhal of
        // new epoch if we have not had this barrier.
        epoch_store
            .within_alive_epoch(self.submit_and_wait_inner(transactions, &epoch_store))
            .await
            .ok(); // result here indicates if epoch ended earlier, we don't care about it
    }

    #[allow(clippy::option_map_unit_fn)]
    async fn submit_and_wait_inner(
        self: Arc<Self>,
        transactions: Vec<ConsensusTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if transactions.is_empty() {
            return;
        }

        // Current code path ensures:
        // - If transactions.len() > 1, it is a soft bundle. Otherwise transactions should have been submitted individually.
        // - If is_soft_bundle, then all transactions are of UserTransaction kind.
        // - If not is_soft_bundle, then transactions must contain exactly 1 tx, and transactions[0] can be of any kind.
        let is_soft_bundle = transactions.len() > 1;

        let mut transaction_keys = Vec::new();

        for transaction in &transactions {
            if matches!(transaction.kind, ConsensusTransactionKind::EndOfPublish(..)) {
                info!(epoch=?epoch_store.epoch(), "Submitting EndOfPublish message to consensus");
                epoch_store.record_epoch_pending_certs_process_time_metric();
            }

            let transaction_key = SequencedConsensusTransactionKey::External(transaction.key());
            transaction_keys.push(transaction_key);
        }
        let tx_type = if !is_soft_bundle {
            classify(&transactions[0])
        } else {
            "soft_bundle"
        };

        let processed_waiter = epoch_store
            .consensus_messages_processed_notify(transaction_keys.clone())
            .boxed();

        pin_mut!(processed_waiter);

        let (await_submit, position, positions_moved, preceding_disconnected) =
            self.await_submit_delay(epoch_store.committee(), &transactions[..]);

        let mut guard = InflightDropGuard::acquire(&self, tx_type);
        let processed_waiter = tokio::select! {
            // We need to wait for some delay until we submit transaction to the consensus
            _ = await_submit => Some(processed_waiter),

            // If epoch ends, don't wait for submit delay
            _ = epoch_store.user_certs_closed_notify() => {
                warn!(epoch = ?epoch_store.epoch(), "Epoch ended, skipping submission delay");
                Some(processed_waiter)
            }

            // If transaction is received by consensus while we wait, we are done.
            processed = &mut processed_waiter => {
                processed.expect("Storage error when waiting for consensus message processed");
                None
            }
        };

        // Log warnings for administrative transactions that fail to get sequenced
        let _monitor = if !is_soft_bundle
            && matches!(
                transactions[0].kind,
                ConsensusTransactionKind::EndOfPublish(_)
                    | ConsensusTransactionKind::CapabilityNotification(_)
                    | ConsensusTransactionKind::CapabilityNotificationV2(_)
                    | ConsensusTransactionKind::RandomnessDkgMessage(_, _)
                    | ConsensusTransactionKind::RandomnessDkgConfirmation(_, _)
            ) {
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
        if let Some(processed_waiter) = processed_waiter {
            debug!("Submitting {:?} to consensus", transaction_keys);

            // populate the position only when this authority submits the transaction
            // to consensus
            guard.position = Some(position);
            guard.positions_moved = Some(positions_moved);
            guard.preceding_disconnected = Some(preceding_disconnected);

            let _permit: SemaphorePermit = self
                .submit_semaphore
                .acquire()
                .count_in_flight(&self.metrics.sequencing_in_flight_semaphore_wait)
                .await
                .expect("Consensus adapter does not close semaphore");
            let _in_flight_submission_guard =
                GaugeGuard::acquire(&self.metrics.sequencing_in_flight_submissions);

            // We enter this branch when in select above await_submit completed and processed_waiter is pending
            // This means it is time for us to submit transaction to consensus
            let submit_inner = async {
                let ack_start = Instant::now();
                let mut retries: u32 = 0;
                while let Err(e) = self
                    .consensus_client
                    .submit_to_consensus(&transactions[..], epoch_store)
                    .await
                {
                    // This can happen during reconfig, or when consensus has full internal buffers
                    // and needs to back pressure, so retry a few times before logging warnings.
                    if retries > 30
                        || (retries > 3 && (is_soft_bundle || !transactions[0].kind.is_dkg()))
                    {
                        warn!(
                            "Failed to submit transactions {transaction_keys:?} to consensus: {e:?}. Retry #{retries}"
                        );
                    }
                    self.metrics
                        .sequencing_certificate_failures
                        .with_label_values(&[tx_type])
                        .inc();
                    retries += 1;

                    if !is_soft_bundle && transactions[0].kind.is_dkg() {
                        // Shorter delay for DKG messages, which are time-sensitive and happen at
                        // start-of-epoch when submit errors due to active reconfig are likely.
                        time::sleep(Duration::from_millis(100)).await;
                    } else {
                        time::sleep(Duration::from_secs(10)).await;
                    };
                }

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
                    .with_label_values(&[&bucket, tx_type])
                    .report(ack_start.elapsed().as_millis() as u64);
            };
            match select(processed_waiter, submit_inner.boxed()).await {
                Either::Left((processed, _submit_inner)) => processed,
                Either::Right(((), processed_waiter)) => {
                    debug!("Submitted {transaction_keys:?} to consensus");
                    processed_waiter.await
                }
            }
            .expect("Storage error when waiting for consensus message processed");
        }
        debug!("{transaction_keys:?} processed by consensus");

        let consensus_keys: Vec<_> = transactions.iter().map(|t| t.key()).collect();
        epoch_store
            .remove_pending_consensus_transactions(&consensus_keys)
            .expect("Storage error when removing consensus transaction");

        let is_user_tx = is_soft_bundle
            || matches!(
                transactions[0].kind,
                ConsensusTransactionKind::UserTransaction(_)
            );
        let send_end_of_publish = if is_user_tx {
            // If we are in RejectUserCerts state and we just drained the list we need to
            // send EndOfPublish to signal other validators that we are not submitting more certificates to the epoch.
            // Note that there could be a race condition here where we enter this check in RejectAllCerts state.
            // In that case we don't need to send EndOfPublish because condition to enter
            // RejectAllCerts is when 2f+1 other validators already sequenced their EndOfPublish message.
            // Also note that we could sent multiple EndOfPublish due to that multiple tasks can enter here with
            // pending_count == 0. This doesn't affect correctness.
            if epoch_store
                .get_reconfig_state_read_lock_guard()
                .is_reject_user_certs()
            {
                let pending_count = epoch_store.pending_consensus_certificates_count();
                debug!(epoch=?epoch_store.epoch(), ?pending_count, "Deciding whether to send EndOfPublish");
                pending_count == 0 // send end of epoch if empty
            } else {
                false
            }
        } else {
            false
        };
        if send_end_of_publish {
            // sending message outside of any locks scope
            info!(epoch=?epoch_store.epoch(), "Sending EndOfPublish message to consensus");
            if let Err(err) = self.submit(
                ConsensusTransaction::new_end_of_publish(self.authority),
                None,
                epoch_store,
            ) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
        self.metrics
            .sequencing_certificate_success
            .with_label_values(&[tx_type])
            .inc();
    }
}

impl CheckConnection for ConnectionMonitorStatus {
    fn check_connection(
        &self,
        ourself: &AuthorityName,
        authority: &AuthorityName,
    ) -> Option<ConnectionStatus> {
        if ourself == authority {
            return Some(ConnectionStatus::Connected);
        }

        let mapping = self.authority_names_to_peer_ids.load_full();
        let peer_id = match mapping.get(authority) {
            Some(p) => p,
            None => {
                warn!(
                    "failed to find peer {:?} in connection monitor listener",
                    authority
                );
                return None;
            }
        };

        let res = match self.connection_statuses.try_get(peer_id) {
            TryResult::Present(c) => Some(c.value().clone()),
            TryResult::Absent => None,
            TryResult::Locked => {
                // update is in progress, assume the status is still or becoming disconnected
                Some(ConnectionStatus::Disconnected)
            }
        };
        res
    }
    fn update_mapping_for_epoch(
        &self,
        authority_names_to_peer_ids: HashMap<AuthorityName, PeerId>,
    ) {
        self.authority_names_to_peer_ids
            .swap(Arc::new(authority_names_to_peer_ids));
    }
}

impl CheckConnection for ConnectionMonitorStatusForTests {
    fn check_connection(
        &self,
        _ourself: &AuthorityName,
        _authority: &AuthorityName,
    ) -> Option<ConnectionStatus> {
        Some(ConnectionStatus::Connected)
    }
    fn update_mapping_for_epoch(
        &self,
        _authority_names_to_peer_ids: HashMap<AuthorityName, PeerId>,
    ) {
    }
}

pub fn get_position_in_list(
    search_authority: AuthorityName,
    positions: Vec<AuthorityName>,
) -> usize {
    positions
        .into_iter()
        .find_position(|authority| *authority == search_authority)
        .expect("Couldn't find ourselves in shuffled committee")
        .0
}

impl ReconfigurationInitiator for Arc<ConsensusAdapter> {
    /// This method is called externally to begin reconfiguration
    /// It transition reconfig state to reject new certificates from user
    /// ConsensusAdapter will send EndOfPublish message once pending certificate queue is drained.
    fn close_epoch(&self, epoch_store: &Arc<AuthorityPerEpochStore>) {
        let send_end_of_publish = {
            let reconfig_guard = epoch_store.get_reconfig_state_write_lock_guard();
            if !reconfig_guard.should_accept_user_certs() {
                // Allow caller to call this method multiple times
                return;
            }
            let pending_count = epoch_store.pending_consensus_certificates_count();
            debug!(epoch=?epoch_store.epoch(), ?pending_count, "Trying to close epoch");
            let send_end_of_publish = pending_count == 0;
            epoch_store.close_user_certs(reconfig_guard);
            send_end_of_publish
            // reconfig_guard lock is dropped here.
        };
        if send_end_of_publish {
            info!(epoch=?epoch_store.epoch(), "Sending EndOfPublish message to consensus");
            if let Err(err) = self.submit(
                ConsensusTransaction::new_end_of_publish(self.authority),
                None,
                epoch_store,
            ) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
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
    position: Option<usize>,
    positions_moved: Option<usize>,
    preceding_disconnected: Option<usize>,
    tx_type: &'static str,
}

impl<'a> InflightDropGuard<'a> {
    pub fn acquire(adapter: &'a ConsensusAdapter, tx_type: &'static str) -> Self {
        adapter
            .num_inflight_transactions
            .fetch_add(1, Ordering::SeqCst);
        adapter
            .metrics
            .sequencing_certificate_inflight
            .with_label_values(&[&tx_type])
            .inc();
        adapter
            .metrics
            .sequencing_certificate_attempt
            .with_label_values(&[&tx_type])
            .inc();
        Self {
            adapter,
            start: Instant::now(),
            position: None,
            positions_moved: None,
            preceding_disconnected: None,
            tx_type,
        }
    }
}

impl<'a> Drop for InflightDropGuard<'a> {
    fn drop(&mut self) {
        self.adapter
            .num_inflight_transactions
            .fetch_sub(1, Ordering::SeqCst);
        self.adapter
            .metrics
            .sequencing_certificate_inflight
            .with_label_values(&[self.tx_type])
            .dec();

        let position = if let Some(position) = self.position {
            self.adapter
                .metrics
                .sequencing_certificate_authority_position
                .observe(position as f64);
            position.to_string()
        } else {
            "not_submitted".to_string()
        };

        if let Some(positions_moved) = self.positions_moved {
            self.adapter
                .metrics
                .sequencing_certificate_positions_moved
                .observe(positions_moved as f64);
        };

        if let Some(preceding_disconnected) = self.preceding_disconnected {
            self.adapter
                .metrics
                .sequencing_certificate_preceding_disconnected
                .observe(preceding_disconnected as f64);
        };

        let latency = self.start.elapsed();
        self.adapter
            .metrics
            .sequencing_certificate_latency
            .with_label_values(&[&position, self.tx_type])
            .observe(latency.as_secs_f64());

        // Only sample latency after consensus quorum is up. Otherwise, the wait for consensus
        // quorum at the beginning of an epoch can distort the sampled latencies.
        // Technically there are more system transaction types that can be included in samples
        // after the first consensus commit, but this set of types should be enough.
        if self.position == Some(0) {
            // Transaction types below require quorum existed in the current epoch.
            // TODO: refactor tx_type to enum.
            let sampled = matches!(
                self.tx_type,
                "shared_certificate" | "owned_certificate" | "checkpoint_signature" | "soft_bundle"
            );
            if sampled {
                self.adapter.latency_observer.report(latency);
            }
        }
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for Arc<ConsensusAdapter> {
    async fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.submit_batch(transactions, None, epoch_store)
            .map(|_| ())
    }
}

pub fn position_submit_certificate(
    committee: &Committee,
    ourselves: &AuthorityName,
    tx_digest: &TransactionDigest,
) -> usize {
    let validators = committee.shuffle_by_stake_from_tx_digest(tx_digest);
    get_position_in_list(*ourselves, validators)
}

#[cfg(test)]
mod adapter_tests {
    use super::position_submit_certificate;
    use crate::consensus_adapter::{
        ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
        LazyNarwhalClient,
    };
    use fastcrypto::traits::KeyPair;
    use rand::Rng;
    use rand::{rngs::StdRng, SeedableRng};
    use std::sync::Arc;
    use std::time::Duration;
    use sui_types::{
        base_types::TransactionDigest,
        committee::Committee,
        crypto::{get_key_pair_from_rng, AuthorityKeyPair, AuthorityPublicKeyBytes},
    };

    fn test_committee(rng: &mut StdRng, size: usize) -> Committee {
        let authorities = (0..size)
            .map(|_k| {
                (
                    AuthorityPublicKeyBytes::from(
                        get_key_pair_from_rng::<AuthorityKeyPair, _>(rng).1.public(),
                    ),
                    rng.gen_range(0u64..10u64),
                )
            })
            .collect::<Vec<_>>();
        Committee::new_for_testing_with_normalized_voting_power(
            0,
            authorities.iter().cloned().collect(),
        )
    }

    #[tokio::test]
    async fn test_await_submit_delay_user_transaction() {
        // grab a random committee and a random stake distribution
        let mut rng = StdRng::from_seed([0; 32]);
        let committee = test_committee(&mut rng, 10);

        // When we define max submit position and delay step
        let consensus_adapter = ConsensusAdapter::new(
            Arc::new(LazyNarwhalClient::new(
                "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
            )),
            *committee.authority_by_index(0).unwrap(),
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            Some(1),
            Some(Duration::from_secs(2)),
            ConsensusAdapterMetrics::new_test(),
            sui_protocol_config::ProtocolConfig::get_for_max_version_UNSAFE(),
        );

        // transaction to submit
        let tx_digest = TransactionDigest::generate(&mut rng);

        // Ensure that the original position is higher
        let (position, positions_moved, _) =
            consensus_adapter.submission_position(&committee, &tx_digest);
        assert_eq!(position, 7);
        assert!(!positions_moved > 0);

        // Make sure that position is set to max value 0
        let (delay_step, position, positions_moved, _) =
            consensus_adapter.await_submit_delay_user_transaction(&committee, &tx_digest);

        assert_eq!(position, 1);
        assert_eq!(delay_step, Duration::from_secs(2));
        assert!(!positions_moved > 0);

        // Without submit position and delay step
        let consensus_adapter = ConsensusAdapter::new(
            Arc::new(LazyNarwhalClient::new(
                "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
            )),
            *committee.authority_by_index(0).unwrap(),
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            sui_protocol_config::ProtocolConfig::get_for_max_version_UNSAFE(),
        );

        let (delay_step, position, positions_moved, _) =
            consensus_adapter.await_submit_delay_user_transaction(&committee, &tx_digest);

        assert_eq!(position, 7);

        // delay_step * position * 2 = 1 * 7 * 2 = 14
        assert_eq!(delay_step, Duration::from_secs(14));
        assert!(!positions_moved > 0);
    }

    #[test]
    fn test_position_submit_certificate() {
        // grab a random committee and a random stake distribution
        let mut rng = StdRng::from_seed([0; 32]);
        let committee = test_committee(&mut rng, 10);

        // generate random transaction digests, and account for validator selection
        const NUM_TEST_TRANSACTIONS: usize = 1000;

        for _tx_idx in 0..NUM_TEST_TRANSACTIONS {
            let tx_digest = TransactionDigest::generate(&mut rng);

            let mut zero_found = false;
            for (name, _) in committee.members() {
                let f = position_submit_certificate(&committee, name, &tx_digest);
                assert!(f < committee.num_members());
                if f == 0 {
                    // One and only one validator gets position 0
                    assert!(!zero_found);
                    zero_found = true;
                }
            }
            assert!(zero_found);
        }
    }
}
