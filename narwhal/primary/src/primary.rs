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
use config::{Parameters, SharedCommittee, WorkerId, WorkerInfo};
use consensus::dag::Dag;
use crypto::{
    traits::{EncodeDecodeBase64, Signer},
    PublicKey, Signature, SignatureService,
};
use multiaddr::{Multiaddr, Protocol};
use network::{metrics::Metrics, PrimaryNetwork, PrimaryToWorkerNetwork};
use prometheus::Registry;
use std::{collections::HashMap, net::Ipv4Addr, sync::Arc};
use store::Store;
use tokio::{sync::watch, task::JoinHandle};
use tonic::{Request, Response, Status};
use tracing::info;
use types::{
    error::DagError,
    metered_channel::{channel, Receiver, Sender},
    BatchDigest, BatchMessage, BincodeEncodedPayload, Certificate, CertificateDigest, Empty,
    Header, HeaderDigest, PrimaryToPrimary, PrimaryToPrimaryServer, ReconfigureNotification,
    WorkerInfoResponse, WorkerPrimaryError, WorkerPrimaryMessage, WorkerToPrimary,
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

    // Spawns the primary and returns the JoinHandles of its tasks, as well as a metered receiver for the Consensus.
    pub fn spawn<Signatory: Signer<Signature> + Send + 'static>(
        name: PublicKey,
        signer: Signatory,
        committee: SharedCommittee,
        parameters: Parameters,
        header_store: Store<HeaderDigest, Header>,
        certificate_store: Store<CertificateDigest, Certificate>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        tx_consensus: Sender<Certificate>,
        rx_consensus: Receiver<Certificate>,
        dag: Option<Arc<Dag>>,
        network_model: NetworkModel,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_committed_certificates: Sender<Certificate>,
        registry: &Registry,
    ) -> Vec<JoinHandle<()>> {
        // Write the parameters to the logs.
        parameters.tracing();

        // Initialize the metrics
        let metrics = initialise_metrics(registry);
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let mut primary_channel_metrics = metrics.primary_channel_metrics.unwrap();
        let primary_endpoint_metrics = metrics.primary_endpoint_metrics.unwrap();
        let node_metrics = Arc::new(metrics.node_metrics.unwrap());
        let network_metrics = Arc::new(metrics.network_metrics.unwrap());

        let (tx_others_digests, rx_others_digests) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_others_digests);
        let (tx_our_digests, rx_our_digests) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_our_digests);
        let (tx_parents, rx_parents) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_parents);
        let (tx_headers, rx_headers) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_headers);
        let (tx_sync_headers, rx_sync_headers) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_sync_headers);
        let (tx_sync_certificates, rx_sync_certificates) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_sync_certificates,
        );
        let (tx_headers_loopback, rx_headers_loopback) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_headers_loopback,
        );
        let (tx_certificates_loopback, rx_certificates_loopback) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificates_loopback,
        );
        let (tx_primary_messages, rx_primary_messages) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_primary_messages,
        );
        let (tx_helper_requests, rx_helper_requests) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_helper_requests,
        );
        let (tx_get_block_commands, rx_get_block_commands) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_get_block_commands,
        );
        let (tx_batches, rx_batches) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_batches);
        let (tx_block_removal_commands, rx_block_removal_commands) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_block_removal_commands,
        );
        let (tx_batch_removal, rx_batch_removal) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_batch_removal);
        let (tx_block_synchronizer_commands, rx_block_synchronizer_commands) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_block_synchronizer_commands,
        );
        let (tx_certificate_responses, rx_certificate_responses) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificate_responses,
        );
        let (tx_payload_availability_responses, rx_payload_availability_responses) = channel(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_payload_availability_responses,
        );
        let (tx_state_handler, rx_state_handler) =
            channel(CHANNEL_CAPACITY, &primary_channel_metrics.tx_state_handler);

        // we need to hack the gauge from this consensus channel into the primary registry
        // This avoids a cyclic dependency in the initialization of consensus and primary
        // TODO: this (tx_committed_certificates, rx_consensus) channel pair name is highly counterintuitive: see initialization in node and rename(?)
        let committed_certificates_gauge = tx_committed_certificates.gauge().clone();
        primary_channel_metrics.replace_registered_committed_certificates_metric(
            registry,
            Box::new(committed_certificates_gauge),
        );
        let new_certificates_gauge = tx_consensus.gauge().clone();
        primary_channel_metrics
            .replace_registered_new_certificates_metric(registry, Box::new(new_certificates_gauge));

        let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

        let our_workers = committee
            .load()
            .authorities
            .get(&name)
            .expect("Our public key or worker id is not in the committee")
            .workers
            .clone();

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
            our_workers,
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
        let core_primary_network = PrimaryNetwork::new(Metrics::new(
            network_metrics.clone(),
            "primary_core".to_string(),
        ));
        let core_handle = Core::spawn(
            name.clone(),
            (**committee.load()).clone(),
            header_store.clone(),
            certificate_store.clone(),
            synchronizer,
            signature_service.clone(),
            tx_consensus_round_updates.subscribe(),
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            /* rx_primaries */ rx_primary_messages,
            /* rx_header_waiter */ rx_headers_loopback,
            /* rx_certificate_waiter */ rx_certificates_loopback,
            /* rx_proposer */ rx_headers,
            tx_consensus,
            /* tx_proposer */ tx_parents,
            node_metrics.clone(),
            core_primary_network,
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
        let block_waiter_primary_network = PrimaryToWorkerNetwork::new(Metrics::new(
            network_metrics.clone(),
            "block_waiter".to_string(),
        ));
        let block_waiter_handle = BlockWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            tx_reconfigure.subscribe(),
            rx_get_block_commands,
            rx_batches,
            block_synchronizer_handler.clone(),
            block_waiter_primary_network,
        );

        // Indicator variable for the gRPC server
        let internal_consensus = dag.is_none();

        // Orchestrates the removal of blocks across the primary and worker nodes.
        let block_remover_primary_network = PrimaryToWorkerNetwork::new(Metrics::new(
            network_metrics.clone(),
            "block_remover".to_string(),
        ));
        let block_remover_handle = BlockRemover::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store.clone(),
            header_store,
            payload_store.clone(),
            dag.clone(),
            block_remover_primary_network,
            tx_reconfigure.subscribe(),
            rx_block_removal_commands,
            rx_batch_removal,
            tx_committed_certificates,
        );

        // Responsible for finding missing blocks (certificates) and fetching
        // them from the primary peers by synchronizing also their batches.
        let block_synchronizer_network = PrimaryNetwork::new(Metrics::new(
            network_metrics.clone(),
            "block_synchronizer_handler".to_string(),
        ));
        let block_synchronizer_handle = BlockSynchronizer::spawn(
            name.clone(),
            (**committee.load()).clone(),
            tx_reconfigure.subscribe(),
            rx_block_synchronizer_commands,
            rx_certificate_responses,
            rx_payload_availability_responses,
            block_synchronizer_network,
            payload_store.clone(),
            certificate_store.clone(),
            parameters.block_synchronizer,
        );

        // Whenever the `Synchronizer` does not manage to validate a header due to missing parent certificates of
        // batch digests, it commands the `HeaderWaiter` to synchronize with other nodes, wait for their reply, and
        // re-schedule execution of the header once we have all missing data.
        let header_waiter_primary_network = PrimaryNetwork::new(Metrics::new(
            network_metrics.clone(),
            "header_waiter".to_string(),
        ));
        let header_waiter_worker_network = PrimaryToWorkerNetwork::new(Metrics::new(
            network_metrics.clone(),
            "header_waiter".to_string(),
        ));
        let header_waiter_handle = HeaderWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store.clone(),
            payload_store.clone(),
            tx_consensus_round_updates.subscribe(),
            parameters.gc_depth,
            parameters.sync_retry_delay,
            parameters.sync_retry_nodes,
            tx_reconfigure.subscribe(),
            /* rx_synchronizer */ rx_sync_headers,
            /* tx_core */ tx_headers_loopback,
            node_metrics.clone(),
            header_waiter_primary_network,
            header_waiter_worker_network,
        );

        // The `CertificateWaiter` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        let certificate_waiter_handle = CertificateWaiter::spawn(
            (**committee.load()).clone(),
            certificate_store.clone(),
            rx_consensus_round_updates,
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
        let helper_primary_network =
            PrimaryNetwork::new(Metrics::new(network_metrics, "primary_helper".to_string()));
        let helper_handle = Helper::spawn(
            name.clone(),
            (**committee.load()).clone(),
            certificate_store,
            payload_store,
            tx_reconfigure.subscribe(),
            rx_helper_requests,
            helper_primary_network,
        );

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        let state_handler_handle = StateHandler::spawn(
            name.clone(),
            committee.clone(),
            rx_consensus,
            tx_consensus_round_updates,
            rx_state_handler,
            tx_reconfigure,
        );

        let consensus_api_handle = if !internal_consensus {
            // Spawn a grpc server to accept requests from external consensus layer.
            Some(ConsensusAPIGrpc::spawn(
                name.clone(),
                parameters.consensus_api_grpc.socket_addr,
                tx_get_block_commands,
                tx_block_removal_commands,
                parameters.consensus_api_grpc.get_collections_timeout,
                parameters.consensus_api_grpc.remove_collections_timeout,
                block_synchronizer_handler,
                dag,
                committee.clone(),
                endpoint_metrics,
            ))
        } else {
            None
        };

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

        let mut handles = vec![
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
        ];

        if let Some(h) = consensus_api_handle {
            handles.push(h);
        }

        handles
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct PrimaryReceiverHandler {
    tx_primary_messages: Sender<PrimaryMessage>,
    tx_helper_requests: Sender<PrimaryMessage>,
    tx_payload_availability_responses: Sender<PayloadAvailabilityResponse>,
    tx_certificate_responses: Sender<CertificatesResponse>,
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
        max_concurrent_requests: usize,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        primary_endpoint_metrics: PrimaryEndpointMetrics,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut config = mysten_network::config::Config::new();
            config.concurrency_limit_per_connection = Some(max_concurrent_requests);
            info!("PrimaryReceiverHandler has started successfully.");
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
impl PrimaryToPrimary for PrimaryReceiverHandler {
    async fn send_message(
        &self,
        request: Request<BincodeEncodedPayload>,
    ) -> Result<Response<Empty>, Status> {
        let message: PrimaryMessage = request
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
struct WorkerReceiverHandler {
    tx_our_digests: Sender<(BatchDigest, WorkerId)>,
    tx_others_digests: Sender<(BatchDigest, WorkerId)>,
    tx_batches: Sender<BatchResult>,
    tx_batch_removal: Sender<DeleteBatchResult>,
    tx_state_handler: Sender<ReconfigureNotification>,
    our_workers: HashMap<WorkerId, WorkerInfo>,
    metrics: Arc<PrimaryMetrics>,
}

impl WorkerReceiverHandler {
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
            info!("WorkerReceiverHandler has started successfully.");
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

    async fn worker_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<BincodeEncodedPayload>, Status> {
        let workers = WorkerInfoResponse {
            workers: self.our_workers.clone(),
        };

        let response = BincodeEncodedPayload::try_from(&workers)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(response))
    }
}
