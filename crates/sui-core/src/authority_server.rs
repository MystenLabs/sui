// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{AuthorityState, ReconfigConsensusMessage},
    consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics, SuiTxValidator},
    metrics::start_timer,
};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use futures::{stream::BoxStream, TryStreamExt};
use multiaddr::Multiaddr;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};
use std::{io, sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};

use sui_types::{error::*, messages::*};
use tap::TapFallible;
use tokio::time::sleep;
use tokio::{sync::mpsc::Receiver, task::JoinHandle};

use sui_metrics::spawn_monitored_task;
use sui_types::messages_checkpoint::CheckpointRequest;
use sui_types::messages_checkpoint::CheckpointResponse;

use crate::consensus_adapter::SubmitToConsensus;
use crate::consensus_handler::ConsensusHandler;
use tracing::{debug, info, Instrument};

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

const MIN_BATCH_SIZE: u64 = 1000;
const MAX_DELAY_MILLIS: u64 = 5_000; // 5 sec

// Assuming 200 consensus tps * 5 sec consensus latency = 1000 inflight consensus txns.
// Leaving a bit more headroom to cap the max inflight consensus txns to 1000*2 = 2000.
const MAX_PENDING_CONSENSUS_TRANSACTIONS: u64 = 2000;

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
    min_batch_size: u64,
    max_delay: Duration,
    pub metrics: Arc<ValidatorServiceMetrics>,
}

impl AuthorityServer {
    pub fn new_for_test(
        address: Multiaddr,
        state: Arc<AuthorityState>,
        consensus_address: Multiaddr,
    ) -> Self {
        use narwhal_types::TransactionsClient;
        let consensus_client = Box::new(TransactionsClient::new(
            mysten_network::client::connect_lazy(&consensus_address)
                .expect("Failed to connect to consensus"),
        ));
        let consensus_adapter = ConsensusAdapter::new(
            consensus_client,
            state.clone(),
            ConsensusAdapterMetrics::new_test(),
        );

        let metrics = Arc::new(ValidatorServiceMetrics::new_for_tests());

        Self {
            address,
            state,
            consensus_adapter,
            min_batch_size: MIN_BATCH_SIZE,
            max_delay: Duration::from_millis(MAX_DELAY_MILLIS),
            metrics,
        }
    }

