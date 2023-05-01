// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    batch_fetcher::BatchFetcher,
    batch_maker::BatchMaker,
    handlers::{PrimaryReceiverHandler, WorkerReceiverHandler},
    metrics::WorkerChannelMetrics,
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
use config::{Authority, AuthorityIdentifier, Committee, Parameters, WorkerCache, WorkerId};
use crypto::{traits::KeyPair as _, NetworkKeyPair, NetworkPublicKey};
use mysten_metrics::spawn_logged_monitored_task;
use mysten_network::{multiaddr::Protocol, Multiaddr};
use network::client::NetworkClient;
use network::epoch_filter::{AllowedEpoch, EPOCH_HEADER_KEY};
use network::failpoints::FailpointsMakeCallbackHandler;
use network::metrics::MetricsMakeCallbackHandler;
use std::collections::HashMap;
use std::time::Duration;
use std::{net::Ipv4Addr, sync::Arc, thread::sleep};
use store::rocks::DBMap;
use tap::TapFallible;
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tracing::{error, info};
use types::{
    metered_channel::channel_with_total, Batch, BatchDigest, ConditionalBroadcastReceiver,
    PreSubscribedBroadcastSender, PrimaryToWorkerServer, WorkerToWorkerServer,
};

#[cfg(test)]
#[path = "tests/worker_tests.rs"]
pub mod worker_tests;

/// The default channel capacity for each channel of the worker.
pub const CHANNEL_CAPACITY: usize = 1_000;

use crate::metrics::{Metrics, WorkerEndpointMetrics, WorkerMetrics};
use crate::transactions_server::TxServer;

pub struct Worker {
    /// This authority.
    authority: Authority,
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
    store: DBMap<BatchDigest, Batch>,
}

impl Worker {
    pub fn spawn(
        authority: Authority,
        keypair: NetworkKeyPair,
        id: WorkerId,
        committee: Committee,
        worker_cache: WorkerCache,
        parameters: Parameters,
        validator: impl TransactionValidator,
        client: NetworkClient,
        store: DBMap<BatchDigest, Batch>,
        metrics: Metrics,
        tx_shutdown: &mut PreSubscribedBroadcastSender,
    ) -> Vec<JoinHandle<()>> {
        let worker_name = keypair.public().clone();
        let worker_peer_id = PeerId(worker_name.0.to_bytes());
        info!("Boot worker node with id {} peer id {}", id, worker_peer_id,);

        // Define a worker instance.
        let worker = Self {
            authority: authority.clone(),
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

        let mut shutdown_receivers = tx_shutdown.subscribe_n(NUM_SHUTDOWN_RECEIVERS);

        let mut worker_service = WorkerToWorkerServer::new(WorkerReceiverHandler {
            id: worker.id,
            client: client.clone(),
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

        // Legacy RPC interface, only used by delete_batches() for external consensus.
        let primary_service = PrimaryToWorkerServer::new(PrimaryReceiverHandler {
            authority_id: worker.authority.id(),
            id: worker.id,
            committee: worker.committee.clone(),
            worker_cache: worker.worker_cache.clone(),
            store: worker.store.clone(),
            request_batch_timeout: worker.parameters.sync_retry_delay,
            request_batch_retry_nodes: worker.parameters.sync_retry_nodes,
            network: None,
            batch_fetcher: None,
            validator: validator.clone(),
        });

        // Receive incoming messages from other workers.
        let address = worker
            .worker_cache
            .worker(authority.protocol_key(), &id)
            .expect("Our public key or worker id is not in the worker cache")
            .worker_address;
        let address = address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap();
        let addr = address.to_anemo_address().unwrap();

        let epoch_string: String = committee.epoch().to_string();

        // Set up anemo Network.
        let our_primary_peer_id = PeerId(authority.network_key().0.to_bytes());
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
            // Allow more concurrent streams for burst activity.
            quic_config.max_concurrent_bidi_streams = Some(10_000);
            // Increase send and receive buffer sizes on the worker, since the worker is
            // responsible for broadcasting and fetching payloads.
            // With 200MiB buffer size and ~500ms RTT, the max throughput ~400MiB.
            quic_config.stream_receive_window = Some(100 << 20);
            quic_config.receive_window = Some(200 << 20);
            quic_config.send_window = Some(200 << 20);
            quic_config.crypto_buffer_size = Some(1 << 20);
            quic_config.max_idle_timeout_ms = Some(30_000);
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

        let batch_fetcher = BatchFetcher::new(
            worker_name,
            network.clone(),
            worker.store.clone(),
            node_metrics.clone(),
        );
        client.set_primary_to_worker_local_handler(
            worker_peer_id,
            Arc::new(PrimaryReceiverHandler {
                authority_id: worker.authority.id(),
                id: worker.id,
                committee: worker.committee.clone(),
                worker_cache: worker.worker_cache.clone(),
                store: worker.store.clone(),
                request_batch_timeout: worker.parameters.sync_retry_delay,
                request_batch_retry_nodes: worker.parameters.sync_retry_nodes,
                network: Some(network.clone()),
                batch_fetcher: Some(batch_fetcher),
                validator: validator.clone(),
            }),
        );

        let mut peer_types = HashMap::new();

        let other_workers = worker
            .worker_cache
            .others_workers_by_id(authority.protocol_key(), &id)
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
        let (peer_id, address) = Self::add_peer_in_network(
            &network,
            authority.network_key(),
            &authority.primary_address(),
        );
        peer_types.insert(peer_id, "our_primary".to_string());
        info!(
            "Adding our primary with peer id {} and address {}",
            peer_id, address
        );

        // update the peer_types with the "other_primary". We do not add them in the Network
        // struct, otherwise the networking library will try to connect to it
        let other_primaries: Vec<(AuthorityIdentifier, Multiaddr, NetworkPublicKey)> =
            committee.others_primaries_by_id(authority.id());
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
            Some(shutdown_receivers.pop().unwrap()),
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

        let client_flow_handles = worker.handle_clients_transactions(
            vec![
                shutdown_receivers.pop().unwrap(),
                shutdown_receivers.pop().unwrap(),
                shutdown_receivers.pop().unwrap(),
            ],
            node_metrics,
            channel_metrics,
            endpoint_metrics,
            validator,
            client,
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
                .worker(authority.protocol_key(), &worker.id)
                .expect("Our public key or worker id is not in the worker cache")
                .transactions
        );

        let mut handles = vec![connection_monitor_handle, network_shutdown_handle];
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
        let address = address.to_anemo_address().unwrap();
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
        node_metrics: Arc<WorkerMetrics>,
        channel_metrics: Arc<WorkerChannelMetrics>,
        endpoint_metrics: WorkerEndpointMetrics,
        validator: impl TransactionValidator,
        client: NetworkClient,
        network: anemo::Network,
    ) -> Vec<JoinHandle<()>> {
        info!("Starting handler for transactions");

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
            .worker(self.authority.protocol_key(), &self.id)
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
            client,
            self.store.clone(),
        );

        // The `QuorumWaiter` waits for 2f authorities to acknowledge reception of the batch. It then forwards
        // the batch to the `Processor`.
        let quorum_waiter_handle = QuorumWaiter::spawn(
            self.authority.clone(),
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
