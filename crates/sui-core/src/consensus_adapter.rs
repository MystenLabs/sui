// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use futures::future::select;
use futures::future::Either;
use futures::FutureExt;
use narwhal_types::TransactionProto;
use narwhal_types::TransactionsClient;
use parking_lot::Mutex;
use prometheus::register_int_gauge_with_registry;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::{register_histogram_with_registry, register_int_counter_with_registry, Histogram};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::{
    error::{SuiError, SuiResult},
    messages::ConsensusTransaction,
};

use tap::prelude::*;
use tokio::task::JoinHandle;
use tokio::time;

use crate::authority::AuthorityState;
use mysten_metrics::spawn_monitored_task;
use sui_types::base_types::AuthorityName;
use sui_types::messages::ConsensusTransactionKind;
use tokio::time::Duration;
use tracing::{error, warn};

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
    pub sequencing_acknowledge_latency: Histogram,
}

pub type OptArcConsensusAdapterMetrics = Option<Arc<ConsensusAdapterMetrics>>;

impl ConsensusAdapterMetrics {
    pub fn new(registry: &Registry) -> OptArcConsensusAdapterMetrics {
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
            sequencing_acknowledge_latency: register_histogram_with_registry!(
                "sequencing_acknowledge_latency",
                "The latency for acknowledgement from sequencing engine .",
                SEQUENCING_CERTIFICATE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
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
    /// Authority state.
    authority: Arc<AuthorityState>,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to register metrics
    opt_metrics: OptArcConsensusAdapterMetrics,
    /// Pending certificates that we are waiting to be sequenced by consensus
    /// Lock ordering: this is a 'leaf' lock, no other locks should be acquired in the scope of this lock
    /// In particular, this lock is always acquired after taking read or write lock on reconfig state
    pending_certificates: Mutex<HashSet<TransactionDigest>>,
}

#[async_trait::async_trait]
pub trait SubmitToConsensus: Sync + Send + 'static {
    async fn submit_to_consensus(&self, transaction: &ConsensusTransaction) -> SuiResult;
}

#[async_trait::async_trait]
impl SubmitToConsensus for TransactionsClient<sui_network::tonic::transport::Channel> {
    async fn submit_to_consensus(&self, transaction: &ConsensusTransaction) -> SuiResult {
        let serialized =
            bincode::serialize(transaction).expect("Serializing consensus transaction cannot fail");
        let bytes = Bytes::from(serialized.clone());
        self.clone()
            .submit_transaction(TransactionProto { transaction: bytes })
            .await
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
            .tap_err(|r| {
                error!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}

impl ConsensusAdapter {
    /// Make a new Consensus adapter instance.
    pub fn new(
        consensus_client: Box<dyn SubmitToConsensus>,
        authority: Arc<AuthorityState>,
        opt_metrics: OptArcConsensusAdapterMetrics,
    ) -> Arc<Self> {
        let num_inflight_transactions = Default::default();
        let this = Arc::new(Self {
            consensus_client,
            authority,
            num_inflight_transactions,
            opt_metrics,
            pending_certificates: Default::default(),
        });
        let recover = this.clone();
        recover.submit_recovered();
        this
    }

    fn submit_recovered(self: Arc<Self>) {
        // Currently narwhal worker might lose transactions on restart, so we need to resend them
        let mut recovered = self
            .authority
            .database
            .epoch_tables()
            .get_all_pending_consensus_transactions();
        let pending_certificates = recovered
            .iter()
            .filter_map(|transaction| {
                if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
                    Some(*certificate.digest())
                } else {
                    None
                }
            })
            .collect();

        // try_lock.unwrap() is safe here because instance of ConsensusAdapter was not
        // yet populated and no-one had a chance to acquire lock
        *self
            .pending_certificates
            .try_lock()
            .expect("Contention on pending_certificates when initializing ConsensusAdapter") =
            pending_certificates;

        #[allow(clippy::collapsible_if)] // This if can be collapsed but it will be ugly
        if self
            .authority
            .get_reconfig_state_read_lock_guard()
            .is_reject_user_certs()
        {
            if recovered
                .iter()
                .any(ConsensusTransaction::is_end_of_publish)
            {
                // This can happen if node crashed inside ConsensusAdapter::close_epoch,
                // after reconfig lock state was written to DB and before we persisted EndOfPublish message
                recovered.push(ConsensusTransaction::new_end_of_publish(
                    self.authority.name,
                ));
            }
        }
        for transaction in recovered {
            self.submit_unchecked(transaction);
        }
    }

    pub fn num_inflight_transactions(&self) -> u64 {
        self.num_inflight_transactions.load(Ordering::Relaxed)
    }

    fn await_submit_delay(
        committee: &Committee,
        ourselves: &AuthorityName,
        transaction: &ConsensusTransaction,
    ) -> impl Future<Output = ()> {
        tokio::time::sleep(Self::submit_delay(committee, ourselves, transaction))
    }

    fn submit_delay(
        committee: &Committee,
        ourselves: &AuthorityName,
        transaction: &ConsensusTransaction,
    ) -> Duration {
        if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
            Self::submit_delay_certificate(committee, ourselves, certificate.digest())
        } else {
            Duration::ZERO
        }
    }

    /// Check when this authority should submit the certificate to consensus.
    fn submit_delay_certificate(
        committee: &Committee,
        ourselves: &AuthorityName,
        tx_digest: &TransactionDigest,
    ) -> Duration {
        let (nominator, denominator) =
            Self::position_submit_certificate(committee, ourselves, tx_digest);
        const MAX_DELAY_MS: u64 = 60_000;
        // There is no overflow as long as total_stake < u64::MAX / MAX_DELAY_MS, which seem to be reasonable expectation
        let duration_ms = MAX_DELAY_MS * nominator / denominator;
        Duration::from_millis(duration_ms)
    }

    /// Returns a position of the current validator in ordered list of validator to submit transaction
    /// You can think of this value as a float in range [0, 1)
    /// Validators closer to 0 are submitting transaction sooner, and validators closer to 1 wait longer
    ///
    /// In practice, instead of returning float we return integer fraction here in form of (nominator, denominator)
    /// If returned tuple (A, B) the value A is in range [0, B)
    fn position_submit_certificate(
        committee: &Committee,
        ourselves: &AuthorityName,
        tx_digest: &TransactionDigest,
    ) -> (StakeUnit, StakeUnit) {
        // the 32 is as requirement of the deault StdRng::from_seed choice
        let digest_bytes = tx_digest.into_bytes();

        // permute the validators deterministically, based on the digest
        let mut rng = StdRng::from_seed(digest_bytes);
        let mut validators = committee.voting_rights.clone();
        validators.shuffle(&mut rng);

        let mut our_position = 0u64;
        loop {
            if let Some((name, weight)) = validators.pop() {
                if name == *ourselves {
                    break;
                }
                // note that our_position is 0 for first validator in the list
                // our_position is strictly less then committee.total_votes for last validator
                our_position += weight;
            } else {
                unreachable!(
                    "We should find ourselves in committee before running out of validators"
                );
            }
        }

        (our_position, committee.total_votes)
    }

    /// This method is called externally to begin reconfiguration
    /// It transition reconfig state to reject new certificates from user
    /// ConsensusAdapter will send EndOfPublish message once pending certificate queue is drained
    pub fn close_epoch(self: &Arc<Self>) -> SuiResult {
        let send_end_of_publish = {
            let reconfig_guard = self.authority.get_reconfig_state_write_lock_guard();
            let pending_certificates = self.pending_certificates.lock();
            let send_end_of_publish = pending_certificates.is_empty();
            self.authority.close_user_certs(reconfig_guard);
            send_end_of_publish
        };
        if send_end_of_publish {
            if let Err(err) = self.submit(ConsensusTransaction::new_end_of_publish(
                self.authority.name,
            )) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
        Ok(())
    }

    /// This method blocks until transaction is persisted in local database
    /// It then returns handle to async task, user can join this handle to await while transaction is processed by consensus
    ///
    /// This method guarantees that once submit(but not returned async handle) returns,
    /// transaction is persisted and will eventually be sent to consensus even after restart
    pub fn submit(
        self: &Arc<Self>,
        transaction: ConsensusTransaction,
    ) -> SuiResult<JoinHandle<()>> {
        let _lock = if transaction.is_user_certificate() {
            let lock = self.authority.get_reconfig_state_read_lock_guard();
            if !lock.should_accept_user_certs() {
                return Err(SuiError::ValidatorHaltedAtEpochEnd);
            }
            Some(lock)
        } else {
            None
        };
        self.authority
            .database
            .epoch_tables()
            .insert_pending_consensus_transactions(&transaction)?;
        if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
            self.pending_certificates
                .lock()
                .insert(*certificate.digest());
        }
        Ok(self.submit_unchecked(transaction))
    }

    fn submit_unchecked(self: &Arc<Self>, transaction: ConsensusTransaction) -> JoinHandle<()> {
        // Reconfiguration lock is dropped when pending_consensus_transactions is persisted, before it is handled by consensus
        let async_stage = self.clone().submit_and_wait(transaction);
        // Number of this tasks is limited by `sequencing_certificate_inflight` limit
        let join_handle = spawn_monitored_task!(async_stage);
        join_handle
    }

    #[allow(clippy::option_map_unit_fn)]
    async fn submit_and_wait(self: Arc<Self>, transaction: ConsensusTransaction) {
        let _guard = InflightDropGuard::acquire(&self);
        let processed_waiter = self
            .authority
            .consensus_message_processed_notify(transaction.key())
            .boxed();
        let await_submit = Self::await_submit_delay(
            &self.authority.committee.load(),
            &self.authority.name,
            &transaction,
        )
        .boxed();
        // We need to wait for some delay until we submit transaction to the consensus
        // However, if transaction is received by consensus while we wait, we don't need to wait
        let processed_waiter = match select(processed_waiter, await_submit).await {
            Either::Left((processed, _await_submit)) => {
                processed.expect("Storage error when waiting for consensus message processed");
                None
            }
            Either::Right(((), processed_waiter)) => Some(processed_waiter),
        };
        if let Some(processed_waiter) = processed_waiter {
            // We enter this branch when in select above await_submit completed and processed_waiter is pending
            // This means it is time for us to submit transaction to consensus
            let _timer = self
                .opt_metrics
                .as_ref()
                .map(|m| m.sequencing_acknowledge_latency.start_timer());
            while let Err(e) = self
                .consensus_client
                .submit_to_consensus(&transaction)
                .await
            {
                error!(
                    "Error submitting transaction to own narwhal worker: {:?}",
                    e
                );
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_failures.inc();
                });
                time::sleep(Duration::from_secs(10)).await;
            }
            processed_waiter
                .await
                .expect("Storage error when waiting for consensus message processed");
        }
        let send_end_of_publish =
            if let ConsensusTransactionKind::UserTransaction(certificate) = &transaction.kind {
                let reconfig_guard = self.authority.get_reconfig_state_read_lock_guard();
                // note that pending_certificates lock is always acquired *after* reconfiguration lock
                // acquiring locks in different order might lead to deadlocks
                let mut pending_certificates = self.pending_certificates.lock();
                pending_certificates.remove(certificate.digest().as_ref());
                // If we are in RejectUserCerts state and we just drained the list we need to
                // send EndOfPublish to signal other validators that we are not submitting more certificates to the epoch.
                // Note that there could be a race condition here where we enter this check in RejectAllCerts state.
                // In that case we don't need to send EndOfPublish because condition to enter
                // RejectAllCerts is when 2f+1 other validators already sequenced their EndOfPublish message.
                if reconfig_guard.is_reject_user_certs() {
                    pending_certificates.is_empty() // send end of epoch if empty
                } else {
                    false
                }
            } else {
                false
            };
        if send_end_of_publish {
            // sending message outside of any locks scope
            if let Err(err) = self.submit(ConsensusTransaction::new_end_of_publish(
                self.authority.name,
            )) {
                warn!("Error when sending end of publish message: {:?}", err);
            }
        }
        // Removing transaction from persistent storage *after* sending end of epoch
        // Doing it in different order won't be restart safe
        self.authority
            .database
            .epoch_tables()
            .remove_pending_consensus_transaction(&transaction.key())
            .expect("Storage error when removing consensus transaction");
        self.opt_metrics.as_ref().map(|metrics| {
            metrics.sequencing_certificate_success.inc();
        });
    }
}

