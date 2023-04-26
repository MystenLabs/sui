// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::{ArcSwap, ArcSwapOption};
use bytes::Bytes;
use dashmap::try_result::TryResult;
use dashmap::DashMap;
use futures::future::{select, Either};
use futures::FutureExt;
use itertools::Itertools;
use mysten_network::Multiaddr;
use narwhal_types::{TransactionProto, TransactionsClient};
use narwhal_worker::LocalNarwhalClient;
use parking_lot::{Mutex, RwLockReadGuard};
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::{
    linear_buckets, register_histogram_with_registry, register_int_counter_with_registry, Histogram,
};
use prometheus::{register_histogram_vec_with_registry, register_int_gauge_with_registry};
use prometheus::{HistogramVec, IntCounter};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::ops::Deref;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::{
    error::{SuiError, SuiResult},
    messages::ConsensusTransaction,
};

use tap::prelude::*;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::task::JoinHandle;
use tokio::time::{self, sleep, timeout};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::epoch::reconfiguration::{ReconfigState, ReconfigurationInitiator};
use mysten_metrics::{spawn_monitored_task, GaugeGuard, GaugeGuardFutureExt};
use sui_simulator::anemo::PeerId;
use sui_simulator::narwhal_network::connectivity::ConnectionStatus;
use sui_types::base_types::AuthorityName;
use sui_types::messages::ConsensusTransactionKind;
use tokio::time::Duration;
use tracing::{debug, info, warn};

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

const SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 1., 2.5, 5., 7.5, 10., 12.5, 15., 20., 25., 30., 60., 90., 120., 180., 300.,
    600.,
];

pub struct ConsensusAdapterMetrics {
    // Certificate sequencing metrics
    pub sequencing_certificate_attempt: IntCounter,
    pub sequencing_certificate_success: IntCounter,
    pub sequencing_certificate_failures: IntCounter,
    pub sequencing_certificate_inflight: IntGauge,
    pub sequencing_acknowledge_latency: HistogramVec,
    pub sequencing_certificate_latency: HistogramVec,
    pub sequencing_certificate_authority_position: Histogram,
    pub sequencing_in_flight_semaphore_wait: IntGauge,
    pub sequencing_in_flight_submissions: IntGauge,
    pub sequencing_estimated_latency: IntGauge,
}

impl ConsensusAdapterMetrics {
    pub fn new(registry: &Registry) -> Self {
        let authority_position_buckets = &[
            linear_buckets(0.0, 1.0, 19).unwrap().as_slice(),
            linear_buckets(20.0, 5.0, 10).unwrap().as_slice(),
        ]
        .concat();

        Self {
            sequencing_certificate_attempt: register_int_counter_with_registry!(
                "sequencing_certificate_attempt",
                "Counts the number of certificates the validator attempts to sequence.",
                registry,
            )
                .unwrap(),
            sequencing_certificate_success: register_int_counter_with_registry!(
                "sequencing_certificate_success",
                "Counts the number of successfully sequenced certificates.",
                registry,
            )
                .unwrap(),
            sequencing_certificate_failures: register_int_counter_with_registry!(
                "sequencing_certificate_failures",
                "Counts the number of sequenced certificates that failed other than by timeout.",
                registry,
            )
                .unwrap(),
            sequencing_certificate_inflight: register_int_gauge_with_registry!(
                "sequencing_certificate_inflight",
                "The inflight requests to sequence certificates.",
                registry,
            )
                .unwrap(),
            sequencing_acknowledge_latency: register_histogram_vec_with_registry!(
                "sequencing_acknowledge_latency",
                "The latency for acknowledgement from sequencing engine. The overall sequencing latency is measured by the sequencing_certificate_latency metric",
                &["retry"],
                SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
                .unwrap(),
            sequencing_certificate_latency: register_histogram_vec_with_registry!(
                "sequencing_certificate_latency",
                "The latency for sequencing a certificate.",
                &["position", "mapped_to_low_scoring"],
                SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
                .unwrap(),
            sequencing_certificate_authority_position: register_histogram_with_registry!(
                "sequencing_certificate_authority_position",
                "The position of the authority when submitted a certificate to consensus.",
                authority_position_buckets.to_vec(),
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
                "Number of transactions submitted to local narwhal instance and not yet sequenced",
                registry,
            )
                .unwrap(),
            sequencing_estimated_latency: register_int_gauge_with_registry!(
                "sequencing_estimated_latency",
                "Consensus latency estimated by consensus adapter",
                registry,
            )
                .unwrap(),
        }
    }

    pub fn new_test() -> Self {
        Self::new(&Registry::default())
    }
}

#[async_trait::async_trait]
pub trait SubmitToConsensus: Sync + Send + 'static {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult;
}

#[async_trait::async_trait]
impl SubmitToConsensus for TransactionsClient<sui_network::tonic::transport::Channel> {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let serialized =
            bcs::to_bytes(transaction).expect("Serializing consensus transaction cannot fail");
        let bytes = Bytes::from(serialized.clone());

