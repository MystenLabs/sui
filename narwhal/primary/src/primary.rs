// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    async_proposer::AsyncProposer,
    block_remover::DeleteBatchResult,
    block_synchronizer::BlockSynchronizer,
    block_waiter::{BatchMessageError, BatchResult, BlockWaiter},
    certificate_waiter::CertificateWaiter,
    core::Core,
    garbage_collector::GarbageCollector,
    grpc_server::ConsensusAPIGrpc,
    header_waiter::HeaderWaiter,
    helper::Helper,
    part_sync_proposer::PartiallySyncProposer,
    payload_receiver::PayloadReceiver,
    synchronizer::Synchronizer,
    BlockRemover, CertificatesResponse, DeleteBatchMessage, PayloadAvailabilityResponse,
};
use async_trait::async_trait;
use config::{Parameters, SharedCommittee, WorkerId};
use consensus::dag::Dag;
use crypto::{
    traits::{EncodeDecodeBase64, Signer, VerifyingKey},
    SignatureService,
};
use multiaddr::{Multiaddr, Protocol};
use network::{PrimaryNetwork, PrimaryToWorkerNetwork};
use serde::{Deserialize, Serialize};
use std::{
    net::Ipv4Addr,
    sync::{atomic::AtomicU64, Arc},
};
use store::Store;
use thiserror::Error;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tonic::{Request, Response, Status};
use tracing::info;
use types::{
    Batch, BatchDigest, BatchMessage, BincodeEncodedPayload, Certificate, CertificateDigest, Empty,
    Header, HeaderDigest, PrimaryToPrimary, PrimaryToPrimaryServer, WorkerToPrimary,
    WorkerToPrimaryServer,
};

/// The default channel capacity for each channel of the primary.
pub const CHANNEL_CAPACITY: usize = 1_000;

use crate::block_synchronizer::handler::BlockSynchronizerHandler;
pub use types::{PrimaryMessage, PrimaryWorkerMessage};

/// The messages sent by the workers to their primary.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum WorkerPrimaryMessage {
    /// The worker indicates it sealed a new batch.
    OurBatch(BatchDigest, WorkerId),
    /// The worker indicates it received a batch's digest from another authority.
    OthersBatch(BatchDigest, WorkerId),
    /// The worker sends a requested batch
    RequestedBatch(BatchDigest, Batch),
    /// When batches are successfully deleted, this message is sent dictating the
    /// batches that have been deleted from the worker.
    DeletedBatches(Vec<BatchDigest>),
    /// An error has been returned by worker
    Error(WorkerPrimaryError),
}

#[derive(Debug, Serialize, Deserialize, Error, Clone, PartialEq)]
pub enum WorkerPrimaryError {
    #[error("Batch with id {0} has not been found")]
    RequestedBatchNotFound(BatchDigest),

    #[error("An error occurred while deleting batches. None deleted")]
    ErrorWhileDeletingBatches(Vec<BatchDigest>),
}

// A type alias marking the "payload" tokens sent by workers to their primary as batch acknowledgements
pub type PayloadToken = u8;

/// The network model in which the primary operates.
pub enum NetworkModel {
    PartiallySynchronous,
    Asynchronous,
}

pub struct Primary;

impl Primary {
    const INADDR_ANY: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

