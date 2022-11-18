// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_maker::BatchMaker,
    handlers::{PrimaryReceiverHandler, WorkerReceiverHandler},
    metrics::WorkerChannelMetrics,
    primary_connector::PrimaryConnector,
    quorum_waiter::QuorumWaiter,
    TransactionValidator,
};
use anemo::types::Address;
use anemo::{types::PeerInfo, Network, PeerId};
use anemo_tower::{
    auth::{AllowedPeers, RequireAuthorizationLayer},
    callback::CallbackLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};
use async_trait::async_trait;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{traits::KeyPair as _, NetworkKeyPair, NetworkPublicKey, PublicKey};
use futures::StreamExt;
use multiaddr::{Multiaddr, Protocol};
use network::metrics::MetricsMakeCallbackHandler;
use network::P2pNetwork;
use std::collections::HashMap;
use std::{net::Ipv4Addr, sync::Arc};
use store::Store;
use sui_metrics::spawn_monitored_task;
use tokio::{sync::watch, task::JoinHandle};
use tonic::{Request, Response, Status};
use tower::ServiceBuilder;
use tracing::info;
use types::{
    error::DagError,
    metered_channel::{channel_with_total, Sender},
    Batch, BatchDigest, Empty, PrimaryToWorkerServer, ReconfigureNotification, Transaction,
    TransactionProto, Transactions, TransactionsServer, TxResponse, WorkerOurBatchMessage,
    WorkerToWorkerServer,
};

#[cfg(test)]
#[path = "tests/worker_tests.rs"]
pub mod worker_tests;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The maximum allowed size of transactions into Narwhal.
pub const MAX_ALLOWED_TRANSACTION_SIZE: usize = 6 * 1024 * 1024;

use crate::metrics::{Metrics, WorkerEndpointMetrics, WorkerMetrics};

pub struct Worker {
    /// The public key of this authority.
    primary_name: PublicKey,
    // The private-public key pair of this worker.
    keypair: NetworkKeyPair,
    /// The id of this worker used for index-based lookup by other NW nodes.
    id: WorkerId,
    /// The committee information.
    committee: SharedCommittee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// The configuration parameters
    parameters: Parameters,
    /// The persistent storage.
    store: Store<BatchDigest, Batch>,
}

