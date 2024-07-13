// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::execution_cache::TransactionCacheRead;
use mysten_metrics::TX_LATENCY_SEC_BUCKETS;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;
use sui_types::transaction::VerifiedSignedTransaction;
use tokio::time::Instant;
use tracing::{debug, error, trace};

/// Only wake up the transaction finalization task for a given transaction
/// after 1 mins of seeing it. This gives plenty of time for the transaction
/// to become final in the normal way. We also don't want this delay to be too long
/// to reduce memory usage held up by the finalizer threads.
const TX_FINALIZATION_DELAY: Duration = Duration::from_secs(60);
/// If a transaction can not be finalized within 1 min of being woken up, give up.
const FINALIZATION_TIMEOUT: Duration = Duration::from_secs(60);

struct ValidatorTxFinalizerMetrics {
    num_finalization_attempts: IntCounter,
    num_successful_finalizations: IntCounter,
    finalization_latency: Histogram,
    #[cfg(test)]
    num_finalization_attempts_for_testing: AtomicU64,
    #[cfg(test)]
    num_successful_finalizations_for_testing: AtomicU64,
}

impl ValidatorTxFinalizerMetrics {
    fn new(registry: &Registry) -> Self {
        Self {
            num_finalization_attempts: register_int_counter_with_registry!(
                "validator_tx_finalizer_num_finalization_attempts",
                "Total number of attempts to finalize a transaction",
                registry,
            )
            .unwrap(),
            num_successful_finalizations: register_int_counter_with_registry!(
                "validator_tx_finalizer_num_successful_finalizations",
                "Number of transactions successfully finalized",
                registry,
            )
            .unwrap(),
            finalization_latency: register_histogram_with_registry!(
                "validator_tx_finalizer_finalization_latency",
                "Latency of transaction finalization",
                TX_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            #[cfg(test)]
            num_finalization_attempts_for_testing: AtomicU64::new(0),
            #[cfg(test)]
            num_successful_finalizations_for_testing: AtomicU64::new(0),
        }
    }

    fn start_finalization(&self) -> Instant {
        self.num_finalization_attempts.inc();
        #[cfg(test)]
        self.num_finalization_attempts_for_testing
            .fetch_add(1, Relaxed);
        Instant::now()
    }

    fn finalization_succeeded(&self, start_time: Instant) {
        let latency = start_time.elapsed();
        self.num_successful_finalizations.inc();
        self.finalization_latency.observe(latency.as_secs_f64());
        #[cfg(test)]
        self.num_successful_finalizations_for_testing
            .fetch_add(1, Relaxed);
    }
}

/// The `ValidatorTxFinalizer` is responsible for finalizing transactions that
/// have been signed by the validator. It does this by waiting for a delay
/// after the transaction has been signed, and then attempting to finalize
/// the transaction if it has not yet been done by a fullnode.
pub struct ValidatorTxFinalizer<C: Clone> {
    agg: Arc<AuthorityAggregator<C>>,
    tx_finalization_delay: Duration,
    finalization_timeout: Duration,
    metrics: Arc<ValidatorTxFinalizerMetrics>,
}

impl<C: Clone> ValidatorTxFinalizer<C> {
    #[allow(dead_code)]
    pub(crate) fn new(agg: Arc<AuthorityAggregator<C>>, registry: &Registry) -> Self {
        Self {
            agg,
            tx_finalization_delay: TX_FINALIZATION_DELAY,
            finalization_timeout: FINALIZATION_TIMEOUT,
            metrics: Arc::new(ValidatorTxFinalizerMetrics::new(registry)),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_testing(
        agg: Arc<AuthorityAggregator<C>>,
        tx_finalization_delay: Duration,
        finalization_timeout: Duration,
    ) -> Self {
        Self {
            agg,
            tx_finalization_delay,
            finalization_timeout,
            metrics: Arc::new(ValidatorTxFinalizerMetrics::new(
                prometheus::default_registry(),
            )),
        }
    }
}

impl<C> ValidatorTxFinalizer<C>
where
    C: Clone + AuthorityAPI + Send + Sync + 'static,
{
    pub async fn track_signed_tx(
        &self,
        cache_read: Arc<dyn TransactionCacheRead>,
        tx: VerifiedSignedTransaction,
    ) {
        let tx_digest = *tx.digest();
        trace!(?tx_digest, "Tracking signed transaction");
        match self.delay_and_finalize_tx(cache_read, tx).await {
            Ok(did_run) => {
                if did_run {
                    debug!(?tx_digest, "Transaction finalized");
                }
            }
            Err(err) => {
                error!(?tx_digest, ?err, "Failed to finalize transaction");
            }
        }
    }

    async fn delay_and_finalize_tx(
        &self,
        cache_read: Arc<dyn TransactionCacheRead>,
        tx: VerifiedSignedTransaction,
    ) -> anyhow::Result<bool> {
        tokio::time::sleep(self.tx_finalization_delay).await;
        let tx_digest = *tx.digest();
        trace!(?tx_digest, "Waking up to finalize transaction");
        if cache_read.is_tx_already_executed(&tx_digest)? {
            trace!(?tx_digest, "Transaction already finalized");
            return Ok(false);
        }
        let start_time = self.metrics.start_finalization();
        debug!(
            ?tx_digest,
            "Invoking authority aggregator to finalize transaction"
        );
        tokio::time::timeout(
            self.finalization_timeout,
            self.agg
                .execute_transaction_block(tx.into_unsigned().inner(), None),
        )
        .await??;
        self.metrics.finalization_succeeded(start_time);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::authority::AuthorityState;
    use crate::authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder};
    use crate::authority_client::AuthorityAPI;
    use crate::validator_tx_finalizer::ValidatorTxFinalizer;
    use async_trait::async_trait;
    use std::collections::BTreeMap;
    use std::iter;
    use std::net::SocketAddr;
    use std::num::NonZeroUsize;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::Relaxed;
    use std::sync::Arc;
    use sui_macros::sim_test;
    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{AuthorityName, ObjectID, SuiAddress, TransactionDigest};
    use sui_types::committee::{CommitteeTrait, StakeUnit};
    use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
    use sui_types::effects::{TransactionEffectsAPI, TransactionEvents};
    use sui_types::error::SuiError;
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    };
    use sui_types::messages_grpc::{
        HandleCertificateRequestV3, HandleCertificateResponseV2, HandleCertificateResponseV3,
        HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
        HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse, SystemStateRequest,
        TransactionInfoRequest, TransactionInfoResponse,
    };
    use sui_types::object::Object;
    use sui_types::sui_system_state::SuiSystemState;
    use sui_types::transaction::{
        CertifiedTransaction, SignedTransaction, Transaction, VerifiedCertificate,
        VerifiedSignedTransaction, VerifiedTransaction,
    };
    use sui_types::utils::to_sender_signed_transaction;

    #[derive(Clone)]
    struct MockAuthorityClient {
        authority: Arc<AuthorityState>,
        inject_fault: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AuthorityAPI for MockAuthorityClient {
        async fn handle_transaction(
            &self,
            transaction: Transaction,
            _client_addr: Option<SocketAddr>,
        ) -> Result<HandleTransactionResponse, SuiError> {
            if self.inject_fault.load(Relaxed) {
                return Err(SuiError::TimeoutError);
            }
            let epoch_store = self.authority.epoch_store_for_testing();
            self.authority
                .handle_transaction(
                    &epoch_store,
                    VerifiedTransaction::new_unchecked(transaction),
                )
                .await
        }

        async fn handle_certificate_v2(
            &self,
            certificate: CertifiedTransaction,
            _client_addr: Option<SocketAddr>,
        ) -> Result<HandleCertificateResponseV2, SuiError> {
            let epoch_store = self.authority.epoch_store_for_testing();
            let (effects, _) = self
                .authority
                .try_execute_immediately(
                    &VerifiedExecutableTransaction::new_from_certificate(
                        VerifiedCertificate::new_unchecked(certificate),
                    ),
                    None,
                    &epoch_store,
                )
                .await?;
            let events = match effects.events_digest() {
                None => TransactionEvents::default(),
                Some(digest) => self.authority.get_transaction_events(digest)?,
            };
            let signed_effects = self
                .authority
                .sign_effects(effects, &epoch_store)?
                .into_inner();
            Ok(HandleCertificateResponseV2 {
                signed_effects,
                events,
                fastpath_input_objects: vec![],
            })
        }

        async fn handle_certificate_v3(
            &self,
            _request: HandleCertificateRequestV3,
            _client_addr: Option<SocketAddr>,
        ) -> Result<HandleCertificateResponseV3, SuiError> {
            unimplemented!()
        }

        async fn handle_soft_bundle_certificates_v3(
            &self,
            _request: HandleSoftBundleCertificatesRequestV3,
            _client_addr: Option<SocketAddr>,
        ) -> Result<HandleSoftBundleCertificatesResponseV3, SuiError> {
            unimplemented!()
        }

        async fn handle_object_info_request(
            &self,
            _request: ObjectInfoRequest,
        ) -> Result<ObjectInfoResponse, SuiError> {
            unimplemented!()
        }

        async fn handle_transaction_info_request(
            &self,
            _request: TransactionInfoRequest,
        ) -> Result<TransactionInfoResponse, SuiError> {
            unimplemented!()
        }

        async fn handle_checkpoint(
            &self,
            _request: CheckpointRequest,
        ) -> Result<CheckpointResponse, SuiError> {
            unimplemented!()
        }

        async fn handle_checkpoint_v2(
            &self,
            _request: CheckpointRequestV2,
        ) -> Result<CheckpointResponseV2, SuiError> {
            unimplemented!()
        }

        async fn handle_system_state_object(
            &self,
            _request: SystemStateRequest,
        ) -> Result<SuiSystemState, SuiError> {
            unimplemented!()
        }
    }

    #[sim_test]
    async fn test_validator_tx_finalizer_basic_flow() {
        telemetry_subscribers::init_for_testing();
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_id = gas_object.id();
        let (states, auth_agg, clients) = create_validators(gas_object).await;
        let finalizer1 = ValidatorTxFinalizer::new_for_testing(
            auth_agg.clone(),
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(60),
        );
        let signed_tx = create_tx(&clients, &states[0], sender, &keypair, gas_object_id).await;
        let tx_digest = *signed_tx.digest();
        let cache_read = states[0].get_transaction_cache_reader().clone();
        let metrics = finalizer1.metrics.clone();
        let handle = tokio::spawn(async move {
            finalizer1.track_signed_tx(cache_read, signed_tx).await;
        });
        handle.await.unwrap();
        check_quorum_execution(&auth_agg, &clients, &tx_digest, true);
        assert_eq!(
            metrics.num_finalization_attempts_for_testing.load(Relaxed),
            1
        );
        assert_eq!(
            metrics
                .num_successful_finalizations_for_testing
                .load(Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn test_validator_tx_finalizer_new_epoch() {
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_id = gas_object.id();
        let (states, auth_agg, clients) = create_validators(gas_object).await;
        let finalizer1 = ValidatorTxFinalizer::new_for_testing(
            auth_agg.clone(),
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(60),
        );
        let signed_tx = create_tx(&clients, &states[0], sender, &keypair, gas_object_id).await;
        let tx_digest = *signed_tx.digest();
        let epoch_store = states[0].epoch_store_for_testing();
        let cache_read = states[0].get_transaction_cache_reader().clone();

        let metrics = finalizer1.metrics.clone();
        let handle = tokio::spawn(async move {
            let _ = epoch_store
                .within_alive_epoch(finalizer1.track_signed_tx(cache_read, signed_tx))
                .await;
        });
        states[0].reconfigure_for_testing().await;
        handle.await.unwrap();
        check_quorum_execution(&auth_agg, &clients, &tx_digest, false);
        assert_eq!(
            metrics.num_finalization_attempts_for_testing.load(Relaxed),
            0
        );
        assert_eq!(
            metrics
                .num_successful_finalizations_for_testing
                .load(Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn test_validator_tx_finalizer_already_executed() {
        telemetry_subscribers::init_for_testing();
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_id = gas_object.id();
        let (states, auth_agg, clients) = create_validators(gas_object).await;
        let finalizer1 = ValidatorTxFinalizer::new_for_testing(
            auth_agg.clone(),
            std::time::Duration::from_secs(20),
            std::time::Duration::from_secs(60),
        );
        let signed_tx = create_tx(&clients, &states[0], sender, &keypair, gas_object_id).await;
        let tx_digest = *signed_tx.digest();
        let cache_read = states[0].get_transaction_cache_reader().clone();

        let metrics = finalizer1.metrics.clone();
        let signed_tx_clone = signed_tx.clone();
        let handle = tokio::spawn(async move {
            finalizer1
                .track_signed_tx(cache_read, signed_tx_clone)
                .await;
        });
        auth_agg
            .execute_transaction_block(&signed_tx.into_inner().into_unsigned(), None)
            .await
            .unwrap();
        handle.await.unwrap();
        check_quorum_execution(&auth_agg, &clients, &tx_digest, true);
        assert_eq!(
            metrics.num_finalization_attempts_for_testing.load(Relaxed),
            0
        );
        assert_eq!(
            metrics
                .num_successful_finalizations_for_testing
                .load(Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn test_validator_tx_finalizer_timeout() {
        telemetry_subscribers::init_for_testing();
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_id = gas_object.id();
        let (states, auth_agg, clients) = create_validators(gas_object).await;
        let finalizer1 = ValidatorTxFinalizer::new_for_testing(
            auth_agg.clone(),
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(30),
        );
        let signed_tx = create_tx(&clients, &states[0], sender, &keypair, gas_object_id).await;
        let tx_digest = *signed_tx.digest();
        let cache_read = states[0].get_transaction_cache_reader().clone();
        for client in clients.values() {
            client.inject_fault.store(true, Relaxed);
        }

        let metrics = finalizer1.metrics.clone();
        let signed_tx_clone = signed_tx.clone();
        let handle = tokio::spawn(async move {
            finalizer1
                .track_signed_tx(cache_read, signed_tx_clone)
                .await;
        });
        handle.await.unwrap();
        check_quorum_execution(&auth_agg, &clients, &tx_digest, false);
        assert_eq!(
            metrics.num_finalization_attempts_for_testing.load(Relaxed),
            1
        );
        assert_eq!(
            metrics
                .num_successful_finalizations_for_testing
                .load(Relaxed),
            0
        );
    }

    async fn create_validators(
        gas_object: Object,
    ) -> (
        Vec<Arc<AuthorityState>>,
        Arc<AuthorityAggregator<MockAuthorityClient>>,
        BTreeMap<AuthorityName, MockAuthorityClient>,
    ) {
        let network_config = ConfigBuilder::new_with_temp_dir()
            .committee_size(NonZeroUsize::new(4).unwrap())
            .with_objects(iter::once(gas_object))
            .build();
        let mut authority_states = vec![];
        for idx in 0..4 {
            let state = TestAuthorityBuilder::new()
                .with_network_config(&network_config, idx)
                .build()
                .await;
            authority_states.push(state);
        }
        let clients: BTreeMap<_, _> = authority_states
            .iter()
            .map(|state| {
                (
                    state.name,
                    MockAuthorityClient {
                        authority: state.clone(),
                        inject_fault: Arc::new(AtomicBool::new(false)),
                    },
                )
            })
            .collect();
        let auth_agg = AuthorityAggregatorBuilder::from_network_config(&network_config)
            .build_custom_clients(clients.clone());
        (authority_states, Arc::new(auth_agg), clients)
    }

    async fn create_tx(
        clients: &BTreeMap<AuthorityName, MockAuthorityClient>,
        state: &Arc<AuthorityState>,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas_object_id: ObjectID,
    ) -> VerifiedSignedTransaction {
        let gas_object_ref = state
            .get_object(&gas_object_id)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference();
        let tx_data = TestTransactionBuilder::new(
            sender,
            gas_object_ref,
            state.reference_gas_price_for_testing().unwrap(),
        )
        .transfer_sui(None, sender)
        .build();
        let tx = to_sender_signed_transaction(tx_data, keypair);
        let response = clients
            .get(&state.name)
            .unwrap()
            .handle_transaction(tx.clone(), None)
            .await
            .unwrap();
        VerifiedSignedTransaction::new_unchecked(SignedTransaction::new_from_data_and_sig(
            tx.into_data(),
            response.status.into_signed_for_testing(),
        ))
    }

    fn check_quorum_execution(
        auth_agg: &Arc<AuthorityAggregator<MockAuthorityClient>>,
        clients: &BTreeMap<AuthorityName, MockAuthorityClient>,
        tx_digest: &TransactionDigest,
        expected: bool,
    ) {
        let quorum = auth_agg.committee.quorum_threshold();
        let executed_weight: StakeUnit = clients
            .iter()
            .filter_map(|(name, client)| {
                client
                    .authority
                    .is_tx_already_executed(tx_digest)
                    .unwrap()
                    .then_some(auth_agg.committee.weight(name))
            })
            .sum();
        assert_eq!(executed_weight >= quorum, expected);
    }
}
