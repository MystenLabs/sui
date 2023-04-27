// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use mysten_metrics::histogram::Histogram as MystenHistogram;
use mysten_metrics::spawn_monitored_task;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use std::{io, sync::Arc};
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};
use sui_types::effects::{TransactionEffectsAPI, TransactionEvents};
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{error::*, messages::*};
use sui_types::{
    fp_ensure,
    messages_checkpoint::{CheckpointRequest, CheckpointResponse},
};
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{error_span, info, Instrument};

use crate::consensus_adapter::{ConnectionMonitorStatusForTests, LazyNarwhalClient};
use crate::{
    authority::AuthorityState,
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics},
};

// Reject a transaction if transaction manager queue length is above this threshold.
// 100_000 = 10k TPS * 5s resident time in transaction manager (pending + executing) * 2.
pub(crate) const MAX_TM_QUEUE_LENGTH: usize = 100_000;

// Reject a transaction if the number of pending transactions depending on the object
// is above the threshold.
pub(crate) const MAX_PER_OBJECT_QUEUE_LENGTH: usize = 1000;

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
            Box::new(LazyNarwhalClient::new(consensus_address)),
            state.name,
            Box::new(Arc::new(ConnectionMonitorStatusForTests {})),
            100_000,
            100_000,
            ConsensusAdapterMetrics::new_test(),
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

    pub(crate) fn check_execution_overload(
        state: Arc<AuthorityState>,
        msg: &SenderSignedData,
    ) -> SuiResult<()> {
        // Too many transactions are pending execution.
        let inflight_queue_len = state.transaction_manager().inflight_queue_len();
        if inflight_queue_len >= MAX_TM_QUEUE_LENGTH {
            return Err(SuiError::TooManyTransactionsPendingExecution {
                queue_len: inflight_queue_len,
                threshold: MAX_TM_QUEUE_LENGTH,
            });
        }

        for (object_id, queue_len) in state.transaction_manager().objects_queue_len(
            msg.intent_message()
                .value
                .kind()
                .input_objects()
                .map_err(SuiError::from)?
                .into_iter()
                .map(|r| r.object_id())
                .collect(),
        ) {
            // When this occurs, most likely transactions piled up on a shared object.
            if queue_len >= MAX_PER_OBJECT_QUEUE_LENGTH {
                return Err(SuiError::TooManyTransactionsPendingOnObject {
                    object_id,
                    queue_len,
                    threshold: MAX_PER_OBJECT_QUEUE_LENGTH,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn check_consensus_overload(
        consensus_adapter: Arc<ConsensusAdapter>,
    ) -> SuiResult<()> {
        if !consensus_adapter.check_limits() {
            return Err(SuiError::TooManyTransactionsPendingConsensus);
        }
        Ok(())
    }

    pub(crate) fn check_system_overload(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        msg: &SenderSignedData,
    ) -> SuiResult<()> {
        Self::check_execution_overload(state, msg)?;
        Self::check_consensus_overload(consensus_adapter)?;
        Ok(())
    }

    async fn handle_transaction(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        request: tonic::Request<Transaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let transaction = request.into_inner();
        let epoch_store = state.load_epoch_store_one_call_per_task();

        // Enforce overall transaction size limit.
        let tx_size = bcs::serialized_size(&transaction).map_err(|e| {
            SuiError::TransactionSerializationError {
                error: e.to_string(),
            }
        })?;
        let max_tx_size_bytes = epoch_store.protocol_config().max_tx_size_bytes();
        fp_ensure!(
            tx_size as u64 <= max_tx_size_bytes,
            SuiError::UserInputError {
                error: UserInputError::SizeLimitExceeded {
                    limit: format!(
                        "serialized transaction size exceeded maximum of {max_tx_size_bytes}"
                    ),
                    value: tx_size.to_string(),
                }
            }
            .into()
        );
        Self::check_system_overload(
            Arc::clone(&state),
            Arc::clone(&consensus_adapter),
            transaction.data(),
        )?;
        let _handle_tx_metrics_guard = metrics.handle_transaction_latency.start_timer();

        let tx_verif_metrics_guard = metrics.tx_verification_latency.start_timer();
        let transaction = epoch_store
            .signature_verifier
            .verify_tx(transaction.data())
            .map(|_| VerifiedTransaction::new_from_verified(transaction))
            .tap_err(|_| {
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

        Ok(tonic::Response::new(info))
    }

    // TODO: reject certificate if TransactionManager or Narwhal is backlogged.
    async fn handle_certificate(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        request: tonic::Request<CertifiedTransaction>,
        metrics: Arc<ValidatorServiceMetrics>,
        wait_for_effects: bool,
    ) -> Result<Option<HandleCertificateResponseV2>, tonic::Status> {
        let epoch_store = state.load_epoch_store_one_call_per_task();

        let certificate = request.into_inner();

        let shared_object_tx = certificate.contains_shared_object();

        let _metrics_guard = if wait_for_effects {
            if shared_object_tx {
                metrics.handle_certificate_consensus_latency.start_timer()
            } else {
                metrics
                    .handle_certificate_non_consensus_latency
                    .start_timer()
            }
        } else {
            metrics.submit_certificate_consensus_latency.start_timer()
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

            let fastpath_input_objects = state.load_fastpath_input_objects(&signed_effects)?;

            return Ok(Some(HandleCertificateResponseV2 {
                signed_effects: signed_effects.into_inner(),
                events,
                fastpath_input_objects,
            }));
        }

        // 2) Validate if cert can be executed, and verify the cert.
        // Fullnode does not serve handle_certificate call.
        fp_ensure!(
            !state.is_fullnode(&epoch_store),
            SuiError::FullNodeCantHandleCertificate.into()
        );

        // CRITICAL! Validators should never sign an external system transaction.
        fp_ensure!(
            !certificate.is_system_tx(),
            SuiError::InvalidSystemTransaction.into()
        );

        // Check system overload
        Self::check_system_overload(
            Arc::clone(&state),
            Arc::clone(&consensus_adapter),
            certificate.data(),
        )?;

        // code block within reconfiguration lock
        let certificate = {
            let certificate = {
                let _timer = metrics.cert_verification_latency.start_timer();
                epoch_store
                    .signature_verifier
                    .verify_cert(certificate)
                    .await?
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

        if !wait_for_effects {
            // It is useful to enqueue owned object transaction for execution locally,
            // even when we are not returning effects to user
            if !certificate.contains_shared_object() {
                state
                    .enqueue_certificates_for_execution(vec![certificate.clone()], &epoch_store)?;
            }
            return Ok(None);
        }

        // 4) Execute the certificate if it contains only owned object transactions, or wait for
        // the execution results if it contains shared objects.
        let effects = state
            .execute_certificate(&certificate, &epoch_store)
            .await?;
        let fastpath_input_objects = state.load_fastpath_input_objects(&effects)?;
        let events = if let Some(event_digest) = effects.events_digest() {
            state.get_transaction_events(event_digest)?
        } else {
            TransactionEvents::default()
        };
        Ok(Some(HandleCertificateResponseV2 {
            signed_effects: effects.into_inner(),
            events,
            fastpath_input_objects,
        }))
    }
}

#[async_trait]
impl Validator for ValidatorService {
    async fn transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<HandleTransactionResponse>, tonic::Status> {
        let state = self.state.clone();
        let consensus_adapter = self.consensus_adapter.clone();

        // Spawns a task which handles the transaction. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(Self::handle_transaction(
            state,
            consensus_adapter,
            request,
            metrics
        ))
        .await
        .unwrap()
    }

    async fn submit_certificate(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<SubmitCertificateResponse>, tonic::Status> {
        let state = self.state.clone();
        let consensus_adapter = self.consensus_adapter.clone();

        // Spawns a task which handles the certificate. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(async move {
            let span = error_span!("submit_certificate", tx_digest = ?request.get_ref().digest());
            Self::handle_certificate(state, consensus_adapter, request, metrics, false)
                .instrument(span)
                .await
        })
        .await
        .unwrap()
        .map(|executed| {
            tonic::Response::new(SubmitCertificateResponse {
                executed: executed.map(|e| e.into()),
            })
        })
    }

    async fn handle_certificate_v2(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<HandleCertificateResponseV2>, tonic::Status> {
        let state = self.state.clone();
        let consensus_adapter = self.consensus_adapter.clone();

        // Spawns a task which handles the certificate. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(async move {
            let span = error_span!("handle_certificate", tx_digest = ?request.get_ref().digest());
            Self::handle_certificate(state, consensus_adapter, request, metrics, true)
                .instrument(span)
                .await
        })
        .await
        .unwrap()
        .map(|v| {
            tonic::Response::new(
                v.expect("handle_certificate should not return none with wait_for_effects=true"),
            )
        })
    }

    async fn handle_certificate(
        &self,
        request: tonic::Request<CertifiedTransaction>,
    ) -> Result<tonic::Response<HandleCertificateResponse>, tonic::Status> {
        self.handle_certificate_v2(request)
            .await
            .map(|v| tonic::Response::new(v.into_inner().into()))
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
    ) -> Result<tonic::Response<SuiSystemState>, tonic::Status> {
        let response = self.state.database.get_sui_system_state_object()?;

        return Ok(tonic::Response::new(response));
    }
}
