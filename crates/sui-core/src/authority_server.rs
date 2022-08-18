// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState,
    consensus_adapter::{
        CheckpointConsensusAdapter, CheckpointSender, ConsensusAdapter, ConsensusAdapterMetrics,
        ConsensusListener, ConsensusListenerMessage,
    },
};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::BoxStream, TryStreamExt};
use multiaddr::Multiaddr;
use narwhal_crypto::traits::KeyPair;
use prometheus::Registry;
use std::{io, sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_network::{
    api::{Validator, ValidatorServer},
    tonic,
};

use sui_types::{error::*, messages::*};
use tokio::{
    sync::mpsc::{channel, Sender},
    task::JoinHandle,
};

use sui_types::messages_checkpoint::CheckpointRequest;
use sui_types::messages_checkpoint::CheckpointResponse;

use tracing::{info, Instrument};

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

const MIN_BATCH_SIZE: u64 = 1000;
const MAX_DELAY_MILLIS: u64 = 5_000; // 5 sec

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
    pub fn new(
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
            /* max_delay */ Duration::from_millis(20_000),
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

    pub async fn spawn(self) -> Result<AuthorityServerHandle, io::Error> {
        let address = self.address.clone();
        self.spawn_with_bind_address(address).await
    }

    pub async fn spawn_with_bind_address(
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

pub struct ValidatorService {
    state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
    _checkpoint_consensus_handle: Option<JoinHandle<()>>,
}

impl ValidatorService {
    /// Spawn all the subsystems run by a Sui authority: a consensus node, a sui authority server,
    /// and a consensus listener bridging the consensus node and the sui authority.
    pub async fn new(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        prometheus_registry: &Registry,
    ) -> Result<Self> {
        let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1_000);
        let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1_000);

        // Spawn the consensus node of this authority.
        let consensus_config = config
            .consensus_config()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;
        let consensus_keypair = config.key_pair().copy();
        let consensus_name = consensus_keypair.public().clone();
        let consensus_store = narwhal_node::NodeStorage::reopen(consensus_config.db_path());
        narwhal_node::Node::spawn_primary(
            consensus_keypair,
            config.genesis()?.narwhal_committee(),
            &consensus_store,
            consensus_config.narwhal_config().to_owned(),
            /* consensus */ true, // Indicate that we want to run consensus.
            /* execution_state */ state.clone(),
            /* tx_confirmation */ tx_consensus_to_sui,
            prometheus_registry,
        )
        .await?;
        narwhal_node::Node::spawn_workers(
            consensus_name,
            /* ids */ vec![0], // We run a single worker with id '0'.
            config.genesis()?.narwhal_committee(),
            &consensus_store,
            consensus_config.narwhal_config().to_owned(),
            prometheus_registry,
        );

        // Spawn a consensus listener. It listen for consensus outputs and notifies the
        // authority server when a sequenced transaction is ready for execution.
        ConsensusListener::spawn(
            rx_sui_to_consensus,
            rx_consensus_to_sui,
            /* max_pending_transactions */ 1_000_000,
        );

        let metrics = ConsensusAdapterMetrics::new(prometheus_registry);

        // The consensus adapter allows the authority to send user certificates through consensus.
        let consensus_adapter = ConsensusAdapter::new(
            consensus_config.address().to_owned(),
            state.clone_committee(),
            tx_sui_to_consensus.clone(),
            /* max_delay */ Duration::from_millis(5_000),
            metrics.clone(),
        );

        // Update the checkpoint store with a consensus client.
        let checkpoint_consensus_handle = if let Some(checkpoint_store) = state.checkpoints() {
            let (tx_checkpoint_consensus_adapter, rx_checkpoint_consensus_adapter) = channel(1_000);
            let consensus_sender = CheckpointSender::new(tx_checkpoint_consensus_adapter);
            checkpoint_store
                .lock()
                .set_consensus(Box::new(consensus_sender))?;

            let handle = CheckpointConsensusAdapter::new(
                /* consensus_address */ consensus_config.address().to_owned(),
                /* tx_consensus_listener */ tx_sui_to_consensus,
                rx_checkpoint_consensus_adapter,
                /* checkpoint_locals */ checkpoint_store,
                /* retry_delay */ Duration::from_millis(5_000),
                /* max_pending_transactions */ 10_000,
                metrics,
            )
            .spawn();
            Some(handle)
        } else {
            None
        };

        Ok(Self {
            state,
            consensus_adapter: Arc::new(consensus_adapter),
            _checkpoint_consensus_handle: checkpoint_consensus_handle,
        })
    }

    async fn handle_transaction(
        state: Arc<AuthorityState>,
        request: tonic::Request<Transaction>,
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let transaction = request.into_inner();

        transaction
            .verify_user_sig()
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        //TODO This is really really bad, we should have different types for signature-verified transactions
        // transaction.is_verified = true;

        let tx_digest = transaction.digest();

        // Enable Trace Propagation across spans/processes using tx_digest
        let span = tracing::debug_span!(
            "process_tx",
            ?tx_digest,
            tx_kind = transaction.data().data.kind_as_str()
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
    ) -> Result<tonic::Response<TransactionInfoResponse>, tonic::Status> {
        let mut certificate = request.into_inner();
        // 1) Verify certificate
        certificate
            .verify_mut(&state.committee.load())
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;

        //TODO This is really really bad, we should have different types for signature verified transactions

        // 2) Check idempotency
        let digest = certificate.digest();
        if let Some(response) = state
            .check_tx_already_executed(digest)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
        {
            return Ok(tonic::Response::new(response));
        }

        // 3) If it's a shared object transaction and requires consensus, we need to do so.
        // This will wait until either timeout or we have heard back from consensus.
        if certificate.contains_shared_object()
            && !state
                .transaction_shared_locks_exist(&certificate)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
        {
            consensus_adapter
                .submit(&ConsensusTransaction::UserTransaction(Box::new(
                    certificate.clone(),
                )))
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        // 4) Execute the certificate.
        let tx_digest = certificate.digest();
        let span = tracing::debug_span!(
            "execute_transaction",
            ?tx_digest,
            tx_kind = certificate.data().data.kind_as_str()
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
        tokio::spawn(async move { Self::handle_transaction(state, request).await })
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
        tokio::spawn(
            async move { Self::handle_certificate(state, consensus_adapter, request).await },
        )
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

    type BatchInfoStream = BoxStream<'static, Result<BatchInfoResponseItem, tonic::Status>>;

    async fn batch_info(
        &self,
        request: tonic::Request<BatchInfoRequest>,
    ) -> Result<tonic::Response<Self::BatchInfoStream>, tonic::Status> {
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

    async fn epoch_info(
        &self,
        request: tonic::Request<EpochRequest>,
    ) -> Result<tonic::Response<EpochResponse>, tonic::Status> {
        let request = request.into_inner();

        let response = self
            .state
            .handle_epoch_request(&request)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        return Ok(tonic::Response::new(response));
    }
}
