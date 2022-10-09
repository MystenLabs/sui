// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_maker::BatchMaker,
    handlers::{PrimaryReceiverHandler, WorkerReceiverHandler},
    metrics::WorkerChannelMetrics,
    primary_connector::PrimaryConnector,
    processor::Processor,
    quorum_waiter::QuorumWaiter,
    synchronizer::Synchronizer,
};
use anemo::{types::PeerInfo, PeerId};
use anemo_tower::{callback::CallbackLayer, trace::TraceLayer};
use async_trait::async_trait;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{traits::KeyPair as _, NetworkKeyPair, PublicKey};
use futures::StreamExt;
use multiaddr::{Multiaddr, Protocol};
use network::metrics::MetricsMakeCallbackHandler;
use network::P2pNetwork;
use primary::PrimaryWorkerMessage;
use std::{net::Ipv4Addr, sync::Arc};
use store::Store;
use tokio::{sync::watch, task::JoinHandle};
use tonic::{Request, Response, Status};
use tower::ServiceBuilder;
use tracing::info;
use types::{
    error::DagError,
    metered_channel::{channel_with_total, Receiver, Sender},
    Batch, BatchDigest, Empty, PrimaryToWorkerServer, ReconfigureNotification, Transaction,
    TransactionProto, Transactions, TransactionsServer, WorkerPrimaryMessage, WorkerToWorkerServer,
};

#[cfg(test)]
#[path = "tests/worker_tests.rs"]
pub mod worker_tests;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

