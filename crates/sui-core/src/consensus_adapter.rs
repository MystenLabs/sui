// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use futures::future::select;
use futures::future::Either;
use futures::FutureExt;
use itertools::Itertools;
use narwhal_types::TransactionProto;
use narwhal_types::TransactionsClient;
use parking_lot::RwLockReadGuard;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::{
    linear_buckets, register_histogram_with_registry, register_int_counter_with_registry, Histogram,
};
use prometheus::{register_histogram_vec_with_registry, register_int_gauge_with_registry};
use prometheus::{HistogramVec, IntCounter};
use rand::rngs::StdRng;
use rand::SeedableRng;
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
use tokio::task::JoinHandle;
use tokio::time;

use mysten_metrics::spawn_monitored_task;
use sui_types::base_types::AuthorityName;
use sui_types::messages::ConsensusTransactionKind;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

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
}

pub type OptArcConsensusAdapterMetrics = Option<Arc<ConsensusAdapterMetrics>>;

impl ConsensusAdapterMetrics {
    pub fn new(registry: &Registry) -> OptArcConsensusAdapterMetrics {
        let authority_position_buckets = &[
            linear_buckets(0.0, 1.0, 19).unwrap().as_slice(),
            linear_buckets(20.0, 5.0, 10).unwrap().as_slice(),
        ]
        .concat();

        Some(Arc::new(ConsensusAdapterMetrics {
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
                &["position"],
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
        }))
    }

    pub fn new_test() -> OptArcConsensusAdapterMetrics {
        None
    }
}