    /// Create a batch subsystem, register it with the authority state, and
    /// launch a task that manages it. Return the join handle of this task.
    pub async fn spawn_batch_subsystem(
        &self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<JoinHandle<()>> {
        // Start the batching subsystem, and register the handles with the authority.
        let state = self.state.clone();
        let batch_join_handle =
            spawn_monitored_task!(state.run_batch_service(min_batch_size, max_delay));

        Ok(batch_join_handle)
    }

    pub async fn spawn_for_test(self) -> Result<AuthorityServerHandle, io::Error> {
        let address = self.address.clone();
        self.spawn_with_bind_address_for_test(address).await
    }

    pub async fn spawn_with_bind_address_for_test(
        self,
        address: Multiaddr,
    ) -> Result<AuthorityServerHandle, io::Error> {
        // Start the batching subsystem
        let _join_handle = self
            .spawn_batch_subsystem(self.min_batch_size, self.max_delay)
            .await;

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
    pub handle_transaction_consensus_latency: Histogram,
    pub handle_transaction_non_consensus_latency: Histogram,
    pub handle_certificate_consensus_latency: Histogram,
    pub handle_certificate_non_consensus_latency: Histogram,
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
            handle_transaction_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_consensus_latency",
                "Latency of handling a consensus transaction",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            handle_transaction_non_consensus_latency: register_histogram_with_registry!(
                "validator_service_handle_transaction_non_consensus_latency",
                "Latency of handling a non-consensus transaction",
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
    /// Spawn all the subsystems run by a Sui authority: a consensus node, a sui authority server,
    /// and a consensus listener bridging the consensus node and the sui authority.
    pub async fn new(
        consensus_client: Box<dyn SubmitToConsensus>,
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        prometheus_registry: Registry,
        rx_reconfigure_consensus: Receiver<ReconfigConsensusMessage>,
    ) -> Result<Self> {
        // Spawn the consensus node of this authority.
        let consensus_config = config
            .consensus_config()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;
        let consensus_keypair = config.protocol_key_pair().copy();
        let consensus_worker_keypair = config.worker_key_pair().copy();
        let consensus_committee = config.genesis()?.narwhal_committee().load();
        let consensus_worker_cache = config.genesis()?.narwhal_worker_cache();
        let consensus_storage_base_path = consensus_config.db_path().to_path_buf();
        let consensus_execution_state = ConsensusHandler::new(state.clone());
        let consensus_execution_state = Arc::new(consensus_execution_state);

        let consensus_parameters = consensus_config.narwhal_config().to_owned();
        let network_keypair = config.network_key_pair.copy();

        let registry = prometheus_registry.clone();
        spawn_monitored_task!(narwhal_node::restarter::NodeRestarter::watch(
            consensus_keypair,
            network_keypair,
            vec![(0, consensus_worker_keypair)],
            &consensus_committee,
            consensus_worker_cache,
            consensus_storage_base_path,
            consensus_execution_state,
            consensus_parameters,
            // TODO: provide something more clever here to specify TX validity
            SuiTxValidator::default(),
            rx_reconfigure_consensus,
            &registry,
        ));

        let ca_metrics = ConsensusAdapterMetrics::new(&prometheus_registry);

        // The consensus adapter allows the authority to send user certificates through consensus.
        let consensus_adapter = ConsensusAdapter::new(consensus_client, state.clone(), ca_metrics);

        Ok(Self {
            state,
            consensus_adapter,
            metrics: Arc::new(ValidatorServiceMetrics::new(&prometheus_registry)),
        })
    }

    async fn handle_transaction(
        state: Arc<AuthorityState>,
        request: tonic::Request<Transaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let transaction = request.into_inner();
        let is_consensus_tx = transaction.contains_shared_object();

        let _metrics_guard = start_timer(if is_consensus_tx {
            metrics.handle_transaction_consensus_latency.clone()
        } else {
            metrics.handle_transaction_non_consensus_latency.clone()
        });
        let tx_verif_metrics_guard = start_timer(metrics.tx_verification_latency.clone());

        let transaction = transaction.verify().tap_err(|_| {
            metrics.signature_errors.inc();
        })?;
        drop(tx_verif_metrics_guard);

        let tx_digest = transaction.digest();

        // Enable Trace Propagation across spans/processes using tx_digest
        let span = tracing::debug_span!(
            "validator_state_process_tx",
            ?tx_digest,
            tx_kind = transaction.data().data.kind_as_str()
        );

        let info = state
            .handle_transaction(transaction)
            .instrument(span)
            .await?;

        Ok(tonic::Response::new(info.into()))
    }

    async fn handle_certificate(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        request: tonic::Request<CertifiedTransaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
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
        if let Some(response) = state.get_tx_info_already_executed(&tx_digest).await? {
            return Ok(tonic::Response::new(response.into()));
        }

        // 2) Verify cert signatures
        let cert_verif_metrics_guard = start_timer(metrics.cert_verification_latency.clone());
        let certificate = certificate.verify(&state.committee.load())?;
        drop(cert_verif_metrics_guard);

        // 3) All certificates are sent to consensus (at least by some authorities)
        // For shared objects this will wait until either timeout or we have heard back from consensus.
        // For owned objects this will return without waiting for certificate to be sequenced
        // First do quick dirty non-async check
        if !state.consensus_message_processed(&certificate)? {
            // Note that num_inflight_transactions() only include user submitted transactions, and only user txns can be dropped here.
            // This backpressure should not affect system transactions, e.g. for checkpointing.
            if consensus_adapter.num_inflight_transactions() > MAX_PENDING_CONSENSUS_TRANSACTIONS {
                return Err(tonic::Status::resource_exhausted("Reached {MAX_PENDING_CONSENSUS_TRANSACTIONS} concurrent consensus transactions",
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
            let waiter = consensus_adapter.submit(transaction).await?;
            if certificate.contains_shared_object() {
                // This is expect on tokio JoinHandle result, not SuiResult
                waiter
                    .await
                    .expect("Tokio runtime failure when waiting for consensus result");
            }
        }

        // 4) Execute the certificate.
        // Often we cannot execute a cert due to dependenties haven't been executed, and we will
        // observe TransactionInputObjectsErrors. In such case, we can wait and retry. It should eventually
        // succeed.
        // TODO: This is a quick hack. We should properly fix this through dependency-based
        // scheduling.
        let mut retry_delay_ms = 200;
        loop {
            let span = tracing::debug_span!(
                "validator_state_process_cert",
                ?tx_digest,
                tx_kind = certificate.data().data.kind_as_str()
            );
            match state
                .handle_certificate(&certificate)
                .instrument(span)
                .await
            {
                // For owned object certificates, we could also be getting this error
                // if this validator hasn't executed some of the causal dependencies.
                // And that's ok because there must exist 2f+1 that has. So we can
                // afford this validator returning error.
                err @ Err(SuiError::TransactionInputObjectsErrors { .. }) if shared_object_tx => {
                    if retry_delay_ms >= 12800 {
                        return Err(tonic::Status::from(err.unwrap_err()));
                    }
                    debug!(
                        ?tx_digest,
                        ?retry_delay_ms,
                        "Certificate failed due to missing dependencies, wait and retry",
                    );
                    sleep(Duration::from_millis(retry_delay_ms)).await;
                    retry_delay_ms *= 2;
                }
                Err(e) => {
                    return Err(tonic::Status::from(e));
                }
                Ok(response) => {
                    return Ok(tonic::Response::new(response.into()));
                }
            }
        }
    }
}

#[async_trait]
impl Validator for ValidatorService {
    async fn transaction(
        &self,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
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
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let state = self.state.clone();
        let consensus_adapter = self.consensus_adapter.clone();

        // Spawns a task which handles the certificate. The task will unconditionally continue
        // processing in the event that the client connection is dropped.
        let metrics = self.metrics.clone();
        spawn_monitored_task!(Self::handle_certificate(
            state,
            consensus_adapter,
            request,
            metrics
        ))
        .await
        .unwrap()
    }

    async fn account_info(
        &self,
        request: tonic::Request<AccountInfoRequest>,
    ) -> Result<tonic::Response<AccountInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_account_info_request(request).await?;

        Ok(tonic::Response::new(response))
    }

    async fn object_info(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> Result<tonic::Response<ObjectInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_object_info_request(request).await?;

        Ok(tonic::Response::new(response.into()))
    }

    async fn transaction_info(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_transaction_info_request(request).await?;

        Ok(tonic::Response::new(response.into()))
    }

    type FollowTxStreamStream = BoxStream<'static, Result<BatchInfoResponseItem, tonic::Status>>;

    async fn batch_info(
        &self,
        request: tonic::Request<BatchInfoRequest>,
    ) -> Result<tonic::Response<Self::FollowTxStreamStream>, tonic::Status> {
        let request = request.into_inner();

        let xstream = self.state.handle_batch_streaming(request).await?;

        let response = xstream.map_err(tonic::Status::from);

        Ok(tonic::Response::new(Box::pin(response)))
    }

    async fn checkpoint(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> Result<tonic::Response<CheckpointResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_checkpoint_request(&request)?;

        return Ok(tonic::Response::new(response));
    }

    async fn committee_info(
        &self,
        request: tonic::Request<CommitteeInfoRequest>,
    ) -> Result<tonic::Response<CommitteeInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self.state.handle_committee_info_request(&request)?;

        return Ok(tonic::Response::new(response));
    }
}
