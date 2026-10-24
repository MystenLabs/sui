// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    path::PathBuf,
    time::Duration,
};

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
    /// Manually configured peer addresses. These override addresses loaded from on-chain configs.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub peer_address_overrides: Vec<PeerAddresses>,
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
            peer_address_overrides: Default::default(),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PeerAddresses {
    pub peer_id: anemo::PeerId,
    pub addresses: Vec<Multiaddr>,
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

    /// If true, the v2 version of get_checkpoint_contents will be used.
    ///
    /// If unspecified, this will default to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_get_checkpoint_contents_v2: Option<bool>,

    /// Maximum lookahead (in checkpoint sequence numbers) for storing unverified checkpoint
    /// summaries received via PushCheckpointSummary.
    ///
    /// If unspecified, this will default to `1,000`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checkpoint_lookahead: Option<u64>,

    /// Maximum number of checkpoint headers to attempt to sync in a single sync task.
    ///
    /// If unspecified, this will default to `400`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checkpoint_sync_batch_size: Option<u64>,

    /// Maximum serialized size in bytes for a CertifiedCheckpointSummary received via
    /// PushCheckpointSummary.
    ///
    /// If unspecified, this will default to `262,144` (256 KiB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_checkpoint_summary_size: Option<usize>,

    /// Minimum timeout for checkpoint content downloads (adaptive timeout lower bound).
    ///
    /// If unspecified, this will default to `10,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_timeout_min_ms: Option<u64>,

    /// Maximum timeout for checkpoint content downloads (adaptive timeout upper bound).
    ///
    /// If unspecified, this will default to `30,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_timeout_max_ms: Option<u64>,

    /// Time window for peer scoring samples.
    ///
    /// If unspecified, this will default to `60,000` milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_scoring_window_ms: Option<u64>,

    /// Probability of selecting an unknown peer to explore its throughput.
    ///
    /// If unspecified, this will default to `0.1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exploration_probability: Option<f64>,

    /// Failure rate threshold for marking a peer as failing.
    /// A peer is marked as failing if its failure rate exceeds this threshold.
    ///
    /// If unspecified, this will default to `0.3` (30%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_failure_rate: Option<f64>,

    /// Duration in milliseconds that a peer must be continuously failing before it is
    /// reported to discovery for disconnection. Set to 0 to disable automatic reporting
    /// of failing peers.
    ///
    /// If unspecified, this will default to `30,000` (30 seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_disconnect_threshold_ms: Option<u64>,

    /// Delay after which an unfinished checkpoint contents request is hedged: a second request
    /// for the same contents goes to another peer that has announced the checkpoint by then,
    /// the first valid response wins, and the other request is cancelled. At most two requests
    /// per checkpoint are in flight, and only checkpoints close to the next one to be released
    /// in order are hedged.
    ///
    /// When set, every request, including one that replaces a failed request, goes to a peer
    /// picked from the peers that have announced the checkpoint at that moment, not when the
    /// download started. Near the next checkpoint to be released, a download whose requests have
    /// all failed keeps checking for newly announced peers every hedge delay until
    /// `wait_interval_when_no_peer_to_sync_content_ms` has passed, instead of sleeping for it.
    ///
    /// If unspecified, hedging is disabled: the peers that had announced the checkpoint when the
    /// download started are tried one at a time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_hedge_delay_ms: Option<NonZeroU64>,

    /// When true, requests to a peer that is slow from this node are hedged sooner than
    /// `checkpoint_content_hedge_delay_ms`: after the peer's slow-response threshold, which is
    /// its fastest recent response time plus 25% or plus 20 ms, whichever is more. A peer is slow
    /// when at least a quarter of its recent responses took longer than that threshold. Requests
    /// cancelled after the threshold because a hedge won count as slow responses, so a peer stays
    /// slow while hedges keep beating it, and stops being slow once it answers within the
    /// threshold again. Requests cancelled sooner count neither way.
    ///
    /// Also, when a hedge is due before any other peer has announced the checkpoint, or every
    /// request has failed, the download checks for newly announced peers every 10 ms instead of
    /// every hedge delay, so the next request goes out within 10 ms of an announcement.
    ///
    /// Has no effect unless `checkpoint_content_hedge_delay_ms` is set. If unspecified, this
    /// defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_adaptive_hedge_delay: Option<bool>,

    /// When true, downloads of checkpoints close to the next one to be released send their
    /// requests to the peers expected to answer soonest. A peer's expected response time is the
    /// longer of its connection round-trip time and its fastest recent response time (just the
    /// round-trip time if it has no recent responses). The first request, the hedge after the
    /// hedge delay, and a request sent after every request failed go to the connected peer with
    /// the lowest expected response time among the peers that have announced the checkpoint,
    /// skipping peers with recent failures. Peers that are slow from this node (see
    /// `checkpoint_content_adaptive_hedge_delay`; this applies whether or not that option is on)
    /// come after all peers that are not.
    ///
    /// While only one request is in flight, the download checks every 10 ms which peers have
    /// announced the checkpoint since, and hedges right away instead of waiting for the hedge
    /// delay in two cases. If the request in flight is to a slow peer and a peer that isn't slow
    /// has the checkpoint, the hedge goes to the one of those peers with the lowest expected
    /// response time. Otherwise, if a peer is expected to finish at least 50 ms before the
    /// request in flight, the hedge goes to it; a request that has already run past its expected
    /// finish gets no such hedge.
    ///
    /// One in four requests that are not hedges sent to a slow peer is a probe that is not hedged
    /// right away just because the peer is slow. While a peer is slow, only probes add response
    /// time samples, so once the samples from before it became slow have been replaced or have
    /// aged out, probes alone decide whether it is still slow and what its fastest recent
    /// response time is. Other requests to it are hedged right away, which cuts their slow
    /// responses short while their fast ones still arrive, so recording them would make the peer
    /// look less slow than it is. A request cancelled before its peer's slow-response threshold,
    /// which counts neither way, is not recorded once the peer has a threshold, so it doesn't
    /// push out samples that count.
    ///
    /// Has no effect unless `checkpoint_content_hedge_delay_ms` is set, and only applies near
    /// the release frontier, where hedging applies. If unspecified, this defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_content_hedge_to_closer_peer: Option<bool>,
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

    pub fn use_get_checkpoint_contents_v2(&self) -> bool {
        const DEFAULT_USE_GET_CHECKPOINT_CONTENTS_V2: bool = true;

        self.use_get_checkpoint_contents_v2
            .unwrap_or(DEFAULT_USE_GET_CHECKPOINT_CONTENTS_V2)
    }

    pub fn max_checkpoint_lookahead(&self) -> u64 {
        const DEFAULT_MAX_CHECKPOINT_LOOKAHEAD: u64 = 1_000;

        self.max_checkpoint_lookahead
            .unwrap_or(DEFAULT_MAX_CHECKPOINT_LOOKAHEAD)
    }

    pub fn max_checkpoint_sync_batch_size(&self) -> u64 {
        const DEFAULT_MAX_CHECKPOINT_SYNC_BATCH_SIZE: u64 = 400;

        self.max_checkpoint_sync_batch_size
            .unwrap_or(DEFAULT_MAX_CHECKPOINT_SYNC_BATCH_SIZE)
    }

    pub fn max_checkpoint_summary_size(&self) -> usize {
        const DEFAULT_MAX_CHECKPOINT_SUMMARY_SIZE: usize = 256 * 1024; // 256 KiB

        self.max_checkpoint_summary_size
            .unwrap_or(DEFAULT_MAX_CHECKPOINT_SUMMARY_SIZE)
    }

    pub fn checkpoint_content_timeout_min(&self) -> Duration {
        const DEFAULT: Duration = Duration::from_secs(5);
        self.checkpoint_content_timeout_min_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT)
    }

    pub fn checkpoint_content_timeout_max(&self) -> Duration {
        const DEFAULT: Duration = Duration::from_secs(30);
        self.checkpoint_content_timeout_max_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT)
    }

    pub fn peer_scoring_window(&self) -> Duration {
        const DEFAULT: Duration = Duration::from_secs(60);
        self.peer_scoring_window_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT)
    }

    pub fn exploration_probability(&self) -> f64 {
        const DEFAULT: f64 = 0.1;
        let value = self.exploration_probability.unwrap_or(DEFAULT);
        assert!(
            (0.0..=1.0).contains(&value),
            "exploration_probability must be in [0, 1], got {value}"
        );
        value
    }

    pub fn peer_failure_rate(&self) -> f64 {
        const DEFAULT: f64 = 0.3;
        let value = self.peer_failure_rate.unwrap_or(DEFAULT);
        assert!(
            (0.0..=1.0).contains(&value),
            "peer_failure_rate must be in [0, 1], got {value}"
        );
        value
    }

    pub fn peer_disconnect_threshold(&self) -> Duration {
        const DEFAULT_MS: u64 = 30_000; // 30 seconds
        Duration::from_millis(self.peer_disconnect_threshold_ms.unwrap_or(DEFAULT_MS))
    }

    pub fn checkpoint_content_hedge_delay(&self) -> Option<Duration> {
        self.checkpoint_content_hedge_delay_ms
            .map(|delay_ms| Duration::from_millis(delay_ms.get()))
    }

    pub fn checkpoint_content_adaptive_hedge_delay(&self) -> bool {
        self.checkpoint_content_adaptive_hedge_delay
            .unwrap_or(false)
    }

    pub fn checkpoint_content_hedge_to_closer_peer(&self) -> bool {
        self.checkpoint_content_hedge_to_closer_peer
            .unwrap_or(false)
    }

    pub fn randomized_for_testing() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let config = Self {
            mailbox_capacity: Some(rng.gen_range(16..=2048)),
            synced_checkpoint_broadcast_channel_capacity: Some(rng.gen_range(16..=2048)),
            checkpoint_header_download_concurrency: Some(rng.gen_range(10..=500)),
            checkpoint_content_download_concurrency: Some(rng.gen_range(10..=500)),
            max_checkpoint_lookahead: Some(rng.gen_range(100..=2000)),
            max_checkpoint_sync_batch_size: Some(rng.gen_range(100..=500)),
            max_checkpoint_summary_size: Some(rng.gen_range(64 * 1024..=512 * 1024)),
            ..Default::default()
        };
        tracing::info!(
            mailbox_capacity = config.mailbox_capacity.unwrap(),
            broadcast_capacity = config.synced_checkpoint_broadcast_channel_capacity.unwrap(),
            header_concurrency = config.checkpoint_header_download_concurrency.unwrap(),
            content_concurrency = config.checkpoint_content_download_concurrency.unwrap(),
            lookahead = config.max_checkpoint_lookahead.unwrap(),
            batch_size = config.max_checkpoint_sync_batch_size.unwrap(),
            summary_size = config.max_checkpoint_summary_size.unwrap(),
            "StateSyncConfig::randomized_for_testing"
        );
        config
    }
}

