// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, num::NonZeroU32, time::Duration};

use serde::{Deserialize, Serialize};
use sui_types::{
    messages_checkpoint::{CheckpointDigest, CheckpointSequenceNumber},
    multiaddr::Multiaddr,
};

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
    /// SeedPeers are preferred and the node will always try to ensure a
    /// connection is established with these nodes.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub seed_peers: Vec<SeedPeer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anemo_config: Option<anemo::Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_sync: Option<StateSyncConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery: Option<DiscoveryConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub randomness: Option<RandomnessConfig>,
    /// Size in bytes above which network messages are considered excessively large. Excessively
    /// large messages will still be handled, but logged and reported in metrics for debugging.
    ///
    /// If unspecified, this will default to 8 MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excessive_message_size: Option<usize>,
}

fn default_listen_address() -> SocketAddr {
    "0.0.0.0:8084".parse().unwrap()
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
            randomness: None,
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

    pub fn set_discovery_config(mut self, discovery_config: DiscoveryConfig) -> Self {
        self.discovery = Some(discovery_config);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SeedPeer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<anemo::PeerId>,
    pub address: Multiaddr,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AllowlistedPeer {
    pub peer_id: anemo::PeerId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Multiaddr>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StateSyncConfig {
    /// List of "known-good" checkpoints that state sync will be forced to use. State sync will
    /// skip verification of pinned checkpoints, and reject checkpoints with digests that don't
    /// match pinned values for a given sequence number.
    ///
    /// This can be used:
    /// - in case of a fork, to prevent the node from syncing to the wrong chain.
    /// - in case of a network stall, to force the node to proceed with a manually-injected
    ///   checkpoint.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pinned_checkpoints: Vec<(CheckpointSequenceNumber, CheckpointDigest)>,

    /// Query peers for their latest checkpoint every interval period.
    ///
    /// If unspecified, this will default to `5,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_period_ms: Option<u64>,

    /// Size of the StateSync actor's mailbox.
    ///
    /// If unspecified, this will default to `1,024`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_capacity: Option<usize>,

    /// Size of the broadcast channel use for notifying other systems of newly sync'ed checkpoints.
    ///
    /// If unspecified, this will default to `1,024`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_checkpoint_broadcast_channel_capacity: Option<usize>,

    /// Set the upper bound on the number of checkpoint headers to be downloaded concurrently.
    ///
    /// If unspecified, this will default to `400`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_header_download_concurrency: Option<usize>,

    /// Set the upper bound on the number of checkpoint contents to be downloaded concurrently.
    ///
    /// If unspecified, this will default to `400`.
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

    /// The amount of time to wait before retry if there are no peers to sync content from.
    /// If unspecified, this will set to default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_interval_when_no_peer_to_sync_content_ms: Option<u64>,
}

impl StateSyncConfig {
    pub fn interval_period(&self) -> Duration {
        const INTERVAL_PERIOD_MS: u64 = 5_000; // 5 seconds

        Duration::from_millis(self.interval_period_ms.unwrap_or(INTERVAL_PERIOD_MS))
    }

    pub fn mailbox_capacity(&self) -> usize {
        const MAILBOX_CAPACITY: usize = 1_024;

        self.mailbox_capacity.unwrap_or(MAILBOX_CAPACITY)
    }

    pub fn synced_checkpoint_broadcast_channel_capacity(&self) -> usize {
        const SYNCED_CHECKPOINT_BROADCAST_CHANNEL_CAPACITY: usize = 1_024;

        self.synced_checkpoint_broadcast_channel_capacity
            .unwrap_or(SYNCED_CHECKPOINT_BROADCAST_CHANNEL_CAPACITY)
    }

    pub fn checkpoint_header_download_concurrency(&self) -> usize {
        const CHECKPOINT_HEADER_DOWNLOAD_CONCURRENCY: usize = 400;

        self.checkpoint_header_download_concurrency
            .unwrap_or(CHECKPOINT_HEADER_DOWNLOAD_CONCURRENCY)
    }

    pub fn checkpoint_content_download_concurrency(&self) -> usize {
        const CHECKPOINT_CONTENT_DOWNLOAD_CONCURRENCY: usize = 400;

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

    pub fn wait_interval_when_no_peer_to_sync_content(&self) -> Duration {
        self.wait_interval_when_no_peer_to_sync_content_ms
            .map(Duration::from_millis)
            .unwrap_or(self.default_wait_interval_when_no_peer_to_sync_content())
    }

    fn default_wait_interval_when_no_peer_to_sync_content(&self) -> Duration {
        if cfg!(msim) {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(10)
        }
    }
}

/// Access Type of a node.
/// AccessType info is shared in the discovery process.
/// * If the node marks itself as Public, other nodes may try to connect to it.
/// * If the node marks itself as Private, only nodes that have it in
///     their `allowlisted_peers` or `seed_peers` will try to connect to it.
/// * If not set, defaults to Public.
///
/// AccessType is useful when a network of nodes want to stay private. To achieve this,
/// mark every node in this network as `Private` and allowlist/seed them to each other.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessType {
    Public,
    Private,
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

    /// See docstring for `AccessType`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_type: Option<AccessType>,

    /// Like `seed_peers` in `P2pConfig`, allowlisted peers will awlays be allowed to establish
    /// connection with this node regardless of the concurrency limit.
    /// Unlike `seed_peers`, a node does not reach out to `allowlisted_peers` preferentially.
    /// It is also used to determine if a peer is accessible when its AccessType is Private.
    /// For example, a node will ignore a peer with Private AccessType if the peer is not in
    /// its `allowlisted_peers`. Namely, the node will not try to establish connections
    /// to this peer, nor advertise this peer's info to other peers in the network.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub allowlisted_peers: Vec<AllowlistedPeer>,
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

    pub fn access_type(&self) -> AccessType {
        // defaults None to Public
        self.access_type.unwrap_or(AccessType::Public)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RandomnessConfig {
    /// Maximum number of rounds ahead of our most recent completed round for which we should
    /// accept partial signatures from other validators.
    ///
    /// If unspecified, this will default to 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_partial_sigs_rounds_ahead: Option<u64>,

    /// Maximum number of rounds for which partial signatures should be concurrently sent.
    ///
    /// If unspecified, this will default to 20.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_partial_sigs_concurrent_sends: Option<usize>,

    /// Interval at which to retry sending partial signatures until the round is complete.
    ///
    /// If unspecified, this will default to `5,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_signature_retry_interval_ms: Option<u64>,

    /// Size of the Randomness actor's mailbox. This should be set large enough to never
    /// overflow unless a bug is encountered.
    ///
    /// If unspecified, this will default to `1,000,000`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_capacity: Option<usize>,

    /// Per-peer inflight limit for the SendPartialSignatures RPC.
    ///
    /// If unspecified, this will default to 20.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_partial_signatures_inflight_limit: Option<usize>,

    /// Maximum proportion of total peer weight to ignore in case of byzantine behavior.
    ///
    /// If unspecified, this will default to 0.2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ignored_peer_weight_factor: Option<f64>,
}

impl RandomnessConfig {
    pub fn max_partial_sigs_rounds_ahead(&self) -> u64 {
        const MAX_PARTIAL_SIGS_ROUNDS_AHEAD: u64 = 50;

        self.max_partial_sigs_rounds_ahead
            .unwrap_or(MAX_PARTIAL_SIGS_ROUNDS_AHEAD)
    }

    pub fn max_partial_sigs_concurrent_sends(&self) -> usize {
        const MAX_PARTIAL_SIGS_CONCURRENT_SENDS: usize = 20;

        self.max_partial_sigs_concurrent_sends
            .unwrap_or(MAX_PARTIAL_SIGS_CONCURRENT_SENDS)
    }
    pub fn partial_signature_retry_interval(&self) -> Duration {
        const PARTIAL_SIGNATURE_RETRY_INTERVAL: u64 = 5_000; // 5 seconds

        Duration::from_millis(
            self.partial_signature_retry_interval_ms
                .unwrap_or(PARTIAL_SIGNATURE_RETRY_INTERVAL),
        )
    }

    pub fn mailbox_capacity(&self) -> usize {
        const MAILBOX_CAPACITY: usize = 1_000_000;

        self.mailbox_capacity.unwrap_or(MAILBOX_CAPACITY)
    }

    pub fn send_partial_signatures_inflight_limit(&self) -> usize {
        const SEND_PARTIAL_SIGNATURES_INFLIGHT_LIMIT: usize = 20;

        self.send_partial_signatures_inflight_limit
            .unwrap_or(SEND_PARTIAL_SIGNATURES_INFLIGHT_LIMIT)
    }

    pub fn max_ignored_peer_weight_factor(&self) -> f64 {
        const MAX_IGNORED_PEER_WEIGHT_FACTOR: f64 = 0.2;

        self.max_ignored_peer_weight_factor
            .unwrap_or(MAX_IGNORED_PEER_WEIGHT_FACTOR)
    }
}
