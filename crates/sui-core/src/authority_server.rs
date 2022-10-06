// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{AuthorityState, ReconfigConsensusMessage},
    consensus_adapter::{
        CheckpointConsensusAdapter, CheckpointSender, ConsensusAdapter, ConsensusAdapterMetrics,
        ConsensusListener, ConsensusListenerMessage,
    },
    metrics::start_timer,
};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use futures::{stream::BoxStream, TryStreamExt};
use multiaddr::Multiaddr;
use prometheus::{register_histogram_with_registry, Histogram, Registry};
use std::{io, sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};

use sui_types::{error::*, messages::*};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};

use sui_types::messages_checkpoint::CheckpointRequest;
use sui_types::messages_checkpoint::CheckpointResponse;

use crate::authority::ConsensusHandler;
use tracing::{info, Instrument};

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
    handle: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
}

impl AuthorityServerHandle {
    pub async fn join(self) -> Result<(), std::io::Error> {
        // Note that dropping `self.complete` would terminate the server.
        self.handle
            .await?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub async fn kill(self) -> Result<(), std::io::Error> {
        self.tx_cancellation.send(()).map_err(|_e| {
            std::io::Error::new(io::ErrorKind::Other, "could not send cancellation signal!")
        })?;
        self.handle
            .await?
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    pub fn address(&self) -> &Multiaddr {
        &self.local_addr
    }
}

pub struct AuthorityServer {
    address: Multiaddr,
    pub state: Arc<AuthorityState>,
    consensus_adapter: ConsensusAdapter,
    min_batch_size: u64,
    max_delay: Duration,
}

impl AuthorityServer {
    pub fn new_for_test(
        address: Multiaddr,
        state: Arc<AuthorityState>,
        consensus_address: Multiaddr,
        tx_consensus_listener: Sender<ConsensusListenerMessage>,
    ) -> Self {
        let metrics = ConsensusAdapterMetrics::new_test();
        let consensus_adapter = ConsensusAdapter::new(
            consensus_address,
            state.clone_committee(),
            tx_consensus_listener,
            Duration::from_secs(20),
            metrics,
        );

        Self {
            address,
            state,
            consensus_adapter,
            min_batch_size: MIN_BATCH_SIZE,
            max_delay: Duration::from_millis(MAX_DELAY_MILLIS),
        }
    }

    /// Create a batch subsystem, register it with the authority state, and
    /// launch a task that manages it. Return the join handle of this task.
    pub async fn spawn_batch_subsystem(
        &self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<tokio::task::JoinHandle<SuiResult<()>>> {
        // Start the batching subsystem, and register the handles with the authority.
        let state = self.state.clone();
        let _batch_join_handle =
            tokio::task::spawn(
                async move { state.run_batch_service(min_batch_size, max_delay).await },
            );

        Ok(_batch_join_handle)
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
                consensus_adapter: Arc::new(self.consensus_adapter),
                _checkpoint_consensus_handle: None,
                metrics: Arc::new(ValidatorServiceMetrics::new_for_tests()),
            }))
            .bind(&address)
            .await
            .unwrap();
        let local_addr = server.local_addr().to_owned();
        info!("Listening to traffic on {local_addr}");
        let handle = AuthorityServerHandle {
            tx_cancellation: server.take_cancel_handle().unwrap(),
            local_addr,
            handle: tokio::spawn(server.serve()),
        };
        Ok(handle)
    }
}

pub struct ValidatorServiceMetrics {
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
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}

pub struct ValidatorService {
    state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    _checkpoint_consensus_handle: Option<JoinHandle<()>>,
    metrics: Arc<ValidatorServiceMetrics>,
}