        self.clone()
            .submit_transaction(TransactionProto { transaction: bytes })
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}

/// A Narwhal client that instantiates LocalNarwhalClient lazily.
pub struct LazyNarwhalClient {
    /// Outer ArcSwapOption allows initialization after the first connection to Narwhal.
    /// Inner ArcSwap allows Narwhal restarts across epoch changes.
    client: ArcSwapOption<ArcSwap<LocalNarwhalClient>>,
    addr: Multiaddr,
}

impl LazyNarwhalClient {
    /// Lazily instantiates LocalNarwhalClient keyed by the address of the Narwhal worker.
    pub fn new(addr: Multiaddr) -> Self {
        Self {
            client: ArcSwapOption::empty(),
            addr,
        }
    }

    async fn get(&self) -> Arc<ArcSwap<LocalNarwhalClient>> {
        // Narwhal may not have started and created LocalNarwhalClient, so retry in a loop.
        // Retries should only happen on Sui process start.
        const NARWHAL_WORKER_START_TIMEOUT: Duration = Duration::from_secs(30);
        if let Ok(client) = timeout(NARWHAL_WORKER_START_TIMEOUT, async {
            loop {
                match LocalNarwhalClient::get_global(&self.addr) {
                    Some(c) => return c,
                    None => {
                        sleep(Duration::from_millis(100)).await;
                    }
                };
            }
        })
        .await
        {
            return client;
        }
        panic!(
            "Timed out after {:?} waiting for Narwhal worker ({}) to start!",
            NARWHAL_WORKER_START_TIMEOUT, self.addr,
        );
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for LazyNarwhalClient {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let transaction =
            bcs::to_bytes(transaction).expect("Serializing consensus transaction cannot fail");
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
            .submit_transaction(transaction)
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
    consensus_client: Box<dyn SubmitToConsensus>,
    /// Authority pubkey.
    authority: AuthorityName,
    /// The limit to number of inflight transactions at this node.
    max_pending_transactions: usize,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to check the connection statuses populated by the Connection Monitor Listener
    connection_monitor_status: Box<Arc<dyn CheckConnection>>,
    /// A structure to check the reputation scores populated by Consensus
    low_scoring_authorities: ArcSwap<Arc<ArcSwap<HashMap<AuthorityName, u64>>>>,
    /// A structure to register metrics
    metrics: ConsensusAdapterMetrics,
    /// Semaphore limiting parallel submissions to narwhal
    submit_semaphore: Semaphore,
    latency_observer: LatencyObserver,
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
        consensus_client: Box<dyn SubmitToConsensus>,
        authority: AuthorityName,
        connection_monitor_status: Box<Arc<dyn CheckConnection>>,
        max_pending_transactions: usize,
        max_pending_local_submissions: usize,
        metrics: ConsensusAdapterMetrics,
    ) -> Self {
        let num_inflight_transactions = Default::default();
        let low_scoring_authorities =
            ArcSwap::from_pointee(Arc::new(ArcSwap::from_pointee(HashMap::new())));
        Self {
            consensus_client,
            authority,
            max_pending_transactions,
            num_inflight_transactions,
            connection_monitor_status,
            low_scoring_authorities,
            metrics,
            submit_semaphore: Semaphore::new(max_pending_local_submissions),
            latency_observer: LatencyObserver::new(),
        }
    }

    pub fn swap_low_scoring_authorities(
        &self,
        new_low_scoring: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    ) {
        self.low_scoring_authorities.swap(Arc::new(new_low_scoring));
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
            self.submit_unchecked(transaction, epoch_store);
        }
    }

