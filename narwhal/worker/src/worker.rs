// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_maker::BatchMaker, helper::Helper, metrics::WorkerChannelMetrics,
    primary_connector::PrimaryConnector, processor::Processor, quorum_waiter::QuorumWaiter,
    synchronizer::Synchronizer,
};
use anemo::{types::PeerInfo, PeerId};
use async_trait::async_trait;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{traits::KeyPair as _, NetworkKeyPair, PublicKey};
use futures::StreamExt;
use multiaddr::{Multiaddr, Protocol};
use network::P2pNetwork;
use primary::PrimaryWorkerMessage;
use std::{net::Ipv4Addr, sync::Arc};
use store::Store;
use tokio::{sync::watch, task::JoinHandle};
use tonic::{Request, Response, Status};
use tracing::info;
use types::{
    error::DagError,
    metered_channel::{channel, Sender},
    Batch, BatchDigest, BincodeEncodedPayload, Empty, PrimaryToWorker, PrimaryToWorkerServer,
    ReconfigureNotification, Transaction, TransactionProto, Transactions, TransactionsServer,
    WorkerPrimaryMessage, WorkerToWorker, WorkerToWorkerServer,
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
            parameters,
            store,
        };

        let node_metrics = Arc::new(metrics.worker_metrics.unwrap());
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let channel_metrics: Arc<WorkerChannelMetrics> = Arc::new(metrics.channel_metrics.unwrap());

        // Spawn all worker tasks.
        let (tx_primary, rx_primary) = channel(CHANNEL_CAPACITY, &channel_metrics.tx_primary);

        let initial_committee = (*(*(*committee).load()).clone()).clone();
        let (tx_reconfigure, rx_reconfigure) =
            watch::channel(ReconfigureNotification::NewEpoch(initial_committee));

        let (tx_worker_helper, rx_worker_helper) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_worker_helper);
        let (tx_worker_processor, rx_worker_processor) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_worker_processor);

        let worker_service = WorkerToWorkerServer::new(WorkerReceiverHandler {
            tx_worker_helper,
            tx_processor: tx_worker_processor,
        });

        // Receive incoming messages from other workers.
        let address = worker
            .worker_cache
            .load()
            .worker(&primary_name, &id)
            .expect("Our public key or worker id is not in the worker cache")
            .worker_to_worker;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let addr = network::multiaddr_to_address(&address).unwrap();

        // Set up anemo Network.
        let routes = anemo::Router::new().add_rpc_service(worker_service);
        let network = anemo::Network::bind(addr)
            .server_name("narwhal")
            .private_key(worker.keypair.copy().private().0.to_bytes())
            .start(routes)
            .unwrap();

        info!("Worker {} listening to worker messages on {}", id, address);

        // Add other workers we want to talk with to the known peers set.
        for (_primary_pubkey, worker_info) in worker
            .worker_cache
            .load()
            .others_workers(&primary_name, &id)
        {
            let peer_id = PeerId(worker_info.name.0.to_bytes());
            let address = network::multiaddr_to_address(&worker_info.worker_to_worker).unwrap();
            let peer_info = PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: vec![address],
            };
            network.known_peers().insert(peer_info);
        }

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
            peer_id: anemo::PeerId(primary_network_key.0.to_bytes()),
            affinity: anemo::types::PeerAffinity::High,
            address: vec![primary_address],
        });
        let handle = PrimaryConnector::spawn(
            primary_network_key,
            rx_reconfigure,
            rx_primary,
            network::WorkerToPrimaryNetwork::new(network.clone()),
        );

        let client_flow_handles = worker.handle_clients_transactions(
            &tx_reconfigure,
            tx_primary.clone(),
            node_metrics.clone(),
            channel_metrics.clone(),
            endpoint_metrics,
            network.clone(),
        );
        let worker_flow_handles = worker.handle_workers_messages(
            &tx_reconfigure,
            tx_primary.clone(),
            rx_worker_helper,
            rx_worker_processor,
            network.clone(),
        );
        let primary_flow_handles = worker.handle_primary_messages(
            tx_reconfigure,
            tx_primary,
            node_metrics,
            channel_metrics,
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

        let mut handles = vec![handle];
        handles.extend(primary_flow_handles);
        handles.extend(client_flow_handles);
        handles.extend(worker_flow_handles);
        handles
    }

    /// Spawn all tasks responsible to handle messages from our primary.
    fn handle_primary_messages(
        &self,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_primary: Sender<WorkerPrimaryMessage>,
        node_metrics: Arc<WorkerMetrics>,
        channel_metrics: Arc<WorkerChannelMetrics>,
        network: anemo::Network,
    ) -> Vec<JoinHandle<()>> {
        let (tx_synchronizer, rx_synchronizer) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_synchronizer);

        // Receive incoming messages from our primary.
        let address = self
            .worker_cache
            .load()
            .worker(&self.primary_name, &self.id)
            .expect("Our public key or worker id is not in the worker cache")
            .primary_to_worker;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let primary_handle = PrimaryReceiverHandler { tx_synchronizer }
            .spawn(address.clone(), tx_reconfigure.subscribe());

        // The `Synchronizer` is responsible to keep the worker in sync with the others. It handles the commands
        // it receives from the primary (which are mainly notifications that we are out of sync).
        let handle = Synchronizer::spawn(
            self.primary_name.clone(),
            self.id,
            self.committee.clone(),
            self.worker_cache.clone(),
            self.store.clone(),
            self.parameters.gc_depth,
            self.parameters.sync_retry_delay,
            self.parameters.sync_retry_nodes,
            /* rx_message */ rx_synchronizer,
            tx_reconfigure,
            tx_primary,
            node_metrics,
            P2pNetwork::new(network),
        );

        info!(
            "Worker {} listening to primary messages on {}",
            self.id, address
        );

        vec![handle, primary_handle]
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
        let (tx_batch_maker, rx_batch_maker) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_batch_maker);
        let (tx_quorum_waiter, rx_quorum_waiter) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_quorum_waiter);
        let (tx_client_processor, rx_client_processor) =
            channel(CHANNEL_CAPACITY, &channel_metrics.tx_client_processor);

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
        rx_worker_request: types::metered_channel::Receiver<(Vec<BatchDigest>, PublicKey)>,
        rx_worker_processor: types::metered_channel::Receiver<Batch>,
        network: anemo::Network,
    ) -> Vec<JoinHandle<()>> {
        // The `Helper` is dedicated to reply to batch requests from other workers.
        let helper_handle = Helper::spawn(
            self.id,
            (*(*(*self.committee).load()).clone()).clone(),
            self.worker_cache.clone(),
            self.store.clone(),
            tx_reconfigure.subscribe(),
            rx_worker_request,
            P2pNetwork::new(network),
        );

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

        vec![helper_handle, processor_handle]
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
        request: tonic::Request<tonic::Streaming<types::TransactionProto>>,
    ) -> Result<tonic::Response<types::Empty>, tonic::Status> {
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

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
struct WorkerReceiverHandler {
    tx_worker_helper: Sender<(Vec<BatchDigest>, PublicKey)>,
    tx_processor: Sender<Batch>,
}

#[async_trait]
impl WorkerToWorker for WorkerReceiverHandler {
    async fn send_message(
        &self,
        request: anemo::Request<types::WorkerMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        match message {
            WorkerMessage::Batch(batch) => self
                .tx_processor
                .send(batch)
                .await
                .map_err(|_| DagError::ShuttingDown),

            WorkerMessage::BatchRequest(missing, requestor) => self
                .tx_worker_helper
                .send((missing, requestor))
                .await
                .map_err(|_| DagError::ShuttingDown),
        }
        .map(|_| anemo::Response::new(()))
        .map_err(|e| anemo::rpc::Status::internal(e.to_string()))
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct PrimaryReceiverHandler {
    tx_synchronizer: Sender<PrimaryWorkerMessage>,
}

impl PrimaryReceiverHandler {
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
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            tokio::select! {
                _result = mysten_network::config::Config::new()
                    .server_builder()
                    .add_service(PrimaryToWorkerServer::new(self))
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
impl PrimaryToWorker for PrimaryReceiverHandler {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: PrimaryWorkerMessage = request
            .into_inner()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        self.tx_synchronizer
            .send(message)
            .await
            .map_err(|_| DagError::ShuttingDown)
            .map_err(|e| Status::not_found(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }
}