impl Worker {
    pub fn spawn(
        primary_name: PublicKey,
        keypair: NetworkKeyPair,
        id: WorkerId,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        parameters: Parameters,
        validator: impl TransactionValidator,
        store: Store<BatchDigest, Batch>,
        metrics: Metrics,
    ) -> Vec<JoinHandle<()>> {
        info!(
            "Boot worker node with id {} peer id {}",
            id,
            PeerId(keypair.public().0.to_bytes())
        );

        // Define a worker instance.
        let worker = Self {
            primary_name: primary_name.clone(),
            keypair,
            id,
            committee: committee.clone(),
            worker_cache,
            parameters: parameters.clone(),
            store,
        };

        let node_metrics = Arc::new(metrics.worker_metrics.unwrap());
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let channel_metrics: Arc<WorkerChannelMetrics> = Arc::new(metrics.channel_metrics.unwrap());
        let inbound_network_metrics = Arc::new(metrics.inbound_network_metrics.unwrap());
        let outbound_network_metrics = Arc::new(metrics.outbound_network_metrics.unwrap());
        let network_connection_metrics = metrics.network_connection_metrics.unwrap();

        // Spawn all worker tasks.
        let (tx_our_batch, rx_our_batch) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_our_batch,
            &channel_metrics.tx_our_batch_total,
        );
        let (tx_others_batch, rx_others_batch) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_others_batch,
            &channel_metrics.tx_others_batch_total,
        );

        let initial_committee = (*(*(*committee).load()).clone()).clone();
        let (tx_reconfigure, rx_reconfigure) =
            watch::channel(ReconfigureNotification::NewEpoch(initial_committee));

        let worker_service = WorkerToWorkerServer::new(WorkerReceiverHandler {
            id: worker.id,
            tx_others_batch,
            store: worker.store.clone(),
            validator: validator.clone(),
        });
        let primary_service = PrimaryToWorkerServer::new(PrimaryReceiverHandler {
            name: worker.primary_name.clone(),
            id: worker.id,
            committee: worker.committee.clone(),
            worker_cache: worker.worker_cache.clone(),
            store: worker.store.clone(),
            request_batch_timeout: worker.parameters.sync_retry_delay,
            request_batch_retry_nodes: worker.parameters.sync_retry_nodes,
            tx_reconfigure,
            validator: validator.clone(),
        });

        // Receive incoming messages from other workers.
        let address = worker
            .worker_cache
            .load()
            .worker(&primary_name, &id)
            .expect("Our public key or worker id is not in the worker cache")
            .worker_address;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let addr = network::multiaddr_to_address(&address).unwrap();

        // Set up anemo Network.
        let our_primary_peer_id = committee
            .load()
            .network_key(&primary_name)
            .map(|public_key| PeerId(public_key.0.to_bytes()))
            .unwrap();
        let primary_to_worker_router = anemo::Router::new()
            .add_rpc_service(primary_service)
            // Add an Authorization Layer to ensure that we only service requests from our primary
            .route_layer(RequireAuthorizationLayer::new(AllowedPeers::new([
                our_primary_peer_id,
            ])));
        let routes = anemo::Router::new()
            .add_rpc_service(worker_service)
            .merge(primary_to_worker_router);

        let service = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                inbound_network_metrics,
            )))
            .service(routes);

        let outbound_layer = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_client_and_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                outbound_network_metrics,
            )))
            .into_inner();

        let anemo_config = {
            let mut quic_config = anemo::QuicConfig::default();
            // Enable keep alives every 5s
            quic_config.keep_alive_interval_ms = Some(5_000);
            let mut config = anemo::Config::default();
            config.quic = Some(quic_config);
            // Set a default timeout of 30s for all outbound RPC requests
            config.outbound_request_timeout_ms = Some(30_000);
            config
        };

        let network = Network::bind(addr)
            .server_name("narwhal")
            .private_key(worker.keypair.copy().private().0.to_bytes())
            .config(anemo_config)
            .outbound_request_layer(outbound_layer)
            .start(service)
            .unwrap();

        info!("Worker {} listening to worker messages on {}", id, address);

        let mut peer_types = HashMap::new();

        let other_workers = worker
            .worker_cache
            .load()
            .others_workers_by_id(&primary_name, &id)
            .into_iter()
            .map(|(_, info)| (info.name, info.worker_address));

        // Add other workers we want to talk with to the known peers set.
        for (public_key, address) in other_workers {
            let (peer_id, address) = Self::add_peer_in_network(&network, public_key, &address);
            peer_types.insert(peer_id, "other_worker".to_string());
            info!(
                "Adding others workers with peer id {} and address {}",
                peer_id, address
            );
        }

        // Connect worker to its corresponding primary.
        let primary_address = committee
            .load()
            .primary(&primary_name)
            .expect("Our primary is not in the committee");

        let primary_network_key = committee
            .load()
            .network_key(&primary_name)
            .expect("Our primary is not in the committee");

        let (peer_id, address) =
            Self::add_peer_in_network(&network, primary_network_key.clone(), &primary_address);
        peer_types.insert(peer_id, "our_primary".to_string());
        info!(
            "Adding our primary with peer id {} and address {}",
            peer_id, address
        );

        // update the peer_types with the "other_primary". We do not add them in the Network
        // struct, otherwise the networking library will try to connect to it
        let other_primaries: Vec<(PublicKey, Multiaddr, NetworkPublicKey)> =
            committee.load().others_primaries(&primary_name);
        for (_, _, network_key) in other_primaries {
            peer_types.insert(
                PeerId(network_key.0.to_bytes()),
                "other_primary".to_string(),
            );
        }

        let connection_monitor_handle = network::connectivity::ConnectionMonitor::spawn(
            network.downgrade(),
            network_connection_metrics,
            peer_types,
        );

        let network_admin_server_base_port = parameters
            .network_admin_server
            .worker_network_admin_server_base_port
            .checked_add(id as u16)
            .unwrap();
        info!(
            "Worker {} listening to network admin messages on 127.0.0.1:{}",
            id, network_admin_server_base_port
        );

        let admin_handles = network::admin::start_admin_server(
            network_admin_server_base_port,
            network.clone(),
            rx_reconfigure.clone(),
            None,
        );

        let primary_connector_handle = PrimaryConnector::spawn(
            primary_network_key,
            rx_reconfigure.clone(),
            rx_our_batch,
            rx_others_batch,
            P2pNetwork::new(network.clone()),
        );
        let client_flow_handles = worker.handle_clients_transactions(
            rx_reconfigure,
            tx_our_batch,
            node_metrics,
            channel_metrics,
            endpoint_metrics,
            validator,
            network,
        );

        // NOTE: This log entry is used to compute performance.
        info!(
            "Worker {} successfully booted on {}",
            id,
            worker
                .worker_cache
                .load()
                .worker(&worker.primary_name, &worker.id)
                .expect("Our public key or worker id is not in the worker cache")
                .transactions
        );

        let mut handles = vec![primary_connector_handle, connection_monitor_handle];
        handles.extend(admin_handles);
        handles.extend(client_flow_handles);
        handles
    }

    fn add_peer_in_network(
        network: &Network,
        peer_name: NetworkPublicKey,
        address: &Multiaddr,
    ) -> (PeerId, Address) {
        let peer_id = PeerId(peer_name.0.to_bytes());
        let address = network::multiaddr_to_address(address).unwrap();
        let peer_info = PeerInfo {
            peer_id,
            affinity: anemo::types::PeerAffinity::High,
            address: vec![address.clone()],
        };
        network.known_peers().insert(peer_info);

        (peer_id, address)
    }

    /// Spawn all tasks responsible to handle clients transactions.
    fn handle_clients_transactions(
        &self,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        tx_our_batch: Sender<(
            WorkerOurBatchMessage,
            Option<tokio::sync::oneshot::Sender<()>>,
        )>,
        node_metrics: Arc<WorkerMetrics>,
        channel_metrics: Arc<WorkerChannelMetrics>,
        endpoint_metrics: WorkerEndpointMetrics,
        validator: impl TransactionValidator,
        network: anemo::Network,
    ) -> Vec<JoinHandle<()>> {
        let (tx_batch_maker, rx_batch_maker) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_batch_maker,
            &channel_metrics.tx_batch_maker_total,
        );
        let (tx_quorum_waiter, rx_quorum_waiter) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_quorum_waiter,
            &channel_metrics.tx_quorum_waiter_total,
        );

        // We first receive clients' transactions from the network.
        let address = self
            .worker_cache
            .load()
            .worker(&self.primary_name, &self.id)
            .expect("Our public key or worker id is not in the worker cache")
            .transactions;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let tx_receiver_handle = TxReceiverHandler {
            tx_batch_maker,
            validator,
        }
        .spawn(address.clone(), rx_reconfigure.clone(), endpoint_metrics);

        // The transactions are sent to the `BatchMaker` that assembles them into batches. It then broadcasts
        // (in a reliable manner) the batches to all other workers that share the same `id` as us. Finally, it
        // gathers the 'cancel handlers' of the messages and send them to the `QuorumWaiter`.
        let batch_maker_handle = BatchMaker::spawn(
            self.id,
            (*(*(*self.committee).load()).clone()).clone(),
            self.parameters.batch_size,
            self.parameters.max_batch_delay,
            rx_reconfigure.clone(),
            rx_batch_maker,
            tx_quorum_waiter,
            node_metrics,
            self.store.clone(),
            tx_our_batch,
        );

        // The `QuorumWaiter` waits for 2f authorities to acknowledge reception of the batch. It then forwards
        // the batch to the `Processor`.
        let quorum_waiter_handle = QuorumWaiter::spawn(
            self.primary_name.clone(),
            self.id,
            (*(*(*self.committee).load()).clone()).clone(),
            self.worker_cache.clone(),
            rx_reconfigure,
            /* rx_message */ rx_quorum_waiter,
            P2pNetwork::new(network),
        );

        info!(
            "Worker {} listening to client transactions on {}",
            self.id, address
        );

        vec![batch_maker_handle, quorum_waiter_handle, tx_receiver_handle]
    }
}