    fn await_submit_delay(
        &self,
        committee: &Committee,
        transaction: &ConsensusTransaction,
    ) -> (impl Future<Output = ()>, usize, bool) {
        let (duration, position, mapped_to_low_scoring) = match &transaction.kind {
            ConsensusTransactionKind::UserTransaction(certificate) => {
                let tx_digest = certificate.digest();
                let (position, mapped_to_low_scoring) =
                    self.submission_position(committee, tx_digest);
                const DEFAULT_LATENCY: Duration = Duration::from_secs(5);
                const MAX_LATENCY: Duration = Duration::from_secs(5 * 60);
                let latency = self.latency_observer.latency().unwrap_or(DEFAULT_LATENCY);
                let latency = std::cmp::max(latency, DEFAULT_LATENCY);
                let latency = std::cmp::min(latency, MAX_LATENCY);
                self.metrics
                    .sequencing_estimated_latency
                    .set(latency.as_millis() as i64);
                let delay_step = latency * 3 / 2;
                (
                    delay_step * position as u32,
                    position,
                    mapped_to_low_scoring,
                )
            }
            _ => (Duration::ZERO, 0, false),
        };
        (
            tokio::time::sleep(duration),
            position,
            mapped_to_low_scoring,
        )
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
    ) -> (usize, bool) {
        let positions = order_validators_for_submission(committee, tx_digest);

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
    ) -> (usize, bool) {
        let low_scoring_authorities = self.low_scoring_authorities.load().load_full();
        if low_scoring_authorities.get(&self.authority).is_some() {
            return (positions.len(), true);
        }
        let initial_position = get_position_in_list(self.authority, positions.clone());

        let filtered_positions = positions
            .into_iter()
            .filter(|authority| {
                self.authority == *authority // Don't filter yourself out!
                    ||
                (
                    self // Filter out any nodes that appear disconnected
                        .connection_monitor_status
                        .check_connection(&self.authority, authority)
                        .unwrap_or(ConnectionStatus::Disconnected)
                        == ConnectionStatus::Connected
                    && // Filter out low scoring nodes
                    low_scoring_authorities.get(authority).is_none()
                )
            })
            .collect();

        let position = get_position_in_list(self.authority, filtered_positions);
        (position, position < initial_position)
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
        epoch_store.insert_pending_consensus_transactions(&transaction, lock)?;
        Ok(self.submit_unchecked(transaction, epoch_store))
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

    fn submit_unchecked(
        self: &Arc<Self>,
        transaction: ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> JoinHandle<()> {
        // Reconfiguration lock is dropped when pending_consensus_transactions is persisted, before it is handled by consensus
        let async_stage = self
            .clone()
            .submit_and_wait(transaction, epoch_store.clone());
        // Number of these tasks is weakly limited based on `num_inflight_transactions`.
        // (Limit is not applied atomically, and only to user transactions.)
        let join_handle = spawn_monitored_task!(async_stage);
        join_handle
    }

    async fn submit_and_wait(
        self: Arc<Self>,
        transaction: ConsensusTransaction,
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
            .within_alive_epoch(self.submit_and_wait_inner(transaction, &epoch_store))
            .await
            .ok(); // result here indicates if epoch ended earlier, we don't care about it
    }

    #[allow(clippy::option_map_unit_fn)]
    async fn submit_and_wait_inner(
        self: Arc<Self>,
        transaction: ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if matches!(transaction.kind, ConsensusTransactionKind::EndOfPublish(..)) {
            info!(epoch=?epoch_store.epoch(), "Submitting EndOfPublish message to Narwhal");
            epoch_store.record_epoch_pending_certs_process_time_metric();
        }

        let transaction_key = SequencedConsensusTransactionKey::External(transaction.key());
        let processed_waiter = epoch_store
            .consensus_message_processed_notify(transaction_key)
            .boxed();

        let (await_submit, position, mapped_to_low_scoring) =
            self.await_submit_delay(epoch_store.committee(), &transaction);
        let mut guard = InflightDropGuard::acquire(&self);

        // We need to wait for some delay until we submit transaction to the consensus
        // However, if transaction is received by consensus while we wait, we don't need to wait
        let processed_waiter = match select(processed_waiter, await_submit.boxed()).await {
            Either::Left((processed, _await_submit)) => {
                processed.expect("Storage error when waiting for consensus message processed");
                None
            }
            Either::Right(((), processed_waiter)) => Some(processed_waiter),
        };
        let transaction_key = transaction.key();
        // Log warnings for capability or end of publish transactions that fail to get sequenced
        let _monitor = if matches!(
            transaction.kind,
            ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::CapabilityNotification(_)
        ) {
            Some(CancelOnDrop(spawn_monitored_task!(async {
                let mut i = 0u64;
                loop {
                    i += 1;
                    const WARN_DELAY_S: u64 = 30;
                    tokio::time::sleep(Duration::from_secs(WARN_DELAY_S)).await;
                    let total_wait = i * WARN_DELAY_S;
                    warn!("Still waiting {total_wait} seconds for transaction {transaction_key:?} to commit in narwhal");
                }
            })))
        } else {
            None
        };
        if let Some(processed_waiter) = processed_waiter {
            debug!("Submitting {transaction_key:?} to consensus");

            // populate the position only when this authority submits the transaction
            // to consensus
            guard.position = Some(position);
            guard.mapped_to_low_scoring = mapped_to_low_scoring;

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
                    .submit_to_consensus(&transaction, epoch_store)
                    .await
                {
                    // This can happen during Narwhal reconfig, or when the Narwhal worker has
                    // full internal buffers and needs to back pressure, so retry a few times.
                    if retries > 3 {
                        warn!(
                            "Failed to submit transaction {:?} to own narwhal worker: {:?}. Retry #{}",
                            transaction_key, e, retries,
                        );
                    }
                    self.metrics.sequencing_certificate_failures.inc();
                    retries += 1;
                    time::sleep(Duration::from_secs(10)).await;
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
                    .with_label_values(&[&bucket])
                    .observe(ack_start.elapsed().as_secs_f64());
            };
            match select(processed_waiter, submit_inner.boxed()).await {
                Either::Left((processed, _submit_inner)) => processed,
                Either::Right(((), processed_waiter)) => {
                    debug!("Submitted {transaction_key:?} to consensus");
                    processed_waiter.await
                }
            }
            .expect("Storage error when waiting for consensus message processed");
        }
        debug!("{transaction_key:?} processed by consensus");
        epoch_store
            .remove_pending_consensus_transaction(&transaction.key())
            .expect("Storage error when removing consensus transaction");
        let send_end_of_publish = if let ConsensusTransactionKind::UserTransaction(_cert) =
            &transaction.kind
        {
            let reconfig_guard = epoch_store.get_reconfig_state_read_lock_guard();
            // If we are in RejectUserCerts state and we just drained the list we need to
            // send EndOfPublish to signal other validators that we are not submitting more certificates to the epoch.
            // Note that there could be a race condition here where we enter this check in RejectAllCerts state.
            // In that case we don't need to send EndOfPublish because condition to enter
            // RejectAllCerts is when 2f+1 other validators already sequenced their EndOfPublish message.
            if reconfig_guard.is_reject_user_certs() {
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
            if let Err(err) = self.submit(
                ConsensusTransaction::new_end_of_publish(self.authority),
                None,
                epoch_store,
            ) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
        self.metrics.sequencing_certificate_success.inc();
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

pub fn order_validators_for_submission(
    committee: &Committee,
    tx_digest: &TransactionDigest,
) -> Vec<AuthorityName> {
    // the 32 is as requirement of the default StdRng::from_seed choice
    let digest_bytes = tx_digest.into_inner();

    // permute the validators deterministically, based on the digest
    let mut rng = StdRng::from_seed(digest_bytes);
    committee.shuffle_by_stake_with_rng(None, None, &mut rng)
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
        };
        if send_end_of_publish {
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
    mapped_to_low_scoring: bool,
}

impl<'a> InflightDropGuard<'a> {
    pub fn acquire(adapter: &'a ConsensusAdapter) -> Self {
        let inflight = adapter
            .num_inflight_transactions
            .fetch_add(1, Ordering::SeqCst);
        adapter.metrics.sequencing_certificate_attempt.inc();
        adapter
            .metrics
            .sequencing_certificate_inflight
            .set(inflight as i64);
        Self {
            adapter,
            start: Instant::now(),
            position: None,
            mapped_to_low_scoring: false,
        }
    }
}

impl<'a> Drop for InflightDropGuard<'a> {
    fn drop(&mut self) {
        let inflight = self
            .adapter
            .num_inflight_transactions
            .fetch_sub(1, Ordering::SeqCst);
        // Store the latest latency
        self.adapter
            .metrics
            .sequencing_certificate_inflight
            .set(inflight as i64);

        let position = if let Some(position) = self.position {
            self.adapter
                .metrics
                .sequencing_certificate_authority_position
                .observe(position as f64);
            position.to_string()
        } else {
            "not_submitted".to_string()
        };

        let latency = self.start.elapsed();
        if self.position == Some(0) {
            self.adapter.latency_observer.report(latency);
        }
        self.adapter
            .metrics
            .sequencing_certificate_latency
            .with_label_values(&[&position, &format!("{:?}", self.mapped_to_low_scoring)])
            .observe(latency.as_secs_f64());
    }
}

struct LatencyObserver {
    data: Mutex<LatencyObserverInner>,
    latency_ms: AtomicU64,
}

#[derive(Default)]
struct LatencyObserverInner {
    points: VecDeque<Duration>,
    sum: Duration,
}

impl LatencyObserver {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(LatencyObserverInner::default()),
            latency_ms: AtomicU64::new(u64::MAX),
        }
    }

    pub fn report(&self, latency: Duration) {
        const MAX_SAMPLES: usize = 64;
        let mut data = self.data.lock();
        data.points.push_back(latency);
        data.sum += latency;
        if data.points.len() >= MAX_SAMPLES {
            let pop = data.points.pop_front().expect("data vector is not empty");
            data.sum -= pop; // This does not overflow because of how running sum is calculated
        }
        let latency = data.sum.as_millis() as u64 / data.points.len() as u64;
        self.latency_ms.store(latency, Ordering::Relaxed);
    }

    pub fn latency(&self) -> Option<Duration> {
        let latency = self.latency_ms.load(Ordering::Relaxed);
        if latency == u64::MAX {
            // Not initialized yet (0 data points)
            None
        } else {
            Some(Duration::from_millis(latency))
        }
    }
}

use crate::consensus_handler::SequencedConsensusTransactionKey;

#[async_trait::async_trait]
impl SubmitToConsensus for Arc<ConsensusAdapter> {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.submit(transaction.clone(), None, epoch_store)
            .map(|_| ())
    }
}

pub fn position_submit_certificate(
    committee: &Committee,
    ourselves: &AuthorityName,
    tx_digest: &TransactionDigest,
) -> usize {
    let validators = order_validators_for_submission(committee, tx_digest);
    get_position_in_list(*ourselves, validators)
}

#[cfg(test)]
mod adapter_tests {
    use super::position_submit_certificate;
    use fastcrypto::traits::KeyPair;
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use sui_types::{
        base_types::TransactionDigest,
        committee::Committee,
        crypto::{get_key_pair_from_rng, AuthorityKeyPair, AuthorityPublicKeyBytes},
    };

    #[test]
    fn test_position_submit_certificate() {
        // grab a random committee and a random stake distribution
        let mut rng = StdRng::from_seed([0; 32]);
        const COMMITTEE_SIZE: usize = 10; // 3 * 3 + 1;
        let authorities = (0..COMMITTEE_SIZE)
            .map(|_k| {
                (
                    AuthorityPublicKeyBytes::from(
                        get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut rng)
                            .1
                            .public(),
                    ),
                    rng.gen_range(0u64..10u64),
                )
            })
            .collect::<Vec<_>>();
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            authorities.iter().cloned().collect(),
        );

        // generate random transaction digests, and account for validator selection
        const NUM_TEST_TRANSACTIONS: usize = 1000;

        for _tx_idx in 0..NUM_TEST_TRANSACTIONS {
            let tx_digest = TransactionDigest::generate(&mut rng);

            let mut zero_found = false;
            for (name, _) in authorities.iter() {
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