impl ValidatorService {
    /// Spawn all the subsystems run by a Sui authority: a consensus node, a sui authority server,
    /// and a consensus listener bridging the consensus node and the sui authority.
    pub async fn new(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        prometheus_registry: Registry,
        rx_reconfigure_consensus: Receiver<ReconfigConsensusMessage>,
    ) -> Result<Self> {
        let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1_000);
        let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1_000);

        // Spawn the consensus node of this authority.
        let consensus_config = config
            .consensus_config()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;
        let consensus_keypair = config.protocol_key_pair().copy();
        let consensus_worker_keypair = config.worker_key_pair().copy();
        let consensus_committee = config.genesis()?.narwhal_committee().load();
        let consensus_worker_cache = config.genesis()?.narwhal_worker_cache();
        let consensus_storage_base_path = consensus_config.db_path().to_path_buf();
        let consensus_execution_state = ConsensusHandler::new(state.clone(), tx_consensus_to_sui);
        let consensus_execution_state = Arc::new(consensus_execution_state);
        let consensus_parameters = consensus_config.narwhal_config().to_owned();
        let network_keypair = config.network_key_pair.copy();

        let registry = prometheus_registry.clone();
        tokio::spawn(async move {
            narwhal_node::restarter::NodeRestarter::watch(
                consensus_keypair,
                network_keypair,
                vec![(0, consensus_worker_keypair)],
                &consensus_committee,
                consensus_worker_cache,
                consensus_storage_base_path,
                consensus_execution_state,
                consensus_parameters,
                rx_reconfigure_consensus,
                &registry,
            )
            .await
        });

        // Spawn a consensus listener. It listen for consensus outputs and notifies the
        // authority server when a sequenced transaction is ready for execution.
        ConsensusListener::spawn(rx_sui_to_consensus, rx_consensus_to_sui);

        let timeout = Duration::from_secs(consensus_config.timeout_secs.unwrap_or(60));
        let ca_metrics = ConsensusAdapterMetrics::new(&prometheus_registry);

        // The consensus adapter allows the authority to send user certificates through consensus.
        let consensus_adapter = ConsensusAdapter::new(
            consensus_config.address().to_owned(),
            state.clone_committee(),
            tx_sui_to_consensus.clone(),
            timeout,
            ca_metrics.clone(),
        );

        // Update the checkpoint store with a consensus client.
        let (tx_checkpoint_consensus_adapter, rx_checkpoint_consensus_adapter) = channel(1_000);
        let consensus_sender = CheckpointSender::new(tx_checkpoint_consensus_adapter);
        state
            .checkpoints
            .lock()
            .set_consensus(Box::new(consensus_sender))?;

        let checkpoint_consensus_handle = Some(
            CheckpointConsensusAdapter::new(
                /* consensus_address */ consensus_config.address().to_owned(),
                /* tx_consensus_listener */ tx_sui_to_consensus,
                rx_checkpoint_consensus_adapter,
                /* checkpoint_locals */ state.checkpoints(),
                /* retry_delay */ timeout,
                /* max_pending_transactions */ 10_000,
                ca_metrics,
            )
            .spawn(),
        );

        Ok(Self {
            state,
            consensus_adapter: Arc::new(consensus_adapter),
            _checkpoint_consensus_handle: checkpoint_consensus_handle,
            metrics: Arc::new(ValidatorServiceMetrics::new(&prometheus_registry)),
        })
    }

    async fn handle_transaction(
        state: Arc<AuthorityState>,
        request: tonic::Request<Transaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let mut transaction = request.into_inner();
        let is_consensus_tx = transaction.contains_shared_object();

        let _metrics_guard = start_timer(if is_consensus_tx {
            metrics.handle_transaction_consensus_latency.clone()
        } else {
            metrics.handle_transaction_non_consensus_latency.clone()
        });
        let tx_verif_metrics_guard = start_timer(metrics.tx_verification_latency.clone());

        transaction
            .verify()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        drop(tx_verif_metrics_guard);
        // TODO This is really really bad, we should have different types for signature-verified transactions
        transaction.is_verified = true;

        let tx_digest = transaction.digest();

        // Enable Trace Propagation across spans/processes using tx_digest
        let span = tracing::debug_span!(
            "validator_state_process_tx",
            ?tx_digest,
            tx_kind = transaction.signed_data.data.kind_as_str()
        );

        let info = state
            .handle_transaction(transaction)
            .instrument(span)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(info))
    }

    async fn handle_certificate(
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        request: tonic::Request<CertifiedTransaction>,
        metrics: Arc<ValidatorServiceMetrics>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let mut certificate = request.into_inner();
        let is_consensus_tx = certificate.contains_shared_object();

        let _metrics_guard = start_timer(if is_consensus_tx {
            metrics.handle_certificate_consensus_latency.clone()
        } else {
            metrics.handle_certificate_non_consensus_latency.clone()
        });

        // 1) Verify certificate
        let cert_verif_metrics_guard = start_timer(metrics.cert_verification_latency.clone());

        certificate
            .verify(&state.committee.load())
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        drop(cert_verif_metrics_guard);
        // TODO This is really really bad, we should have different types for signature verified transactions
        certificate.is_verified = true;

        // 2) Check idempotency
        let tx_digest = certificate.digest();
        if let Some(response) = state
            .get_tx_info_already_executed(tx_digest)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
        {
            return Ok(tonic::Response::new(response));
        }

        // 3) If the validator is already halted, we stop here, to avoid
        // sending the transaction to consensus.
        if state.is_halted() && !certificate.signed_data.data.kind.is_system_tx() {
            return Err(tonic::Status::internal(
                SuiError::ValidatorHaltedAtEpochEnd.to_string(),
            ));
        }

        // 4) If it's a shared object transaction and requires consensus, we need to do so.
        // This will wait until either timeout or we have heard back from consensus.
        if is_consensus_tx
            && !state
                .transaction_shared_locks_exist(&certificate)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
        {
            // Note that num_inflight_transactions() only include user submitted transactions, and only user txns can be dropped here.
            // This backpressure should not affect system transactions, e.g. for checkpointing.
            if consensus_adapter.num_inflight_transactions() > MAX_PENDING_CONSENSUS_TRANSACTIONS {
                return Err(tonic::Status::resource_exhausted("Reached {MAX_PENDING_CONSENSUS_TRANSACTIONS} concurrent consensus transactions",
                ));
            }
            let _metrics_guard = start_timer(metrics.consensus_latency.clone());
            consensus_adapter
                .submit(&state.name, &certificate)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        // 5) Execute the certificate.
        let span = tracing::debug_span!(
            "validator_state_process_cert",
            ?tx_digest,
            tx_kind = certificate.signed_data.data.kind_as_str()
        );

        let response = state
            .handle_certificate(certificate)
            .instrument(span)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(response))
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
        tokio::spawn(async move { Self::handle_transaction(state, request, metrics).await })
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
        tokio::spawn(async move {
            Self::handle_certificate(state, consensus_adapter, request, metrics).await
        })
        .await
        .unwrap()
    }

    async fn account_info(
        &self,
        request: tonic::Request<AccountInfoRequest>,
    ) -> Result<tonic::Response<AccountInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_account_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(response))
    }

    async fn object_info(
        &self,
        request: tonic::Request<ObjectInfoRequest>,
    ) -> Result<tonic::Response<ObjectInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_object_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(response))
    }

    async fn transaction_info(
        &self,
        request: tonic::Request<TransactionInfoRequest>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_transaction_info_request(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(response))
    }

    type FollowTxStreamStream = BoxStream<'static, Result<BatchInfoResponseItem, tonic::Status>>;

    async fn batch_info(
        &self,
        request: tonic::Request<BatchInfoRequest>,
    ) -> Result<tonic::Response<Self::FollowTxStreamStream>, tonic::Status> {
        let request = request.into_inner();

        let xstream = self
            .state
            .handle_batch_streaming(request)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let response = xstream.map_err(|e| tonic::Status::internal(e.to_string()));

        Ok(tonic::Response::new(Box::pin(response)))
    }

    async fn checkpoint(
        &self,
        request: tonic::Request<CheckpointRequest>,
    ) -> Result<tonic::Response<CheckpointResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_checkpoint_request(&request)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        return Ok(tonic::Response::new(response));
    }

    async fn committee_info(
        &self,
        request: tonic::Request<CommitteeInfoRequest>,
    ) -> Result<tonic::Response<CommitteeInfoResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_committee_info_request(&request)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        return Ok(tonic::Response::new(response));
    }
}