    pub fn spawn<PublicKey: VerifyingKey, Signatory: Signer<PublicKey::Sig> + Send + 'static>(
        name: PublicKey,
        signer: Signatory,
        committee: SharedCommittee<PublicKey>,
        parameters: Parameters,
        header_store: Store<HeaderDigest, Header<PublicKey>>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        tx_consensus: Sender<Certificate<PublicKey>>,
        rx_consensus: Receiver<Certificate<PublicKey>>,
        dag: Option<Arc<Dag<PublicKey>>>,
        network_model: NetworkModel,
    ) -> JoinHandle<()> {
        let (tx_others_digests, rx_others_digests) = channel(CHANNEL_CAPACITY);
        let (tx_our_digests, rx_our_digests) = channel(CHANNEL_CAPACITY);
        let (tx_parents, rx_parents) = channel(CHANNEL_CAPACITY);
        let (tx_headers, rx_headers) = channel(CHANNEL_CAPACITY);
        let (tx_sync_headers, rx_sync_headers) = channel(CHANNEL_CAPACITY);
        let (tx_sync_certificates, rx_sync_certificates) = channel(CHANNEL_CAPACITY);
        let (tx_headers_loopback, rx_headers_loopback) = channel(CHANNEL_CAPACITY);
        let (tx_certificates_loopback, rx_certificates_loopback) = channel(CHANNEL_CAPACITY);
        let (tx_primary_messages, rx_primary_messages) = channel(CHANNEL_CAPACITY);
        let (tx_helper_requests, rx_helper_requests) = channel(CHANNEL_CAPACITY);
        let (tx_get_block_commands, rx_get_block_commands) = channel(CHANNEL_CAPACITY);
        let (tx_batches, rx_batches) = channel(CHANNEL_CAPACITY);
        let (tx_block_removal_commands, rx_block_removal_commands) = channel(CHANNEL_CAPACITY);
        let (tx_batch_removal, rx_batch_removal) = channel(CHANNEL_CAPACITY);
        let (tx_block_synchronizer_commands, rx_block_synchronizer_commands) =
            channel(CHANNEL_CAPACITY);
        let (tx_certificate_responses, rx_certificate_responses) = channel(CHANNEL_CAPACITY);
        let (tx_payload_availability_responses, rx_payload_availability_responses) =
            channel(CHANNEL_CAPACITY);

        // Write the parameters to the logs.
        parameters.tracing();

        // Atomic variable use to synchronize all tasks with the latest consensus round. This is only
        // used for cleanup. The only task that write into this variable is `GarbageCollector`.
        let consensus_round = Arc::new(AtomicU64::new(0));

        // Spawn the network receiver listening to messages from the other primaries.
        let address = committee
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .primary_to_primary;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Primary::INADDR_ANY)))
            .unwrap();
        PrimaryReceiverHandler {
            tx_primary_messages: tx_primary_messages.clone(),
            tx_helper_requests,
            tx_payload_availability_responses,
            tx_certificate_responses,
        }
        .spawn(address.clone(), parameters.max_concurrent_requests);
        info!(
            "Primary {} listening to primary messages on {}",
            name.encode_base64(),
            address
        );

        // Spawn the network receiver listening to messages from our workers.
        let address = committee
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .worker_to_primary;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Primary::INADDR_ANY)))
            .unwrap();
        WorkerReceiverHandler {
            tx_our_digests,
            tx_others_digests,
            tx_batches,
            tx_batch_removal,
        }
        .spawn(address.clone());
        info!(
            "Primary {} listening to workers messages on {}",
            name.encode_base64(),
            address
        );

        // The `Synchronizer` provides auxiliary methods helping the `Core` to sync.
        let synchronizer = Synchronizer::new(
            name.clone(),
            &committee,
            certificate_store.clone(),
            payload_store.clone(),
            /* tx_header_waiter */ tx_sync_headers,
            /* tx_certificate_waiter */ tx_sync_certificates,
        );

        // The `SignatureService` is used to require signatures on specific digests.
        let signature_service = SignatureService::new(signer);

        // The `Core` receives and handles headers, votes, and certificates from the other primaries.
        let primary_handle = Core::spawn(
            name.clone(),
            committee.clone(),
            header_store.clone(),
            certificate_store.clone(),
            synchronizer,
            signature_service.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            /* rx_primaries */ rx_primary_messages,
            /* rx_header_waiter */ rx_headers_loopback,
            /* rx_certificate_waiter */ rx_certificates_loopback,
            /* rx_proposer */ rx_headers,
            tx_consensus,
            /* tx_proposer */ tx_parents,
        );

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        GarbageCollector::spawn(&name, &committee, consensus_round.clone(), rx_consensus);

        // Receives batch digests from other workers. They are only used to validate headers.
        PayloadReceiver::spawn(
            payload_store.clone(),
            /* rx_workers */ rx_others_digests,
        );

        let block_synchronizer_handler = Arc::new(BlockSynchronizerHandler::new(
            tx_block_synchronizer_commands,
            tx_primary_messages,
            certificate_store.clone(),
            parameters
                .block_synchronizer
                .handler_certificate_deliver_timeout,
        ));

        // Retrieves a block's data by contacting the worker nodes that contain the
        // underlying batches and their transactions.
        BlockWaiter::spawn(
            name.clone(),
            committee.clone(),
            rx_get_block_commands,
            rx_batches,
            block_synchronizer_handler.clone(),
        );

        // Indicator variable for the gRPC server
        let internal_consensus = dag.is_none();

        // Orchestrates the removal of blocks across the primary and worker nodes.
        BlockRemover::spawn(
            name.clone(),
            committee.clone(),
            certificate_store.clone(),
            header_store,
            payload_store.clone(),
            dag.clone(),
            PrimaryToWorkerNetwork::default(),
            rx_block_removal_commands,
            rx_batch_removal,
        );

        // Responsible for finding missing blocks (certificates) and fetching
        // them from the primary peers by synchronizing also their batches.
        BlockSynchronizer::spawn(
            name.clone(),
            committee.clone(),
            rx_block_synchronizer_commands,
            rx_certificate_responses,
            rx_payload_availability_responses,
            PrimaryNetwork::default(),
            payload_store.clone(),
            certificate_store.clone(),
            parameters.block_synchronizer,
        );

        // Whenever the `Synchronizer` does not manage to validate a header due to missing parent certificates of
        // batch digests, it commands the `HeaderWaiter` to synchronize with other nodes, wait for their reply, and
        // re-schedule execution of the header once we have all missing data.
        HeaderWaiter::spawn(
            name.clone(),
            committee.clone(),
            certificate_store.clone(),
            payload_store.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            parameters.sync_retry_delay,
            parameters.sync_retry_nodes,
            /* rx_synchronizer */ rx_sync_headers,
            /* tx_core */ tx_headers_loopback,
        );

        // The `CertificateWaiter` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        CertificateWaiter::spawn(
            certificate_store.clone(),
            consensus_round,
            parameters.gc_depth,
            /* rx_synchronizer */ rx_sync_certificates,
            /* tx_core */ tx_certificates_loopback,
        );

        // When the `Core` collects enough parent certificates, the `Proposer` generates a new header with new batch
        // digests from our workers and sends it back to the `Core`.
        match network_model {
            NetworkModel::PartiallySynchronous => PartiallySyncProposer::spawn(
                name.clone(),
                committee.clone(),
                signature_service,
                parameters.header_size,
                parameters.max_header_delay,
                /* rx_core */ rx_parents,
                /* rx_workers */ rx_our_digests,
                /* tx_core */ tx_headers,
            ),
            NetworkModel::Asynchronous => AsyncProposer::spawn(
                name.clone(),
                committee.clone(),
                signature_service,
                parameters.header_size,
                parameters.max_header_delay,
                /* rx_core */ rx_parents,
                /* rx_workers */ rx_our_digests,
                /* tx_core */ tx_headers,
            ),
        }

        // The `Helper` is dedicated to reply to certificates & payload availability requests
        // from other primaries.
        Helper::spawn(
            name.clone(),
            committee.clone(),
            certificate_store,
            payload_store,
            rx_helper_requests,
        );

        if !internal_consensus {
            // Spawn a grpc server to accept requests from external consensus layer.
            ConsensusAPIGrpc::spawn(
                parameters.consensus_api_grpc.socket_addr,
                tx_get_block_commands,
                tx_block_removal_commands,
                parameters.consensus_api_grpc.get_collections_timeout,
                parameters.consensus_api_grpc.remove_collections_timeout,
                block_synchronizer_handler,
                dag,
                committee.clone(),
            );
        }

        // NOTE: This log entry is used to compute performance.
        info!(
            "Primary {} successfully booted on {}",
            name.encode_base64(),
            committee
                .primary(&name)
                .expect("Our public key or worker id is not in the committee")
                .primary_to_primary
        );

        primary_handle
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct PrimaryReceiverHandler<PublicKey: VerifyingKey> {
    tx_primary_messages: Sender<PrimaryMessage<PublicKey>>,
    tx_helper_requests: Sender<PrimaryMessage<PublicKey>>,
    tx_payload_availability_responses: Sender<PayloadAvailabilityResponse<PublicKey>>,
    tx_certificate_responses: Sender<CertificatesResponse<PublicKey>>,
}

