// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_maker::BatchMaker,
    handlers::{PrimaryReceiverHandler, WorkerReceiverHandler},
    metrics::WorkerChannelMetrics,
    primary_connector::PrimaryConnector,
    quorum_waiter::QuorumWaiter,
    TransactionValidator, NUM_SHUTDOWN_RECEIVERS,
};
use anemo::{codegen::InboundRequestLayer, types::Address};
use anemo::{types::PeerInfo, Network, PeerId};
use anemo_tower::{
    auth::{AllowedPeers, RequireAuthorizationLayer},
    callback::CallbackLayer,
    set_header::SetRequestHeaderLayer,
    trace::{DefaultMakeSpan, DefaultOnFailure, TraceLayer},
};
use anemo_tower::{rate_limit, set_header::SetResponseHeaderLayer};
use config::{Committee, Parameters, WorkerCache, WorkerId};
use crypto::{traits::KeyPair as _, NetworkKeyPair, NetworkPublicKey, PublicKey};
use multiaddr::{Multiaddr, Protocol};
use mysten_metrics::spawn_logged_monitored_task;
use network::epoch_filter::{AllowedEpoch, EPOCH_HEADER_KEY};
use network::failpoints::FailpointsMakeCallbackHandler;
use network::metrics::MetricsMakeCallbackHandler;
use std::collections::HashMap;
use std::time::Duration;
use std::{net::Ipv4Addr, sync::Arc, thread::sleep};
use store::Store;
use tap::TapFallible;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tracing::{error, info};
use types::{
    metered_channel::{channel_with_total, Sender},
    Batch, BatchDigest, ConditionalBroadcastReceiver, PreSubscribedBroadcastSender,
    PrimaryToWorkerServer, WorkerOurBatchMessage, WorkerToWorkerServer,
};

#[cfg(test)]
#[path = "tests/worker_tests.rs"]
pub mod worker_tests;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

use crate::metrics::{Metrics, WorkerEndpointMetrics, WorkerMetrics};
use crate::transactions_server::TxServer;

pub struct Worker {
    /// The public key of this authority.
    primary_name: PublicKey,
    // The private-public key pair of this worker.
    keypair: NetworkKeyPair,
    /// The id of this worker used for index-based lookup by other NW nodes.
    id: WorkerId,
    /// The committee information.
    committee: Committee,
    /// The worker information cache.
    worker_cache: WorkerCache,
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
        committee: Committee,
        worker_cache: WorkerCache,
        parameters: Parameters,
        validator: impl TransactionValidator,
        store: Store<BatchDigest, Batch>,
        metrics: Metrics,
        tx_shutdown: &mut PreSubscribedBroadcastSender,
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

        let mut shutdown_receivers = tx_shutdown.subscribe_n(NUM_SHUTDOWN_RECEIVERS);

        let mut worker_service = WorkerToWorkerServer::new(WorkerReceiverHandler {
            id: worker.id,
            tx_others_batch,
            store: worker.store.clone(),
            validator: validator.clone(),
        });
        // Apply rate limits from configuration as needed.
        if let Some(limit) = parameters.anemo.report_batch_rate_limit {
            worker_service = worker_service.add_layer_for_report_batch(InboundRequestLayer::new(
                rate_limit::RateLimitLayer::new(
                    governor::Quota::per_second(limit),
                    rate_limit::WaitMode::Block,
                ),
            ));
        }
        if let Some(limit) = parameters.anemo.request_batch_rate_limit {
            worker_service = worker_service.add_layer_for_request_batch(InboundRequestLayer::new(
                rate_limit::RateLimitLayer::new(
                    governor::Quota::per_second(limit),
                    rate_limit::WaitMode::Block,
                ),
            ));
        }

        let primary_service = PrimaryToWorkerServer::new(PrimaryReceiverHandler {
            name: worker.primary_name.clone(),
            id: worker.id,
            committee: worker.committee.clone(),
            worker_cache: worker.worker_cache.clone(),
            store: worker.store.clone(),
            request_batch_timeout: worker.parameters.sync_retry_delay,
            request_batch_retry_nodes: worker.parameters.sync_retry_nodes,
            validator: validator.clone(),
        });

        // Receive incoming messages from other workers.
        let address = worker
            .worker_cache
            .worker(&primary_name, &id)
            .expect("Our public key or worker id is not in the worker cache")
            .worker_address;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let addr = network::multiaddr_to_address(&address).unwrap();

        let epoch_string: String = committee.epoch.to_string();

        // Set up anemo Network.
        let our_primary_peer_id = committee
            .network_key(&primary_name)
            .map(|public_key| PeerId(public_key.0.to_bytes()))
            .unwrap();
        let primary_to_worker_router = anemo::Router::new()
            .add_rpc_service(primary_service)
            // Add an Authorization Layer to ensure that we only service requests from our primary
            .route_layer(RequireAuthorizationLayer::new(AllowedPeers::new([
                our_primary_peer_id,
            ])))
            .route_layer(RequireAuthorizationLayer::new(AllowedEpoch::new(
                epoch_string.clone(),
            )));

        let routes = anemo::Router::new()
            .add_rpc_service(worker_service)
            .route_layer(RequireAuthorizationLayer::new(AllowedEpoch::new(
                epoch_string.clone(),
            )))
            .merge(primary_to_worker_router);

