// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use futures::{TryFutureExt, future};
use itertools::Itertools as _;
use mysten_common::{assert_reachable, debug_fatal};
use mysten_metrics::spawn_monitored_task;
use prometheus::{
    Gauge, Histogram, HistogramVec, IntCounter, IntCounterVec, Registry,
    register_gauge_with_registry, register_histogram_vec_with_registry,
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry,
};
use std::{
    cmp::Ordering,
    future::Future,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime},
};
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
    validator::server::SUI_TLS_SERVER_NAME,
};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::message_envelope::Message;
use sui_types::messages_consensus::ConsensusPosition;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::messages_grpc::{
    ObjectInfoRequest, ObjectInfoResponse, RawSubmitTxResponse, SystemStateRequest,
    TransactionInfoRequest, TransactionInfoResponse,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Object;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::traffic_control::{ClientIdSource, Weight};
use sui_types::{
    base_types::ObjectID,
    digests::{TransactionDigest, TransactionEffectsDigest},
    error::{SuiErrorKind, UserInputError},
};
use sui_types::{
    effects::TransactionEffects,
    messages_grpc::{
        ExecutedData, RawSubmitTxRequest, RawWaitForEffectsRequest, RawWaitForEffectsResponse,
        SubmitTxResult, WaitForEffectsRequest, WaitForEffectsResponse,
    },
};
use sui_types::{effects::TransactionEvents, messages_grpc::SubmitTxType};
use sui_types::{error::*, transaction::*};
use sui_types::{
    fp_ensure,
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tonic::metadata::{Ascii, MetadataValue};
use tracing::{debug, error, info, instrument};

use crate::consensus_adapter::ConnectionMonitorStatusForTests;
use crate::{
    authority::{AuthorityState, consensus_tx_status_cache::ConsensusTxStatus},
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics},
    traffic_controller::{TrafficController, parse_ip, policies::TrafficTally},
};
use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        consensus_tx_status_cache::NotifyReadConsensusTxStatusResult,
    },
    checkpoints::CheckpointStore,
    mysticeti_adapter::LazyMysticetiClient,
    transaction_outputs::TransactionOutputs,
};
use sui_config::local_ip_utils::new_local_tcp_address_for_testing;
use sui_types::messages_grpc::PingType;

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

#[cfg(test)]
#[path = "unit_tests/wait_for_effects_tests.rs"]
mod wait_for_effects_tests;

#[cfg(test)]
#[path = "unit_tests/submit_transaction_tests.rs"]
mod submit_transaction_tests;

pub struct AuthorityServerHandle {
    server_handle: sui_network::validator::server::Server,
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
            CheckpointStore::new_for_tests(),
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
        let config = mysten_network::config::Config::new();
        let server = sui_network::validator::server::ServerBuilder::from_config(
            &config,
            mysten_network::metrics::DefaultMetricsCallbackProvider::default(),
        )
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
    pub submit_certificate_consensus_latency: Histogram,
    pub handle_certificate_consensus_latency: Histogram,
    pub handle_certificate_non_consensus_latency: Histogram,
    pub handle_soft_bundle_certificates_consensus_latency: Histogram,
    pub handle_soft_bundle_certificates_count: Histogram,
    pub handle_soft_bundle_certificates_size_bytes: Histogram,
    pub handle_transaction_consensus_latency: Histogram,
    pub handle_submit_transaction_consensus_latency: HistogramVec,
    pub handle_wait_for_effects_ping_latency: HistogramVec,

    handle_submit_transaction_latency: HistogramVec,
    handle_submit_transaction_bytes: HistogramVec,
    handle_submit_transaction_batch_size: HistogramVec,

