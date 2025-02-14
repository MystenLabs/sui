// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use mysten_metrics::spawn_monitored_task;
use mysten_network::server::SUI_TLS_SERVER_NAME;
use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, Histogram, IntCounter, IntCounterVec, Registry,
};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::SystemTime,
};
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use sui_types::messages_grpc::{
    HandleCertificateRequestV3, HandleCertificateResponseV3, HandleTransactionResponseV2,
};
use sui_types::messages_grpc::{
    HandleCertificateResponseV2, HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse,
    SubmitCertificateResponse, SystemStateRequest, TransactionInfoRequest, TransactionInfoResponse,
};
use sui_types::messages_grpc::{
    HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::traffic_control::{ClientIdSource, PolicyConfig, RemoteFirewallConfig, Weight};
use sui_types::{effects::TransactionEffectsAPI, messages_grpc::HandleTransactionRequestV2};
use sui_types::{error::*, transaction::*};
use sui_types::{
    fp_ensure,
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
};
use tap::TapFallible;
use tonic::metadata::{Ascii, MetadataValue};
use tracing::{error, error_span, info, Instrument};

use crate::{
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
    mysticeti_adapter::LazyMysticetiClient,
};
use crate::{
    authority::AuthorityState,
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics},
    traffic_controller::parse_ip,
    traffic_controller::policies::TrafficTally,
    traffic_controller::TrafficController,
};
use crate::{
    consensus_adapter::ConnectionMonitorStatusForTests,
    traffic_controller::metrics::TrafficControllerMetrics,
};
use nonempty::{nonempty, NonEmpty};
use sui_config::local_ip_utils::new_local_tcp_address_for_testing;
use tonic::transport::server::TcpConnectInfo;

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

pub struct AuthorityServerHandle {
    server_handle: mysten_network::server::Server,
}

impl AuthorityServerHandle {
    pub async fn join(self) -> Result<(), io::Error> {
        self.server_handle.handle().wait_for_shutdown().await;
        Ok(())
    }

    pub async fn kill(self) -> Result<(), io::Error> {
        self.server_handle.handle().shutdown().await;
        Ok(())
    }

    pub fn address(&self) -> &Multiaddr {
        self.server_handle.local_addr()
    }
}

pub struct AuthorityServer {
    address: Multiaddr,
    pub state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    pub metrics: Arc<ValidatorServiceMetrics>,
}

impl AuthorityServer {
    pub fn new_for_test_with_consensus_adapter(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
    ) -> Self {
        let address = new_local_tcp_address_for_testing();
        let metrics = Arc::new(ValidatorServiceMetrics::new_for_tests());

        Self {
            address,
            state,
            consensus_adapter,
            metrics,
        }
    }

    pub fn new_for_test(state: Arc<AuthorityState>) -> Self {
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(LazyMysticetiClient::new()),
            state.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            state.epoch_store_for_testing().protocol_config().clone(),
        ));
        Self::new_for_test_with_consensus_adapter(state, consensus_adapter)
    }

    pub async fn spawn_for_test(self) -> Result<AuthorityServerHandle, io::Error> {
        let address = self.address.clone();
        self.spawn_with_bind_address_for_test(address).await
    }

    pub async fn spawn_with_bind_address_for_test(
        self,
        address: Multiaddr,
    ) -> Result<AuthorityServerHandle, io::Error> {
        let tls_config = sui_tls::create_rustls_server_config(
            self.state.config.network_key_pair().copy().private(),
            SUI_TLS_SERVER_NAME.to_string(),
        );
        let server = mysten_network::config::Config::new()
            .server_builder()
            .add_service(ValidatorServer::new(ValidatorService::new_for_tests(
                self.state,
                self.consensus_adapter,
                self.metrics,
            )))
            .bind(&address, Some(tls_config))
            .await
            .unwrap();
        let local_addr = server.local_addr().to_owned();
        info!("Listening to traffic on {local_addr}");
        let handle = AuthorityServerHandle {
            server_handle: server,
        };
        Ok(handle)
    }
}

pub struct ValidatorServiceMetrics {
    pub signature_errors: IntCounter,
    pub tx_verification_latency: Histogram,
    pub cert_verification_latency: Histogram,
    pub consensus_latency: Histogram,
    pub handle_transaction_latency: Histogram,
    pub handle_transaction_v2_latency: Histogram,
    pub submit_certificate_consensus_latency: Histogram,
    pub handle_certificate_consensus_latency: Histogram,
    pub handle_certificate_non_consensus_latency: Histogram,
    pub handle_soft_bundle_certificates_consensus_latency: Histogram,
    pub handle_soft_bundle_certificates_count: Histogram,
    pub handle_soft_bundle_certificates_size_bytes: Histogram,
    pub handle_transaction_consensus_latency: Histogram,

    num_rejected_tx_in_epoch_boundary: IntCounter,
    num_rejected_cert_in_epoch_boundary: IntCounter,
    num_rejected_tx_during_overload: IntCounterVec,
    num_rejected_cert_during_overload: IntCounterVec,
    connection_ip_not_found: IntCounter,
    forwarded_header_parse_error: IntCounter,
    forwarded_header_invalid: IntCounter,
    forwarded_header_not_included: IntCounter,
    client_id_source_config_mismatch: IntCounter,
}

