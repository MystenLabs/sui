// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_maker::BatchMaker, helper::Helper, primary_connector::PrimaryConnector,
    processor::Processor, quorum_waiter::QuorumWaiter, synchronizer::Synchronizer,
};
use async_trait::async_trait;
use bytes::Bytes;
use config::{Committee, Parameters, WorkerId};
use crypto::traits::VerifyingKey;
use futures::{Stream, StreamExt};
use primary::{PrimaryWorkerMessage, WorkerPrimaryMessage};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
};
use store::Store;
use tokio::sync::mpsc::{channel, Sender};
use tonic::{Request, Response, Status};
use tracing::info;
use types::{
    BatchDigest, BincodeEncodedPayload, ClientBatchRequest, Empty, PrimaryToWorker,
    PrimaryToWorkerServer, Transaction, TransactionProto, Transactions, TransactionsServer,
    WorkerToWorker, WorkerToWorkerServer,
};

#[cfg(test)]
#[path = "tests/worker_tests.rs"]
pub mod worker_tests;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The primary round number.
// TODO: Move to the primary.
pub type Round = u64;

/// Indicates a serialized `WorkerMessage::Batch` message.
pub type SerializedBatchMessage = Vec<u8>;

pub use types::WorkerMessage;

pub struct Worker<PublicKey: VerifyingKey> {
    /// The public key of this authority.
    name: PublicKey,
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The configuration parameters.
    parameters: Parameters,
    /// The persistent storage.
    store: Store<BatchDigest, SerializedBatchMessage>,
}

const INADDR_ANY: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

impl<PublicKey: VerifyingKey> Worker<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        id: WorkerId,
        committee: Committee<PublicKey>,
        parameters: Parameters,
        store: Store<BatchDigest, SerializedBatchMessage>,
    ) {
        // Define a worker instance.
        let worker = Self {
            name,
            id,
            committee,
            parameters,
            store,
        };

        // Spawn all worker tasks.
        let (tx_primary, rx_primary) = channel(CHANNEL_CAPACITY);
        worker.handle_primary_messages(tx_primary.clone());
        worker.handle_clients_transactions(tx_primary.clone());
        worker.handle_workers_messages(tx_primary);

        // The `PrimaryConnector` allows the worker to send messages to its primary.
        PrimaryConnector::spawn(
            worker
                .committee
                .primary(&worker.name)
                .expect("Our public key is not in the committee")
                .worker_to_primary,
            rx_primary,
        );

        // NOTE: This log entry is used to compute performance.
        info!(
            "Worker {} successfully booted on {}",
            id,
            worker
                .committee
                .worker(&worker.name, &worker.id)
                .expect("Our public key or worker id is not in the committee")
                .transactions
                .ip()
        );
    }

    /// Spawn all tasks responsible to handle messages from our primary.
    fn handle_primary_messages(&self, tx_primary: Sender<WorkerPrimaryMessage>) {
        let (tx_synchronizer, rx_synchronizer) = channel(CHANNEL_CAPACITY);

        // Receive incoming messages from our primary.
        let mut address = self
            .committee
            .worker(&self.name, &self.id)
            .expect("Our public key or worker id is not in the committee")
            .primary_to_worker;
        address.set_ip(INADDR_ANY);
        PrimaryReceiverHandler { tx_synchronizer }.spawn(address);

        // The `Synchronizer` is responsible to keep the worker in sync with the others. It handles the commands
        // it receives from the primary (which are mainly notifications that we are out of sync).
        Synchronizer::spawn(
            self.name.clone(),
            self.id,
            self.committee.clone(),
            self.store.clone(),
            self.parameters.gc_depth,
            self.parameters.sync_retry_delay,
            self.parameters.sync_retry_nodes,
            /* rx_message */ rx_synchronizer,
            tx_primary,
        );

        info!(
            "Worker {} listening to primary messages on {}",
            self.id, address
        );
    }

    /// Spawn all tasks responsible to handle clients transactions.
    fn handle_clients_transactions(&self, tx_primary: Sender<WorkerPrimaryMessage>) {
        let (tx_batch_maker, rx_batch_maker) = channel(CHANNEL_CAPACITY);
        let (tx_quorum_waiter, rx_quorum_waiter) = channel(CHANNEL_CAPACITY);
        let (tx_processor, rx_processor) = channel(CHANNEL_CAPACITY);

        // We first receive clients' transactions from the network.
        let mut address = self
            .committee
            .worker(&self.name, &self.id)
            .expect("Our public key or worker id is not in the committee")
            .transactions;
        address.set_ip(INADDR_ANY);
        TxReceiverHandler { tx_batch_maker }.spawn(address);

        // The transactions are sent to the `BatchMaker` that assembles them into batches. It then broadcasts
        // (in a reliable manner) the batches to all other workers that share the same `id` as us. Finally, it
        // gathers the 'cancel handlers' of the messages and send them to the `QuorumWaiter`.
        BatchMaker::spawn(
            self.parameters.batch_size,
            self.parameters.max_batch_delay,
            /* rx_transaction */ rx_batch_maker,
            /* tx_message */ tx_quorum_waiter,
            /* workers_addresses */
            self.committee
                .others_workers(&self.name, &self.id)
                .iter()
                .map(|(name, addresses)| (name.clone(), addresses.worker_to_worker))
                .collect(),
        );

        // The `QuorumWaiter` waits for 2f authorities to acknowledge reception of the batch. It then forwards
        // the batch to the `Processor`.
        QuorumWaiter::spawn(
            self.committee.clone(),
            /* stake */ self.committee.stake(&self.name),
            /* rx_message */ rx_quorum_waiter,
            /* tx_batch */ tx_processor,
        );

        // The `Processor` hashes and stores the batch. It then forwards the batch's digest to the `PrimaryConnector`
        // that will send it to our primary machine.
        Processor::spawn(
            self.id,
            self.store.clone(),
            /* rx_batch */ rx_processor,
            /* tx_digest */ tx_primary,
            /* own_batch */ true,
        );

        info!(
            "Worker {} listening to client transactions on {}",
            self.id, address
        );
    }

    /// Spawn all tasks responsible to handle messages from other workers.
    fn handle_workers_messages(&self, tx_primary: Sender<WorkerPrimaryMessage>) {
        let (tx_worker_helper, rx_worker_helper) = channel(CHANNEL_CAPACITY);
        let (tx_client_helper, rx_client_helper) = channel(CHANNEL_CAPACITY);
        let (tx_processor, rx_processor) = channel(CHANNEL_CAPACITY);

        // Receive incoming messages from other workers.
        let mut address = self
            .committee
            .worker(&self.name, &self.id)
            .expect("Our public key or worker id is not in the committee")
            .worker_to_worker;
        address.set_ip(INADDR_ANY);
        WorkerReceiverHandler {
            tx_worker_helper,
            tx_client_helper,
            tx_processor,
        }
        .spawn(address, self.parameters.max_concurrent_requests);

        // The `Helper` is dedicated to reply to batch requests from other workers.
        Helper::spawn(
            self.id,
            self.committee.clone(),
            self.store.clone(),
            /* rx_worker_request */ rx_worker_helper,
            /* rx_client_request */ rx_client_helper,
        );

        // This `Processor` hashes and stores the batches we receive from the other workers. It then forwards the
        // batch's digest to the `PrimaryConnector` that will send it to our primary.
        Processor::spawn(
            self.id,
            self.store.clone(),
            /* rx_batch */ rx_processor,
            /* tx_digest */ tx_primary,
            /* own_batch */ false,
        );

        info!(
            "Worker {} listening to worker messages on {}",
            self.id, address
        );
    }
}