/// Access Type of a node.
/// AccessType info is shared in the discovery process.
/// * If the node marks itself as Public, other nodes may try to connect to it.
/// * If the node marks itself as Private, only nodes that have it in
///   their `allowlisted_peers` or `seed_peers` will try to connect to it. The
///   node's info will not be shared through discovery.
/// * Trusted is the same as Private, except it allows sharing the node's info
///   only to other preconfigured peers (i.e. those in `allowlisted_peers` and
///   `seed_peers`, or active Sui validators).
/// * If not set, defaults to Public.
///
/// AccessType is useful when a network of nodes want to stay private. To achieve this,
/// mark every node in this network as `Private` or `Trusted`, and allowlist/seed them
/// to each other.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessType {
    Public,
    Private,
    Trusted,
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

    /// Like `seed_peers` in `P2pConfig`, allowlisted peers will always be allowed to establish
    /// connection with this node regardless of the concurrency limit.
    /// Unlike `seed_peers`, a node does not reach out to `allowlisted_peers` preferentially.
    /// It is also used to determine if a peer is accessible when its AccessType is Private.
    /// For example, a node will ignore a peer with Private AccessType if the peer is not in
    /// its `allowlisted_peers`. Namely, the node will not try to establish connections
    /// to this peer, nor advertise this peer's info to other peers in the network.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub allowlisted_peers: Vec<AllowlistedPeer>,

    /// Size of the Discovery loop's mailbox.
    ///
    /// If unspecified, this will default to `1,024`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox_capacity: Option<usize>,

    /// Use get_known_peers_v3 RPC to fetch peer info.
    ///
    /// If unspecified, this will default to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_get_known_peers_v3: Option<bool>,

    /// Path to store discovered peer addresses across restarts.
    ///
    /// If unspecified, defaults to `<db_path>/discovery_peer_cache.yaml` when
    /// started via `SuiNode`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_addr_store_path: Option<PathBuf>,

    /// Duration in milliseconds to cooldown a peer after a failure report from state-sync.
    /// During cooldown, the peer is deprioritized for new connections.
    ///
    /// If unspecified, this will default to `3,600,000` (1 hour).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_failure_cooldown_ms: Option<u64>,

    /// Minimum number of connected peers below which no failing-peer disconnect will occur.
    ///
    /// If unspecified, this will default to `2`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_peers_for_disconnect: Option<usize>,
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

    pub fn mailbox_capacity(&self) -> usize {
        // Sized with headroom for bursts of PeerAddressChange messages: a
        // single GetKnownPeers response can enqueue one message per trusted
        // peer, and multiple concurrent query tasks can stack their batches.
        const MAILBOX_CAPACITY: usize = 2_048;

        self.mailbox_capacity.unwrap_or(MAILBOX_CAPACITY)
    }

    pub fn use_get_known_peers_v3(&self) -> bool {
        self.use_get_known_peers_v3.unwrap_or(false)
    }

    pub fn peer_failure_cooldown(&self) -> Duration {
        const DEFAULT_MS: u64 = 3_600_000; // 1 hour
        Duration::from_millis(self.peer_failure_cooldown_ms.unwrap_or(DEFAULT_MS))
    }

    pub fn min_peers_for_disconnect(&self) -> usize {
        const DEFAULT: usize = 2;
        self.min_peers_for_disconnect.unwrap_or(DEFAULT)
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
