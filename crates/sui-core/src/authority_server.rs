// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use mysten_metrics::histogram::Histogram as MystenHistogram;
use mysten_metrics::spawn_monitored_task;
use narwhal_worker::LazyNarwhalClient;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_counter_with_registry, IntCounter,
    IntCounterVec, Registry,
};
use std::{io, net::SocketAddr, sync::Arc, time::SystemTime};
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::messages_grpc::{
    HandleCertificateResponseV2, HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse,
    SubmitCertificateResponse, SystemStateRequest, TransactionInfoRequest, TransactionInfoResponse,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{effects::TransactionEffectsAPI, message_envelope::Message};
use sui_types::{effects::TransactionEvents, traffic_control::PolicyConfig};
use sui_types::{error::*, transaction::*};
use sui_types::{
    fp_ensure,
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
};
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{error, error_span, info, Instrument};

use crate::consensus_adapter::ConnectionMonitorStatusForTests;
use crate::{
    authority::AuthorityState,
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics},
    traffic_controller::TrafficController,
};
use sui_types::traffic_control::TrafficTally;
use tonic::transport::server::TcpConnectInfo;

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

pub struct AuthorityServerHandle {
    tx_cancellation: tokio::sync::oneshot::Sender<()>,
    local_addr: Multiaddr,
    handle: JoinHandle<Result<(), tonic::transport::Error>>,
}

impl AuthorityServerHandle {
    pub async fn join(self) -> Result<(), io::Error> {
        // Note that dropping `self.complete` would terminate the server.
        self.handle
            .await?
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub async fn kill(self) -> Result<(), io::Error> {
        self.tx_cancellation.send(()).map_err(|_e| {
            io::Error::new(io::ErrorKind::Other, "could not send cancellation signal!")
        })?;
        self.handle
            .await?
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub fn address(&self) -> &Multiaddr {
        &self.local_addr
    }
}

pub struct AuthorityServer {
    address: Multiaddr,
    pub state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    pub metrics: Arc<ValidatorServiceMetrics>,
}

impl AuthorityServer {
    pub fn new_for_test(
        address: Multiaddr,
        state: Arc<AuthorityState>,
        consensus_address: Multiaddr,
    ) -> Self {
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(LazyNarwhalClient::new(consensus_address)),
            state.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            state.epoch_store_for_testing().protocol_config().clone(),
        ));

        let metrics = Arc::new(ValidatorServiceMetrics::new_for_tests());

        Self {
            address,
            state,
            consensus_adapter,
            metrics,
        }
    }

    pub async fn spawn_for_test(self) -> Result<AuthorityServerHandle, io::Error> {
        let address = self.address.clone();
        self.spawn_with_bind_address_for_test(address).await
    }

    pub async fn spawn_with_bind_address_for_test(
        self,
        address: Multiaddr,
    ) -> Result<AuthorityServerHandle, io::Error> {
        let mut server = mysten_network::config::Config::new()
            .server_builder()
            .add_service(ValidatorServer::new(ValidatorService {
                state: self.state,
                consensus_adapter: self.consensus_adapter,
                metrics: self.metrics.clone(),
                traffic_controller: Arc::new(
                    TrafficController::spawn(PolicyConfig::default()).await,
                ),
            }))
            .bind(&address)
            .await
            .unwrap();
        let local_addr = server.local_addr().to_owned();
        info!("Listening to traffic on {local_addr}");
        let handle = AuthorityServerHandle {
            tx_cancellation: server.take_cancel_handle().unwrap(),
            local_addr,
            handle: spawn_monitored_task!(server.serve()),
        };
        Ok(handle)
    }
}

pub struct ValidatorServiceMetrics {
    pub signature_errors: IntCounter,
    pub tx_verification_latency: MystenHistogram,
    pub cert_verification_latency: MystenHistogram,
    pub consensus_latency: MystenHistogram,
    pub handle_transaction_latency: MystenHistogram,
    pub submit_certificate_consensus_latency: MystenHistogram,
    pub handle_certificate_consensus_latency: MystenHistogram,
    pub handle_certificate_non_consensus_latency: MystenHistogram,

