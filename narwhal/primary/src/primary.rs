// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{handler::BlockSynchronizerHandler, BlockSynchronizer},
    block_waiter::BlockWaiter,
    certificate_fetcher::CertificateFetcher,
    core::Core,
    grpc_server::ConsensusAPIGrpc,
    metrics::initialise_metrics,
    proposer::Proposer,
    state_handler::StateHandler,
    synchronizer::Synchronizer,
    BlockRemover, PrimaryReceiverHandler, WorkerReceiverHandler,
};

use anemo::types::Address;
use anemo::{types::PeerInfo, Network, PeerId};
use anemo_tower::auth::{AllowedPeers, RequireAuthorizationLayer};
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::{DefaultMakeSpan, TraceLayer};
use arc_swap::ArcSwapOption;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use consensus::dag::Dag;
use crypto::{KeyPair, NetworkKeyPair, NetworkPublicKey, PublicKey};
use dashmap::DashSet;
use fastcrypto::{
    traits::{EncodeDecodeBase64, KeyPair as _},
    SignatureService,
};
use multiaddr::{Multiaddr, Protocol};
use mysten_metrics::spawn_monitored_task;
use network::metrics::{MetricsMakeCallbackHandler, NetworkConnectionMetrics, NetworkMetrics};
use network::P2pNetwork;
use prometheus::Registry;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use storage::{CertificateStore, PayloadToken, ProposerStore};
use store::Store;
use tokio::sync::oneshot;
use tokio::{sync::watch, task::JoinHandle};
use tower::ServiceBuilder;
use tracing::info;

use crate::handlers::{PrimaryReceiverController, WorkerReceiverController};
use types::{
    metered_channel::{channel_with_total, Receiver, Sender},
    BatchDigest, Certificate, Header, HeaderDigest, ReconfigureNotification, Round, VoteInfo,
};
pub use types::{PrimaryMessage, PrimaryToPrimaryServer, WorkerToPrimaryServer};

#[cfg(any(test))]
#[path = "tests/primary_tests.rs"]
pub mod primary_tests;

/// The default channel capacity for each channel of the primary.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The network model in which the primary operates.
pub enum NetworkModel {
    PartiallySynchronous,
    Asynchronous,
}

pub struct Primary;

impl Primary {
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
        vote_digest_store: Store<PublicKey, VoteInfo>,
        tx_new_certificates: Sender<Certificate>,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        rx_consensus_round_updates: watch::Receiver<Round>,
        dag: Option<Arc<Dag>>,
        network_model: NetworkModel,
        tx_reconfigure: watch::Sender<ReconfigureNotification>,
        tx_committed_certificates: Sender<(Round, Vec<Certificate>)>,
        registry: &Registry,
        // See comments in Subscriber::spawn
        tx_executor_network: Option<oneshot::Sender<P2pNetwork>>,

        // swappable network controllers
        primary_receiver_controller: Arc<ArcSwapOption<PrimaryReceiverController>>,

        worker_receiver_controller: Arc<ArcSwapOption<WorkerReceiverController>>,

