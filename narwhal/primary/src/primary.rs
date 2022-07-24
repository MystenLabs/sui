// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_remover::DeleteBatchResult,
    block_synchronizer::{handler::BlockSynchronizerHandler, BlockSynchronizer},
    block_waiter::{BatchMessageError, BatchResult, BlockWaiter},
    certificate_waiter::CertificateWaiter,
    core::Core,
    grpc_server::ConsensusAPIGrpc,
    header_waiter::HeaderWaiter,
    helper::Helper,
    metrics::{initialise_metrics, PrimaryEndpointMetrics, PrimaryMetrics},
    payload_receiver::PayloadReceiver,
    proposer::Proposer,
    state_handler::StateHandler,
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
use prometheus::Registry;
use std::{
    net::Ipv4Addr,
    sync::{atomic::AtomicU64, Arc},
};
use store::Store;
use tokio::{
    sync::{
        mpsc::{channel, Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tonic::{Request, Response, Status};
use tracing::info;
use types::{
    error::DagError, BatchDigest, BatchMessage, BincodeEncodedPayload, Certificate,
    CertificateDigest, Empty, Header, HeaderDigest, PrimaryToPrimary, PrimaryToPrimaryServer,
    ReconfigureNotification, WorkerPrimaryError, WorkerPrimaryMessage, WorkerToPrimary,
    WorkerToPrimaryServer,
};
pub use types::{PrimaryMessage, PrimaryWorkerMessage};

/// The default channel capacity for each channel of the primary.
pub const CHANNEL_CAPACITY: usize = 1_000;

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
        tx_reconfigure: watch::Sender<ReconfigureNotification<PublicKey>>,
        tx_committed_certificates: Sender<Certificate<PublicKey>>,
        registry: &Registry,
    ) -> Vec<JoinHandle<()>> {
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
        let (tx_state_handler, rx_state_handler) = channel(CHANNEL_CAPACITY);

        // Write the parameters to the logs.
        parameters.tracing();

        // Initialize the metrics
        let metrics = initialise_metrics(registry);
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let primary_endpoint_metrics = metrics.primary_endpoint_metrics.unwrap();
        let node_metrics = Arc::new(metrics.node_metrics.unwrap());

        // Atomic variable use to synchronize all tasks with the latest consensus round. This is only
        // used for cleanup. The only task that write into this variable is `GarbageCollector`.
        let consensus_round = Arc::new(AtomicU64::new(0));

        // Spawn the network receiver listening to messages from the other primaries.
        let address = committee
            .load()
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .primary_to_primary;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Primary::INADDR_ANY)))
            .unwrap();
        let primary_receiver_handle = PrimaryReceiverHandler {
            tx_primary_messages: tx_primary_messages.clone(),
            tx_helper_requests,
            tx_payload_availability_responses,
            tx_certificate_responses,
        }
        .spawn(
            address.clone(),
            parameters.max_concurrent_requests,
            tx_reconfigure.subscribe(),
            primary_endpoint_metrics,
        );
        info!(
            "Primary {} listening to primary messages on {}",
            name.encode_base64(),
            address
        );

        // Spawn the network receiver listening to messages from our workers.
        let address = committee
            .load()
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .worker_to_primary;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Primary::INADDR_ANY)))
            .unwrap();
        let worker_receiver_handle = WorkerReceiverHandler {
            tx_our_digests,
            tx_others_digests,
            tx_batches,
            tx_batch_removal,
            tx_state_handler,
            metrics: node_metrics.clone(),
        }
        .spawn(address.clone(), tx_reconfigure.subscribe());
        info!(
            "Primary {} listening to workers messages on {}",
            name.encode_base64(),
            address
        );

        // The `Synchronizer` provides auxiliary methods helping the `Core` to sync.
        let synchronizer = Synchronizer::new(
            name.clone(),
            &committee.load(),
            certificate_store.clone(),
            payload_store.clone(),
            /* tx_header_waiter */ tx_sync_headers,
            /* tx_certificate_waiter */ tx_sync_certificates,
            dag.clone(),
        );

        // The `SignatureService` is used to require signatures on specific digests.
        let signature_service = SignatureService::new(signer);

        // The `Core` receives and handles headers, votes, and certificates from the other primaries.
        let core_handle = Core::spawn(
            name.clone(),
            (**committee.load()).clone(),
            header_store.clone(),
            certificate_store.clone(),
            synchronizer,
            signature_service.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            /* rx_primaries */ rx_primary_messages,
            /* rx_header_waiter */ rx_headers_loopback,
            /* rx_certificate_waiter */ rx_certificates_loopback,
            /* rx_proposer */ rx_headers,
            tx_consensus,
            /* tx_proposer */ tx_parents,
            node_metrics.clone(),
        );

        // Receives batch digests from other workers. They are only used to validate headers.
        let payload_receiver_handle = PayloadReceiver::spawn(
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
        let block_waiter_handle = BlockWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            tx_reconfigure.subscribe(),
            rx_get_block_commands,
            rx_batches,
            block_synchronizer_handler.clone(),
        );

        // Indicator variable for the gRPC server
        let internal_consensus = dag.is_none();

        // Orchestrates the removal of blocks across the primary and worker nodes.
        let block_remover_handle = BlockRemover::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store.clone(),
            header_store,
            payload_store.clone(),
            dag.clone(),
            PrimaryToWorkerNetwork::default(),
            tx_reconfigure.subscribe(),
            rx_block_removal_commands,
            rx_batch_removal,
            tx_committed_certificates,
        );

        // Responsible for finding missing blocks (certificates) and fetching
        // them from the primary peers by synchronizing also their batches.
        let block_synchronizer_handle = BlockSynchronizer::spawn(
            name.clone(),
            (**committee.load()).clone(),
            tx_reconfigure.subscribe(),
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
        let header_waiter_handle = HeaderWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store.clone(),
            payload_store.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            parameters.sync_retry_delay,
            parameters.sync_retry_nodes,
            tx_reconfigure.subscribe(),
            /* rx_synchronizer */ rx_sync_headers,
            /* tx_core */ tx_headers_loopback,
            node_metrics.clone(),
        );

        // The `CertificateWaiter` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        let certificate_waiter_handle = CertificateWaiter::spawn(
            (**committee.load()).clone(),
            certificate_store.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            /* rx_synchronizer */ rx_sync_certificates,
            /* tx_core */ tx_certificates_loopback,
            node_metrics.clone(),
        );

        // When the `Core` collects enough parent certificates, the `Proposer` generates a new header with new batch
        // digests from our workers and sends it back to the `Core`.
        let proposer_handle = Proposer::spawn(
            name.clone(),
            (**committee.load()).clone(),
            signature_service,
            parameters.header_size,
            parameters.max_header_delay,
            network_model,
            tx_reconfigure.subscribe(),
            /* rx_core */ rx_parents,
            /* rx_workers */ rx_our_digests,
            /* tx_core */ tx_headers,
            node_metrics,
        );

        // The `Helper` is dedicated to reply to certificates & payload availability requests
        // from other primaries.
        let helper_handle = Helper::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store,
            payload_store,
            tx_reconfigure.subscribe(),
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
                endpoint_metrics,
            );
        }

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        let state_handler_handle = StateHandler::spawn(
            name.clone(),
            committee.clone(),
            consensus_round,
            rx_consensus,
            rx_state_handler,
            tx_reconfigure,
        );

        // NOTE: This log entry is used to compute performance.
        info!(
            "Primary {} successfully booted on {}",
            name.encode_base64(),
            committee
                .load()
                .primary(&name)
                .expect("Our public key or worker id is not in the committee")
                .primary_to_primary
        );

        vec![
            primary_receiver_handle,
            worker_receiver_handle,
            core_handle,
            payload_receiver_handle,
            block_synchronizer_handle,
            block_waiter_handle,
            block_remover_handle,
            header_waiter_handle,
            certificate_waiter_handle,
            proposer_handle,
            helper_handle,
            state_handler_handle,
        ]
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
    async fn wait_for_shutdown(
        mut rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    ) {
        loop {
            let result = rx_reconfigure.changed().await;
            result.expect("Committee channel dropped");
            let message = rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::Shutdown = message {
                break;
            }
        }
    }

    fn spawn(
        self,
        address: Multiaddr,
        max_concurrent_requests: usize,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        primary_endpoint_metrics: PrimaryEndpointMetrics,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut config = mysten_network::config::Config::new();
            config.concurrency_limit_per_connection = Some(max_concurrent_requests);
            tokio::select! {
                _result = config
                    .server_builder_with_metrics(primary_endpoint_metrics)
                    .add_service(PrimaryToPrimaryServer::new(self))
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
                .map_err(|_| DagError::ShuttingDown),
            PrimaryMessage::CertificatesBatchRequest { .. } => self
                .tx_helper_requests
                .send(message)
                .await
                .map_err(|_| DagError::ShuttingDown),
            PrimaryMessage::CertificatesBatchResponse { certificates, from } => self
                .tx_certificate_responses
                .send(CertificatesResponse {
                    certificates: certificates.to_vec(),
                    from: from.clone(),
                })
                .await
                .map_err(|_| DagError::ShuttingDown),
            PrimaryMessage::PayloadAvailabilityRequest { .. } => self
                .tx_helper_requests
                .send(message)
                .await
                .map_err(|_| DagError::ShuttingDown),
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
                .map_err(|_| DagError::ShuttingDown),
            _ => self
                .tx_primary_messages
                .send(message)
                .await
                .map_err(|_| DagError::ShuttingDown),
        }
        .map_err(|e| Status::not_found(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
struct WorkerReceiverHandler<PublicKey: VerifyingKey> {
    tx_our_digests: Sender<(BatchDigest, WorkerId)>,
    tx_others_digests: Sender<(BatchDigest, WorkerId)>,
    tx_batches: Sender<BatchResult>,
    tx_batch_removal: Sender<DeleteBatchResult>,
    tx_state_handler: Sender<ReconfigureNotification<PublicKey>>,
    metrics: Arc<PrimaryMetrics>,
}

impl<PublicKey: VerifyingKey> WorkerReceiverHandler<PublicKey> {
    async fn wait_for_shutdown(
        mut rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    ) {
        loop {
            let result = rx_reconfigure.changed().await;
            result.expect("Committee channel dropped");
            let message = rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::Shutdown = message {
                break;
            }
        }
    }

    fn spawn(
        self,
        address: Multiaddr,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            tokio::select! {
                _result = mysten_network::config::Config::default()
                    .server_builder()
                    .add_service(WorkerToPrimaryServer::new(self))
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
impl<PublicKey: VerifyingKey> WorkerToPrimary for WorkerReceiverHandler<PublicKey> {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: WorkerPrimaryMessage<_> = request
            .into_inner()
            .deserialize()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        match message {
            WorkerPrimaryMessage::OurBatch(digest, worker_id) => {
                self.metrics
                    .batches_received
                    .with_label_values(&[&worker_id.to_string(), "our_batch"])
                    .inc();
                self.tx_our_digests
                    .send((digest, worker_id))
                    .await
                    .map_err(|_| DagError::ShuttingDown)
            }
            WorkerPrimaryMessage::OthersBatch(digest, worker_id) => {
                self.metrics
                    .batches_received
                    .with_label_values(&[&worker_id.to_string(), "others_batch"])
                    .inc();
                self.tx_others_digests
                    .send((digest, worker_id))
                    .await
                    .map_err(|_| DagError::ShuttingDown)
            }
            WorkerPrimaryMessage::RequestedBatch(digest, transactions) => self
                .tx_batches
                .send(Ok(BatchMessage {
                    id: digest,
                    transactions,
                }))
                .await
                .map_err(|_| DagError::ShuttingDown),
            WorkerPrimaryMessage::DeletedBatches(batch_ids) => self
                .tx_batch_removal
                .send(Ok(DeleteBatchMessage { ids: batch_ids }))
                .await
                .map_err(|_| DagError::ShuttingDown),
            WorkerPrimaryMessage::Error(error) => match error.clone() {
                WorkerPrimaryError::RequestedBatchNotFound(digest) => self
                    .tx_batches
                    .send(Err(BatchMessageError { id: digest }))
                    .await
                    .map_err(|_| DagError::ShuttingDown),
                WorkerPrimaryError::ErrorWhileDeletingBatches(batch_ids) => self
                    .tx_batch_removal
                    .send(Err(DeleteBatchMessage { ids: batch_ids }))
                    .await
                    .map_err(|_| DagError::ShuttingDown),
            },
            WorkerPrimaryMessage::Reconfigure(notification) => self
                .tx_state_handler
                .send(notification)
                .await
                .map_err(|_| DagError::ShuttingDown),
        }
        .map_err(|e| Status::not_found(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }
}
