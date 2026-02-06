// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    Discovery, DiscoveryEventLoop, DiscoveryMessage, DiscoveryServer, Handle, State,
    metrics::Metrics, server::Server,
};
use anemo::codegen::InboundRequestLayer;
use anemo::types::PeerAffinity;
use anemo::{PeerId, types::PeerInfo};
use anemo_tower::rate_limit;
use fastcrypto::traits::KeyPair;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};
use sui_config::p2p::P2pConfig;
use sui_types::crypto::NetworkKeyPair;
use tap::{Pipe, TapFallible};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use tracing::warn;

/// Discovery Service Builder.
pub struct Builder {
    config: Option<P2pConfig>,
    metrics: Option<Metrics>,
}

impl Builder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            config: None,
            metrics: None,
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
            discovery_server = discovery_server.add_layer_for_get_known_peers_v3(
                InboundRequestLayer::new(rate_limit::RateLimitLayer::new(
                    governor::Quota::per_second(limit),
                    rate_limit::WaitMode::Block,
                )),
            );
        }
        (builder, discovery_server)
    }

    pub(super) fn build_internal(self) -> (UnstartedDiscovery, Server) {
        let Builder { config, metrics } = self;
        let config = config.unwrap();
        let discovery_config = config.discovery.clone().unwrap_or_default();
        let metrics = metrics.unwrap_or_else(Metrics::disabled);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (mailbox_tx, mailbox_rx) = mpsc::channel(discovery_config.mailbox_capacity());

        let handle = Handle {
            _shutdown_handle: Arc::new(shutdown_tx),
            sender: mailbox_tx.clone(),
        };

        let state = State {
            our_info: None,
            our_info_v2: None,
            connected_peers: HashMap::default(),
            known_peers: HashMap::default(),
            known_peers_v2: HashMap::default(),
            peer_address_overrides: HashMap::default(),
        }
        .pipe(RwLock::new)
        .pipe(Arc::new);

        let configured_peers = Arc::new(OnceLock::new());

        let server = Server {
            state: state.clone(),
            configured_peers: configured_peers.clone(),
            mailbox_sender: mailbox_tx.clone(),
        };

        (
            UnstartedDiscovery {
                handle,
                config,
                shutdown_handle: shutdown_rx,
                state,
                mailbox: mailbox_rx,
                metrics,
                configured_peers,
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
    pub(super) mailbox: mpsc::Receiver<DiscoveryMessage>,
    pub(super) metrics: Metrics,
    pub(super) configured_peers: Arc<OnceLock<HashMap<PeerId, PeerInfo>>>,
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
            mailbox,
            metrics,
            configured_peers,
        } = self;

        let discovery_config = config.discovery.clone().unwrap_or_default();
        let (built_configured_peers, unidentified_seed_peers) =
            build_peer_config(&config, &discovery_config);

        // Populate the shared configured_peers for the Server.
        configured_peers
            .set(built_configured_peers.clone())
            .expect("configured_peers should only be set once");

        (
            DiscoveryEventLoop {
                config,
                discovery_config: Arc::new(discovery_config),
                configured_peers: Arc::new(built_configured_peers),
                unidentified_seed_peers,
                network,
                keypair,
                tasks: JoinSet::new(),
                pending_dials: Default::default(),
                dial_seed_peers_task: None,
                shutdown_handle,
                state,
                mailbox,
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

/// Returns (configured_peers, unidentified_seed_peers).
fn build_peer_config(
    config: &P2pConfig,
    discovery_config: &sui_config::p2p::DiscoveryConfig,
) -> (HashMap<PeerId, PeerInfo>, Vec<anemo::types::Address>) {
    let mut configured_peers = HashMap::new();
    let mut unidentified_seed_peers = Vec::new();

    for seed in &config.seed_peers {
        let anemo_addr = seed
            .address
            .to_anemo_address()
            .tap_err(|_| warn!(p2p_address=?seed.address, "Skipping seed peer address: can't convert to anemo address"))
            .ok();
        match (seed.peer_id, anemo_addr) {
            (Some(peer_id), addr) => {
                configured_peers.insert(
                    peer_id,
                    PeerInfo {
                        peer_id,
                        affinity: PeerAffinity::High,
                        address: addr.into_iter().collect(),
                    },
                );
            }
            (None, Some(addr)) => {
                unidentified_seed_peers.push(addr);
            }
            (None, None) => {}
        }
    }

    for ap in &discovery_config.allowlisted_peers {
        let addresses = ap
            .address
            .iter()
            .filter_map(|addr| {
                addr.to_anemo_address()
                    .tap_err(|_| warn!(p2p_address=?addr, "Skipping allowlisted peer address: can't convert to anemo address"))
                    .ok()
            })
            .collect();
        configured_peers.insert(
            ap.peer_id,
            PeerInfo {
                peer_id: ap.peer_id,
                affinity: PeerAffinity::Allowed,
                address: addresses,
            },
        );
    }

    (configured_peers, unidentified_seed_peers)
}
