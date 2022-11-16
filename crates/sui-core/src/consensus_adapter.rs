// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use narwhal_types::TransactionProto;
use narwhal_types::TransactionsClient;
use prometheus::register_int_gauge_with_registry;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::{register_histogram_with_registry, register_int_counter_with_registry, Histogram};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::{
    error::{SuiError, SuiResult},
    messages::{ConsensusTransaction, VerifiedCertificate},
};

use tap::prelude::*;

use crate::authority::AuthorityState;
use sui_types::base_types::AuthorityName;
use tokio::time::{timeout, Duration};
use tracing::debug;
use tracing::error;

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
    pub sequencing_certificate_timeouts: IntCounter,
    pub sequencing_certificate_failures: IntCounter,
    pub sequencing_certificate_inflight: IntGauge,
    pub sequencing_acknowledge_latency: Histogram,

    // Fragment sequencing metrics
    pub sequencing_fragment_attempt: IntCounter,
    pub sequencing_fragment_success: IntCounter,
    pub sequencing_fragment_timeouts: IntCounter,
    pub sequencing_fragment_control_delay: IntGauge,
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
            sequencing_certificate_timeouts: register_int_counter_with_registry!(
                "sequencing_certificate_timeouts",
                "Counts the number of sequenced certificates that timed out.",
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
            sequencing_fragment_attempt: register_int_counter_with_registry!(
                "sequencing_fragment_attempt",
                "Counts the number of sequenced fragments submitted.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_success: register_int_counter_with_registry!(
                "sequencing_fragment_success",
                "Counts the number of successfully sequenced fragments.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_timeouts: register_int_counter_with_registry!(
                "sequencing_fragment_timeouts",
                "Counts the number of sequenced fragments that timed out.",
                registry,
            )
            .unwrap(),
            sequencing_fragment_control_delay: register_int_gauge_with_registry!(
                "sequencing_fragment_control_delay",
                "The estimated latency of sequencing fragments.",
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
    /// Retries sending a transaction to consensus after this timeout.
    timeout: Duration,
    /// Number of submitted transactions still inflight at this node.
    num_inflight_transactions: AtomicU64,
    /// A structure to register metrics
    opt_metrics: OptArcConsensusAdapterMetrics,
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
        timeout: Duration,
        opt_metrics: OptArcConsensusAdapterMetrics,
    ) -> Self {
        let num_inflight_transactions = Default::default();
        Self {
            consensus_client,
            authority,
            timeout,
            num_inflight_transactions,
            opt_metrics,
        }
    }

    pub fn num_inflight_transactions(&self) -> u64 {
        self.num_inflight_transactions.load(Ordering::Relaxed)
    }

    /// Check if this authority should submit the transaction to consensus.
    fn should_submit(
        committee: &Committee,
        ourselves: &AuthorityName,
        tx_digest: &TransactionDigest,
    ) -> bool {
        // the 32 is as requirement of the deault StdRng::from_seed choice
        let digest_bytes: [u8; 32] = tx_digest.to_bytes()[..32].try_into().unwrap();

        // permute the validators deterministically, based on the digest
        let mut rng = StdRng::from_seed(digest_bytes);
        let mut validators = committee.voting_rights.clone();
        validators.shuffle(&mut rng);

        // the last (f+1) elements by weight are the submitters for this transaction
        let mut total_weight = 0u64;
        let mut found = false;
        while total_weight < committee.validity_threshold() {
            if let Some((name, weight)) = validators.pop() {
                total_weight += weight;
                if name == *ourselves {
                    found = true;
                    break;
                }
            } else {
                unreachable!(
                    "We should cross the validity threshold before running out of validators"
                );
            }
        }
        // Are we one of the submitters?
        found

        // TODO [issue #1647]: Right now every transaction is submitted to (f+1) authorities.
        // We should bring this number down to one, and make sure the mapping to submitters is
        // refreshed frequently enough to make sure this is Byzantine-resistant
    }

    /// Submit a transaction to consensus, wait for its processing, and notify the caller.
    // Use .inspect when its stable.
    #[allow(clippy::option_map_unit_fn)]
    pub async fn submit(&self, certificate: &VerifiedCertificate) -> SuiResult {
        let processed_waiter = self
            .authority
            .consensus_message_processed_notify(certificate.digest());
        // Serialize the certificate in a way that is understandable to consensus (i.e., using
        // bincode) and it certificate to consensus.
        let transaction = ConsensusTransaction::new_certificate_message(
            &self.authority.name,
            certificate.clone().into(),
        );
        let tracking_id = transaction.get_tracking_id();
        let tx_digest = certificate.digest();
        debug!(
            ?tracking_id,
            ?tx_digest,
            "Certified transaction consensus message created"
        );

        // Check if this authority submits the transaction to consensus.
        let should_submit = Self::should_submit(
            &self.authority.committee.load(),
            &self.authority.name,
            tx_digest,
        );
        let _inflight_guard = if should_submit {
            // Timer to record latency of acknowledgements from consensus
            let _timer = self
                .opt_metrics
                .as_ref()
                .map(|m| m.sequencing_acknowledge_latency.start_timer());

            // TODO - we need stronger guarantees for checkpoints here (issue #5763)
            // TODO - for owned objects this can also be done async
            //
            // TODO: Somewhere here we check whether we should have stopped sending transactions
            // to consensus due to epoch boundary.
            // For normal transactions, we call state.get_reconfig_state_read_lock_guard first
            // to hold the guard before sending the transaction;
            // For the last EndOfPublish message, we call state.get_reconfig_state_write_lock_guard
            // to hold the guard before sending the last message, and then call
            // state.close_user_certs with the guard.
            self.consensus_client
                .submit_to_consensus(&transaction)
                .await?;

            Some(InflightDropGuard::acquire(self))
        } else {
            None
        };

        // We do not wait unless its a share object transaction being sequenced.
        if !certificate.contains_shared_object() {
            // We only record for shared object transactions
            return Ok(());
        };

        // Now consensus guarantees delivery after submit_transaction() if primary/workers are live
        match timeout(self.timeout, processed_waiter).await {
            Ok(Ok(())) => {
                // Increment the attempted certificate sequencing success
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_success.inc();
                });
                Ok(())
            }
            Ok(Err(e)) => {
                // Increment the attempted certificate sequencing failure
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_failures.inc();
                });
                Err(e)
            }
            Err(e) => {
                // Increment the attempted certificate sequencing timeout
                self.opt_metrics.as_ref().map(|metrics| {
                    metrics.sequencing_certificate_timeouts.inc();
                });

                // We drop the waiter which will signal to the conensus listener task to clean up
                // the channels.
                Err(SuiError::FailedToHearBackFromConsensus(e.to_string()))
            }
        }
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
    fn should_submit_selects_valid_submitters() {
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

            let total_stake_this_committee = authorities.iter().map(|(_name, stake)| stake).sum();
            // collect the stake of authorities which will be selected to submit the transaction
            let mut submitters_total_stake = 0u64;
            for (name, stake) in authorities.iter() {
                if ConsensusAdapter::should_submit(&committee, name, &tx_digest) {
                    submitters_total_stake += stake;
                }
            }
            assert!(submitters_total_stake >= committee.validity_threshold());
            assert!(submitters_total_stake < total_stake_this_committee);
        }
    }
}
