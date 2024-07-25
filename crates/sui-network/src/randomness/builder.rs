// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use super::{
    auth::AllowedPeersUpdatable, metrics::Metrics, server::Server, Handle, RandomnessEventLoop,
    RandomnessMessage, RandomnessServer,
};
use anemo::codegen::InboundRequestLayer;
use anemo_tower::{auth::RequireAuthorizationLayer, inflight_limit};
use sui_config::p2p::RandomnessConfig;
use sui_types::{base_types::AuthorityName, committee::EpochId, crypto::RandomnessRound};
use tokio::sync::mpsc;

/// Randomness Service Builder.
pub struct Builder {
    name: AuthorityName,
    config: Option<RandomnessConfig>,
    metrics: Option<Metrics>,
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
}

impl Builder {
    pub fn new(
        name: AuthorityName,
        randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
    ) -> Self {
        Self {
            name,
            config: None,
            metrics: None,
            randomness_tx,
        }
    }

    pub fn config(mut self, config: RandomnessConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_metrics(mut self, registry: &prometheus::Registry) -> Self {
        self.metrics = Some(Metrics::enabled(registry));
        self
    }

    pub fn build(self) -> (UnstartedRandomness, anemo::Router) {
        let Builder {
            name,
            config,
            metrics,
            randomness_tx,
        } = self;
        let config = config.unwrap_or_default();
        let metrics = metrics.unwrap_or_else(Metrics::disabled);
        let (sender, mailbox) = mpsc::channel(config.mailbox_capacity());
        let mailbox_sender = sender.downgrade();
        let handle = Handle {
            sender: sender.clone(),
        };
        let server = Server {
            sender: sender.downgrade(),
        };
        let randomness_server = RandomnessServer::new(server).add_layer_for_send_signatures(
            InboundRequestLayer::new(inflight_limit::InflightLimitLayer::new(
                config.send_partial_signatures_inflight_limit(),
                inflight_limit::WaitMode::ReturnError,
            )),
        );

        let allowed_peers = AllowedPeersUpdatable::new(Arc::new(HashSet::new()));
        let router = anemo::Router::new()
            .route_layer(RequireAuthorizationLayer::new(allowed_peers.clone()))
            .add_rpc_service(randomness_server);

        (
            UnstartedRandomness {
                name,
                config,
                handle,
                mailbox,
                mailbox_sender,
                allowed_peers,
                metrics,
                randomness_tx,
            },
            router,
        )
    }
}

/// Handle to an unstarted randomness network system
pub struct UnstartedRandomness {
    pub(super) name: AuthorityName,
    pub(super) config: RandomnessConfig,
    pub(super) handle: Handle,
    pub(super) mailbox: mpsc::Receiver<RandomnessMessage>,
    pub(super) mailbox_sender: mpsc::WeakSender<RandomnessMessage>,
    pub(super) allowed_peers: AllowedPeersUpdatable,
    pub(super) metrics: Metrics,
    pub(super) randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
}

impl UnstartedRandomness {
    pub(super) fn build(self, network: anemo::Network) -> (RandomnessEventLoop, Handle) {
        let Self {
            name,
            config,
            handle,
            mailbox,
            mailbox_sender,
            allowed_peers,
            metrics,
            randomness_tx,
        } = self;
        (
            RandomnessEventLoop {
                name,
                config,
                mailbox,
                mailbox_sender,
                network,
                allowed_peers,
                allowed_peers_set: HashSet::new(),
                metrics,
                randomness_tx,

                epoch: 0,
                authority_info: Arc::new(HashMap::new()),
                peer_share_ids: None,
                blocked_share_id_count: 0,
                dkg_output: None,
                aggregation_threshold: 0,
                highest_requested_round: BTreeMap::new(),
                send_tasks: BTreeMap::new(),
                round_request_time: BTreeMap::new(),
                future_epoch_partial_sigs: BTreeMap::new(),
                received_partial_sigs: BTreeMap::new(),
                completed_sigs: BTreeMap::new(),
                highest_completed_round: BTreeMap::new(),
            },
            handle,
        )
    }

    pub fn start(self, network: anemo::Network) -> Handle {
        let (event_loop, handle) = self.build(network);
        tokio::spawn(event_loop.start());

        handle
    }
}
