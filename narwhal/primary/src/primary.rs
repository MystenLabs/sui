// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{handler::BlockSynchronizerHandler, BlockSynchronizer},
    block_waiter::BlockWaiter,
    certificate_waiter::CertificateWaiter,
    core::Core,
    grpc_server::ConsensusAPIGrpc,
    header_waiter::HeaderWaiter,
    metrics::initialise_metrics,
    proposer::Proposer,
    state_handler::StateHandler,
    synchronizer::Synchronizer,
    BlockRemover,
};

use anemo::{types::PeerInfo, PeerId};
use anemo_tower::{
    auth::{AllowedPeers, RequireAuthorizationLayer},
    callback::CallbackLayer,
    trace::TraceLayer,
};
use async_trait::async_trait;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId, WorkerInfo};
use consensus::dag::Dag;
use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use fastcrypto::{
    traits::{EncodeDecodeBase64, KeyPair as _},
    SignatureService,
};
use multiaddr::Protocol;
use network::metrics::MetricsMakeCallbackHandler;
use network::P2pNetwork;
use prometheus::Registry;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
    net::Ipv4Addr,
    sync::Arc,
};
use storage::{CertificateStore, ProposerStore};
use store::Store;
use tokio::sync::oneshot;
use tokio::{sync::watch, task::JoinHandle};
use tower::ServiceBuilder;
use tracing::{error, info};
pub use types::PrimaryMessage;
use types::{
    metered_channel::{channel_with_total, Receiver, Sender},
    BatchDigest, Certificate, CertificateDigest, ConsensusStore, FetchCertificatesRequest,
    FetchCertificatesResponse, GetCertificatesRequest, GetCertificatesResponse, Header,
    HeaderDigest, PayloadAvailabilityRequest, PayloadAvailabilityResponse, PrimaryToPrimary,
    PrimaryToPrimaryServer, ReconfigureNotification, RoundVoteDigestPair, WorkerInfoResponse,
    WorkerOthersBatchMessage, WorkerOurBatchMessage, WorkerToPrimary, WorkerToPrimaryServer,
};

