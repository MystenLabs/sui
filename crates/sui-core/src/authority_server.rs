// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use multiaddr::Multiaddr;
use mysten_metrics::spawn_monitored_task;
use narwhal_types::TransactionsClient;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};
use std::{io, sync::Arc};
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};
use sui_types::{error::*, messages::*, sui_system_state::SuiSystemStateInnerBenchmark};
use sui_types::{
    fp_ensure,
    messages_checkpoint::{CheckpointRequest, CheckpointResponse},
};
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{error_span, info, Instrument};

use crate::consensus_adapter::ConnectionMonitorStatusForTests;
use crate::{
    authority::{AuthorityState, MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH},
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics},
};

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
        let consensus_client = Box::new(TransactionsClient::new(
            mysten_network::client::connect_lazy(&consensus_address)
                .expect("Failed to connect to consensus"),
        ));

        let consensus_adapter = ConsensusAdapter::new(
            consensus_client,
            state.name,
            Box::new(Arc::new(ConnectionMonitorStatusForTests {})),
            ConsensusAdapterMetrics::new_test(),
        );

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
    pub tx_verification_latency: Histogram,
    pub cert_verification_latency: Histogram,
    pub consensus_latency: Histogram,
    pub handle_transaction_latency: Histogram,
    pub handle_certificate_consensus_latency: Histogram,
    pub handle_certificate_non_consensus_latency: Histogram,

    num_rejected_tx_in_epoch_boundary: IntCounter,
    num_rejected_cert_in_epoch_boundary: IntCounter,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

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
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            cert_verification_latency: register_histogram_with_registry!(
                "validator_service_cert_verification_latency",
                "Latency of verifying a certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            consensus_latency: register_histogram_with_registry!(
                "validator_service_consensus_latency",
                "Time spent between submitting a shared obj txn to consensus and getting result",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_latency",
                "Latency of handling a transaction",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_certificate_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_certificate_consensus_latency",
                "Latency of handling a consensus transaction certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_certificate_non_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_certificate_non_consensus_latency",
                "Latency of handling a non-consensus transaction certificate",
                LATENCY_SEC_BUCKETS.to_vec(),
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
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

pub struct ValidatorService {
    state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    metrics: Arc<ValidatorServiceMetrics>,
}

impl ValidatorService {
    pub async fn new(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        prometheus_registry: &Registry,
    ) -> Result<Self> {
        Ok(Self {
            state,
            consensus_adapter,
            metrics: Arc::new(ValidatorServiceMetrics::new(prometheus_registry)),
        })
    }

    async fn handle_transaction(
        state: Arc<AuthorityState>,
        request: tonic::Request<Transaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let transaction = request.into_inner();
        let epoch_store = state.load_epoch_store_one_call_per_task();

        // Enforce overall transaction size limit.
        let tx_size = bcs::serialized_size(&transaction)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let max_tx_size = epoch_store.protocol_config().max_tx_size();
        fp_ensure!(
            tx_size as u64 <= max_tx_size,
            tonic::Status::resource_exhausted(format!(
                "serialized transaction size ({tx_size}) exceeded maximum of {max_tx_size}"
            ))
        );

        let _metrics_guard = metrics.handle_transaction_latency.start_timer();
        let tx_verif_metrics_guard = metrics.tx_verification_latency.start_timer();

        let transaction = transaction.verify().tap_err(|_| {
            metrics.signature_errors.inc();
        })?;
        tx_verif_metrics_guard.stop_and_record();

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

        Ok(tonic::Response::new(info))
    }

    // TODO: reject certificate if TransactionManager or Narwhal is backlogged.
    async fn handle_certificate(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        request: tonic::Request<CertifiedTransaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<HandleCertificateResponse>, tonic::Status> {
        let epoch_store = state.load_epoch_store_one_call_per_task();

        let certificate = request.into_inner();

        let shared_object_tx = certificate.contains_shared_object();

        let _metrics_guard = if shared_object_tx {
            metrics.handle_certificate_consensus_latency.start_timer()
        } else {
            metrics
                .handle_certificate_non_consensus_latency
                .start_timer()
        };

        // 1) Check if cert already executed
        let tx_digest = *certificate.digest();
        if let Some(signed_effects) =
            state.get_signed_effects_and_maybe_resign(&tx_digest, &epoch_store)?
        {
            let events = if let Some(digest) = signed_effects.events_digest() {
                state.get_transaction_events(digest)?
            } else {
                TransactionEvents::default()
            };

            return Ok(tonic::Response::new(HandleCertificateResponse {
                signed_effects: signed_effects.into_inner(),
                events,
            }));
        }

        // 2) Validate if cert can be executed, and verify the cert.
        if state.is_fullnode(&epoch_store) {
            return Err(tonic::Status::unimplemented(format!(
                "Cannot execute certificate without effects on fullnode! {:?}",
                certificate.digest()
            )));
        }
        if certificate.is_system_tx() {
            return Err(tonic::Status::invalid_argument(format!(
                "Cannot execute system certificate via RPC interface! {certificate:?}"
            )));
        }
        for (object_id, queue_len) in state.transaction_manager().objects_queue_len(
            certificate
                .data()
                .intent_message()
                .value
                .kind()
                .input_objects()
                .map_err(SuiError::from)?
                .into_iter()
                .map(|r| r.object_id())
                .collect(),
        ) {
            // When this occurs, most likely transactions piled up on a shared object.
            if queue_len >= MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH {
                return Err(SuiError::TooManyTransactionsPendingOnObject {
                    object_id,
                    queue_len,
                    threshold: MAX_PER_OBJECT_EXECUTION_QUEUE_LENGTH,
                }
                .into());
            }
        }
        // code block within reconfiguration lock
        let certificate = {
            let certificate = {
                let _timer = metrics.cert_verification_latency.start_timer();
                epoch_store.batch_verifier.verify_cert(certificate).await?
            };

            let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
            if !reconfiguration_lock.should_accept_user_certs() {
                metrics.num_rejected_cert_in_epoch_boundary.inc();
                return Err(SuiError::ValidatorHaltedAtEpochEnd.into());
            }

            // 3) All certificates are sent to consensus (at least by some authorities)
            // For shared objects this will wait until either timeout or we have heard back from consensus.
            // For owned objects this will return without waiting for certificate to be sequenced
            // First do quick dirty non-async check
            if !epoch_store.is_tx_cert_consensus_message_processed(&certificate)? {
                if !consensus_adapter.check_limits() {
                    return Err(tonic::Status::resource_exhausted(
                        "Reached maximum transactions pending in consensus. Consensus is overloaded.".to_string()
                    ));
                }
                let _metrics_guard = if shared_object_tx {
                    Some(metrics.consensus_latency.start_timer())
                } else {
                    None
                };
                let transaction = ConsensusTransaction::new_certificate_message(
                    &state.name,
                    certificate.clone().into(),
                );
                consensus_adapter.submit(transaction, Some(&reconfiguration_lock), &epoch_store)?;
                // Do not wait for the result, because the transaction might have already executed.
                // Instead, check or wait for the existence of certificate effects below.
            }
            drop(reconfiguration_lock);
            certificate
        };

        // 4) Execute the certificate if it contains only owned object transactions, or wait for
        // the execution results if it contains shared objects.
        let res = state.execute_certificate(&certificate, &epoch_store).await;
        match res {
            Ok(effects) => {
                let events = if let Some(event_digest) = effects.events_digest() {
                    state.get_transaction_events(event_digest)?
                } else {
                    TransactionEvents::default()
                };
                Ok(tonic::Response::new(HandleCertificateResponse {
                    signed_effects: effects.into_inner(),
                    events,
                }))
            }
            Err(e) => Err(tonic::Status::from(e)),
        }
    }
}

#[async_trait]
impl Validator for ValidatorService {
    async fn transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let state = self.state.clone();

        // Spawns a task which handles the transaction. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(Self::handle_transaction(state, request, metrics))
            .await
            .unwrap()
    }

    async fn handle_certificate(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<HandleCertificateResponse>, tonic::Status> {
        let state = self.state.clone();
        let consensus_adapter = self.consensus_adapter.clone();

        // Spawns a task which handles the certificate. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(async move {
            let span = error_span!("handle_certificate", tx_digest = ?request.get_ref().digest());
            Self::handle_certificate(state, consensus_adapter, request, metrics)
                .instrument(span)
                .await
        })
        .await
        .unwrap()
    }

    async fn object_info(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> Result<tonic::Response<ObjectInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_object_info_request(request).await?;

        Ok(tonic::Response::new(response))
    }

    async fn transaction_info(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_transaction_info_request(request).await?;

        Ok(tonic::Response::new(response))
    }

    async fn checkpoint(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> Result<tonic::Response<CheckpointResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_checkpoint_request(&request)?;

        return Ok(tonic::Response::new(response));
    }

    async fn get_system_state_object(
        &self,
        _request: tonic::Request<SystemStateRequest>,
    ) -> Result<tonic::Response<SuiSystemStateInnerBenchmark>, tonic::Status> {
        let response = self
            .state
            .database
            .get_sui_system_state_object()?
            .into_benchmark_version();

        return Ok(tonic::Response::new(response));
    }
}
