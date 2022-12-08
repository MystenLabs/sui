// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, time::Duration};

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct P2pConfig {
    /// The address that the p2p network will bind on.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
    /// The external address other nodes can use to reach this node.
    /// This will be shared with other peers through the discovery service
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_address: Option<Multiaddr>,
    /// SeedPeers configured with a PeerId are preferred and the node will always try to ensure a
    /// connection is established with these nodes.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub seed_peers: Vec<SeedPeer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anemo_config: Option<anemo::Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_sync: Option<StateSyncConfig>,
}

fn default_listen_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            external_address: Default::default(),
            seed_peers: Default::default(),
            anemo_config: Default::default(),
            state_sync: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SeedPeer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<anemo::PeerId>,
    pub address: Multiaddr,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateSyncConfig {
    /// Query peers for their latest checkpoint every interval period.
    ///
    /// If unspecified, this will default to `5,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_period_ms: Option<u64>,

    /// Size of the StateSync actor's mailbox.
    ///
    /// If unspecified, this will default to `128`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_capacity: Option<usize>,

    /// Size of the broadcast channel use for notifying other systems of newly sync'ed checkpoints.
    ///
    /// If unspecified, this will default to `128`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_checkpoint_broadcast_channel_capacity: Option<usize>,

    /// Set the upper bound on the number of checkpoint headers to be downloaded concurrently.
    ///
    /// If unspecified, this will default to `100`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_header_download_concurrency: Option<usize>,

    /// Set the upper bound on the number of transactions to be downloaded concurrently from a
    /// single checkpoint.
    ///
    /// If unspecified, this will default to `100`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_download_concurrency: Option<usize>,
}

impl StateSyncConfig {
    pub fn interval_period(&self) -> Duration {
        const INTERVAL_PERIOD_MS: u64 = 5_000; // 5 seconds

        Duration::from_millis(self.interval_period_ms.unwrap_or(INTERVAL_PERIOD_MS))
    }

    pub fn mailbox_capacity(&self) -> usize {
        const MAILBOX_CAPACITY: usize = 128;

        self.mailbox_capacity.unwrap_or(MAILBOX_CAPACITY)
    }

    pub fn synced_checkpoint_broadcast_channel_capacity(&self) -> usize {
        const SYNCED_CHECKPOINT_BROADCAST_CHANNEL_CAPACITY: usize = 128;

        self.synced_checkpoint_broadcast_channel_capacity
            .unwrap_or(SYNCED_CHECKPOINT_BROADCAST_CHANNEL_CAPACITY)
    }

    pub fn checkpoint_header_download_concurrency(&self) -> usize {
        const CHECKPOINT_HEADER_DOWNLOAD_CONCURRENCY: usize = 100;

        self.checkpoint_header_download_concurrency
            .unwrap_or(CHECKPOINT_HEADER_DOWNLOAD_CONCURRENCY)
    }

    pub fn transaction_download_concurrency(&self) -> usize {
        const TRANSACTION_DOWNLOAD_CONCURRENCY: usize = 100;

        self.transaction_download_concurrency
            .unwrap_or(TRANSACTION_DOWNLOAD_CONCURRENCY)
    }
}