/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler<V> {
    tx_batch_maker: Sender<(Transaction, TxResponse)>,
    validator: V,
}

impl<V: TransactionValidator> TxReceiverHandler<V> {
    async fn wait_for_shutdown(mut rx_reconfigure: watch::Receiver<ReconfigureNotification>) {
        loop {
            let result = rx_reconfigure.changed().await;
            result.expect("Committee channel dropped");
            let message = rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::Shutdown = message {
                break;
            }
        }
    }

    #[must_use]
    fn spawn(
        self,
        address: Multiaddr,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        endpoint_metrics: WorkerEndpointMetrics,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(async move {
            tokio::select! {
                _result =  mysten_network::config::Config::new()
                    .server_builder_with_metrics(endpoint_metrics)
                    .add_service(TransactionsServer::new(self))
                    .bind(&address)
                    .await
                    .unwrap()
                    .serve() => (),

                () = Self::wait_for_shutdown(rx_reconfigure) => ()
            }
        })
    }
}

#[async_trait]
impl<V: TransactionValidator> Transactions for TxReceiverHandler<V> {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<Empty>, Status> {
        let message = request.into_inner().transaction;
        if message.len() > MAX_ALLOWED_TRANSACTION_SIZE {
            return Err(Status::resource_exhausted(format!(
                "Transaction size is too large: {} > {}",
                message.len(),
                MAX_ALLOWED_TRANSACTION_SIZE
            )));
        }
        if self.validator.validate(message.as_ref()).is_err() {
            return Err(Status::invalid_argument("Invalid transaction"));
        }
        // Send the transaction to the batch maker.
        let (notifier, when_done) = tokio::sync::oneshot::channel();
        self.tx_batch_maker
            .send((message.to_vec(), notifier))
            .await
            .map_err(|_| DagError::ShuttingDown)
            .map_err(|e| Status::not_found(e.to_string()))?;