impl<PublicKey: VerifyingKey> PrimaryReceiverHandler<PublicKey> {
    fn spawn(self, address: Multiaddr, max_concurrent_requests: usize) {
        tokio::spawn(async move {
            let mut config = mysten_network::config::Config::new();
            config.concurrency_limit_per_connection = Some(max_concurrent_requests);
            config
                .server_builder()
                .add_service(PrimaryToPrimaryServer::new(self))
                .bind(&address)
                .await
                .unwrap()
                .serve()
                .await
        });
    }
}

#[async_trait]
impl<PublicKey: VerifyingKey> PrimaryToPrimary for PrimaryReceiverHandler<PublicKey> {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: PrimaryMessage<PublicKey> = request
            .into_inner()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        match message {
            PrimaryMessage::CertificatesRequest(_, _) => self
                .tx_helper_requests
                .send(message)
                .await
                .expect("Failed to send primary message"),
            PrimaryMessage::CertificatesBatchRequest { .. } => self
                .tx_helper_requests
                .send(message)
                .await
                .expect("Failed to send primary message"),
            PrimaryMessage::CertificatesBatchResponse { certificates, from } => self
                .tx_certificate_responses
                .send(CertificatesResponse {
                    certificates: certificates.to_vec(),
                    from: from.clone(),
                })
                .await
                .expect("Failed to send primary message"),
            PrimaryMessage::PayloadAvailabilityRequest { .. } => self
                .tx_helper_requests
                .send(message)
                .await
                .expect("Failed to send primary message"),
            PrimaryMessage::PayloadAvailabilityResponse {
                payload_availability,
                from,
            } => self
                .tx_payload_availability_responses
                .send(PayloadAvailabilityResponse {
                    block_ids: payload_availability.to_vec(),
                    from: from.clone(),
                })
                .await
                .expect("Failed to send primary message"),
            _ => self
                .tx_primary_messages
                .send(message)
                .await
                .expect("Failed to send certificate"),
        }