    num_rejected_cert_in_epoch_boundary: IntCounter,
    num_rejected_tx_during_overload: IntCounterVec,
    submission_rejected_transactions: IntCounterVec,
    connection_ip_not_found: IntCounter,
    forwarded_header_parse_error: IntCounter,
    forwarded_header_invalid: IntCounter,
    forwarded_header_not_included: IntCounter,
    client_id_source_config_mismatch: IntCounter,
    x_forwarded_for_num_hops: Gauge,
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
            handle_submit_transaction_consensus_latency: register_histogram_vec_with_registry!(
                "validator_service_submit_transaction_consensus_latency",
                "Latency of submitting a user transaction sent through consensus",
                &["req_type"],
                mysten_metrics::COARSE_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_submit_transaction_latency: register_histogram_vec_with_registry!(
                "validator_service_submit_transaction_latency",
                "Latency of submit transaction handler",
                &["req_type"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_wait_for_effects_ping_latency: register_histogram_vec_with_registry!(
                "validator_service_handle_wait_for_effects_ping_latency",
                "Latency of handling a ping request for wait_for_effects",
                &["req_type"],
                mysten_metrics::SUBSECOND_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_submit_transaction_bytes: register_histogram_vec_with_registry!(
                "validator_service_submit_transaction_bytes",
                "The size of transactions in the submit transaction request",
                &["req_type"],
                mysten_metrics::BYTES_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_submit_transaction_batch_size: register_histogram_vec_with_registry!(
                "validator_service_submit_transaction_batch_size",
                "The number of transactions in the submit transaction request",
                &["req_type"],
                mysten_metrics::COUNT_BUCKETS.to_vec(),
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
            submission_rejected_transactions: register_int_counter_vec_with_registry!(
                "validator_service_submission_rejected_transactions",
                "Number of transactions rejected during submission",
                &["reason"],
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
            x_forwarded_for_num_hops: register_gauge_with_registry!(
                "validator_service_x_forwarded_for_num_hops",
                "Number of hops in x-forwarded-for header",
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
        client_id_source: Option<ClientIdSource>,
    ) -> Self {
        let traffic_controller = state.traffic_controller.clone();
        Self {
            state,
            consensus_adapter,
            metrics: validator_metrics,
            traffic_controller,
            client_id_source,
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

    /// Test method that performs transaction validation without going through gRPC.
    pub fn handle_transaction_for_testing(&self, transaction: Transaction) -> SuiResult<()> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();

        // Validity check (basic structural validation)
        transaction.validity_check(&epoch_store.tx_validity_check_context())?;

        // Signature verification
        let transaction = epoch_store
            .verify_transaction_require_no_aliases(transaction)?
            .into_tx();

        // Validate the transaction
        self.state
            .handle_vote_transaction(&epoch_store, transaction)?;

        Ok(())
    }

    /// Test method that performs transaction validation with overload checking.
    /// Used for testing validator overload behavior.
    pub fn handle_transaction_for_testing_with_overload_check(
        &self,
        transaction: Transaction,
    ) -> SuiResult<()> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task();

        // Validity check (basic structural validation)
        transaction.validity_check(&epoch_store.tx_validity_check_context())?;

        // Check system overload
        self.state.check_system_overload(
            self.consensus_adapter.as_ref(),
            transaction.data(),
            self.state.check_system_overload_at_signing(),
        )?;

        // Signature verification
        let transaction = epoch_store
            .verify_transaction_require_no_aliases(transaction)?
            .into_tx();

        // Validate the transaction
        self.state
            .handle_vote_transaction(&epoch_store, transaction)?;

        Ok(())
    }

    /// Collect the IDs of input objects that are immutable.
    /// This is used to create the ImmutableInputObjects claim for consensus messages.
    async fn collect_immutable_object_ids(
        &self,
        tx: &VerifiedTransaction,
        state: &AuthorityState,
    ) -> SuiResult<Vec<ObjectID>> {
        let input_objects = tx.data().transaction_data().input_objects()?;

        // Collect object IDs from ImmOrOwnedMoveObject inputs
        let object_ids: Vec<ObjectID> = input_objects
            .iter()
            .filter_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();
        if object_ids.is_empty() {
            return Ok(vec![]);
        }

        // Load objects from cache and filter to immutable ones
        let objects = state.get_object_cache_reader().get_objects(&object_ids);

        // All objects should be found, since owned input objects have been validated to exist.
        objects
            .into_iter()
            .zip(object_ids.iter())
            .filter_map(|(obj, id)| {
                let Some(o) = obj else {
                    return Some(Err::<ObjectID, SuiError>(
                        SuiErrorKind::UserInputError {
                            error: UserInputError::ObjectNotFound {
                                object_id: *id,
                                version: None,
                            },
                        }
                        .into(),
                    ));
                };
                if o.is_immutable() {
                    Some(Ok(*id))
                } else {
                    None
                }
            })
            .collect::<SuiResult<Vec<ObjectID>>>()
    }

    #[instrument(
        name = "ValidatorService::handle_submit_transaction",
        level = "error",
        skip_all,
        err(level = "debug")
    )]
    async fn handle_submit_transaction(
        &self,
        request: tonic::Request<RawSubmitTxRequest>,
    ) -> WrappedServiceResponse<RawSubmitTxResponse> {
        let Self {
            state,
            consensus_adapter,
            metrics,
            traffic_controller: _,
            client_id_source,
        } = self.clone();

        let submitter_client_addr = if let Some(client_id_source) = &client_id_source {
            self.get_client_ip_addr(&request, client_id_source)
        } else {
            self.get_client_ip_addr(&request, &ClientIdSource::SocketAddr)
        };

        let inner = request.into_inner();
        let start_epoch = state.load_epoch_store_one_call_per_task().epoch();

        let next_epoch = start_epoch + 1;
        let mut max_retries = 1;

        loop {
            let res = self
                .handle_submit_transaction_inner(
                    &state,
                    &consensus_adapter,
                    &metrics,
                    &inner,
                    submitter_client_addr,
                )
                .await;
            match res {
                Ok((response, weight)) => return Ok((tonic::Response::new(response), weight)),
                Err(err) => {
                    if max_retries > 0
                        && let SuiErrorKind::ValidatorHaltedAtEpochEnd = err.as_inner()
                    {
                        max_retries -= 1;

                        debug!(
                            "ValidatorHaltedAtEpochEnd. Will retry after validator reconfigures"
                        );

                        if let Ok(Ok(new_epoch)) =
                            timeout(Duration::from_secs(15), state.wait_for_epoch(next_epoch)).await
                        {
                            assert_reachable!("retry submission at epoch end");
                            if new_epoch == next_epoch {
                                continue;
                            }

                            debug_fatal!(
                                "expected epoch {} after reconfiguration. got {}",
                                next_epoch,
                                new_epoch
                            );
                        }
                    }
                    return Err(err.into());
                }
            }
        }
    }

    async fn handle_submit_transaction_inner(
        &self,
        state: &AuthorityState,
        consensus_adapter: &ConsensusAdapter,
        metrics: &ValidatorServiceMetrics,
        request: &RawSubmitTxRequest,
        submitter_client_addr: Option<IpAddr>,
    ) -> SuiResult<(RawSubmitTxResponse, Weight)> {
        let epoch_store = state.load_epoch_store_one_call_per_task();
        if !epoch_store.protocol_config().mysticeti_fastpath() {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: "Mysticeti fastpath".to_string(),
            }
            .into());
        }

        let submit_type = SubmitTxType::try_from(request.submit_type).map_err(|e| {
            SuiErrorKind::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxRequest.submit_type".to_string(),
                error: e.to_string(),
            }
        })?;

        let is_ping_request = submit_type == SubmitTxType::Ping;
        if is_ping_request {
            fp_ensure!(
                request.transactions.is_empty(),
                SuiErrorKind::InvalidRequest(format!(
                    "Ping request cannot contain {} transactions",
                    request.transactions.len()
                ))
                .into()
            );
        } else {
            // Ensure default and soft bundle requests contain at least one transaction.
            fp_ensure!(
                !request.transactions.is_empty(),
                SuiErrorKind::InvalidRequest(
                    "At least one transaction needs to be submitted".to_string(),
                )
                .into()
            );
        }

        // NOTE: for soft bundle requests, the system tries to sequence the transactions in the same order
        // if they use the same gas price. But this is only done with best effort.
        // Transactions in a soft bundle can be individually rejected or deferred, without affecting
        // other transactions in the same bundle.
        let is_soft_bundle_request = submit_type == SubmitTxType::SoftBundle;

        let max_num_transactions = if is_soft_bundle_request {
            // Soft bundle cannot contain too many transactions.
            // Otherwise it is hard to include all of them in a single block.
            epoch_store.protocol_config().max_soft_bundle_size()
        } else {
            // Still enforce a limit even when transactions do not need to be in the same block.
            epoch_store
                .protocol_config()
                .max_num_transactions_in_block()
        };
        fp_ensure!(
            request.transactions.len() <= max_num_transactions as usize,
            SuiErrorKind::InvalidRequest(format!(
                "Too many transactions in request: {} vs {}",
                request.transactions.len(),
                max_num_transactions
            ))
            .into()
        );

        // Transaction digests.
        let mut tx_digests = Vec::with_capacity(request.transactions.len());
        // Transactions to submit to consensus.
        let mut consensus_transactions = Vec::with_capacity(request.transactions.len());
        // Indexes of transactions above in the request transactions.
        let mut transaction_indexes = Vec::with_capacity(request.transactions.len());
        // Results corresponding to each transaction in the request.
        let mut results: Vec<Option<SubmitTxResult>> = vec![None; request.transactions.len()];
        // Total size of all transactions in the request.
        let mut total_size_bytes = 0;

        let req_type = if is_ping_request {
            "ping"
        } else if request.transactions.len() == 1 {
            "single_transaction"
        } else if is_soft_bundle_request {
            "soft_bundle"
        } else {
            "batch"
        };

        let _handle_tx_metrics_guard = metrics
            .handle_submit_transaction_latency
            .with_label_values(&[req_type])
            .start_timer();

        for (idx, tx_bytes) in request.transactions.iter().enumerate() {
            let transaction = match bcs::from_bytes::<Transaction>(tx_bytes) {
                Ok(txn) => txn,
                Err(e) => {
                    // Ok to fail the request when any transaction is invalid.
                    return Err(SuiErrorKind::TransactionDeserializationError {
                        error: format!("Failed to deserialize transaction at index {}: {}", idx, e),
                    }
                    .into());
                }
            };

            // Ok to fail the request when any transaction is invalid.
            let tx_size = transaction.validity_check(&epoch_store.tx_validity_check_context())?;

            let overload_check_res = state.check_system_overload(
                consensus_adapter,
                transaction.data(),
                state.check_system_overload_at_signing(),
            );
            if let Err(error) = overload_check_res {
                metrics
                    .num_rejected_tx_during_overload
                    .with_label_values(&[error.as_ref()])
                    .inc();
                results[idx] = Some(SubmitTxResult::Rejected { error });
                continue;
            }

            // Ok to fail the request when any signature is invalid.
            let verified_transaction = {
                let _metrics_guard = metrics.tx_verification_latency.start_timer();
                if epoch_store.protocol_config().address_aliases() {
                    match epoch_store.verify_transaction_with_current_aliases(transaction) {
                        Ok(tx) => tx,
                        Err(e) => {
                            metrics.signature_errors.inc();
                            return Err(e);
                        }
                    }
                } else {
                    match epoch_store.verify_transaction_require_no_aliases(transaction) {
                        Ok(tx) => tx,
                        Err(e) => {
                            metrics.signature_errors.inc();
                            return Err(e);
                        }
                    }
                }
            };

            let tx_digest = verified_transaction.tx().digest();
            tx_digests.push(*tx_digest);

            debug!(
                ?tx_digest,
                "handle_submit_transaction: verified transaction"
            );

            // Check if the transaction has executed, before checking input objects
            // which could have been consumed.
            if let Some(effects) = state
                .get_transaction_cache_reader()
                .get_executed_effects(tx_digest)
            {
                let effects_digest = effects.digest();
                if let Ok(executed_data) = self.complete_executed_data(effects, None).await {
                    let executed_result = SubmitTxResult::Executed {
                        effects_digest,
                        details: Some(executed_data),
                        fast_path: false,
                    };
                    results[idx] = Some(executed_result);
                    debug!(?tx_digest, "handle_submit_transaction: already executed");
                    continue;
                }
            }

            if self
                .state
                .get_transaction_cache_reader()
                .transaction_executed_in_last_epoch(tx_digest, epoch_store.epoch())
            {
                results[idx] = Some(SubmitTxResult::Rejected {
                    error: UserInputError::TransactionAlreadyExecuted { digest: *tx_digest }.into(),
                });
                debug!(
                    ?tx_digest,
                    "handle_submit_transaction: transaction already executed in previous epoch"
                );
                continue;
            }

            debug!(
                ?tx_digest,
                "handle_submit_transaction: waiting for fastpath dependency objects"
            );
            if !state
                .wait_for_fastpath_dependency_objects(
                    verified_transaction.tx(),
                    epoch_store.epoch(),
                )
                .await?
            {
                debug!(
                    ?tx_digest,
                    "fastpath input objects are still unavailable after waiting"
                );
            }

            match state.handle_vote_transaction(&epoch_store, verified_transaction.tx().clone()) {
                Ok(_) => { /* continue processing */ }
                Err(e) => {
                    // Check if transaction has been executed while being validated.
                    // This is an edge case so checking executed effects twice is acceptable.
                    if let Some(effects) = state
                        .get_transaction_cache_reader()
                        .get_executed_effects(tx_digest)
                    {
                        let effects_digest = effects.digest();
                        if let Ok(executed_data) = self.complete_executed_data(effects, None).await
                        {
                            let executed_result = SubmitTxResult::Executed {
                                effects_digest,
                                details: Some(executed_data),
                                fast_path: false,
                            };
                            results[idx] = Some(executed_result);
                            continue;
                        }
                    }

                    // When the transaction has not been executed, record the error for the transaction.
                    debug!(?tx_digest, "Transaction rejected during submission: {e}");
                    metrics
                        .submission_rejected_transactions
                        .with_label_values(&[e.to_variant_name()])
                        .inc();
                    results[idx] = Some(SubmitTxResult::Rejected { error: e });
                    continue;
                }
            }

            // Create claims with aliases and / or immutable objects.
            if epoch_store.protocol_config().address_aliases()
                || epoch_store.protocol_config().disable_preconsensus_locking()
            {
                let mut claims = vec![];

                if epoch_store.protocol_config().disable_preconsensus_locking() {
                    let immutable_object_ids = self
                        .collect_immutable_object_ids(verified_transaction.tx(), state)
                        .await?;
                    if !immutable_object_ids.is_empty() {
                        claims.push(TransactionClaim::ImmutableInputObjects(
                            immutable_object_ids,
                        ));
                    }
                }

                let (tx, aliases) = verified_transaction.into_inner();
                if epoch_store.protocol_config().address_aliases() {
                    claims.push(TransactionClaim::AddressAliases(aliases));
                }

                let tx_with_claims = TransactionWithClaims::new(tx.into(), claims);

                consensus_transactions.push(ConsensusTransaction::new_user_transaction_v2_message(
                    &state.name,
                    tx_with_claims,
                ));
            } else {
                consensus_transactions.push(ConsensusTransaction::new_user_transaction_message(
                    &state.name,
                    verified_transaction.into_tx().into(),
                ));
            }
            transaction_indexes.push(idx);
            total_size_bytes += tx_size;
        }

        if consensus_transactions.is_empty() && !is_ping_request {
            return Ok((Self::try_from_submit_tx_response(results)?, Weight::zero()));
        }

        // Set the max bytes size of the soft bundle to be half of the consensus max transactions in block size.
        // We do this to account for serialization overheads and to ensure that the soft bundle is not too large
        // when is attempted to be posted via consensus.
        let max_transaction_bytes = if is_soft_bundle_request {
            epoch_store
                .protocol_config()
                .consensus_max_transactions_in_block_bytes()
                / 2
        } else {
            epoch_store
                .protocol_config()
                .consensus_max_transactions_in_block_bytes()
        };
        fp_ensure!(
            total_size_bytes <= max_transaction_bytes as usize,
            SuiErrorKind::UserInputError {
                error: UserInputError::TotalTransactionSizeTooLargeInBatch {
                    size: total_size_bytes,
                    limit: max_transaction_bytes,
                },
            }
            .into()
        );

        metrics
            .handle_submit_transaction_bytes
            .with_label_values(&[req_type])
            .observe(total_size_bytes as f64);
        metrics
            .handle_submit_transaction_batch_size
            .with_label_values(&[req_type])
            .observe(consensus_transactions.len() as f64);

        let _latency_metric_guard = metrics
            .handle_submit_transaction_consensus_latency
            .with_label_values(&[req_type])
            .start_timer();

        let consensus_positions = if is_soft_bundle_request || is_ping_request {
            // We only allow the `consensus_transactions` to be empty for ping requests. This is how it should and is be treated from the downstream components.
            // For any other case, having an empty `consensus_transactions` vector is an invalid state and we should have never reached at this point.
            assert!(
                is_ping_request || !consensus_transactions.is_empty(),
                "A valid soft bundle must have at least one transaction"
            );
            debug!(
                "handle_submit_transaction: submitting consensus transactions ({}): {}",
                req_type,
                consensus_transactions
                    .iter()
                    .map(|t| t.local_display())
                    .join(", ")
            );
            self.handle_submit_to_consensus_for_position(
                consensus_transactions,
                &epoch_store,
                submitter_client_addr,
            )
            .await?
        } else {
            let futures = consensus_transactions.into_iter().map(|t| {
                debug!(
                    "handle_submit_transaction: submitting consensus transaction ({}): {}",
                    req_type,
                    t.local_display(),
                );
                self.handle_submit_to_consensus_for_position(
                    vec![t],
                    &epoch_store,
                    submitter_client_addr,
                )
            });
            future::try_join_all(futures)
                .await?
                .into_iter()
                .flatten()
                .collect()
        };

        if is_ping_request {
            // For ping requests, return the special consensus position.
            assert_eq!(consensus_positions.len(), 1);
            results.push(Some(SubmitTxResult::Submitted {
                consensus_position: consensus_positions[0],
            }));
        } else {
            // Otherwise, return the consensus position for each transaction.
            for ((idx, tx_digest), consensus_position) in transaction_indexes
                .into_iter()
                .zip(tx_digests)
                .zip(consensus_positions)
            {
                debug!(
                    ?tx_digest,
                    "handle_submit_transaction: submitted consensus transaction at {}",
                    consensus_position,
                );
                results[idx] = Some(SubmitTxResult::Submitted { consensus_position });
            }
        }

        Ok((Self::try_from_submit_tx_response(results)?, Weight::zero()))
    }

    fn try_from_submit_tx_response(
        results: Vec<Option<SubmitTxResult>>,
    ) -> Result<RawSubmitTxResponse, SuiError> {
        let mut raw_results = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            let result = result.ok_or_else(|| SuiErrorKind::GenericAuthorityError {
                error: format!("Missing transaction result at {}", i),
            })?;
            let raw_result = result.try_into()?;
            raw_results.push(raw_result);
        }
        Ok(RawSubmitTxResponse {
            results: raw_results,
        })
    }

    #[instrument(
        name = "ValidatorService::handle_submit_to_consensus_for_position",
        level = "debug",
        skip_all,
        err(level = "debug")
    )]
    async fn handle_submit_to_consensus_for_position(
        &self,
        // Empty when this is a ping request.
        consensus_transactions: Vec<ConsensusTransaction>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        submitter_client_addr: Option<IpAddr>,
    ) -> Result<Vec<ConsensusPosition>, tonic::Status> {
        let (tx_consensus_positions, rx_consensus_positions) = oneshot::channel();

        {
            // code block within reconfiguration lock
            let reconfiguration_lock = epoch_store.get_reconfig_state_read_lock_guard();
            if !reconfiguration_lock.should_accept_user_certs() {
                self.metrics.num_rejected_cert_in_epoch_boundary.inc();
                return Err(SuiErrorKind::ValidatorHaltedAtEpochEnd.into());
            }

            // Submit to consensus and wait for position, we do not check if tx
            // has been processed by consensus already as this method is called
            // to get back a consensus position.
            let _metrics_guard = self.metrics.consensus_latency.start_timer();

            self.consensus_adapter.submit_batch(
                &consensus_transactions,
                Some(&reconfiguration_lock),
                epoch_store,
                Some(tx_consensus_positions),
                submitter_client_addr,
            )?;
        }

        Ok(rx_consensus_positions.await.map_err(|e| {
            SuiErrorKind::FailedToSubmitToConsensus(format!(
                "Failed to get consensus position: {e}"
            ))
        })?)
    }

    async fn collect_effects_data(
        &self,
        effects: &TransactionEffects,
        include_events: bool,
        include_input_objects: bool,
        include_output_objects: bool,
        fastpath_outputs: Option<Arc<TransactionOutputs>>,
    ) -> SuiResult<(Option<TransactionEvents>, Vec<Object>, Vec<Object>)> {
        let events = if include_events && effects.events_digest().is_some() {
            if let Some(fastpath_outputs) = &fastpath_outputs {
                Some(fastpath_outputs.events.clone())
            } else {
                Some(
                    self.state
                        .get_transaction_events(effects.transaction_digest())?,
                )
            }
        } else {
            None
        };

        let input_objects = if include_input_objects {
            self.state.get_transaction_input_objects(effects)?
        } else {
            vec![]
        };

        let output_objects = if include_output_objects {
            if let Some(fastpath_outputs) = &fastpath_outputs {
                fastpath_outputs.written.values().cloned().collect()
            } else {
                self.state.get_transaction_output_objects(effects)?
            }
        } else {
            vec![]
        };

        Ok((events, input_objects, output_objects))
    }
}