/// Tracks number of inflight consensus requests and relevant metrics
struct InflightDropGuard<'a> {
    adapter: &'a ConsensusAdapter,
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
        Self { adapter }
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
        }
    }
}

/// An implementation of the Narwhal TxValidator trait that validates transactions coming from Sui, and those coming from the network.
// TODO: replace by a ConsensusTxValidator in order to make Narwhal-side validation effective
pub use narwhal_worker::TrivialTransactionValidator as SuiTxValidator;

#[async_trait::async_trait]
impl SubmitToConsensus for Arc<ConsensusAdapter> {
    async fn submit_to_consensus(&self, transaction: &ConsensusTransaction) -> SuiResult {
        self.submit(transaction.clone()).map(|_| ())
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::ConsensusAdapter;
    use fastcrypto::traits::KeyPair;
    use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};
    use sui_types::{
        base_types::{TransactionDigest, TRANSACTION_DIGEST_LENGTH},
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
        let committee = Committee::new(0, authorities.iter().cloned().collect()).unwrap();

        // generate random transaction digests, and account for validator selection
        const NUM_TEST_TRANSACTIONS: usize = 1000;

        for _tx_idx in 0..NUM_TEST_TRANSACTIONS {
            let mut tx_digest_bytes = [0u8; TRANSACTION_DIGEST_LENGTH];
            rng.fill_bytes(&mut tx_digest_bytes);
            let tx_digest = TransactionDigest::new(tx_digest_bytes);

            let mut zero_found = false;
            for (name, _) in authorities.iter() {
                let (f, total_votes) =
                    ConsensusAdapter::position_submit_certificate(&committee, name, &tx_digest);
                assert!(f < total_votes);
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