use crate::metrics::{Metrics, WorkerEndpointMetrics, WorkerMetrics};
pub use types::WorkerMessage;

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
        store: Store<BatchDigest, Batch>,
        metrics: Metrics,
    ) -> Vec<JoinHandle<()>> {
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
        let (tx_primary, rx_primary) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_primary,
            &channel_metrics.tx_primary_total,
        );

        let initial_committee = (*(*(*committee).load()).clone()).clone();
        let (tx_reconfigure, rx_reconfigure) =
            watch::channel(ReconfigureNotification::NewEpoch(initial_committee));

        let (tx_worker_processor, rx_worker_processor) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_worker_processor,
            &channel_metrics.tx_worker_processor_total,
        );
        let (tx_synchronizer, rx_synchronizer) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_synchronizer,
            &channel_metrics.tx_synchronizer_total,
        );

        let worker_service = WorkerToWorkerServer::new(WorkerReceiverHandler {
            tx_processor: tx_worker_processor.clone(),
            store: worker.store.clone(),
        });
        let primary_service = PrimaryToWorkerServer::new(PrimaryReceiverHandler {
            name: worker.primary_name.clone(),
            id: worker.id,
            worker_cache: worker.worker_cache.clone(),
            store: worker.store.clone(),
            request_batches_timeout: worker.parameters.sync_retry_delay,
            request_batches_retry_nodes: worker.parameters.sync_retry_nodes,
            tx_synchronizer,
            tx_primary: tx_primary.clone(),
            tx_batch_processor: tx_worker_processor,
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
        let routes = anemo::Router::new()
            .add_rpc_service(worker_service)
            .add_rpc_service(primary_service);

        let service = ServiceBuilder::new()
            .layer(TraceLayer::new())
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                inbound_network_metrics,
            )))
            .service(routes);

        let outbound_layer = ServiceBuilder::new()
            .layer(TraceLayer::new())
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

        let network = anemo::Network::bind(addr)
            .server_name("narwhal")
            .private_key(worker.keypair.copy().private().0.to_bytes())
            .config(anemo_config)
            .outbound_request_layer(outbound_layer)
            .start(service)
            .unwrap();

        info!("Worker {} listening to worker messages on {}", id, address);

        let connection_monitor_handle = network::connectivity::ConnectionMonitor::spawn(
            network.clone(),
            network_connection_metrics,
            tx_reconfigure.subscribe(),
        );

        let other_workers = worker
            .worker_cache
            .load()
            .others_workers(&primary_name, &id)
            .into_iter()
            .map(|(_, info)| (info.name, info.worker_address));
        let our_primary = std::iter::once((
            committee.load().network_key(&primary_name).unwrap(),
            committee.load().primary(&primary_name).unwrap(),
        ));

        // Add other workers we want to talk with to the known peers set.
        for (public_key, address) in other_workers.chain(our_primary) {
            let peer_id = PeerId(public_key.0.to_bytes());
            let address = network::multiaddr_to_address(&address).unwrap();
            let peer_info = PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: vec![address],
            };
            network.known_peers().insert(peer_info);
        }

        let network_admin_server_base_port = parameters
            .network_admin_server
            .worker_network_admin_server_base_port
            .checked_add(id as u16)
            .unwrap();
        info!(
            "Worker {} listening to network admin messages on 127.0.0.1:{}",
            id, network_admin_server_base_port
        );

        network::admin::start_admin_server(
            network_admin_server_base_port,
            network.clone(),
            tx_reconfigure.subscribe(),
        );

        // Connect worker to its corresponding primary.
        let primary_address = network::multiaddr_to_address(
            &committee
                .load()
                .primary(&primary_name)
                .expect("Our primary is not in the committee"),
        )
        .unwrap();
        let primary_network_key = committee
            .load()
            .network_key(&primary_name)
            .expect("Our primary is not in the committee");
        network.known_peers().insert(PeerInfo {
            peer_id: PeerId(primary_network_key.0.to_bytes()),
            affinity: anemo::types::PeerAffinity::High,
            address: vec![primary_address],
        });
        let primary_connector_handle = PrimaryConnector::spawn(
            primary_network_key,
            rx_reconfigure,
            rx_primary,
            P2pNetwork::new(network.clone()),
        );
        let client_flow_handles = worker.handle_clients_transactions(
            &tx_reconfigure,
            tx_primary.clone(),
            node_metrics,
            channel_metrics,
            endpoint_metrics,
            network.clone(),
        );
        let worker_flow_handles = worker.handle_workers_messages(
            &tx_reconfigure,
            tx_primary.clone(),
            rx_worker_processor,
        );
        let primary_flow_handles =
            worker.handle_primary_messages(rx_synchronizer, tx_reconfigure, tx_primary, network);

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
        handles.extend(primary_flow_handles);
        handles.extend(client_flow_handles);
        handles.extend(worker_flow_handles);
        handles
    }

    /// Spawn all tasks responsible to handle messages from our primary.
    fn handle_primary_messages(
        &self,
        rx_synchronizer: Receiver<PrimaryWorkerMessage>,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_primary: Sender<WorkerPrimaryMessage>,
        network: anemo::Network,
    ) -> Vec<JoinHandle<()>> {
        // The `Synchronizer` is responsible to keep the worker in sync with the others. It handles the commands
        // it receives from the primary (which are mainly notifications that we are out of sync).
        let handle = Synchronizer::spawn(
            self.committee.clone(),
            self.worker_cache.clone(),
            self.store.clone(),
            /* rx_message */ rx_synchronizer,
            tx_reconfigure,
            tx_primary,
            P2pNetwork::new(network),
        );

        vec![handle]
    }

    /// Spawn all tasks responsible to handle clients transactions.
    fn handle_clients_transactions(
        &self,
        tx_reconfigure: &watch::Sender<ReconfigureNotification>,
        tx_primary: Sender<WorkerPrimaryMessage>,
        node_metrics: Arc<WorkerMetrics>,
        channel_metrics: Arc<WorkerChannelMetrics>,
        endpoint_metrics: WorkerEndpointMetrics,
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
        let (tx_client_processor, rx_client_processor) = channel_with_total(
            CHANNEL_CAPACITY,
            &channel_metrics.tx_client_processor,
            &channel_metrics.tx_client_processor_total,
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
        let tx_receiver_handle = TxReceiverHandler { tx_batch_maker }.spawn(
            address.clone(),
            tx_reconfigure.subscribe(),
            endpoint_metrics,
        );

        // The transactions are sent to the `BatchMaker` that assembles them into batches. It then broadcasts
        // (in a reliable manner) the batches to all other workers that share the same `id` as us. Finally, it
        // gathers the 'cancel handlers' of the messages and send them to the `QuorumWaiter`.
        let batch_maker_handle = BatchMaker::spawn(
            (*(*(*self.committee).load()).clone()).clone(),
            self.parameters.batch_size,
            self.parameters.max_batch_delay,
            tx_reconfigure.subscribe(),
            /* rx_transaction */ rx_batch_maker,
            /* tx_message */ tx_quorum_waiter,
            node_metrics,
        );

        // The `QuorumWaiter` waits for 2f authorities to acknowledge reception of the batch. It then forwards
        // the batch to the `Processor`.
        let quorum_waiter_handle = QuorumWaiter::spawn(
            self.primary_name.clone(),
            self.id,
            (*(*(*self.committee).load()).clone()).clone(),
            self.worker_cache.clone(),
            tx_reconfigure.subscribe(),
            /* rx_message */ rx_quorum_waiter,
            /* tx_batch */ tx_client_processor,
            P2pNetwork::new(network),
        );

        // The `Processor` hashes and stores the batch. It then forwards the batch's digest to the `PrimaryConnector`
        // that will send it to our primary machine.
        let processor_handle = Processor::spawn(
            self.id,
            self.store.clone(),
            tx_reconfigure.subscribe(),
            /* rx_batch */ rx_client_processor,
            /* tx_digest */ tx_primary,
            /* own_batch */ true,
        );

        info!(
            "Worker {} listening to client transactions on {}",
            self.id, address
        );

        vec![
            batch_maker_handle,
            quorum_waiter_handle,
            processor_handle,
            tx_receiver_handle,
        ]
    }

    /// Spawn all tasks responsible to handle messages from other workers.
    fn handle_workers_messages(
        &self,
        tx_reconfigure: &watch::Sender<ReconfigureNotification>,
        tx_primary: Sender<WorkerPrimaryMessage>,
        rx_worker_processor: Receiver<Batch>,
    ) -> Vec<JoinHandle<()>> {
        // This `Processor` hashes and stores the batches we receive from the other workers. It then forwards the
        // batch's digest to the `PrimaryConnector` that will send it to our primary.
        let processor_handle = Processor::spawn(
            self.id,
            self.store.clone(),
            tx_reconfigure.subscribe(),
            /* rx_batch */ rx_worker_processor,
            /* tx_digest */ tx_primary,
            /* own_batch */ false,
        );

        vec![processor_handle]
    }
}

/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler {
    tx_batch_maker: Sender<Transaction>,
}

impl TxReceiverHandler {
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
        tokio::spawn(async move {
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
impl Transactions for TxReceiverHandler {
    async fn submit_transaction(
        &self,
        request: Request<TransactionProto>,
    ) -> Result<Response<Empty>, Status> {
        let message = request.into_inner().transaction;
        // Send the transaction to the batch maker.
        self.tx_batch_maker
            .send(message.to_vec())
            .await
            .map_err(|_| DagError::ShuttingDown)
            .map_err(|e| Status::not_found(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn submit_transaction_stream(
        &self,
        request: Request<tonic::Streaming<types::TransactionProto>>,
    ) -> Result<Response<types::Empty>, Status> {
        let mut transactions = request.into_inner();

        while let Some(Ok(txn)) = transactions.next().await {
            // Send the transaction to the batch maker.
            self.tx_batch_maker
                .send(txn.transaction.to_vec())
                .await
                .expect("Failed to send transaction");
        }
        Ok(Response::new(Empty {}))
    }
}