type WrappedServiceResponse<T> = Result<(tonic::Response<T>, Weight), tonic::Status>;

impl ValidatorService {
    async fn handle_submit_transaction_impl(
        &self,
        request: tonic::Request<RawSubmitTxRequest>,
    ) -> WrappedServiceResponse<RawSubmitTxResponse> {
        self.handle_submit_transaction(request).await
    }

    async fn wait_for_effects_impl(
        &self,
        request: tonic::Request<RawWaitForEffectsRequest>,
    ) -> WrappedServiceResponse<RawWaitForEffectsResponse> {
        let request: WaitForEffectsRequest = request.into_inner().try_into()?;
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let response = timeout(
            // TODO(fastpath): Tune this once we have a good estimate of the typical delay.
            Duration::from_secs(20),
            epoch_store
                .within_alive_epoch(self.wait_for_effects_response(request, &epoch_store))
                .map_err(|_| SuiErrorKind::EpochEnded(epoch_store.epoch())),
        )
        .await
        .map_err(|_| tonic::Status::internal("Timeout waiting for effects"))???
        .try_into()?;
        Ok((tonic::Response::new(response), Weight::zero()))
    }

    #[instrument(name= "ValidatorService::wait_for_effects_response", level = "error", skip_all, fields(consensus_position = ?request.consensus_position, fast_path_effects = tracing::field::Empty))]
    async fn wait_for_effects_response(
        &self,
        request: WaitForEffectsRequest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<WaitForEffectsResponse> {
        if request.ping_type.is_some() {
            return timeout(
                Duration::from_secs(10),
                self.ping_response(request, epoch_store),
            )
            .await
            .map_err(|_| SuiErrorKind::TimeoutError)?;
        }

        let Some(tx_digest) = request.transaction_digest else {
            return Err(SuiErrorKind::InvalidRequest(
                "Transaction digest is required for wait for effects requests".to_string(),
            )
            .into());
        };
        let tx_digests = [tx_digest];

        let fastpath_effects_future: Pin<Box<dyn Future<Output = _> + Send>> =
            if let Some(consensus_position) = request.consensus_position {
                Box::pin(self.wait_for_fastpath_effects(
                    consensus_position,
                    &tx_digests,
                    request.include_details,
                    epoch_store,
                ))
            } else {
                Box::pin(futures::future::pending())
            };

        tokio::select! {
            // Ensure that finalized effects are always prioritized.
            biased;
            // We always wait for effects regardless of consensus position via
            // notify_read_executed_effects. This is safe because we have separated
            // mysticeti fastpath outputs to a separate dirty cache
            // UncommittedData::fastpath_transaction_outputs that will only get flushed
            // once finalized. So the output of notify_read_executed_effects is
            // guaranteed to be finalized effects or effects from QD execution.
            mut effects = self.state
                .get_transaction_cache_reader()
                .notify_read_executed_effects(
                    "AuthorityServer::wait_for_effects::notify_read_executed_effects_finalized",
                    &tx_digests,
                ) => {
                tracing::Span::current().record("fast_path_effects", false);
                let effects = effects.pop().unwrap();
                let details = if request.include_details {
                    Some(self.complete_executed_data(effects.clone(), None).await?)
                } else {
                    None
                };

                Ok(WaitForEffectsResponse::Executed {
                    effects_digest: effects.digest(),
                    details,
                    fast_path: false,
                })
            }

            fastpath_response = fastpath_effects_future => {
                tracing::Span::current().record("fast_path_effects", true);
                fastpath_response
            }
        }
    }

    #[instrument(level = "error", skip_all, err(level = "debug"))]
    async fn ping_response(
        &self,
        request: WaitForEffectsRequest,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<WaitForEffectsResponse> {
        let Some(consensus_tx_status_cache) = epoch_store.consensus_tx_status_cache.as_ref() else {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: "Mysticeti fastpath".to_string(),
            }
            .into());
        };

        let Some(consensus_position) = request.consensus_position else {
            return Err(SuiErrorKind::InvalidRequest(
                "Consensus position is required for Ping requests".to_string(),
            )
            .into());
        };

        // We assume that the caller has already checked for the existence of the `ping` field, but handling it gracefully here.
        let Some(ping) = request.ping_type else {
            return Err(SuiErrorKind::InvalidRequest(
                "Ping type is required for ping requests".to_string(),
            )
            .into());
        };

        let _metrics_guard = self
            .metrics
            .handle_wait_for_effects_ping_latency
            .with_label_values(&[ping.as_str()])
            .start_timer();

        consensus_tx_status_cache.check_position_too_ahead(&consensus_position)?;

        let mut last_status = None;
        let details = if request.include_details {
            Some(Box::new(ExecutedData::default()))
        } else {
            None
        };

        loop {
            let status = consensus_tx_status_cache
                .notify_read_transaction_status_change(consensus_position, last_status)
                .await;
            match status {
                NotifyReadConsensusTxStatusResult::Status(status) => match status {
                    ConsensusTxStatus::FastpathCertified => {
                        // If the request is for consensus, we need to wait for the transaction to be finalised via Consensus.
                        if ping == PingType::Consensus {
                            last_status = Some(status);
                            continue;
                        }
                        return Ok(WaitForEffectsResponse::Executed {
                            effects_digest: TransactionEffectsDigest::ZERO,
                            details,
                            fast_path: true,
                        });
                    }
                    ConsensusTxStatus::Rejected => {
                        return Ok(WaitForEffectsResponse::Rejected { error: None });
                    }
                    ConsensusTxStatus::Finalized => {
                        return Ok(WaitForEffectsResponse::Executed {
                            effects_digest: TransactionEffectsDigest::ZERO,
                            details,
                            fast_path: false,
                        });
                    }
                    ConsensusTxStatus::Dropped => {
                        // Transaction was dropped post-consensus, currently only due to invalid owned object inputs..
                        // Fetch the detailed error (e.g., ObjectLockConflict) from the rejection reason cache.
                        return Ok(WaitForEffectsResponse::Rejected {
                            error: epoch_store.get_rejection_vote_reason(consensus_position),
                        });
                    }
                },
                NotifyReadConsensusTxStatusResult::Expired(round) => {
                    return Ok(WaitForEffectsResponse::Expired {
                        epoch: epoch_store.epoch(),
                        round: Some(round),
                    });
                }
            }
        }
    }