/// Submit Sui certificates to the consensus.
pub struct ConsensusAdapter {
    /// The network client connecting to the consensus node of this authority.
    consensus_client: Box<dyn SubmitToConsensus>,
    /// Authority pubkey.
    authority: AuthorityName,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to register metrics
    opt_metrics: OptArcConsensusAdapterMetrics,
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
            bincode::serialize(transaction).expect("Serializing consensus transaction cannot fail");
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

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_client: Box<dyn SubmitToConsensus>,
        authority: AuthorityName,
        opt_metrics: OptArcConsensusAdapterMetrics,
    ) -> Arc<Self> {
        let num_inflight_transactions = Default::default();
        Arc::new(Self {
            consensus_client,
            authority,
            num_inflight_transactions,
            opt_metrics,
        })
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

    pub fn num_inflight_transactions(&self) -> u64 {
        self.num_inflight_transactions.load(Ordering::Relaxed)
    }

    fn await_submit_delay(
        committee: &Committee,
        ourselves: &AuthorityName,
        transaction: &ConsensusTransaction,
    ) -> (impl Future<Output = ()>, usize) {
        let (duration, position) = Self::submit_delay(committee, ourselves, transaction);
        (tokio::time::sleep(duration), position)
    }

    fn submit_delay(
        committee: &Committee,
        ourselves: &AuthorityName,
        transaction: &ConsensusTransaction,
    ) -> (Duration, usize) {
        if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
            Self::submit_delay_certificate(committee, ourselves, certificate.digest())
        } else {
            (Duration::ZERO, 0)
        }
    }

    /// Check when this authority should submit the certificate to consensus.
    /// This sorts all authorities based on pseudo-random distribution derived from transaction hash.
    /// Authorities higher in the list wait less time.
    ///
    /// The function targets having only 1 consensus transaction submitted per user transaction
    /// when system operates normally.
    ///
    /// The function returns the delay to wait and the position of authority in the list.
    fn submit_delay_certificate(
        committee: &Committee,
        ourselves: &AuthorityName,
        tx_digest: &TransactionDigest,
    ) -> (Duration, usize) {
        let position = position_submit_certificate(committee, ourselves, tx_digest);
        const MAX_DELAY_MUL: usize = 10;
        // DELAY_STEP is chosen as 1.5 * mean consensus delay
        // In the future we can actually use information about consensus rounds instead of this delay
        const DELAY_STEP: Duration = Duration::from_secs(7);
        (
            DELAY_STEP * std::cmp::min(position, MAX_DELAY_MUL) as u32,
            position,
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
        epoch_store.insert_pending_consensus_transactions(&transaction, lock)?;
        Ok(self.submit_unchecked(transaction, epoch_store))
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
        // Number of this tasks is limited by `sequencing_certificate_inflight` limit
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

        let processed_waiter = epoch_store
            .consensus_message_processed_notify(transaction.key())
            .boxed();
        let (await_submit, position) =
            Self::await_submit_delay(epoch_store.committee(), &self.authority, &transaction);
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
        let _monitor = CancelOnDrop(spawn_monitored_task!(async {
            let mut i = 0u64;
            loop {
                i += 1;
                const WARN_DELAY_S: u64 = 30;
                tokio::time::sleep(Duration::from_secs(WARN_DELAY_S)).await;
                let total_wait = i * WARN_DELAY_S;
                warn!("Still waiting {total_wait} seconds for transaction {transaction_key:?} to commit in narwhal");
            }
        }));
        if let Some(processed_waiter) = processed_waiter {
            debug!("Submitting {transaction_key:?} to consensus");

            // populate the position only when this authority submits the transaction
            // to consensus
            guard.position = Some(position);

            // We enter this branch when in select above await_submit completed and processed_waiter is pending
            // This means it is time for us to submit transaction to consensus
            {
                let ack_start = Instant::now();
                let mut retries: u32 = 0;
                while let Err(e) = self
                    .consensus_client
                    .submit_to_consensus(&transaction, epoch_store)
                    .await
                {
                    // This can happen during Narwhal reconfig, so wait for a few retries.
                    if retries > 3 {
                        error!(
                            "Error submitting transaction to own narwhal worker: {:?}",
                            e
                        );
                    }
                    self.opt_metrics.as_ref().map(|metrics| {
                        metrics.sequencing_certificate_failures.inc();
                    });
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

                self.opt_metrics.as_ref().map(|metrics| {
                    metrics
                        .sequencing_acknowledge_latency
                        .with_label_values(&[&bucket])
                        .observe(ack_start.elapsed().as_secs_f64());
                });
            }
            debug!("Submitted {transaction_key:?} to consensus");
            processed_waiter
                .await
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
        self.opt_metrics.as_ref().map(|metrics| {
            metrics.sequencing_certificate_success.inc();
        });
    }
}

/// Returns a position of the current validator in ordered list of validator to submit transaction
pub fn position_submit_certificate(
    committee: &Committee,
    ourselves: &AuthorityName,
    tx_digest: &TransactionDigest,
) -> usize {
    // the 32 is as requirement of the default StdRng::from_seed choice
    let digest_bytes = tx_digest.into_inner();

    // permute the validators deterministically, based on the digest
    let mut rng = StdRng::from_seed(digest_bytes);
    let validators = committee.shuffle_by_stake_with_rng(None, None, &mut rng);
    let (position, _) = validators
        .into_iter()
        .find_position(|a| a == ourselves)
        .expect("Could not find ourselves in shuffled committee");
    position
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
}

impl<'a> InflightDropGuard<'a> {
    pub fn acquire(adapter: &'a ConsensusAdapter) -> Self {
        let inflight = adapter
            .num_inflight_transactions
            .fetch_add(1, Ordering::SeqCst);
        if let Some(metrics) = adapter.opt_metrics.as_ref() {
            metrics.sequencing_certificate_attempt.inc();
            metrics.sequencing_certificate_inflight.set(inflight as i64);
        }
        Self {
            adapter,
            start: Instant::now(),
            position: None,
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
        if let Some(metrics) = self.adapter.opt_metrics.as_ref() {
            metrics.sequencing_certificate_inflight.set(inflight as i64);

            let position = if let Some(position) = self.position {
                metrics
                    .sequencing_certificate_authority_position
                    .observe(position as f64);
                position.to_string()
            } else {
                "not_submitted".to_string()
            };

            metrics
                .sequencing_certificate_latency
                .with_label_values(&[&position])
                .observe(self.start.elapsed().as_secs_f64());
        }
    }
}

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::epoch::reconfiguration::{ReconfigState, ReconfigurationInitiator};

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

#[cfg(test)]
mod adapter_tests {
    use super::position_submit_certificate;
    use fastcrypto::traits::KeyPair;
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use sui_types::{
        base_types::TransactionDigest,
        committee::{Committee, ProtocolVersion},
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
        let committee = Committee::new(
            0,
            ProtocolVersion::MIN,
            authorities.iter().cloned().collect(),
        )
        .unwrap();

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