    num_rejected_tx_in_epoch_boundary: IntCounter,
    num_rejected_cert_in_epoch_boundary: IntCounter,
    num_rejected_tx_during_overload: IntCounterVec,
    num_rejected_cert_during_overload: IntCounterVec,
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
            tx_verification_latency: MystenHistogram::new_in_registry(
                "validator_service_tx_verification_latency",
                "Latency of verifying a transaction",
                registry,
            ),
            cert_verification_latency: MystenHistogram::new_in_registry(
                "validator_service_cert_verification_latency",
                "Latency of verifying a certificate",
                registry,
            ),
            consensus_latency: MystenHistogram::new_in_registry(
                "validator_service_consensus_latency",
                "Time spent between submitting a shared obj txn to consensus and getting result",
                registry,
            ),
            handle_transaction_latency: MystenHistogram::new_in_registry(
                "validator_service_handle_transaction_latency",
                "Latency of handling a transaction",
                registry,
            ),
            handle_certificate_consensus_latency: MystenHistogram::new_in_registry(
                "validator_service_handle_certificate_consensus_latency",
                "Latency of handling a consensus transaction certificate",
                registry,
            ),
            submit_certificate_consensus_latency: MystenHistogram::new_in_registry(
                "validator_service_submit_certificate_consensus_latency",
                "Latency of submit_certificate RPC handler",
                registry,
            ),
            handle_certificate_non_consensus_latency: MystenHistogram::new_in_registry(
                "validator_service_handle_certificate_non_consensus_latency",
                "Latency of handling a non-consensus transaction certificate",
                registry,
            ),
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
    traffic_controller: Arc<TrafficController>,
}

impl ValidatorService {
    pub async fn new(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        metrics: Arc<ValidatorServiceMetrics>,
        traffic_control_config: PolicyConfig,
    ) -> Self {
        Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: Arc::new(TrafficController::spawn(traffic_control_config).await),
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

    async fn handle_transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: _,
        } = self.clone();
        let transaction = request.into_inner();

        let epoch_store = state.load_epoch_store_one_call_per_task();

        transaction.validity_check(epoch_store.protocol_config())?;