        let service = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                inbound_network_metrics,
                parameters.anemo.excessive_message_size(),
            )))
            .layer(CallbackLayer::new(FailpointsMakeCallbackHandler::new()))
            .layer(SetResponseHeaderLayer::overriding(
                EPOCH_HEADER_KEY.parse().unwrap(),
                epoch_string.clone(),
            ))
            .service(routes);

        let outbound_layer = ServiceBuilder::new()
            .layer(
                TraceLayer::new_for_client_and_server_errors()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                    .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
            )
            .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                outbound_network_metrics,
                parameters.anemo.excessive_message_size(),
            )))
            .layer(CallbackLayer::new(FailpointsMakeCallbackHandler::new()))
            .layer(SetRequestHeaderLayer::overriding(
                EPOCH_HEADER_KEY.parse().unwrap(),
                epoch_string,
            ))
            .into_inner();

        let anemo_config = {
            let mut quic_config = anemo::QuicConfig::default();
            // Enable keep alives every 5s
            quic_config.keep_alive_interval_ms = Some(5_000);
            let mut config = anemo::Config::default();
            config.quic = Some(quic_config);
            // Set the max_frame_size to be 2 GB to work around the issue of there being too many
            // delegation events in the epoch change txn.
            config.max_frame_size = Some(2 << 30);
            // Set a default timeout of 300s for all RPC requests
            config.inbound_request_timeout_ms = Some(300_000);
            config.outbound_request_timeout_ms = Some(300_000);
            config.shutdown_idle_timeout_ms = Some(1_000);
            config.connectivity_check_interval_ms = Some(2_000);
            config.connection_backoff_ms = Some(1_000);
            config.max_connection_backoff_ms = Some(20_000);
            config
        };

        let network;
        let mut retries_left = 90;

        loop {
            let network_result = anemo::Network::bind(addr.clone())
                .server_name("narwhal")
                .private_key(worker.keypair.copy().private().0.to_bytes())
                .config(anemo_config.clone())
                .outbound_request_layer(outbound_layer.clone())
                .start(service.clone());
            match network_result {
                Ok(n) => {
                    network = n;
                    break;
                }
                Err(_) => {
                    retries_left -= 1;

                    if retries_left <= 0 {
                        panic!();
                    }
                    error!(
                        "Address {} should be available for the primary Narwhal service, retrying in one second",
                        addr
                    );
                    sleep(Duration::from_secs(1));
                }
            }
        }

        info!("Worker {} listening to worker messages on {}", id, address);

        let mut peer_types = HashMap::new();

        let other_workers = worker
            .worker_cache
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
            .primary(&primary_name)
            .expect("Our primary is not in the committee");

        let primary_network_key = committee
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
            committee.others_primaries(&primary_name);
        for (_, _, network_key) in other_primaries {
            peer_types.insert(
                PeerId(network_key.0.to_bytes()),
                "other_primary".to_string(),
            );
        }

        let (connection_monitor_handle, _) = network::connectivity::ConnectionMonitor::spawn(
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
            shutdown_receivers.pop().unwrap(),
        );

        let primary_connector_handle = PrimaryConnector::spawn(
            primary_network_key,
            shutdown_receivers.pop().unwrap(),
            rx_our_batch,
            rx_others_batch,
            network.clone(),
        );
        let client_flow_handles = worker.handle_clients_transactions(
            vec![
                shutdown_receivers.pop().unwrap(),
                shutdown_receivers.pop().unwrap(),
                shutdown_receivers.pop().unwrap(),
            ],
            tx_our_batch,
            node_metrics,
            channel_metrics,
            endpoint_metrics,
            validator,
            network.clone(),
        );

        let network_shutdown_handle =
            Self::shutdown_network_listener(shutdown_receivers.pop().unwrap(), network);

        // NOTE: This log entry is used to compute performance.
        info!(
            "Worker {} successfully booted on {}",
            id,
            worker
                .worker_cache
                .worker(&worker.primary_name, &worker.id)
                .expect("Our public key or worker id is not in the worker cache")
                .transactions
        );

        let mut handles = vec![
            primary_connector_handle,
            connection_monitor_handle,
            network_shutdown_handle,
        ];
        handles.extend(admin_handles);
        handles.extend(client_flow_handles);
        handles
    }

    // Spawns a task responsible for explicitly shutting down the network
    // when a shutdown signal has been sent to the node.
    fn shutdown_network_listener(
        mut rx_shutdown: ConditionalBroadcastReceiver,
        network: Network,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                match rx_shutdown.receiver.recv().await {
                    Ok(()) | Err(_) => {
                        let _ = network
                            .shutdown()
                            .await
                            .tap_err(|err| error!("Error while shutting down network: {err}"));
                        info!("Worker network server shutdown");
                    }
                }
            },
            "WorkerShutdownNetworkListenerTask"
        )
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
        mut shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
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
            .worker(&self.primary_name, &self.id)
            .expect("Our public key or worker id is not in the worker cache")
            .transactions;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();

        let tx_server_handle = TxServer::spawn(
            address.clone(),
            shutdown_receivers.pop().unwrap(),
            endpoint_metrics,
            tx_batch_maker,
            validator,
        );

        // The transactions are sent to the `BatchMaker` that assembles them into batches. It then broadcasts
        // (in a reliable manner) the batches to all other workers that share the same `id` as us. Finally, it
        // gathers the 'cancel handlers' of the messages and send them to the `QuorumWaiter`.
        let batch_maker_handle = BatchMaker::spawn(
            self.id,
            self.parameters.batch_size,
            self.parameters.max_batch_delay,
            shutdown_receivers.pop().unwrap(),
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
            self.committee.clone(),
            self.worker_cache.clone(),
            shutdown_receivers.pop().unwrap(),
            rx_quorum_waiter,
            network,
        );

        info!(
            "Worker {} listening to client transactions on {}",
            self.id, address
        );

        vec![batch_maker_handle, quorum_waiter_handle, tx_server_handle]
    }
}