impl ValidatorServiceMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            signature_errors: register_int_counter_with_registry!(
                "total_signature_errors",
                "Number of transaction signature errors",
                registry,
            )
            .unwrap(),
            tx_verification_latency: register_histogram_with_registry!(
                "validator_service_tx_verification_latency",
                "Latency of verifying a transaction",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            cert_verification_latency: register_histogram_with_registry!(
                "validator_service_cert_verification_latency",
                "Latency of verifying a certificate",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            consensus_latency: register_histogram_with_registry!(
                "validator_service_consensus_latency",
                "Time spent between submitting a txn to consensus and getting back local acknowledgement. Execution and finalization time are not included.",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_latency",
                "Latency of handling a transaction",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_v2_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_v2_latency",
                "Latency of v2 transaction handler",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_certificate_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_certificate_consensus_latency",
                "Latency of handling a consensus transaction certificate",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            submit_certificate_consensus_latency: register_histogram_with_registry!(
                "validator_service_submit_certificate_consensus_latency",
                "Latency of submit_certificate RPC handler",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_certificate_non_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_certificate_non_consensus_latency",
                "Latency of handling a non-consensus transaction certificate",
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_soft_bundle_certificates_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_soft_bundle_certificates_consensus_latency",
                "Latency of handling a consensus soft bundle",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_soft_bundle_certificates_count: register_histogram_with_registry!(
                "handle_soft_bundle_certificates_count",
                "The number of certificates included in a soft bundle",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_soft_bundle_certificates_size_bytes: register_histogram_with_registry!(
                "handle_soft_bundle_certificates_size_bytes",
                "The size of soft bundle in bytes",
                mysten_metrics::BYTES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_consensus_latency",
                "Latency of handling a user transaction sent through consensus",
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            num_rejected_tx_in_epoch_boundary: register_int_counter_with_registry!(
                "validator_service_num_rejected_tx_in_epoch_boundary",
                "Number of rejected transaction during epoch transitioning",
                registry,
            )
            .unwrap(),
            num_rejected_cert_in_epoch_boundary: register_int_counter_with_registry!(
                "validator_service_num_rejected_cert_in_epoch_boundary",
                "Number of rejected transaction certificate during epoch transitioning",
                registry,
            )
            .unwrap(),
            num_rejected_tx_during_overload: register_int_counter_vec_with_registry!(
                "validator_service_num_rejected_tx_during_overload",
                "Number of rejected transaction due to system overload",
                &["error_type"],
                registry,
            )
            .unwrap(),
            num_rejected_cert_during_overload: register_int_counter_vec_with_registry!(
                "validator_service_num_rejected_cert_during_overload",
                "Number of rejected transaction certificate due to system overload",
                &["error_type"],
                registry,
            )
            .unwrap(),
            connection_ip_not_found: register_int_counter_with_registry!(
                "validator_service_connection_ip_not_found",
                "Number of times connection IP was not extractable from request",
                registry,
            )
            .unwrap(),
            forwarded_header_parse_error: register_int_counter_with_registry!(
                "validator_service_forwarded_header_parse_error",
                "Number of times x-forwarded-for header could not be parsed",
                registry,
            )
            .unwrap(),
            forwarded_header_invalid: register_int_counter_with_registry!(
                "validator_service_forwarded_header_invalid",
                "Number of times x-forwarded-for header was invalid",
                registry,
            )
            .unwrap(),
            forwarded_header_not_included: register_int_counter_with_registry!(
                "validator_service_forwarded_header_not_included",
                "Number of times x-forwarded-for header was (unexpectedly) not included in request",
                registry,
            )
            .unwrap(),
            client_id_source_config_mismatch: register_int_counter_with_registry!(
                "validator_service_client_id_source_config_mismatch",
                "Number of times detected that client id source config doesn't agree with x-forwarded-for header",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

#[derive(Clone)]
pub struct ValidatorService {
    state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    metrics: Arc<ValidatorServiceMetrics>,
    traffic_controller: Option<Arc<TrafficController>>,
    client_id_source: Option<ClientIdSource>,
}

impl ValidatorService {
    pub fn new(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        validator_metrics: Arc<ValidatorServiceMetrics>,
        traffic_controller_metrics: TrafficControllerMetrics,
        policy_config: Option<PolicyConfig>,
        firewall_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        Self {
            state,
            consensus_adapter,
            metrics: validator_metrics,
            traffic_controller: policy_config.clone().map(|policy| {
                Arc::new(TrafficController::init(
                    policy,
                    traffic_controller_metrics,
                    firewall_config,
                ))
            }),
            client_id_source: policy_config.map(|policy| policy.client_id_source),
        }
    }

    pub fn new_for_tests(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Self {
        Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: None,
            client_id_source: None,
        }
    }

    pub fn validator_state(&self) -> &Arc<AuthorityState> {
        &self.state
    }

    pub async fn execute_certificate_for_testing(
        &self,
        cert: CertifiedTransaction,
    ) -> Result<tonic::Response<HandleCertificateResponseV2>, tonic::Status> {
        let request = make_tonic_request_for_testing(cert);
        self.handle_certificate_v2(request).await
    }

    pub async fn handle_transaction_for_benchmarking(
        &self,
        transaction: Transaction,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let request = make_tonic_request_for_testing(transaction);
        self.transaction(request).await
    }

    // When making changes to this function, see if the changes should be applied to
    // `Self::handle_transaction_v2()` and `SuiTxValidator::vote_transaction()` as well.
    async fn handle_transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> WrappedServiceResponse<HandleTransactionResponse> {
        let Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: _,
            client_id_source: _,
        } = self.clone();
        let transaction = request.into_inner();
        let epoch_store = state.load_epoch_store_one_call_per_task();

        transaction.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        // When authority is overloaded and decide to reject this tx, we still lock the object
        // and ask the client to retry in the future. This is because without locking, the
        // input objects can be locked by a different tx in the future, however, the input objects
        // may already be locked by this tx in other validators. This can cause non of the txes
        // to have enough quorum to form a certificate, causing the objects to be locked for
        // the entire epoch. By doing locking but pushback, retrying transaction will have
        // higher chance to succeed.
        let mut validator_pushback_error = None;
        let overload_check_res = state.check_system_overload(
            &*consensus_adapter,
            transaction.data(),
            state.check_system_overload_at_signing(),
        );
        if let Err(error) = overload_check_res {
            metrics
                .num_rejected_tx_during_overload
                .with_label_values(&[error.as_ref()])
                .inc();
            // TODO: consider change the behavior for other types of overload errors.
            match error {
                SuiError::ValidatorOverloadedRetryAfter { .. } => {
                    validator_pushback_error = Some(error)
                }
                _ => return Err(error.into()),
            }
        }

        let _handle_tx_metrics_guard = metrics.handle_transaction_latency.start_timer();

        let tx_verif_metrics_guard = metrics.tx_verification_latency.start_timer();
        let transaction = epoch_store.verify_transaction(transaction).tap_err(|_| {
            metrics.signature_errors.inc();
        })?;
        drop(tx_verif_metrics_guard);

        let tx_digest = transaction.digest();

        // Enable Trace Propagation across spans/processes using tx_digest
        let span = error_span!("validator_state_process_tx", ?tx_digest);

        let info = state
            .handle_transaction(&epoch_store, transaction.clone())
            .instrument(span)
            .await
            .tap_err(|e| {
                if let SuiError::ValidatorHaltedAtEpochEnd = e {
                    metrics.num_rejected_tx_in_epoch_boundary.inc();
                }
            })?;

        if let Some(error) = validator_pushback_error {
            // TODO: right now, we still sign the txn, but just don't return it. We can also skip signing
            // to save more CPU.
            return Err(error.into());
        }

        Ok((tonic::Response::new(info), Weight::zero()))
    }

    async fn handle_transaction_v2(
        &self,
        request: tonic::Request<HandleTransactionRequestV2>,
    ) -> WrappedServiceResponse<HandleTransactionResponseV2> {
        let Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: _,
            client_id_source: _,
        } = self.clone();
        let epoch_store = state.load_epoch_store_one_call_per_task();
        if !epoch_store.protocol_config().mysticeti_fastpath() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "Mysticeti fastpath".to_string(),
            }
            .into());
        }

        let HandleTransactionRequestV2 {
            transaction,
            include_events,
            include_input_objects,
            include_output_objects,
            include_auxiliary_data,
        } = request.into_inner();

        transaction.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        // Check system overload
        let overload_check_res = self.state.check_system_overload(
            &*consensus_adapter,
            transaction.data(),
            state.check_system_overload_at_signing(),
        );
        if let Err(error) = overload_check_res {
            metrics
                .num_rejected_tx_during_overload
                .with_label_values(&[error.as_ref()])
                .inc();
            return Err(error.into());
        }

        let _handle_tx_metrics_guard = metrics.handle_transaction_v2_latency.start_timer();

        let transaction = {
            let _metrics_guard = metrics.tx_verification_latency.start_timer();
            epoch_store.verify_transaction(transaction).tap_err(|_| {
                metrics.signature_errors.inc();
            })?
        };

        // Enable Trace Propagation across spans/processes using tx_digest
        let tx_digest = transaction.digest();
        let _span = error_span!("validator_state_handle_tx_v2", ?tx_digest);

        let tx_output = state
            .handle_vote_transaction(&epoch_store, transaction.clone())
            .tap_err(|e| {
                if let SuiError::ValidatorHaltedAtEpochEnd = e {
                    metrics.num_rejected_tx_in_epoch_boundary.inc();
                }
            })?;
        // Fetch remaining fields if the transaction has been executed.
        if let Some((effects, events)) = tx_output {
            let input_objects = include_input_objects
                .then(|| state.get_transaction_input_objects(&effects))
                .and_then(Result::ok);
            let output_objects = include_output_objects
                .then(|| state.get_transaction_output_objects(&effects))
                .and_then(Result::ok);

            return Ok((
                tonic::Response::new(HandleTransactionResponseV2 {
                    effects,
                    events: include_events.then_some(events),
                    input_objects,
                    output_objects,
                    auxiliary_data: None, // We don't have any aux data generated presently
                }),
                Weight::zero(),
            ));
        }

        let _latency_metric_guard = metrics.handle_transaction_consensus_latency.start_timer();
        let span = error_span!("handle_transaction_v2", tx_digest = ?transaction.digest());
        self.handle_submit_to_consensus(
            nonempty![ConsensusTransaction::new_user_transaction_message(
                &self.state.name,
                transaction.into()
            )],
            include_events,
            include_input_objects,
            include_output_objects,
            include_auxiliary_data,
            &epoch_store,
            true,
        )
        .instrument(span)
        .await
        .map(|(resp, spam_weight)| {
            (
                tonic::Response::new(
                    resp.expect(
                        "handle_submit_to_consensus should not return none with wait_for_effects=true",
                    )
                    .remove(0),
                ),
                spam_weight,
            )
        })
    }

    // In addition to the response from handling the certificates,
    // returns a bool indicating whether the request should be tallied
    // toward spam count. In general, this should be set to true for
    // requests that are read-only and thus do not consume gas, such
    // as when the transaction is already executed.
    async fn handle_certificates(
        &self,
        certificates: NonEmpty<CertifiedTransaction>,
        include_events: bool,
        include_input_objects: bool,
        include_output_objects: bool,
        include_auxiliary_data: bool,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        wait_for_effects: bool,
    ) -> Result<(Option<Vec<HandleCertificateResponseV3>>, Weight), tonic::Status> {
        // Validate if cert can be executed
        // Fullnode does not serve handle_certificate call.
        fp_ensure!(
            !self.state.is_fullnode(epoch_store),
            SuiError::FullNodeCantHandleCertificate.into()
        );

        let shared_object_tx = certificates
            .iter()
            .any(|cert| cert.contains_shared_object());

        let metrics = if certificates.len() == 1 {
            if wait_for_effects {
                if shared_object_tx {
                    &self.metrics.handle_certificate_consensus_latency
                } else {
                    &self.metrics.handle_certificate_non_consensus_latency
                }
            } else {
                &self.metrics.submit_certificate_consensus_latency
            }
        } else {
            // `soft_bundle_validity_check` ensured that all certificates contain shared objects.
            &self
                .metrics
                .handle_soft_bundle_certificates_consensus_latency
        };

        let _metrics_guard = metrics.start_timer();

        // 1) Check if the certificate is already executed.
        //    This is only needed when we have only one certificate (not a soft bundle).
        //    When multiple certificates are provided, we will either submit all of them or none of them to consensus.
        if certificates.len() == 1 {
            let tx_digest = *certificates[0].digest();

            if let Some(signed_effects) = self
                .state
                .get_signed_effects_and_maybe_resign(&tx_digest, epoch_store)?
            {
                let events = if include_events {
                    if let Some(digest) = signed_effects.events_digest() {
                        Some(self.state.get_transaction_events(digest)?)
                    } else {
                        None
                    }
                } else {
                    None
                };

                return Ok((
                    Some(vec![HandleCertificateResponseV3 {
                        effects: signed_effects.into_inner(),
                        events,
                        input_objects: None,
                        output_objects: None,
                        auxiliary_data: None,
                    }]),
                    Weight::one(),
                ));
            };
        }

        // 2) Verify the certificates.
        // Check system overload
        for certificate in &certificates {
            let overload_check_res = self.state.check_system_overload(
                &*self.consensus_adapter,
                certificate.data(),
                self.state.check_system_overload_at_execution(),
            );
            if let Err(error) = overload_check_res {
                self.metrics
                    .num_rejected_cert_during_overload
                    .with_label_values(&[error.as_ref()])
                    .inc();
                return Err(error.into());
            }
        }

        let verified_certificates = {
            let _timer = self.metrics.cert_verification_latency.start_timer();
            epoch_store
                .signature_verifier
                .multi_verify_certs(certificates.into())
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
        };
        let consensus_transactions =
            NonEmpty::collect(verified_certificates.iter().map(|certificate| {
                ConsensusTransaction::new_certificate_message(
                    &self.state.name,
                    certificate.clone().into(),
                )
            }))
            .unwrap();

        let (responses, weight) = self
            .handle_submit_to_consensus(
                consensus_transactions,
                include_events,
                include_input_objects,
                include_output_objects,
                include_auxiliary_data,
                epoch_store,
                wait_for_effects,
            )
            .await?;
        // Sign the returned TransactionEffects.
        let responses = if let Some(responses) = responses {
            Some(
                responses
                    .into_iter()
                    .map(|response| {
                        let signed_effects =
                            self.state.sign_effects(response.effects, epoch_store)?;
                        Ok(HandleCertificateResponseV3 {
                            effects: signed_effects.into_inner(),
                            events: response.events,
                            input_objects: response.input_objects,
                            output_objects: response.output_objects,
                            auxiliary_data: response.auxiliary_data,
                        })
                    })
                    .collect::<Result<Vec<HandleCertificateResponseV3>, tonic::Status>>()?,
            )
        } else {
            None
        };

        Ok((responses, weight))
    }

    async fn handle_submit_to_consensus(
        &self,
        consensus_transactions: NonEmpty<ConsensusTransaction>,
        include_events: bool,
        include_input_objects: bool,
        include_output_objects: bool,
        _include_auxiliary_data: bool,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        wait_for_effects: bool,
    ) -> Result<(Option<Vec<HandleTransactionResponseV2>>, Weight), tonic::Status> {
        let consensus_transactions: Vec<_> = consensus_transactions.into();
        {
            // code block within reconfiguration lock
            let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
            if !reconfiguration_lock.should_accept_user_certs() {
                self.metrics.num_rejected_cert_in_epoch_boundary.inc();
                return Err(SuiError::ValidatorHaltedAtEpochEnd.into());
            }

            // 3) All transactions are sent to consensus (at least by some authorities)
            // For certs with shared objects this will wait until either timeout or we have heard back from consensus.
            // For certs with owned objects this will return without waiting for certificate to be sequenced.
            // For uncertified transactions this will wait for fast path processing.
            // First do quick dirty non-async check.
            if !epoch_store.all_external_consensus_messages_processed(
                consensus_transactions.iter().map(|tx| tx.key()),
            )? {
                let _metrics_guard = self.metrics.consensus_latency.start_timer();
                self.consensus_adapter.submit_batch(
                    &consensus_transactions,
                    Some(&reconfiguration_lock),
                    epoch_store,
                )?;
                // Do not wait for the result, because the transaction might have already executed.
                // Instead, check or wait for the existence of certificate effects below.
            }
        }

        if !wait_for_effects {
            // It is useful to enqueue owned object transaction for execution locally,
            // even when we are not returning effects to user
            let certificates_without_shared_objects = consensus_transactions
                .iter()
                .filter_map(|tx| {
                    if let ConsensusTransactionKind::CertifiedTransaction(certificate) = &tx.kind {
                        (!certificate.contains_shared_object())
                            // Certificates already verified by callers of this function.
                            .then_some(VerifiedCertificate::new_unchecked(*(certificate.clone())))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            if !certificates_without_shared_objects.is_empty() {
                self.state.enqueue_certificates_for_execution(
                    certificates_without_shared_objects,
                    epoch_store,
                );
            }
            return Ok((None, Weight::zero()));
        }

        // 4) Execute the certificates immediately if they contain only owned object transactions,
        // or wait for the execution results if it contains shared objects.
        let responses = futures::future::try_join_all(consensus_transactions.into_iter().map(
            |tx| async move {
                let effects = match &tx.kind {
                    ConsensusTransactionKind::CertifiedTransaction(certificate) => {
                        // Certificates already verified by callers of this function.
                        let certificate = VerifiedCertificate::new_unchecked(*(certificate.clone()));
                        self.state
                            .execute_certificate(&certificate, epoch_store)
                            .await?
                    }
                    ConsensusTransactionKind::UserTransaction(tx) => {
                        self.state.await_transaction_effects(*tx.digest(), epoch_store).await?
                    }
                    _ => panic!("`handle_submit_to_consensus` received transaction that is not a CertifiedTransaction or UserTransaction"),
                };
                let events = if include_events {
                    if let Some(digest) = effects.events_digest() {
                        Some(self.state.get_transaction_events(digest)?)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let input_objects = include_input_objects
                    .then(|| self.state.get_transaction_input_objects(&effects))
                    .and_then(Result::ok);

                let output_objects = include_output_objects
                    .then(|| self.state.get_transaction_output_objects(&effects))
                    .and_then(Result::ok);

                if let ConsensusTransactionKind::CertifiedTransaction(certificate) = &tx.kind {
                    epoch_store.insert_tx_cert_sig(certificate.digest(), certificate.auth_sig())?;
                    // TODO(fastpath): Make sure consensus handler does this for a UserTransaction.
                }

                Ok::<_, SuiError>(HandleTransactionResponseV2 {
                    effects,
                    events,
                    input_objects,
                    output_objects,
                    auxiliary_data: None, // We don't have any aux data generated presently
                })
            },
        ))
        .await?;

        Ok((Some(responses), Weight::zero()))
    }
}

type WrappedServiceResponse<T> = Result<(tonic::Response<T>, Weight), tonic::Status>;

impl ValidatorService {
    async fn transaction_impl(
        &self,
        request: tonic::Request<Transaction>,
    ) -> WrappedServiceResponse<HandleTransactionResponse> {
        self.handle_transaction(request).await
    }

    async fn transaction_v2_impl(
        &self,
        request: tonic::Request<HandleTransactionRequestV2>,
    ) -> WrappedServiceResponse<HandleTransactionResponseV2> {
        self.handle_transaction_v2(request).await
    }

    async fn submit_certificate_impl(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> WrappedServiceResponse<SubmitCertificateResponse> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let certificate = request.into_inner();
        certificate.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        let span = error_span!("submit_certificate", tx_digest = ?certificate.digest());
        self.handle_certificates(
            nonempty![certificate],
            true,
            false,
            false,
            false,
            &epoch_store,
            false,
        )
        .instrument(span)
        .await
        .map(|(executed, spam_weight)| {
            (
                tonic::Response::new(SubmitCertificateResponse {
                    executed: executed.map(|mut x| x.remove(0)).map(Into::into),
                }),
                spam_weight,
            )
        })
    }

    async fn handle_certificate_v2_impl(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> WrappedServiceResponse<HandleCertificateResponseV2> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let certificate = request.into_inner();
        certificate.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        let span = error_span!("handle_certificate", tx_digest = ?certificate.digest());
        self.handle_certificates(
            nonempty![certificate],
            true,
            false,
            false,
            false,
            &epoch_store,
            true,
        )
        .instrument(span)
        .await
        .map(|(resp, spam_weight)| {
            (
                tonic::Response::new(
                    resp.expect(
                        "handle_certificate should not return none with wait_for_effects=true",
                    )
                    .remove(0)
                    .into(),
                ),
                spam_weight,
            )
        })
    }

    async fn handle_certificate_v3_impl(
        &self,
        request: tonic::Request<HandleCertificateRequestV3>,
    ) -> WrappedServiceResponse<HandleCertificateResponseV3> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let request = request.into_inner();
        request
            .certificate
            .validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        let span = error_span!("handle_certificate_v3", tx_digest = ?request.certificate.digest());
        self.handle_certificates(
            nonempty![request.certificate],
            request.include_events,
            request.include_input_objects,
            request.include_output_objects,
            request.include_auxiliary_data,
            &epoch_store,
            true,
        )
        .instrument(span)
        .await
        .map(|(resp, spam_weight)| {
            (
                tonic::Response::new(
                    resp.expect(
                        "handle_certificate should not return none with wait_for_effects=true",
                    )
                    .remove(0),
                ),
                spam_weight,
            )
        })
    }

    async fn soft_bundle_validity_check(
        &self,
        certificates: &NonEmpty<CertifiedTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        total_size_bytes: u64,
    ) -> Result<(), tonic::Status> {
        let protocol_config = epoch_store.protocol_config();
        let node_config = &self.state.config;

        // Soft Bundle MUST be enabled both in protocol config and local node config.
        //
        // The local node config is by default enabled, but can be turned off by the node operator.
        // This acts an extra safety measure where a validator node have the choice to turn this feature off,
        // without having to upgrade the entire network.
        fp_ensure!(
            protocol_config.soft_bundle() && node_config.enable_soft_bundle,
            SuiError::UnsupportedFeatureError {
                error: "Soft Bundle".to_string()
            }
            .into()
        );

        // Enforce these checks per [SIP-19](https://github.com/sui-foundation/sips/blob/main/sips/sip-19.md):
        // - All certs must access at least one shared object.
        // - All certs must not be already executed.
        // - All certs must have the same gas price.
        // - Number of certs must not exceed the max allowed.
        // - Total size of all certs must not exceed the max allowed.
        fp_ensure!(
            certificates.len() as u64 <= protocol_config.max_soft_bundle_size(),
            SuiError::UserInputError {
                error: UserInputError::TooManyTransactionsInSoftBundle {
                    limit: protocol_config.max_soft_bundle_size()
                }
            }
            .into()
        );

        // We set the soft bundle max size to be half of the consensus max transactions in block size. We do this to account for
        // serialization overheads and to ensure that the soft bundle is not too large when is attempted to be posted via consensus.
        // Although half the block size is on the extreme side, it's should be good enough for now.
        let soft_bundle_max_size_bytes =
            protocol_config.consensus_max_transactions_in_block_bytes() / 2;
        fp_ensure!(
            total_size_bytes <= soft_bundle_max_size_bytes,
            SuiError::UserInputError {
                error: UserInputError::SoftBundleTooLarge {
                    size: total_size_bytes,
                    limit: soft_bundle_max_size_bytes,
                },
            }
            .into()
        );

        let mut gas_price = None;
        for certificate in certificates {
            let tx_digest = *certificate.digest();
            fp_ensure!(
                certificate.contains_shared_object(),
                SuiError::UserInputError {
                    error: UserInputError::NoSharedObjectError { digest: tx_digest }
                }
                .into()
            );
            fp_ensure!(
                !self.state.is_tx_already_executed(&tx_digest),
                SuiError::UserInputError {
                    error: UserInputError::AlreadyExecutedError { digest: tx_digest }
                }
                .into()
            );
            if let Some(gas) = gas_price {
                fp_ensure!(
                    gas == certificate.gas_price(),
                    SuiError::UserInputError {
                        error: UserInputError::GasPriceMismatchError {
                            digest: tx_digest,
                            expected: gas,
                            actual: certificate.gas_price()
                        }
                    }
                    .into()
                );
            } else {
                gas_price = Some(certificate.gas_price());
            }
        }

        // For Soft Bundle, if at this point we know at least one certificate has already been processed,
        // reject the entire bundle.  Otherwise, submit all certificates in one request.
        // This is not a strict check as there may be race conditions where one or more certificates are
        // already being processed by another actor, and we could not know it.
        fp_ensure!(
            !epoch_store.is_any_tx_certs_consensus_message_processed(certificates.iter())?,
            SuiError::UserInputError {
                error: UserInputError::CertificateAlreadyProcessed
            }
            .into()
        );

        Ok(())
    }

    async fn handle_soft_bundle_certificates_v3_impl(
        &self,
        request: tonic::Request<HandleSoftBundleCertificatesRequestV3>,
    ) -> WrappedServiceResponse<HandleSoftBundleCertificatesResponseV3> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let client_addr = if self.client_id_source.is_none() {
            self.get_client_ip_addr(&request, &ClientIdSource::SocketAddr)
        } else {
            self.get_client_ip_addr(&request, self.client_id_source.as_ref().unwrap())
        };
        let request = request.into_inner();

        let certificates = NonEmpty::from_vec(request.certificates)
            .ok_or_else(|| SuiError::NoCertificateProvidedError)?;
        let mut total_size_bytes = 0;
        for certificate in &certificates {
            // We need to check this first because we haven't verified the cert signature.
            total_size_bytes += certificate
                .validity_check(epoch_store.protocol_config(), epoch_store.epoch())?
                as u64;
        }

        self.metrics
            .handle_soft_bundle_certificates_count
            .observe(certificates.len() as f64);

        self.metrics
            .handle_soft_bundle_certificates_size_bytes
            .observe(total_size_bytes as f64);

        // Now that individual certificates are valid, we check if the bundle is valid.
        self.soft_bundle_validity_check(&certificates, &epoch_store, total_size_bytes)
            .await?;

        info!(
            "Received Soft Bundle with {} certificates, from {}, tx digests are [{}], total size [{}]bytes",
            certificates.len(),
            client_addr
                .map(|x| x.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            certificates
                .iter()
                .map(|x| x.digest().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            total_size_bytes
        );

        let span = error_span!("handle_soft_bundle_certificates_v3");
        self.handle_certificates(
            certificates,
            request.include_events,
            request.include_input_objects,
            request.include_output_objects,
            request.include_auxiliary_data,
            &epoch_store,
            request.wait_for_effects,
        )
        .instrument(span)
        .await
        .map(|(resp, spam_weight)| {
            (
                tonic::Response::new(HandleSoftBundleCertificatesResponseV3 {
                    responses: resp.unwrap_or_default(),
                }),
                spam_weight,
            )
        })
    }

    async fn object_info_impl(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> WrappedServiceResponse<ObjectInfoResponse> {
        let request = request.into_inner();
        let response = self.state.handle_object_info_request(request).await?;
        Ok((tonic::Response::new(response), Weight::one()))
    }

    async fn transaction_info_impl(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> WrappedServiceResponse<TransactionInfoResponse> {
        let request = request.into_inner();
        let response = self.state.handle_transaction_info_request(request).await?;
        Ok((tonic::Response::new(response), Weight::one()))
    }

    async fn checkpoint_impl(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> WrappedServiceResponse<CheckpointResponse> {
        let request = request.into_inner();
        let response = self.state.handle_checkpoint_request(&request)?;
        Ok((tonic::Response::new(response), Weight::one()))
    }

    async fn checkpoint_v2_impl(
        &self,
        request: tonic::Request<CheckpointRequestV2>,
    ) -> WrappedServiceResponse<CheckpointResponseV2> {
        let request = request.into_inner();
        let response = self.state.handle_checkpoint_request_v2(&request)?;
        Ok((tonic::Response::new(response), Weight::one()))
    }

    async fn get_system_state_object_impl(
        &self,
        _request: tonic::Request<SystemStateRequest>,
    ) -> WrappedServiceResponse<SuiSystemState> {
        let response = self
            .state
            .get_object_cache_reader()
            .get_sui_system_state_object_unsafe()?;
        Ok((tonic::Response::new(response), Weight::one()))
    }

    fn get_client_ip_addr<T>(
        &self,
        request: &tonic::Request<T>,
        source: &ClientIdSource,
    ) -> Option<IpAddr> {
        match source {
            ClientIdSource::SocketAddr => {
                let socket_addr: Option<SocketAddr> = request.remote_addr();

                // We will hit this case if the IO type used does not
                // implement Connected or when using a unix domain socket.
                // TODO: once we have confirmed that no legitimate traffic
                // is hitting this case, we should reject such requests that
                // hit this case.
                if let Some(socket_addr) = socket_addr {
                    Some(socket_addr.ip())
                } else {
                    if cfg!(msim) {
                        // Ignore the error from simtests.
                    } else if cfg!(test) {
                        panic!("Failed to get remote address from request");
                    } else {
                        self.metrics.connection_ip_not_found.inc();
                        error!("Failed to get remote address from request");
                    }
                    None
                }
            }
            ClientIdSource::XForwardedFor(num_hops) => {
                let do_header_parse = |op: &MetadataValue<Ascii>| {
                    match op.to_str() {
                        Ok(header_val) => {
                            let header_contents =
                                header_val.split(',').map(str::trim).collect::<Vec<_>>();
                            if *num_hops == 0 {
                                error!(
                                    "x-forwarded-for: 0 specified. x-forwarded-for contents: {:?}. Please assign nonzero value for \
                                    number of hops here, or use `socket-addr` client-id-source type if requests are not being proxied \
                                    to this node. Skipping traffic controller request handling.",
                                    header_contents,
                                );
                                return None;
                            }
                            let contents_len = header_contents.len();
                            if contents_len < *num_hops {
                                error!(
                                    "x-forwarded-for header value of {:?} contains {} values, but {} hops were specified. \
                                    Expected at least {} values. Please correctly set the `x-forwarded-for` value under \
                                    `client-id-source` in the node config.",
                                    header_contents,
                                    contents_len,
                                    num_hops,
                                    contents_len,
                                );
                                self.metrics.client_id_source_config_mismatch.inc();
                                return None;
                            }
                            let Some(client_ip) = header_contents.get(contents_len - num_hops)
                            else {
                                error!(
                                    "x-forwarded-for header value of {:?} contains {} values, but {} hops were specificed. \
                                    Expected at least {} values. Skipping traffic controller request handling.",
                                    header_contents,
                                    contents_len,
                                    num_hops,
                                    contents_len,
                                );
                                return None;
                            };
                            parse_ip(client_ip).or_else(|| {
                                self.metrics.forwarded_header_parse_error.inc();
                                None
                            })
                        }
                        Err(e) => {
                            // TODO: once we have confirmed that no legitimate traffic
                            // is hitting this case, we should reject such requests that
                            // hit this case.
                            self.metrics.forwarded_header_invalid.inc();
                            error!("Invalid UTF-8 in x-forwarded-for header: {:?}", e);
                            None
                        }
                    }
                };
                if let Some(op) = request.metadata().get("x-forwarded-for") {
                    do_header_parse(op)
                } else if let Some(op) = request.metadata().get("X-Forwarded-For") {
                    do_header_parse(op)
                } else {
                    self.metrics.forwarded_header_not_included.inc();
                    error!("x-forwarded-for header not present for request despite node configuring x-forwarded-for tracking type");
                    None
                }
            }
        }
    }

    async fn handle_traffic_req(&self, client: Option<IpAddr>) -> Result<(), tonic::Status> {
        if let Some(traffic_controller) = &self.traffic_controller {
            if !traffic_controller.check(&client, &None).await {
                // Entity in blocklist
                Err(tonic::Status::from_error(SuiError::TooManyRequests.into()))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn handle_traffic_resp<T>(
        &self,
        client: Option<IpAddr>,
        wrapped_response: WrappedServiceResponse<T>,
    ) -> Result<tonic::Response<T>, tonic::Status> {
        let (error, spam_weight, unwrapped_response) = match wrapped_response {
            Ok((result, spam_weight)) => (None, spam_weight.clone(), Ok(result)),
            Err(status) => (
                Some(SuiError::from(status.clone())),
                Weight::zero(),
                Err(status.clone()),
            ),
        };

        if let Some(traffic_controller) = self.traffic_controller.clone() {
            traffic_controller.tally(TrafficTally {
                direct: client,
                through_fullnode: None,
                error_info: error.map(|e| {
                    let error_type = String::from(e.clone().as_ref());
                    let error_weight = normalize(e);
                    (error_weight, error_type)
                }),
                spam_weight,
                timestamp: SystemTime::now(),
            })
        }
        unwrapped_response
    }
}

fn make_tonic_request_for_testing<T>(message: T) -> tonic::Request<T> {
    // simulate a TCP connection, which would have added extensions to
    // the request object that would be used downstream
    let mut request = tonic::Request::new(message);
    let tcp_connect_info = TcpConnectInfo {
        local_addr: None,
        remote_addr: Some(SocketAddr::new([127, 0, 0, 1].into(), 0)),
    };
    request.extensions_mut().insert(tcp_connect_info);
    request
}

// TODO: refine error matching here
fn normalize(err: SuiError) -> Weight {
    match err {
        SuiError::UserInputError {
            error: UserInputError::IncorrectUserSignature { .. },
        } => Weight::one(),
        SuiError::InvalidSignature { .. }
        | SuiError::SignerSignatureAbsent { .. }
        | SuiError::SignerSignatureNumberMismatch { .. }
        | SuiError::IncorrectSigner { .. }
        | SuiError::UnknownSigner { .. }
        | SuiError::WrongEpoch { .. } => Weight::one(),
        _ => Weight::zero(),
    }
}

/// Implements generic pre- and post-processing. Since this is on the critical
/// path, any heavy lifting should be done in a separate non-blocking task
/// unless it is necessary to override the return value.
#[macro_export]
macro_rules! handle_with_decoration {
    ($self:ident, $func_name:ident, $request:ident) => {{
        if $self.client_id_source.is_none() {
            return $self.$func_name($request).await.map(|(result, _)| result);
        }

        let client = $self.get_client_ip_addr(&$request, $self.client_id_source.as_ref().unwrap());

        // check if either IP is blocked, in which case return early
        $self.handle_traffic_req(client.clone()).await?;

        // handle traffic tallying
        let wrapped_response = $self.$func_name($request).await;
        $self.handle_traffic_resp(client, wrapped_response)
    }};
}

#[async_trait]
impl Validator for ValidatorService {
    async fn transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let validator_service = self.clone();

        // Spawns a task which handles the transaction. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        spawn_monitored_task!(async move {
            // NB: traffic tally wrapping handled within the task rather than on task exit
            // to prevent an attacker from subverting traffic control by severing the connection
            handle_with_decoration!(validator_service, transaction_impl, request)
        })
        .await
        .unwrap()
    }

    async fn transaction_v2(
        &self,
        request: tonic::Request<HandleTransactionRequestV2>,
    ) -> Result<tonic::Response<HandleTransactionResponseV2>, tonic::Status> {
        let validator_service = self.clone();

        // Spawns a task which handles the transaction. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        spawn_monitored_task!(async move {
            // NB: traffic tally wrapping handled within the task rather than on task exit
            // to prevent an attacker from subverting traffic control by severing the connection
            handle_with_decoration!(validator_service, transaction_v2_impl, request)
        })
        .await
        .unwrap()
    }

    async fn submit_certificate(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<SubmitCertificateResponse>, tonic::Status> {
        let validator_service = self.clone();

        // Spawns a task which handles the certificate. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        spawn_monitored_task!(async move {
            // NB: traffic tally wrapping handled within the task rather than on task exit
            // to prevent an attacker from subverting traffic control by severing the connection.
            handle_with_decoration!(validator_service, submit_certificate_impl, request)
        })
        .await
        .unwrap()
    }

    async fn handle_certificate_v2(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<HandleCertificateResponseV2>, tonic::Status> {
        handle_with_decoration!(self, handle_certificate_v2_impl, request)
    }

    async fn handle_certificate_v3(
        &self,
        request: tonic::Request<HandleCertificateRequestV3>,
    ) -> Result<tonic::Response<HandleCertificateResponseV3>, tonic::Status> {
        handle_with_decoration!(self, handle_certificate_v3_impl, request)
    }

    async fn handle_soft_bundle_certificates_v3(
        &self,
        request: tonic::Request<HandleSoftBundleCertificatesRequestV3>,
    ) -> Result<tonic::Response<HandleSoftBundleCertificatesResponseV3>, tonic::Status> {
        handle_with_decoration!(self, handle_soft_bundle_certificates_v3_impl, request)
    }

    async fn object_info(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> Result<tonic::Response<ObjectInfoResponse>, tonic::Status> {
        handle_with_decoration!(self, object_info_impl, request)
    }

    async fn transaction_info(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        handle_with_decoration!(self, transaction_info_impl, request)
    }

    async fn checkpoint(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> Result<tonic::Response<CheckpointResponse>, tonic::Status> {
        handle_with_decoration!(self, checkpoint_impl, request)
    }

    async fn checkpoint_v2(
        &self,
        request: tonic::Request<CheckpointRequestV2>,
    ) -> Result<tonic::Response<CheckpointResponseV2>, tonic::Status> {
        handle_with_decoration!(self, checkpoint_v2_impl, request)
    }

    async fn get_system_state_object(
        &self,
        request: tonic::Request<SystemStateRequest>,
    ) -> Result<tonic::Response<SuiSystemState>, tonic::Status> {
        handle_with_decoration!(self, get_system_state_object_impl, request)
    }
}