    #[instrument(level = "error", skip_all, err(level = "debug"))]
    async fn wait_for_fastpath_effects(
        &self,
        consensus_position: ConsensusPosition,
        tx_digests: &[TransactionDigest],
        include_details: bool,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<WaitForEffectsResponse> {
        let Some(consensus_tx_status_cache) = epoch_store.consensus_tx_status_cache.as_ref() else {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: "Mysticeti fastpath".to_string(),
            }
            .into());
        };

        let local_epoch = epoch_store.epoch();
        match consensus_position.epoch.cmp(&local_epoch) {
            Ordering::Less => {
                // Ask TransactionDriver to retry submitting the transaction and get a new ConsensusPosition,
                // if response from this validator is desired.
                let response = WaitForEffectsResponse::Expired {
                    epoch: local_epoch,
                    round: None,
                };
                return Ok(response);
            }
            Ordering::Greater => {
                // Ask TransactionDriver to retry this RPC until the validator's epoch catches up.
                return Err(SuiErrorKind::WrongEpoch {
                    expected_epoch: local_epoch,
                    actual_epoch: consensus_position.epoch,
                }
                .into());
            }
            Ordering::Equal => {
                // The validator's epoch is the same as the epoch of the transaction.
                // We can proceed with the normal flow.
            }
        };