        if !epoch_store.protocol_config().zklogin_auth() && transaction.has_zklogin_sig() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "zklogin is not enabled on this network".to_string(),
            }
            .into());
        }

        if !epoch_store.protocol_config().supports_upgraded_multisig()
            && transaction.has_upgraded_multisig()
        {
            return Err(SuiError::UnsupportedFeatureError {
                error: "upgraded multisig format not enabled on this network".to_string(),
            }
            .into());
        }

        if !epoch_store.randomness_state_enabled() && transaction.is_randomness_reader() {
            return Err(SuiError::UnsupportedFeatureError {
                error: "randomness is not enabled on this network".to_string(),
            }
            .into());
        }

        // When authority is overloaded and decide to reject this tx, we still lock the object
        // and ask the client to retry in the future. This is because without locking, the
        // input objects can be locked by a different tx in the future, however, the input objects
        // may already be locked by this tx in other validators. This can cause non of the txes
        // to have enough quorum to form a certificate, causing the objects to be locked for
        // the entire epoch. By doing locking but pushback, retrying transaction will have
        // higher chance to succeed.
        let mut validator_pushback_error = None;
        let overload_check_res = state.check_system_overload(
            &consensus_adapter,
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
            .handle_transaction(&epoch_store, transaction)
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
        Ok(tonic::Response::new(info))
    }

    // TODO: reject certificate if TransactionManager or Narwhal is backlogged.
    async fn handle_certificate(
        &self,
        request: tonic::Request<CertifiedTransaction>,
        wait_for_effects: bool,
    ) -> Result<Option<HandleCertificateResponseV2>, tonic::Status> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let certificate = request.into_inner();

        // Validate if cert can be executed
        // Fullnode does not serve handle_certificate call.
        fp_ensure!(
            !self.state.is_fullnode(&epoch_store),
            SuiError::FullNodeCantHandleCertificate.into()
        );

        // CRITICAL! Validators should never sign an external system transaction.
        fp_ensure!(
            !certificate.is_system_tx(),
            SuiError::InvalidSystemTransaction.into()
        );

        certificate
            .data()
            .validity_check(epoch_store.protocol_config())?;

        let shared_object_tx = certificate.contains_shared_object();

        let _metrics_guard = if wait_for_effects {
            if shared_object_tx {
                self.metrics
                    .handle_certificate_consensus_latency
                    .start_timer()
            } else {
                self.metrics
                    .handle_certificate_non_consensus_latency
                    .start_timer()
            }
        } else {
            self.metrics
                .submit_certificate_consensus_latency
                .start_timer()
        };

        // 1) Check if cert already executed
        let tx_digest = *certificate.digest();
        if let Some(signed_effects) = self
            .state
            .get_signed_effects_and_maybe_resign(&tx_digest, &epoch_store)?
        {
            let events = if let Some(digest) = signed_effects.events_digest() {
                self.state.get_transaction_events(digest)?
            } else {
                TransactionEvents::default()
            };

            return Ok(Some(HandleCertificateResponseV2 {
                signed_effects: signed_effects.into_inner(),
                events,
                fastpath_input_objects: vec![], // unused field
            }));
        }

        // 2) Verify the cert.
        // Check system overload
        let overload_check_res = self.state.check_system_overload(
            &self.consensus_adapter,
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

        // code block within reconfiguration lock
        let certificate = {
            let certificate = {
                let _timer = self.metrics.cert_verification_latency.start_timer();
                epoch_store
                    .signature_verifier
                    .verify_cert(certificate)
                    .await?
            };

            let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
            if !reconfiguration_lock.should_accept_user_certs() {
                self.metrics.num_rejected_cert_in_epoch_boundary.inc();
                return Err(SuiError::ValidatorHaltedAtEpochEnd.into());
            }

            // 3) All certificates are sent to consensus (at least by some authorities)
            // For shared objects this will wait until either timeout or we have heard back from consensus.
            // For owned objects this will return without waiting for certificate to be sequenced
            // First do quick dirty non-async check
            if !epoch_store.is_tx_cert_consensus_message_processed(&certificate)? {
                let _metrics_guard = if shared_object_tx {
                    Some(self.metrics.consensus_latency.start_timer())
                } else {
                    None
                };
                let transaction = ConsensusTransaction::new_certificate_message(
                    &self.state.name,
                    certificate.clone().into(),
                );
                self.consensus_adapter.submit(
                    transaction,
                    Some(&reconfiguration_lock),
                    &epoch_store,
                )?;
                // Do not wait for the result, because the transaction might have already executed.
                // Instead, check or wait for the existence of certificate effects below.
            }
            drop(reconfiguration_lock);
            certificate
        };

        if !wait_for_effects {
            // It is useful to enqueue owned object transaction for execution locally,
            // even when we are not returning effects to user
            if !certificate.contains_shared_object() {
                self.state
                    .enqueue_certificates_for_execution(vec![certificate.clone()], &epoch_store);
            }
            return Ok(None);
        }

        // 4) Execute the certificate if it contains only owned object transactions, or wait for
        // the execution results if it contains shared objects.
        let effects = self
            .state
            .execute_certificate(&certificate, &epoch_store)
            .await?;
        let events = if let Some(event_digest) = effects.events_digest() {
            self.state.get_transaction_events(event_digest)?
        } else {
            TransactionEvents::default()
        };
        Ok(Some(HandleCertificateResponseV2 {
            signed_effects: effects.into_inner(),
            events,
            fastpath_input_objects: vec![], // unused field
        }))
    }
}

impl ValidatorService {
    async fn transaction_impl(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        self.handle_transaction(request).await
    }

