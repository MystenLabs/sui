// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use super::{
    metrics::Metrics, server::Server, Handle, Randomness, RandomnessEventLoop, RandomnessMessage,
    RandomnessServer,
};
use anemo::codegen::InboundRequestLayer;
use anemo_tower::inflight_limit;
use sui_config::p2p::RandomnessConfig;
use sui_types::{committee::EpochId, crypto::RandomnessRound};
use tokio::sync::mpsc;

/// Randomness Service Builder.
pub struct Builder {
    config: Option<RandomnessConfig>,
    metrics: Option<Metrics>,
    randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
}

impl Builder {
    pub fn new(randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>) -> Self {
        Self {
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

    pub fn build(self) -> (UnstartedRandomness, RandomnessServer<impl Randomness>) {
        let Builder {
            config,
            metrics,
            randomness_tx,
        } = self;
        let config = config.unwrap_or_default();
        let metrics = metrics.unwrap_or_else(Metrics::disabled);
        let (sender, mailbox) = mpsc::channel(config.mailbox_capacity());
        let handle = Handle {
            sender: sender.clone(),
        };
        let server = Server {
            sender: sender.downgrade(),
        };
        // TODO-DNS add auth layer to reject requests from non-authority peers
        let randomness_server = RandomnessServer::new(server)
            .add_layer_for_send_partial_signatures(InboundRequestLayer::new(
                inflight_limit::InflightLimitLayer::new(
                    config.send_partial_signatures_inflight_limit(),
                    inflight_limit::WaitMode::ReturnError,
                ),
            ));

        (
            UnstartedRandomness {
                config,
                handle,
                mailbox,
                metrics,
                randomness_tx,
            },
            randomness_server,
        )
    }
}

/// Handle to an unstarted randomness network system
pub struct UnstartedRandomness {
    pub(super) config: RandomnessConfig,
    pub(super) handle: Handle,
    pub(super) mailbox: mpsc::Receiver<RandomnessMessage>,
    pub(super) metrics: Metrics,
    pub(super) randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
}

impl UnstartedRandomness {
    pub(super) fn build(self, network: anemo::Network) -> (RandomnessEventLoop, Handle) {
        let Self {
            config,
            handle,
            mailbox,
            metrics,
            randomness_tx,
        } = self;
        (
            RandomnessEventLoop {
                mailbox,
                network,
                randomness_tx,

                epoch: 0,
                authority_info: Arc::new(HashMap::new()),
                peer_share_counts: None,
                dkg_output: None,
                aggregation_threshold: 0,
                pending_tasks: BTreeMap::new(),
                send_tasks: BTreeMap::new(),
                received_partial_sigs: BTreeMap::new(),
                completed_sigs: BTreeMap::new(),
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