        consensus_tx_status_cache.check_position_too_ahead(&consensus_position)?;

        let mut current_status = None;
        loop {
            tokio::select! {
                status_result = consensus_tx_status_cache
                    .notify_read_transaction_status_change(consensus_position, current_status) => {
                    match status_result {
                        NotifyReadConsensusTxStatusResult::Status(new_status) => {
                            match new_status {
                                ConsensusTxStatus::Rejected => {
                                    return Ok(WaitForEffectsResponse::Rejected {
                                        error: epoch_store.get_rejection_vote_reason(
                                            consensus_position
                                        )
                                    });
                                }
                                ConsensusTxStatus::FastpathCertified => {
                                    current_status = Some(new_status);
                                    continue;
                                }
                                ConsensusTxStatus::Finalized => {
                                    current_status = Some(new_status);
                                    continue;
                                }
                                ConsensusTxStatus::Dropped => {
                                    // Transaction was dropped post-consensus, currently only due to invalid owned object inputs.
                                    // Fetch the detailed error from the rejection reason cache.
                                    return Ok(WaitForEffectsResponse::Rejected {
                                        error: epoch_store
                                            .get_rejection_vote_reason(consensus_position),
                                    });
                                }
                            }
                        }
                        NotifyReadConsensusTxStatusResult::Expired(round) => {
                            return Ok(WaitForEffectsResponse::Expired {
                                epoch: epoch_store.epoch(),
                                round: Some(round),
                            });
                        }
                    }
                }

                mut outputs = self.state.get_transaction_cache_reader().notify_read_fastpath_transaction_outputs(tx_digests),
                    if current_status == Some(ConsensusTxStatus::FastpathCertified) || current_status == Some(ConsensusTxStatus::Finalized) => {
                    let outputs = outputs.pop().unwrap();
                    let effects = outputs.effects.clone();

                    let details = if include_details {
                        Some(self.complete_executed_data(effects.clone(), Some(outputs)).await?)
                    } else {
                        None
                    };

                    return Ok(WaitForEffectsResponse::Executed {
                        effects_digest: effects.digest(),
                        details,
                        fast_path: current_status == Some(ConsensusTxStatus::FastpathCertified),
                    });
                }
            }
        }
    }

    async fn complete_executed_data(
        &self,
        effects: TransactionEffects,
        fastpath_outputs: Option<Arc<TransactionOutputs>>,
    ) -> SuiResult<Box<ExecutedData>> {
        let (events, input_objects, output_objects) = self
            .collect_effects_data(
                &effects,
                /* include_events */ true,
                /* include_input_objects */ true,
                /* include_output_objects */ true,
                fastpath_outputs,
            )
            .await?;
        Ok(Box::new(ExecutedData {
            effects,
            events,
            input_objects,
            output_objects,
        }))
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

    async fn validator_health_impl(
        &self,
        _request: tonic::Request<sui_types::messages_grpc::RawValidatorHealthRequest>,
    ) -> WrappedServiceResponse<sui_types::messages_grpc::RawValidatorHealthResponse> {
        let state = &self.state;

        // Get epoch store once for both metrics
        let epoch_store = state.load_epoch_store_one_call_per_task();

        // Get in-flight execution transactions from execution scheduler
        let num_inflight_execution_transactions =
            state.execution_scheduler().num_pending_certificates() as u64;

        // Get in-flight consensus transactions from consensus adapter
        let num_inflight_consensus_transactions =
            self.consensus_adapter.num_inflight_transactions();

        // Get last committed leader round from epoch store
        let last_committed_leader_round = epoch_store
            .consensus_tx_status_cache
            .as_ref()
            .and_then(|cache| cache.get_last_committed_leader_round())
            .unwrap_or(0);

        // Get last locally built checkpoint sequence
        let last_locally_built_checkpoint = epoch_store
            .last_built_checkpoint_summary()
            .ok()
            .flatten()
            .map(|(_, summary)| summary.sequence_number)
            .unwrap_or(0);

        let typed_response = sui_types::messages_grpc::ValidatorHealthResponse {
            num_inflight_consensus_transactions,
            num_inflight_execution_transactions,
            last_locally_built_checkpoint,
            last_committed_leader_round,
        };

        let raw_response = typed_response
            .try_into()
            .map_err(|e: sui_types::error::SuiError| {
                tonic::Status::internal(format!("Failed to serialize health response: {}", e))
            })?;

        Ok((tonic::Response::new(raw_response), Weight::one()))
    }

    fn get_client_ip_addr<T>(
        &self,
        request: &tonic::Request<T>,
        source: &ClientIdSource,
    ) -> Option<IpAddr> {
        let forwarded_header = request.metadata().get_all("x-forwarded-for").iter().next();

        if let Some(header) = forwarded_header {
            let num_hops = header
                .to_str()
                .map(|h| h.split(',').count().saturating_sub(1))
                .unwrap_or(0);

            self.metrics.x_forwarded_for_num_hops.set(num_hops as f64);
        }

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
                                    header_contents, contents_len, num_hops, contents_len,
                                );
                                self.metrics.client_id_source_config_mismatch.inc();
                                return None;
                            }
                            let Some(client_ip) = header_contents.get(contents_len - num_hops)
                            else {
                                error!(
                                    "x-forwarded-for header value of {:?} contains {} values, but {} hops were specified. \
                                    Expected at least {} values. Skipping traffic controller request handling.",
                                    header_contents, contents_len, num_hops, contents_len,
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
                    error!(
                        "x-forwarded-for header not present for request despite node configuring x-forwarded-for tracking type"
                    );
                    None
                }
            }
        }
    }

    async fn handle_traffic_req(&self, client: Option<IpAddr>) -> Result<(), tonic::Status> {
        if let Some(traffic_controller) = &self.traffic_controller {
            if !traffic_controller.check(&client, &None).await {
                // Entity in blocklist
                Err(tonic::Status::from_error(
                    SuiErrorKind::TooManyRequests.into(),
                ))
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

// TODO: refine error matching here
fn normalize(err: SuiError) -> Weight {
    match err.as_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::IncorrectUserSignature { .. },
        } => Weight::one(),
        SuiErrorKind::InvalidSignature { .. }
        | SuiErrorKind::SignerSignatureAbsent { .. }
        | SuiErrorKind::SignerSignatureNumberMismatch { .. }
        | SuiErrorKind::IncorrectSigner { .. }
        | SuiErrorKind::UnknownSigner { .. }
        | SuiErrorKind::WrongEpoch { .. } => Weight::one(),
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
    async fn submit_transaction(
        &self,
        request: tonic::Request<RawSubmitTxRequest>,
    ) -> Result<tonic::Response<RawSubmitTxResponse>, tonic::Status> {
        let validator_service = self.clone();

        // Spawns a task which handles the transaction. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        spawn_monitored_task!(async move {
            // NB: traffic tally wrapping handled within the task rather than on task exit
            // to prevent an attacker from subverting traffic control by severing the connection
            handle_with_decoration!(validator_service, handle_submit_transaction_impl, request)
        })
        .await
        .unwrap()
    }

    async fn wait_for_effects(
        &self,
        request: tonic::Request<RawWaitForEffectsRequest>,
    ) -> Result<tonic::Response<RawWaitForEffectsResponse>, tonic::Status> {
        handle_with_decoration!(self, wait_for_effects_impl, request)
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

    async fn validator_health(
        &self,
        request: tonic::Request<sui_types::messages_grpc::RawValidatorHealthRequest>,
    ) -> Result<tonic::Response<sui_types::messages_grpc::RawValidatorHealthResponse>, tonic::Status>
    {
        handle_with_decoration!(self, validator_health_impl, request)
    }
}