    async fn submit_certificate_impl(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<SubmitCertificateResponse>, tonic::Status> {
        // The call to digest() assumes the transaction is valid, so we need to verify it first.
        request.get_ref().verify_user_input()?;

        let span = error_span!("submit_certificate", tx_digest = ?request.get_ref().digest());
        self.handle_certificate(request, false)
            .instrument(span)
            .await
            .map(|executed| tonic::Response::new(SubmitCertificateResponse { executed }))
    }

    async fn handle_certificate_v2_impl(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<HandleCertificateResponseV2>, tonic::Status> {
        // The call to digest() assumes the transaction is valid, so we need to verify it first.
        request.get_ref().verify_user_input()?;

        let span = error_span!("handle_certificate", tx_digest = ?request.get_ref().digest());
        self.handle_certificate(request, true)
            .instrument(span)
            .await
            .map(|v| {
                tonic::Response::new(
                    v.expect(
                        "handle_certificate should not return none with wait_for_effects=true",
                    ),
                )
            })
    }

    async fn object_info_impl(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> Result<tonic::Response<ObjectInfoResponse>, tonic::Status> {
        let request = request.into_inner();
        let response = self.state.handle_object_info_request(request).await?;
        Ok(tonic::Response::new(response))
    }

    async fn transaction_info_impl(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let request = request.into_inner();
        let response = self.state.handle_transaction_info_request(request).await?;
        Ok(tonic::Response::new(response))
    }

    async fn checkpoint_impl(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> Result<tonic::Response<CheckpointResponse>, tonic::Status> {
        let request = request.into_inner();
        let response = self.state.handle_checkpoint_request(&request)?;
        Ok(tonic::Response::new(response))
    }

    async fn checkpoint_v2_impl(
        &self,
        request: tonic::Request<CheckpointRequestV2>,
    ) -> Result<tonic::Response<CheckpointResponseV2>, tonic::Status> {
        let request = request.into_inner();
        let response = self.state.handle_checkpoint_request_v2(&request)?;
        Ok(tonic::Response::new(response))
    }

    async fn get_system_state_object_impl(
        &self,
        _request: tonic::Request<SystemStateRequest>,
    ) -> Result<tonic::Response<SuiSystemState>, tonic::Status> {
        let response = self
            .state
            .get_cache_reader()
            .get_sui_system_state_object_unsafe()?;

        Ok(tonic::Response::new(response))
    }

    async fn handle_traffic_req(
        &self,
        connection_ip: Option<SocketAddr>,
        proxy_ip: Option<SocketAddr>,
    ) -> Result<(), tonic::Status> {
        if !self.traffic_controller.check(connection_ip, proxy_ip).await {
            // Entity in blocklist
            Err(tonic::Status::resource_exhausted("Too many requests"))
        } else {
            Ok(())
        }
    }

    fn handle_traffic_resp<T>(
        &self,
        connection_ip: Option<SocketAddr>,
        proxy_ip: Option<SocketAddr>,
        response: &Result<tonic::Response<T>, tonic::Status>,
    ) {
        let result: SuiResult = if let Err(status) = response {
            Err(SuiError::from(status.clone()))
        } else {
            Ok(())
        };

        self.traffic_controller.tally(TrafficTally {
            connection_ip,
            proxy_ip,
            result,
            timestamp: SystemTime::now(),
        });
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

/// Implements generic pre- and post-processing. Since this is on the critical
/// path, any heavy lifting should be done in a separate non-blocking task
/// unless it is necessary to override the return value.
#[macro_export]
macro_rules! handle_with_decoration {
    ($self:ident, $func_name:ident, $request:ident) => {{
        // extract IP info. Note that in addition to extracting the client IP from
        // the request header, we also get the remote address in case we need to
        // throttle a fullnode, or an end user is running a local quorum driver.
        let connection_ip: Option<SocketAddr> = $request.remote_addr();

        // This should never happen except perhaps in simtest or some other non-standard
        // environment. If we are seeing this, we should investigate
        // TODO: add metric here
        if connection_ip.is_none() {
            if cfg!(all(test, not(msim))) {
                panic!("Failed to get remote address from request");
            } else {
                // We will hit this case if the IO type used does not
                // implement Connected or when using a unix domain socket.
                // TODO: once we have confirmed that no legitimate traffic
                // is hitting this case, we should reject such requests that
                // hit this case.
                // TODO(william) add metric here
                error!("Failed to get remote address from request");
            }
        }

        let proxy_ip: Option<SocketAddr> =
            if let Some(op) = $request.metadata().get("x-forwarded-for") {
                match op.to_str() {
                    Ok(ip) => match ip.parse() {
                        Ok(ret) => Some(ret),
                        Err(e) => {
                            // TODO(william) add metric here
                            error!("Failed to parse x-forwarded-for header value to SocketAddr: {:?}", e);
                            return Err(tonic::Status::internal("Failed to parse tonic request metadata"));
                        }
                    },
                    Err(e) => {
                        // TODO(william) add metric here
                        // TODO: once we have confirmed that no legitimate traffic
                        // is hitting this case, we should reject such requests that
                        // hit this case.
                        error!("Invalid UTF-8 in x-forwarded-for header: {:?}", e);
                        return Err(tonic::Status::internal("Invalid tonic request metadata"));
                    }
                }
            } else {
                // TODO(william) add metric here
                None
            };

        // check if either IP is blocked, in which case return early
        $self.handle_traffic_req(connection_ip, proxy_ip).await?;
        // handle request
        let response = $self.$func_name($request).await;
        // handle response tallying
        $self.handle_traffic_resp(connection_ip, proxy_ip, &response);
        response
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
