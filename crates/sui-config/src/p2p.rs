// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, num::NonZeroU32, time::Duration};

use serde::{Deserialize, Serialize};
use sui_types::multiaddr::Multiaddr;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery: Option<DiscoveryConfig>,
    /// Size in bytes above which network messages are considered excessively large. Excessively
    /// large messages will still be handled, but logged and reported in metrics for debugging.
    ///
    /// If unspecified, this will default to 8 MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excessive_message_size: Option<usize>,
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
            discovery: None,
            excessive_message_size: None,
        }
    }
}

impl P2pConfig {
    pub fn excessive_message_size(&self) -> usize {
        const EXCESSIVE_MESSAGE_SIZE: usize = 32 << 20;

        self.excessive_message_size
            .unwrap_or(EXCESSIVE_MESSAGE_SIZE)
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

    /// Set the upper bound on the number of checkpoint contents to be downloaded concurrently.
    ///
    /// If unspecified, this will default to `100`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_download_concurrency: Option<usize>,

    /// Set the upper bound on the number of individual transactions contained in checkpoint
    /// contents to be downloaded concurrently. If both this value and
    /// `checkpoint_content_download_concurrency` are set, the lower of the two will apply.
    ///
    /// If unspecified, this will default to `50,000`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_download_tx_concurrency: Option<u64>,

    /// Set the timeout that should be used when sending most state-sync RPC requests.
    ///
    /// If unspecified, this will default to `10,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Set the timeout that should be used when sending RPC requests to sync checkpoint contents.
    ///
    /// If unspecified, this will default to `10,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_timeout_ms: Option<u64>,

    /// Per-peer rate-limit (in requests/sec) for the PushCheckpointSummary RPC.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_checkpoint_summary_rate_limit: Option<NonZeroU32>,

    /// Per-peer rate-limit (in requests/sec) for the GetCheckpointSummary RPC.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_checkpoint_summary_rate_limit: Option<NonZeroU32>,

    /// Per-peer rate-limit (in requests/sec) for the GetCheckpointContents RPC.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_checkpoint_contents_rate_limit: Option<NonZeroU32>,

    /// Per-peer inflight limit for the GetCheckpointContents RPC.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_checkpoint_contents_inflight_limit: Option<usize>,

    /// Per-checkpoint inflight limit for the GetCheckpointContents RPC. This is enforced globally
    /// across all peers.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_checkpoint_contents_per_checkpoint_limit: Option<usize>,
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

    pub fn checkpoint_content_download_concurrency(&self) -> usize {
        const CHECKPOINT_CONTENT_DOWNLOAD_CONCURRENCY: usize = 100;

        self.checkpoint_content_download_concurrency
            .unwrap_or(CHECKPOINT_CONTENT_DOWNLOAD_CONCURRENCY)
    }

    pub fn checkpoint_content_download_tx_concurrency(&self) -> u64 {
        const CHECKPOINT_CONTENT_DOWNLOAD_TX_CONCURRENCY: u64 = 50_000;

        self.checkpoint_content_download_tx_concurrency
            .unwrap_or(CHECKPOINT_CONTENT_DOWNLOAD_TX_CONCURRENCY)
    }

    pub fn timeout(&self) -> Duration {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

        self.timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT)
    }

    pub fn checkpoint_content_timeout(&self) -> Duration {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

        self.checkpoint_content_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DiscoveryConfig {
    /// Query peers for their latest checkpoint every interval period.
    ///
    /// If unspecified, this will default to `5,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_period_ms: Option<u64>,

    /// Target number of concurrent connections to establish.
    ///
    /// If unspecified, this will default to `4`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_concurrent_connections: Option<usize>,

    /// Number of peers to query each interval.
    ///
    /// Sets the number of peers, to be randomly selected, that are queried for their known peers
    /// each interval.
    ///
    /// If unspecified, this will default to `1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peers_to_query: Option<usize>,

    /// Per-peer rate-limit (in requests/sec) for the GetKnownPeers RPC.
    ///
    /// If unspecified, this will default to no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_known_peers_rate_limit: Option<NonZeroU32>,
}

impl DiscoveryConfig {
    pub fn interval_period(&self) -> Duration {
        const INTERVAL_PERIOD_MS: u64 = 5_000; // 5 seconds

        Duration::from_millis(self.interval_period_ms.unwrap_or(INTERVAL_PERIOD_MS))
    }

    pub fn target_concurrent_connections(&self) -> usize {
        const TARGET_CONCURRENT_CONNECTIONS: usize = 4;

        self.target_concurrent_connections
            .unwrap_or(TARGET_CONCURRENT_CONNECTIONS)
    }

    pub fn peers_to_query(&self) -> usize {
        const PEERS_TO_QUERY: usize = 1;

        self.peers_to_query.unwrap_or(PEERS_TO_QUERY)
    }
}