        Ok(Response::new(Empty {}))
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
struct WorkerReceiverHandler {
    tx_our_digests: Sender<(BatchDigest, WorkerId)>,
    tx_others_digests: Sender<(BatchDigest, WorkerId)>,
    tx_batches: Sender<BatchResult>,
    tx_batch_removal: Sender<DeleteBatchResult>,
}

impl WorkerReceiverHandler {
    fn spawn(self, address: Multiaddr) {
        tokio::spawn(async move {
            let config = mysten_network::config::Config::default();
            config
                .server_builder()
                .add_service(WorkerToPrimaryServer::new(self))
                .bind(&address)
                .await
                .unwrap()
                .serve()
                .await
        });
    }
}

#[async_trait]
impl WorkerToPrimary for WorkerReceiverHandler {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: WorkerPrimaryMessage = request
            .into_inner()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        match message {
            WorkerPrimaryMessage::OurBatch(digest, worker_id) => self
                .tx_our_digests
                .send((digest, worker_id))
                .await
                .expect("Failed to send workers' digests"),
            WorkerPrimaryMessage::OthersBatch(digest, worker_id) => self
                .tx_others_digests
                .send((digest, worker_id))
                .await
                .expect("Failed to send workers' digests"),
            WorkerPrimaryMessage::RequestedBatch(digest, transactions) => self
                .tx_batches
                .send(Ok(BatchMessage {
                    id: digest,
                    transactions,
                }))
                .await
                .expect("Failed to send batch result"),
            WorkerPrimaryMessage::DeletedBatches(batch_ids) => self
                .tx_batch_removal
                .send(Ok(DeleteBatchMessage { ids: batch_ids }))
                .await
                .expect("Failed to send batch delete result"),
            WorkerPrimaryMessage::Error(error) => match error.clone() {
                WorkerPrimaryError::RequestedBatchNotFound(digest) => self
                    .tx_batches
                    .send(Err(BatchMessageError { id: digest }))
                    .await
                    .expect("Failed to send batch result"),
                WorkerPrimaryError::ErrorWhileDeletingBatches(batch_ids) => self
                    .tx_batch_removal
                    .send(Err(DeleteBatchMessage { ids: batch_ids }))
                    .await
                    .expect("Failed to send error batch delete result"),
            },
        }

        Ok(Response::new(Empty {}))
    }
}