/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler {
    tx_batch_maker: Sender<Transaction>,
}

impl TxReceiverHandler {
    fn spawn(self, address: SocketAddr) {
        let service = tonic::transport::Server::builder()
            .add_service(TransactionsServer::new(self))
            .serve(address);
        tokio::spawn(service);
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
            .expect("Failed to send transaction");

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
struct WorkerReceiverHandler<PublicKey: VerifyingKey> {
    tx_worker_helper: Sender<(Vec<BatchDigest>, PublicKey)>,
    tx_client_helper: Sender<(Vec<BatchDigest>, Sender<SerializedBatchMessage>)>,
    tx_processor: Sender<SerializedBatchMessage>,
}

impl<PublicKey: VerifyingKey> WorkerReceiverHandler<PublicKey> {
    fn spawn(self, address: SocketAddr, max_concurrent_requests: usize) {
        let service = tonic::transport::Server::builder()
            .concurrency_limit_per_connection(max_concurrent_requests)
            .add_service(WorkerToWorkerServer::new(self))
            .serve(address);
        tokio::spawn(service);
    }
}

#[async_trait]
impl<PublicKey: VerifyingKey> WorkerToWorker for WorkerReceiverHandler<PublicKey> {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: WorkerMessage<PublicKey> = request
            .get_ref()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        match message {
            WorkerMessage::Batch(..) => {
                self.tx_processor
                    .send(request.get_ref().payload.to_vec())
                    .await
                    .expect("Failed to send batch");
            }
            WorkerMessage::BatchRequest(missing, requestor) => {
                self.tx_worker_helper
                    .send((missing, requestor))
                    .await
                    .expect("Failed to send batch request");
            }
        }

        Ok(Response::new(Empty {}))
    }

    type ClientBatchRequestStream =
        Pin<Box<dyn Stream<Item = Result<BincodeEncodedPayload, Status>> + Send>>;

    async fn client_batch_request(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Self::ClientBatchRequestStream>, Status> {
        let missing = request
            .into_inner()
            .deserialize::<ClientBatchRequest>()
            .map_err(|e| Status::invalid_argument(e.to_string()))?
            .0;

        // TODO [issue #7]: Do some accounting to prevent bad actors from use all our
        // resources (in this case allocate a gigantic channel).
        let (sender, receiver) = channel(missing.len());

        self.tx_client_helper
            .send((missing, sender))
            .await
            .expect("Failed to send batch request");

        let stream = tokio_stream::wrappers::ReceiverStream::new(receiver).map(|batch| {
            let payload = BincodeEncodedPayload {
                payload: Bytes::from(batch),
            };
            Ok(payload)
        });

        Ok(Response::new(
            Box::pin(stream) as Self::ClientBatchRequestStream
        ))
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct PrimaryReceiverHandler<PublicKey: VerifyingKey> {
    tx_synchronizer: Sender<PrimaryWorkerMessage<PublicKey>>,
}

impl<PublicKey: VerifyingKey> PrimaryReceiverHandler<PublicKey> {
    fn spawn(self, address: SocketAddr) {
        let service = tonic::transport::Server::builder()
            .add_service(PrimaryToWorkerServer::new(self))
            .serve(address);
        tokio::spawn(service);
    }
}

#[async_trait]
impl<PublicKey: VerifyingKey> PrimaryToWorker for PrimaryReceiverHandler<PublicKey> {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: PrimaryWorkerMessage<PublicKey> = request
            .into_inner()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        self.tx_synchronizer
            .send(message)
            .await
            .expect("Failed to send transaction");

        Ok(Response::new(Empty {}))
    }
}