        // TODO: distingush between a digest being returned vs the channel closing
        // suggesting an error.
        let _digest = when_done.await;

        Ok(Response::new(Empty {}))
    }

    async fn submit_transaction_stream(
        &self,
        request: Request<tonic::Streaming<types::TransactionProto>>,
    ) -> Result<Response<types::Empty>, Status> {
        let mut transactions = request.into_inner();
        let mut responses = Vec::new();

        while let Some(Ok(txn)) = transactions.next().await {
            if let Err(err) = self.validator.validate(txn.transaction.as_ref()) {
                // If the transaction is invalid (often cryptographically), better to drop the client
                return Err(Status::invalid_argument(format!(
                    "Stream contains an invalid transaction {err}"
                )));
            }
            // Send the transaction to the batch maker.
            let (notifier, when_done) = tokio::sync::oneshot::channel();
            self.tx_batch_maker
                .send((txn.transaction.to_vec(), notifier))
                .await
                .expect("Failed to send transaction");

            // Note that here we do not wait for a response because this would
            // mean that we process only a single message from this stream at a
            // time. Instead we gather them and resolve them once the stream is over.
            responses.push(when_done);
        }

        // TODO: activate when we provide a meaningful guarantee, and
        // distingush between a digest being returned vs the channel closing
        // suggesting an error.
        // for response in responses {
        //     let _digest = response.await;
        // }

        Ok(Response::new(Empty {}))
    }
}
