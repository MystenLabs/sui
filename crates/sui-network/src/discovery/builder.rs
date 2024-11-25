// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    metrics::Metrics, server::Server, Discovery, DiscoveryEventLoop, DiscoveryServer, State,
};
use crate::discovery::TrustedPeerChangeEvent;
use anemo::codegen::InboundRequestLayer;
use anemo_tower::rate_limit;
use fastcrypto::traits::KeyPair;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use sui_config::p2p::P2pConfig;
use sui_types::crypto::NetworkKeyPair;
use tap::Pipe;
use tokio::{
    sync::{oneshot, watch},
    task::JoinSet,
};

/// Discovery Service Builder.
pub struct Builder {
    config: Option<P2pConfig>,
    metrics: Option<Metrics>,
    trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>,
}

impl Builder {
    #[allow(clippy::new_without_default)]
    pub fn new(trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>) -> Self {
        Self {
            config: None,
            metrics: None,
            trusted_peer_change_rx,
        }
    }

    pub fn config(mut self, config: P2pConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_metrics(mut self, registry: &prometheus::Registry) -> Self {
        self.metrics = Some(Metrics::enabled(registry));
        self
    }

    pub fn build(self) -> (UnstartedDiscovery, DiscoveryServer<impl Discovery>) {
        let discovery_config = self
            .config
            .clone()
            .and_then(|config| config.discovery)
            .unwrap_or_default();
        let (builder, server) = self.build_internal();
        let mut discovery_server = DiscoveryServer::new(server);

        // Apply rate limits from configuration as needed.
        if let Some(limit) = discovery_config.get_known_peers_rate_limit {
            discovery_server = discovery_server.add_layer_for_get_known_peers_v2(
                InboundRequestLayer::new(rate_limit::RateLimitLayer::new(
                    governor::Quota::per_second(limit),
                    rate_limit::WaitMode::Block,
                )),
            );
        }
        (builder, discovery_server)
    }

    pub(super) fn build_internal(self) -> (UnstartedDiscovery, Server) {
        let Builder {
            config,
            metrics,
            trusted_peer_change_rx,
        } = self;
        let config = config.unwrap();
        let metrics = metrics.unwrap_or_else(Metrics::disabled);
        let (sender, receiver) = oneshot::channel();

        let handle = Handle {
            _shutdown_handle: Arc::new(sender),
        };

        let state = State {
            our_info: None,
            connected_peers: HashMap::default(),
            known_peers: HashMap::default(),
        }
        .pipe(RwLock::new)
        .pipe(Arc::new);

        let server = Server {
            state: state.clone(),
        };

        (
            UnstartedDiscovery {
                handle,
                config,
                shutdown_handle: receiver,
                state,
                trusted_peer_change_rx,
                metrics,
            },
            server,
        )
    }
}

/// Handle to an unstarted discovery system
pub struct UnstartedDiscovery {
    pub(super) handle: Handle,
    pub(super) config: P2pConfig,
    pub(super) shutdown_handle: oneshot::Receiver<()>,
    pub(super) state: Arc<RwLock<State>>,
    pub(super) trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>,
    pub(super) metrics: Metrics,
}

impl UnstartedDiscovery {
    pub(super) fn build(
        self,
        network: anemo::Network,
        keypair: NetworkKeyPair,
    ) -> (DiscoveryEventLoop, Handle) {
        let Self {
            handle,
            config,
            shutdown_handle,
            state,
            trusted_peer_change_rx,
            metrics,
        } = self;

        let discovery_config = config.discovery.clone().unwrap_or_default();
        let allowlisted_peers = Arc::new(
            discovery_config
                .allowlisted_peers
                .clone()
                .into_iter()
                .map(|ap| (ap.peer_id, ap.address))
                .chain(config.seed_peers.iter().filter_map(|peer| {
                    peer.peer_id
                        .map(|peer_id| (peer_id, Some(peer.address.clone())))
                }))
                .collect::<HashMap<_, _>>(),
        );
        (
            DiscoveryEventLoop {
                config,
                discovery_config: Arc::new(discovery_config),
                allowlisted_peers,
                network,
                keypair,
                tasks: JoinSet::new(),
                pending_dials: Default::default(),
                dial_seed_peers_task: None,
                shutdown_handle,
                state,
                trusted_peer_change_rx,
                metrics,
            },
            handle,
        )
    }

    pub fn start(self, network: anemo::Network, keypair: NetworkKeyPair) -> Handle {
        assert_eq!(network.peer_id().0, *keypair.public().0.as_bytes());
        let (event_loop, handle) = self.build(network, keypair);
        tokio::spawn(event_loop.start());

        handle
    }
}

/// A Handle to the Discovery subsystem. The Discovery system will be shutdown once its Handle has
/// been dropped.
pub struct Handle {
    _shutdown_handle: Arc<oneshot::Sender<()>>,
}