#[cfg(any(test))]
#[path = "tests/primary_tests.rs"]
pub mod primary_tests;

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
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        signer: KeyPair,
        network_signer: NetworkKeyPair,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        parameters: Parameters,
        header_store: Store<HeaderDigest, Header>,
        certificate_store: CertificateStore,
        proposer_store: ProposerStore,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        vote_digest_store: Store<PublicKey, RoundVoteDigestPair>,
        consensus_store: Arc<ConsensusStore>,
        tx_new_certificates: Sender<Certificate>,
        rx_committed_certificates: Receiver<Certificate>,
        dag: Option<Arc<Dag>>,
        network_model: NetworkModel,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_committed_certificates: Sender<Certificate>,
        registry: &Registry,
        // See comments in Subscriber::spawn
        rx_executor_network: Option<oneshot::Sender<P2pNetwork>>,
    ) -> Vec<JoinHandle<()>> {
        // Write the parameters to the logs.
        parameters.tracing();

        // Initialize the metrics
        let metrics = initialise_metrics(registry);
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let mut primary_channel_metrics = metrics.primary_channel_metrics.unwrap();
        let inbound_network_metrics = Arc::new(metrics.inbound_network_metrics.unwrap());
        let outbound_network_metrics = Arc::new(metrics.outbound_network_metrics.unwrap());
        let node_metrics = Arc::new(metrics.node_metrics.unwrap());
        let network_connection_metrics = metrics.network_connection_metrics.unwrap();

        let (tx_our_digests, rx_our_digests) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_our_digests,
            &primary_channel_metrics.tx_our_digests_total,
        );
        let (tx_parents, rx_parents) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_parents,
            &primary_channel_metrics.tx_parents_total,
        );
        let (tx_headers, rx_headers) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_headers,
            &primary_channel_metrics.tx_headers_total,
        );
        let (tx_header_waiter, rx_header_waiter) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_header_waiter,
            &primary_channel_metrics.tx_header_waiter_total,
        );
        let (tx_certificate_waiter, rx_certificate_waiter) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificate_waiter,
            &primary_channel_metrics.tx_certificate_waiter_total,
        );
        let (tx_headers_loopback, rx_headers_loopback) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_headers_loopback,
            &primary_channel_metrics.tx_headers_loopback_total,
        );
        let (tx_certificates_loopback, rx_certificates_loopback) = channel_with_total(
            1, // Only one inflight item is possible.
            &primary_channel_metrics.tx_certificates_loopback,
            &primary_channel_metrics.tx_certificates_loopback_total,
        );
        let (tx_primary_messages, rx_primary_messages) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_primary_messages,
            &primary_channel_metrics.tx_primary_messages_total,
        );
        let (tx_block_synchronizer_commands, rx_block_synchronizer_commands) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_block_synchronizer_commands,
            &primary_channel_metrics.tx_block_synchronizer_commands_total,
        );
        let (tx_state_handler, rx_state_handler) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_state_handler,
            &primary_channel_metrics.tx_state_handler_total,
        );

        // we need to hack the gauge from this consensus channel into the primary registry
        // This avoids a cyclic dependency in the initialization of consensus and primary
        let committed_certificates_gauge = tx_committed_certificates.gauge().clone();
        primary_channel_metrics.replace_registered_committed_certificates_metric(
            registry,
            Box::new(committed_certificates_gauge),
        );

        let new_certificates_gauge = tx_new_certificates.gauge().clone();
        primary_channel_metrics
            .replace_registered_new_certificates_metric(registry, Box::new(new_certificates_gauge));

        let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

        let our_workers = worker_cache
            .load()
            .workers
            .get(&name)
            .expect("Our public key is not in the worker cache")
            .0
            .clone();

        // Spawn the network receiver listening to messages from the other primaries.
        let address = committee
            .load()
            .primary(&name)
            .expect("Our public key or worker id is not in the committee");
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Primary::INADDR_ANY)))
            .unwrap();
        let primary_service = PrimaryToPrimaryServer::new(PrimaryReceiverHandler {
            tx_primary_messages: tx_primary_messages.clone(),
            certificate_store: certificate_store.clone(),
            payload_store: payload_store.clone(),
        });
        let worker_service = WorkerToPrimaryServer::new(WorkerReceiverHandler {
            tx_our_digests,
            payload_store: payload_store.clone(),
            our_workers,
        });

        let addr = network::multiaddr_to_address(&address).unwrap();

        let our_worker_peer_ids = worker_cache
            .load()
            .our_workers(&name)
            .unwrap()
            .into_iter()
            .map(|worker_info| PeerId(worker_info.name.0.to_bytes()));
        let worker_to_primary_router = anemo::Router::new()
            .add_rpc_service(worker_service)
            // Add an Authorization Layer to ensure that we only service requests from our workers
            .route_layer(RequireAuthorizationLayer::new(AllowedPeers::new(
                our_worker_peer_ids,
            )));

        let routes = anemo::Router::new()
            .add_rpc_service(primary_service)
            .merge(worker_to_primary_router);

        let service = ServiceBuilder::new()
            .layer(TraceLayer::new_for_server_errors())
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                inbound_network_metrics,
            )))
            .service(routes);

        let outbound_layer = ServiceBuilder::new()
            .layer(TraceLayer::new_for_client_and_server_errors())
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

        let network = anemo::Network::bind(addr.clone())
            .server_name("narwhal")
            .private_key(network_signer.copy().private().0.to_bytes())
            .config(anemo_config)
            .outbound_request_layer(outbound_layer)
            .start(service)
            .unwrap_or_else(|_| {
                panic!(
                    "Address {} should be available for the primary Narwhal service",
                    addr
                )
            });
        info!("Primary {} listening on {}", name.encode_base64(), address);

        let connection_monitor_handle = network::connectivity::ConnectionMonitor::spawn(
            network.downgrade(),
            network_connection_metrics,
        );

        let primaries = committee
            .load()
            .others_primaries(&name)
            .into_iter()
            .map(|(_, address, network_key)| (network_key, address));
        let workers = worker_cache.load().all_workers().into_iter();
        for (public_key, address) in primaries.chain(workers) {
            let peer_id = PeerId(public_key.0.to_bytes());
            let address = network::multiaddr_to_address(&address).unwrap();
            let peer_info = PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: vec![address],
            };
            network.known_peers().insert(peer_info);
        }

        info!(
            "Primary {} listening to network admin messages on 127.0.0.1:{}",
            name.encode_base64(),
            parameters
                .network_admin_server
                .primary_network_admin_server_port
        );

        network::admin::start_admin_server(
            parameters
                .network_admin_server
                .primary_network_admin_server_port,
            network.clone(),
            tx_reconfigure.subscribe(),
            Some(tx_state_handler),
        );

        // The `Synchronizer` provides auxiliary methods helping the `Core` to sync.
        let synchronizer = Synchronizer::new(
            name.clone(),
            &committee.load(),
            certificate_store.clone(),
            payload_store.clone(),
            tx_header_waiter,
            tx_certificate_waiter,
            dag.clone(),
        );

        // The `SignatureService` is used to require signatures on specific digests.
        let signature_service = SignatureService::new(signer);

        if let Some(rx_executor_network) = rx_executor_network {
            let executor_network = P2pNetwork::new(network.clone());
            if rx_executor_network.send(executor_network).is_err() {
                panic!("Executor shut down before primary has a chance to start");
            }
        }

        // TODO (Laura): if we are restarting and not advancing, for the headers in the header
        // TODO (Laura): store that do not have a matching certificate, re-create and send a vote
        // The `Core` receives and handles headers, votes, and certificates from the other primaries.
        let core_primary_network = P2pNetwork::new(network.clone());
        let core_handle = Core::spawn(
            name.clone(),
            (**committee.load()).clone(),
            worker_cache.clone(),
            header_store.clone(),
            certificate_store.clone(),
            vote_digest_store,
            synchronizer,
            signature_service.clone(),
            rx_consensus_round_updates.clone(),
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            rx_primary_messages,
            rx_headers_loopback,
            rx_certificates_loopback,
            rx_headers,
            tx_new_certificates,
            tx_parents,
            node_metrics.clone(),
            core_primary_network,
        );

        let block_synchronizer_handler = Arc::new(BlockSynchronizerHandler::new(
            tx_block_synchronizer_commands,
            tx_primary_messages.clone(),
            certificate_store.clone(),
            parameters
                .block_synchronizer
                .handler_certificate_deliver_timeout,
        ));

        // Indicator variable for components to operate in internal vs external consensus modes.
        let internal_consensus = dag.is_none();

        // Responsible for finding missing blocks (certificates) and fetching
        // them from the primary peers by synchronizing also their batches.
        let block_synchronizer_network = P2pNetwork::new(network.clone());
        let block_synchronizer_handle = BlockSynchronizer::spawn(
            name.clone(),
            (**committee.load()).clone(),
            worker_cache.clone(),
            tx_reconfigure.subscribe(),
            rx_block_synchronizer_commands,
            block_synchronizer_network,
            payload_store.clone(),
            certificate_store.clone(),
            parameters.clone(),
        );

        // Whenever the `Synchronizer` does not manage to validate a header due to missing parent certificates of
        // batch digests, it commands the `HeaderWaiter` to synchronize with other nodes, wait for their reply, and
        // re-schedule execution of the header once we have all missing data.
        let header_waiter_primary_network = P2pNetwork::new(network.clone());
        let header_waiter_handle = HeaderWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            worker_cache.clone(),
            certificate_store.clone(),
            payload_store.clone(),
            rx_consensus_round_updates.clone(),
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            rx_header_waiter,
            tx_headers_loopback,
            tx_primary_messages,
            node_metrics.clone(),
            header_waiter_primary_network,
        );

        // The `CertificateWaiter` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        let certificate_waiter_handle = CertificateWaiter::spawn(
            name.clone(),
            (**committee.load()).clone(),
            P2pNetwork::new(network.clone()),
            certificate_store.clone(),
            if internal_consensus {
                Some(consensus_store)
            } else {
                None
            },
            rx_consensus_round_updates,
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            rx_certificate_waiter,
            tx_certificates_loopback,
            node_metrics.clone(),
        );

        // When the `Core` collects enough parent certificates, the `Proposer` generates a new header with new batch
        // digests from our workers and sends it back to the `Core`.
        let proposer_handle = Proposer::spawn(
            name.clone(),
            (**committee.load()).clone(),
            signature_service,
            proposer_store,
            parameters.header_num_of_batches_threshold,
            parameters.max_header_num_of_batches,
            parameters.max_header_delay,
            network_model,
            tx_reconfigure.subscribe(),
            rx_parents,
            rx_our_digests,
            tx_headers,
            node_metrics,
        );

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        let state_handler_handle = StateHandler::spawn(
            name.clone(),
            committee.clone(),
            worker_cache.clone(),
            rx_committed_certificates,
            tx_consensus_round_updates,
            rx_state_handler,
            tx_reconfigure,
            P2pNetwork::new(network.clone()),
        );

        let consensus_api_handle = if !internal_consensus {
            // Retrieves a block's data by contacting the worker nodes that contain the
            // underlying batches and their transactions.
            let block_waiter_primary_network = P2pNetwork::new(network.clone());
            let block_waiter = BlockWaiter::new(
                name.clone(),
                worker_cache.clone(),
                block_waiter_primary_network,
                block_synchronizer_handler.clone(),
            );

            // Orchestrates the removal of blocks across the primary and worker nodes.
            let block_remover_primary_network = P2pNetwork::new(network);
            let block_remover = BlockRemover::new(
                name.clone(),
                worker_cache,
                certificate_store,
                header_store,
                payload_store,
                dag.clone(),
                block_remover_primary_network,
                tx_committed_certificates,
            );

            // Spawn a grpc server to accept requests from external consensus layer.
            Some(ConsensusAPIGrpc::spawn(
                name.clone(),
                parameters.consensus_api_grpc.socket_addr,
                block_waiter,
                block_remover,
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
        );

        let mut handles = vec![
            core_handle,
            block_synchronizer_handle,
            header_waiter_handle,
            certificate_waiter_handle,
            proposer_handle,
            state_handler_handle,
            connection_monitor_handle,
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
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
}

#[async_trait]
impl PrimaryToPrimary for PrimaryReceiverHandler {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        self.tx_primary_messages
            .try_send(message)
            .map(|_| anemo::Response::new(()))
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))
    }

    async fn get_certificates(
        &self,
        request: anemo::Request<GetCertificatesRequest>,
    ) -> Result<anemo::Response<GetCertificatesResponse>, anemo::rpc::Status> {
        let digests = request.into_body().digests;
        if digests.is_empty() {
            return Ok(anemo::Response::new(GetCertificatesResponse {
                certificates: Vec::new(),
            }));
        }

        // TODO [issue #195]: Do some accounting to prevent bad nodes from monopolizing our resources.
        let certificates = self.certificate_store.read_all(digests).map_err(|e| {
            anemo::rpc::Status::internal(format!("error while retrieving certificates: {e}"))
        })?;
        Ok(anemo::Response::new(GetCertificatesResponse {
            certificates: certificates.into_iter().flatten().collect(),
        }))
    }

    async fn fetch_certificates(
        &self,
        request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        let request = request.into_body();
        let mut response = FetchCertificatesResponse {
            certificates: Vec::new(),
        };
        if request.exclusive_lower_bounds.is_empty() || request.max_items == 0 {
            return Ok(anemo::Response::new(response));
        }

        // Use a min-queue for (round, authority) to keep track of the next certificate to fetch.
        // If the requestor is providing its current round on all authorities, it should be able to
        // process certificates returned in this order without any missing parents.
        //
        // Compared to fetching certificates iteratatively round by round, using a heap is simpler,
        // and avoids the pathological case of iterating through many missing rounds of a downed authority.
        let mut fetch_queue = BinaryHeap::new();

        // Initialize the min-heap for each authority the next higher round from the requested
        // lower bound.
        for (authority, round) in request.exclusive_lower_bounds.into_iter() {
            let next_round = self
                .certificate_store
                .next_round_number(&authority, round)
                .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;
            if let Some(r) = next_round {
                fetch_queue.push(Reverse((r, authority)));
            }
        }

        // Iteratively pop one of the (Round, Authority) pairs with the smallest round,
        // and push to min-heap the next higher round of the same authority.
        // The process ends when there are no more pairs in the min-heap.
        while let Some(Reverse((round, authority))) = fetch_queue.pop() {
            match self
                .certificate_store
                .read_by_index(authority.clone(), round)
                .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?
            {
                Some(cert) => {
                    response.certificates.push(cert);
                    let next_round = self
                        .certificate_store
                        .next_round_number(&authority, round)
                        .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;
                    if let Some(r) = next_round {
                        fetch_queue.push(Reverse((r, authority)));
                    }
                }
                None => continue,
            };
            if response.certificates.len() == request.max_items {
                break;
            }
            assert!(response.certificates.len() < request.max_items);
        }

        Ok(anemo::Response::new(response))
    }

    async fn get_payload_availability(
        &self,
        request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        let digests = request.into_body().certificate_ids;
        let certificates = self
            .certificate_store
            .read_all(digests.to_owned())
            .map_err(|e| {
                anemo::rpc::Status::internal(format!("error reading certificates: {e:?}"))
            })?;

        let mut result: Vec<(CertificateDigest, bool)> = Vec::new();
        for (id, certificate_option) in digests.into_iter().zip(certificates) {
            // Find batches only for certificates that exist.
            if let Some(certificate) = certificate_option {
                let payload_available = match self
                    .payload_store
                    .read_all(certificate.header.payload)
                    .await
                {
                    Ok(payload_result) => payload_result.into_iter().all(|x| x.is_some()),
                    Err(err) => {
                        // Assume that we don't have the payloads available,
                        // otherwise an error response should be sent back.
                        error!("Error while retrieving payloads: {err}");
                        false
                    }
                };
                result.push((id, payload_available));
            } else {
                // We don't have the certificate available in first place,
                // so we can't even look up the batches.
                result.push((id, false));
            }
        }

        Ok(anemo::Response::new(PayloadAvailabilityResponse {
            payload_availability: result,
        }))
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
struct WorkerReceiverHandler {
    tx_our_digests: Sender<(BatchDigest, WorkerId, u64)>,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    our_workers: BTreeMap<WorkerId, WorkerInfo>,
}

#[async_trait]
impl WorkerToPrimary for WorkerReceiverHandler {
    async fn report_our_batch(
        &self,
        request: anemo::Request<WorkerOurBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        self.tx_our_digests
            .send((
                message.digest,
                message.worker_id,
                message.metadata.created_at,
            ))
            .await
            .map(|_| anemo::Response::new(()))
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))
    }

    async fn report_others_batch(
        &self,
        request: anemo::Request<WorkerOthersBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        self.payload_store
            .async_write((message.digest, message.worker_id), 0u8)
            .await;
        Ok(anemo::Response::new(()))
    }

    async fn worker_info(
        &self,
        _request: anemo::Request<()>,
    ) -> Result<anemo::Response<WorkerInfoResponse>, anemo::rpc::Status> {
        Ok(anemo::Response::new(WorkerInfoResponse {
            workers: self.our_workers.clone(),
        }))
    }
}