        // The network handler
        network: Network,
    ) -> Vec<JoinHandle<()>> {
        // Write the parameters to the logs.
        parameters.tracing();

        // Some info statements
        info!(
            "Boot primary node with peer id {} and public key {}",
            PeerId(network_signer.public().0.to_bytes()),
            name.encode_base64()
        );

        // Initialize the metrics
        let metrics = initialise_metrics(registry);
        let endpoint_metrics = metrics.endpoint_metrics.unwrap();
        let mut primary_channel_metrics = metrics.primary_channel_metrics.unwrap();
        let node_metrics = Arc::new(metrics.node_metrics.unwrap());

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
        let (tx_certificate_fetcher, rx_certificate_fetcher) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificate_fetcher,
            &primary_channel_metrics.tx_certificate_fetcher_total,
        );
        let (tx_certificates_loopback, rx_certificates_loopback) = channel_with_total(
            1, // Only one inflight item is possible.
            &primary_channel_metrics.tx_certificates_loopback,
            &primary_channel_metrics.tx_certificates_loopback_total,
        );
        let (tx_certificates, rx_certificates) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_certificates,
            &primary_channel_metrics.tx_certificates_total,
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
        let (tx_commited_own_headers, rx_commited_own_headers) = channel_with_total(
            CHANNEL_CAPACITY,
            &primary_channel_metrics.tx_commited_own_headers,
            &primary_channel_metrics.tx_commited_own_headers_total,
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

        let (tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

        let synchronizer = Arc::new(Synchronizer::new(
            name.clone(),
            committee.clone(),
            worker_cache.clone(),
            certificate_store.clone(),
            payload_store.clone(),
            tx_certificate_fetcher,
            rx_consensus_round_updates.clone(),
            dag.clone(),
        ));

        let signature_service = SignatureService::new(signer);

        let our_workers = worker_cache
            .load()
            .workers
            .get(&name)
            .expect("Our public key is not in the worker cache")
            .0
            .clone();

        primary_receiver_controller.swap(Some(Arc::new(PrimaryReceiverController {
            name: name.clone(),
            committee: committee.clone(),
            worker_cache: worker_cache.clone(),
            synchronizer: synchronizer.clone(),
            signature_service: signature_service.clone(),
            tx_certificates: tx_certificates.clone(),
            header_store: header_store.clone(),
            certificate_store: certificate_store.clone(),
            payload_store: payload_store.clone(),
            vote_digest_store,
            rx_narwhal_round_updates: rx_narwhal_round_updates.clone(),
            metrics: node_metrics.clone(),
            request_vote_inflight: Arc::new(DashSet::new()),
        })));

        worker_receiver_controller.swap(Some(Arc::new(WorkerReceiverController {
            tx_our_digests,
            payload_store: payload_store.clone(),
            our_workers,
        })));

        let admin_handles = network::admin::start_admin_server(
            parameters
                .network_admin_server
                .primary_network_admin_server_port,
            network.clone(),
            tx_reconfigure.subscribe(),
            Some(tx_state_handler),
        );

        info!(
            "Primary {} listening to network admin messages on 127.0.0.1:{}",
            name.encode_base64(),
            parameters
                .network_admin_server
                .primary_network_admin_server_port
        );

        if let Some(tx_executor_network) = tx_executor_network {
            let executor_network = P2pNetwork::new(network.clone());
            if tx_executor_network.send(executor_network).is_err() {
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
            synchronizer,
            signature_service.clone(),
            rx_consensus_round_updates.clone(),
            rx_narwhal_round_updates,
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            rx_certificates,
            rx_certificates_loopback,
            rx_headers,
            tx_new_certificates,
            tx_parents,
            node_metrics.clone(),
            core_primary_network,
        );

        let block_synchronizer_handler = Arc::new(BlockSynchronizerHandler::new(
            tx_block_synchronizer_commands,
            tx_certificates,
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

        // The `CertificateFetcher` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        let certificate_fetcher_handle = CertificateFetcher::spawn(
            name.clone(),
            (**committee.load()).clone(),
            P2pNetwork::new(network.clone()),
            certificate_store.clone(),
            rx_consensus_round_updates,
            parameters.gc_depth,
            tx_reconfigure.subscribe(),
            rx_certificate_fetcher,
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
            None,
            network_model,
            tx_reconfigure.subscribe(),
            rx_parents,
            rx_our_digests,
            tx_headers,
            tx_narwhal_round_updates,
            rx_commited_own_headers,
            node_metrics,
        );

        // spawn a task to swap the controllers in network when shutdown
        let mut rx_reconfigure = tx_reconfigure.subscribe();
        let shutdown_monitor_handle = spawn_monitored_task!(async move {
            while (rx_reconfigure.changed().await).is_ok() {
                let message = rx_reconfigure.borrow().clone();
                if let ReconfigureNotification::Shutdown = message {
                    // swap the handlers
                    primary_receiver_controller.swap(None);
                    worker_receiver_controller.swap(None);
                    info!("Swapped the network handlers");
                    break;
                }
            }
            info!("Network swapper ended");
        });

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        let state_handler_handle = StateHandler::spawn(
            name.clone(),
            committee.clone(),
            worker_cache.clone(),
            rx_committed_certificates,
            rx_state_handler,
            tx_reconfigure,
            Some(tx_commited_own_headers),
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
            certificate_fetcher_handle,
            proposer_handle,
            state_handler_handle,
            shutdown_monitor_handle,
        ];

        handles.extend(admin_handles);

        if let Some(h) = consensus_api_handle {
            handles.push(h);
        }

        handles
    }
}

pub struct PrimaryNetwork {
    pub network: Network,
    pub primary_receiver_controller: Arc<ArcSwapOption<PrimaryReceiverController>>,
    pub worker_receiver_controller: Arc<ArcSwapOption<WorkerReceiverController>>,
    pub connection_monitor_handle: JoinHandle<()>,
}

pub fn create_primary_networking(
    keypair: &KeyPair,
    network_signer: &NetworkKeyPair,
    committee: SharedCommittee,
    worker_cache: SharedWorkerCache,
    registry: &Registry,
) -> PrimaryNetwork {
    let name = keypair.public().clone();

    // The metrics used for communicating over the network
    let inbound_network_metrics = Arc::new(NetworkMetrics::new("primary", "inbound", registry));
    let outbound_network_metrics = Arc::new(NetworkMetrics::new("primary", "outbound", registry));

    // Network metrics for the primary connection
    let network_connection_metrics = NetworkConnectionMetrics::new("primary", registry);

    let address = committee
        .load()
        .primary(&name)
        .expect("Our public key or worker id is not in the committee");
    let address = address
        .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
        .unwrap();

    let primary_receiver = Arc::new(ArcSwapOption::new(None));
    let primary_service =
        PrimaryToPrimaryServer::new(PrimaryReceiverHandler::new(primary_receiver.clone()));

    let worker_receiver = Arc::new(ArcSwapOption::new(None));
    let worker_service =
        WorkerToPrimaryServer::new(WorkerReceiverHandler::new(worker_receiver.clone()));

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

    // update peer types
    let peer_types = update_network_peers(name, committee, &network, worker_cache);

    // now spin up the connection monitor handle
    // TODO: move this to a better place - now this can stay here as there is no meaning
    // to bring it down
    let connection_monitor_handle = network::connectivity::ConnectionMonitor::spawn(
        network.downgrade(),
        network_connection_metrics,
        peer_types,
    );

    PrimaryNetwork {
        network,
        primary_receiver_controller: primary_receiver,
        worker_receiver_controller: worker_receiver,
        connection_monitor_handle,
    }
}

/// Updates the peers in the provided Network. It also returns
/// a map of the peer_ids and their corresponding type.
fn update_network_peers(
    name: PublicKey,
    committee: SharedCommittee,
    network: &Network,
    worker_cache: SharedWorkerCache,
) -> HashMap<PeerId, String> {
    let mut peer_types = HashMap::new();

    // Add my workers
    for worker in worker_cache.load().our_workers(&name).unwrap() {
        let (peer_id, address) = add_peer_in_network(
            network,
            worker.name,
            worker
                .internal_worker_address
                .as_ref()
                .unwrap_or(&worker.worker_address),
        );
        peer_types.insert(peer_id, "our_worker".to_string());
        info!(
            "Adding our worker with peer id {} and address {}",
            peer_id, address
        );
    }

    // Add others workers
    for (_, worker) in worker_cache.load().others_workers(&name) {
        let (peer_id, address) = add_peer_in_network(network, worker.name, &worker.worker_address);
        peer_types.insert(peer_id, "other_worker".to_string());
        info!(
            "Adding others worker with peer id {} and address {}",
            peer_id, address
        );
    }

    // Add other primaries
    let primaries = committee
        .load()
        .others_primaries(&name)
        .into_iter()
        .map(|(_, address, network_key)| (network_key, address));

    for (public_key, address) in primaries {
        let (peer_id, address) = add_peer_in_network(network, public_key, &address);
        peer_types.insert(peer_id, "other_primary".to_string());
        info!(
            "Adding others primaries with peer id {} and address {}",
            peer_id, address
        );
    }

    peer_types
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
