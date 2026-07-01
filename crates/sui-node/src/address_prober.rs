// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The adddress prober periodically checks addresses of trusted peers for connectability, reported
//! via Prometheus metrics.
//!
//! - P2P: `anemo::Network::probe_address`, which verifies reachability + the peer's `PeerId` without
//!   joining the peer set or disturbing any existing connection.
//! - Consensus: a throwaway tonic+rustls `connect()` replicating the real consensus client TLS
//!   (expected network key, `consensus_epoch_{epoch}` server name, our network key as the client
//!   cert). Only a current committee member can complete this handshake.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anemo::{Network, PeerId};
use consensus_config::{
    Authority as ConsensusAuthority, Committee as ConsensusCommittee,
    NetworkKeyPair as ConsensusNetworkKeyPair, NetworkPublicKey as ConsensusNetworkPublicKey,
};
use fastcrypto::encoding::{Encoding, Hex};
use futures::future::{BoxFuture, FutureExt, join_all};
use futures::stream::{FuturesUnordered, StreamExt};
use mysten_metrics::spawn_monitored_task;
use mysten_network::Multiaddr;
use prometheus::{
    IntCounterVec, IntGaugeVec, Registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry,
};
use serde::Serialize;
use sui_config::AddressProberConfig;
use sui_core::consensus_manager::ConsensusManager;
use sui_network::discovery::{Sender as DiscoverySender, TrustedPeerP2pAddresses};
use sui_network::endpoint_manager::AddressSource;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, info};

const MAILBOX_CAPACITY: usize = 128; // updates are rare (once per epoch)

/// Which transport an address belongs to. The `&'static str` is the Prometheus `endpoint_type`
/// label.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EndpointType {
    P2p,
    Consensus,
}

impl EndpointType {
    fn as_str(self) -> &'static str {
        match self {
            EndpointType::P2p => "p2p",
            EndpointType::Consensus => "consensus",
        }
    }
}

/// Outcome of probing a single address, unified across the P2P and consensus paths. The string form
/// is the Prometheus `result` label on the attempts counter.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeResult {
    Reachable,
    Unreachable,
    WrongIdentity,
    BadAddress,
    Timeout,
    // Note: Update ProbeResult::ALL if adding new variants.
}

impl ProbeResult {
    /// All variants, so the `result`-labelled attempts series can be enumerated — e.g. to drop a
    /// churned-out peer's series, or in test helpers.
    const ALL: [ProbeResult; 5] = [
        ProbeResult::Reachable,
        ProbeResult::Unreachable,
        ProbeResult::WrongIdentity,
        ProbeResult::BadAddress,
        ProbeResult::Timeout,
    ];

    fn is_reachable(self) -> bool {
        matches!(self, ProbeResult::Reachable)
    }

    fn as_str(self) -> &'static str {
        match self {
            ProbeResult::Reachable => "reachable",
            ProbeResult::Unreachable => "unreachable",
            ProbeResult::WrongIdentity => "wrong_identity",
            ProbeResult::BadAddress => "bad_address",
            ProbeResult::Timeout => "timeout",
        }
    }
}

impl From<anemo::ProbeOutcome> for ProbeResult {
    fn from(outcome: anemo::ProbeOutcome) -> Self {
        match outcome {
            anemo::ProbeOutcome::Reachable => ProbeResult::Reachable,
            anemo::ProbeOutcome::Unreachable => ProbeResult::Unreachable,
            anemo::ProbeOutcome::WrongIdentity => ProbeResult::WrongIdentity,
            anemo::ProbeOutcome::BadAddress => ProbeResult::BadAddress,
            anemo::ProbeOutcome::Timeout => ProbeResult::Timeout,
        }
    }
}

/// One concrete address to probe, tagged with how to probe it.
#[derive(Clone)]
enum AddressTarget {
    P2p {
        peer_id: PeerId,
        address: anemo::types::Address,
    },
    Consensus {
        target_key: ConsensusNetworkPublicKey,
        address: Multiaddr,
    },
}

impl AddressTarget {
    fn display(&self) -> String {
        match self {
            AddressTarget::P2p { address, .. } => address.to_string(),
            AddressTarget::Consensus { address, .. } => address.to_string(),
        }
    }
}

/// Operator-facing identity of a peer that is a current committee validator, resolved from the
/// consensus committee `Authority`. `None` for trusted non-validator peers (seeds / configured
/// fullnodes) that aren't in the committee.
#[derive(Clone)]
struct AuthorityInfo {
    /// On-chain authority name (protocol public key), hex-encoded — matches the Sui-side
    /// `AuthorityName`.
    authority_name: String,
    /// The validator's advertised hostname from the committee.
    hostname: String,
}

/// All addresses advertised for one `(peer, endpoint_type, source)` triple.
struct ProbeGroup {
    peer_label: String,
    endpoint_type: EndpointType,
    source: AddressSource,
    /// Validator identity (name + hostname), if this peer is in the current committee.
    authority: Option<AuthorityInfo>,
    addresses: Vec<AddressTarget>,
}

/// Identifies a `(peer, endpoint_type, source)` triple in the prober's per-group state.
type GroupKey = (String, &'static str, &'static str);

/// The most recent probe result for a single concrete address within a group. Retained only for the
/// admin report (the metrics deliberately omit the address to bound cardinality).
struct AddressOutcome {
    address: String,
    result: ProbeResult,
}

/// A trusted peer's `(peer, endpoint_type, source)` triple tracked across probe cycles.
struct Group {
    peer_label: String,
    endpoint_type: EndpointType,
    source: AddressSource,
    /// Validator identity (name + hostname), if this peer is in the current committee.
    authority: Option<AuthorityInfo>,
    /// Addresses to probe, refreshed from discovery/committee each scan.
    targets: Vec<AddressTarget>,
    /// True while this group's probe is in flight, so a scan doesn't re-spawn it.
    probing: bool,
    last_probed: Option<Instant>,
    consecutive_failures: u32,
    /// Smoothed connectability (mirrors the `discovery_probe_connectable` gauge): `true` until
    /// `failure_threshold` consecutive failures flip it to `false`.
    connectable: bool,
    last_success_unix_secs: Option<i64>,
    outcomes: Vec<AddressOutcome>,
}

impl Group {
    fn new(candidate: ProbeGroup) -> Self {
        Self {
            peer_label: candidate.peer_label,
            endpoint_type: candidate.endpoint_type,
            source: candidate.source,
            authority: candidate.authority,
            targets: candidate.addresses,
            probing: false,
            last_probed: None,
            consecutive_failures: 0,
            connectable: false,
            last_success_unix_secs: None,
            outcomes: Vec::new(),
        }
    }

    /// When this group is next due to be probed: a never-probed group is due now, otherwise its last
    /// probe plus the good/failed interval selected by its recent history.
    fn next_due(
        &self,
        good_interval: Duration,
        failed_interval: Duration,
        now: Instant,
    ) -> Instant {
        match self.last_probed {
            None => now,
            Some(last_probed) => {
                let interval = if self.consecutive_failures == 0 {
                    good_interval
                } else {
                    failed_interval
                };
                last_probed + interval
            }
        }
    }
}

/// Epoch-scoped inputs to the prober.
pub struct ProberEpochContext {
    pub epoch: u64,
    pub consensus_committee: ConsensusCommittee,
    pub consensus_manager: Arc<ConsensusManager>,
}

enum ProberMessage {
    /// This node is a current validator for `epoch`; probe its committee's consensus endpoints + the
    /// trusted peers' P2P endpoints.
    UpdateEpoch {
        epoch: u64,
        consensus_committee: ConsensusCommittee,
        consensus_manager: Arc<ConsensusManager>,
    },
    /// This node is no longer a current validator; the prober idles until the next `UpdateEpoch`.
    LeaveCommittee,
    /// Admin snapshot request: reply with the prober's latest results (see [`Handle::probe_report`]).
    GetReport { reply: oneshot::Sender<ProbeReport> },
}

/// A point-in-time snapshot of the prober's latest results, served by the admin endpoint.
#[derive(Clone, Debug, Serialize)]
pub struct ProbeReport {
    pub epoch: Option<u64>,
    pub groups: Vec<ProbeGroupReport>,
}

/// Latest probe outcome for one `(peer, endpoint_type, source)` group.
#[derive(Clone, Debug, Serialize)]
pub struct ProbeGroupReport {
    /// `peer_id` hex (P2P) or consensus network public key hex (consensus).
    pub peer: String,
    /// On-chain authority name (protocol public key) hex, if this peer is a current committee
    /// validator; `None` for trusted non-validator peers.
    pub authority_name: Option<String>,
    /// The validator's committee hostname, if this peer is a current committee validator.
    pub hostname: Option<String>,
    pub endpoint_type: String,
    pub address_source: String,
    /// Smoothed connectability (matches the `discovery_probe_connectable` gauge).
    pub connectable: bool,
    pub consecutive_failures: u32,
    /// How long ago the group was last probed, in seconds.
    pub seconds_since_last_probe: u64,
    /// Unix timestamp (seconds) of the last successful probe, if ever reachable.
    pub last_success_unix_seconds: Option<i64>,
    pub addresses: Vec<ProbeAddressReport>,
}

/// Latest probe result for a single concrete address.
#[derive(Clone, Debug, Serialize)]
pub struct ProbeAddressReport {
    pub address: String,
    /// One of `reachable`, `unreachable`, `wrong_identity`, `bad_address`, `timeout`.
    pub result: String,
}

pub struct AddressProberMetrics {
    connectable: IntGaugeVec,
    last_success_timestamp_seconds: IntGaugeVec,
    attempts_total: IntCounterVec,
}

impl AddressProberMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            connectable: register_int_gauge_vec_with_registry!(
                "discovery_probe_connectable",
                "1 if a trusted peer's advertised address for this endpoint/source is connectable, \
                 0 after N consecutive failed probe cycles",
                &["peer_id", "endpoint_type", "address_source"],
                registry
            )
            .unwrap(),
            last_success_timestamp_seconds: register_int_gauge_vec_with_registry!(
                "discovery_probe_last_success_timestamp_seconds",
                "Unix timestamp (seconds) of the last successful probe for this peer/endpoint/source",
                &["peer_id", "endpoint_type", "address_source"],
                registry
            )
            .unwrap(),
            attempts_total: register_int_counter_vec_with_registry!(
                "discovery_probe_attempts_total",
                "Total address probe attempts by peer/endpoint/source and result",
                &["peer_id", "endpoint_type", "address_source", "result"],
                registry
            )
            .unwrap(),
        })
    }
}

#[cfg(any(test, msim))]
impl AddressProberMetrics {
    /// Current value of the smoothed connectability gauge for a triple (creates the series at 0 if
    /// it has never been written, so distinguish via [`Self::attempts_value_for_testing`]).
    pub fn connectable_for_testing(
        &self,
        peer_label: &str,
        endpoint_type: &str,
        source: &str,
    ) -> i64 {
        self.connectable
            .with_label_values(&[peer_label, endpoint_type, source])
            .get()
    }

    /// Number of probe attempts recorded for a triple with the given result.
    pub fn attempts_value_for_testing(
        &self,
        peer_label: &str,
        endpoint_type: &str,
        source: &str,
        result: &str,
    ) -> u64 {
        self.attempts_total
            .with_label_values(&[peer_label, endpoint_type, source, result])
            .get()
    }

    /// Total probe attempts recorded for a triple across all results.
    pub fn total_attempts_for_testing(
        &self,
        peer_label: &str,
        endpoint_type: &str,
        source: &str,
    ) -> u64 {
        ProbeResult::ALL
            .into_iter()
            .map(|result| {
                self.attempts_value_for_testing(peer_label, endpoint_type, source, result.as_str())
            })
            .sum()
    }
}

/// Handle to the address prober. Dropping all clones closes the mailbox and the event loop
/// shuts down. Holds the metrics so tests can read them.
#[derive(Clone)]
pub struct Handle {
    sender: mpsc::Sender<ProberMessage>,
    // Retained only so tests can read the prober's metrics.
    #[cfg(any(test, msim))]
    metrics: Arc<AddressProberMetrics>,
}

impl Handle {
    /// Activates the prober for an epoch. Call if this node is a current validator.
    pub fn update_epoch(
        &self,
        epoch: u64,
        consensus_committee: ConsensusCommittee,
        consensus_manager: Arc<ConsensusManager>,
    ) {
        self.sender
            .try_send(ProberMessage::UpdateEpoch {
                epoch,
                consensus_committee,
                consensus_manager,
            })
            .expect("address prober mailbox should not overflow or be closed");
    }

    /// Deactivates the prober.
    pub fn leave_committee(&self) {
        self.sender
            .try_send(ProberMessage::LeaveCommittee)
            .expect("address prober mailbox should not overflow or be closed");
    }

    /// Snapshot the prober's latest results (full addresses + per-address outcomes).
    /// Returns `None` if the event loop has shut down or dropped the reply.
    pub async fn probe_report(&self) -> Option<ProbeReport> {
        let (reply, response) = oneshot::channel();
        if self
            .sender
            .send(ProberMessage::GetReport { reply })
            .await
            .is_err()
        {
            return None;
        }
        response.await.ok()
    }

    #[cfg(any(test, msim))]
    pub fn metrics_for_testing(&self) -> Arc<AddressProberMetrics> {
        self.metrics.clone()
    }
}

pub struct Builder {
    config: AddressProberConfig,
    metrics: Option<Arc<AddressProberMetrics>>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            config: AddressProberConfig::default(),
            metrics: None,
        }
    }

    pub fn config(mut self, config: AddressProberConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_metrics(mut self, registry: &Registry) -> Self {
        self.metrics = Some(AddressProberMetrics::new(registry));
        self
    }

    pub fn build(self) -> UnstartedAddressProber {
        let metrics = self
            .metrics
            .unwrap_or_else(|| AddressProberMetrics::new(&Registry::new()));
        let (sender, mailbox) = mpsc::channel(MAILBOX_CAPACITY);
        let handle = Handle {
            sender,
            #[cfg(any(test, msim))]
            metrics: metrics.clone(),
        };
        UnstartedAddressProber {
            config: self.config,
            metrics,
            handle,
            mailbox,
        }
    }
}

/// A built-but-not-started prober: holds the runtime-independent state until [`start`] injects the
/// network/discovery/keypair and spawns the event loop.
///
/// [`start`]: UnstartedAddressProber::start
pub struct UnstartedAddressProber {
    config: AddressProberConfig,
    metrics: Arc<AddressProberMetrics>,
    handle: Handle,
    mailbox: mpsc::Receiver<ProberMessage>,
}

impl UnstartedAddressProber {
    /// Spawns the prober loop.
    pub fn start(
        self,
        network: Network,
        discovery: DiscoverySender,
        own_consensus_keypair: ConsensusNetworkKeyPair,
    ) -> Handle {
        let event_loop = AddressProberEventLoop::new(
            self.config,
            self.metrics,
            self.mailbox,
            network,
            discovery,
            own_consensus_keypair,
        );
        spawn_monitored_task!(event_loop.start());
        self.handle
    }
}

struct AddressProberEventLoop {
    // Resolved config knobs (`Copy`); the per-probe futures capture these directly.
    good_interval: Duration,
    failed_interval: Duration,
    failure_threshold: u32,
    consensus_probe_timeout: Duration,
    // Node-lifetime inputs.
    network: Network,
    discovery: DiscoverySender,
    /// This node's consensus network keypair, used as the client cert for consensus probes.
    own_consensus_keypair: ConsensusNetworkKeyPair,
    own_peer_id: PeerId,
    own_consensus_key: ConsensusNetworkPublicKey,
    metrics: Arc<AddressProberMetrics>,
    mailbox: mpsc::Receiver<ProberMessage>,
    inflight_probe_limit: Arc<Semaphore>,
    /// In-flight probes; each yields `(group key, per-address outcomes)` when it completes.
    tasks: FuturesUnordered<BoxFuture<'static, (GroupKey, Vec<AddressOutcome>)>>,
    groups: HashMap<GroupKey, Group>,
    /// Current epoch's probe inputs, or `None` when this node is not a current validator.
    epoch_state: Option<Arc<ProberEpochContext>>,
}

impl AddressProberEventLoop {
    fn new(
        config: AddressProberConfig,
        metrics: Arc<AddressProberMetrics>,
        mailbox: mpsc::Receiver<ProberMessage>,
        network: Network,
        discovery: DiscoverySender,
        own_consensus_keypair: ConsensusNetworkKeyPair,
    ) -> Self {
        let own_peer_id = network.peer_id();
        let own_consensus_key = own_consensus_keypair.public();
        Self {
            good_interval: config.good_interval(),
            failed_interval: config.failed_interval(),
            failure_threshold: config.failure_threshold(),
            consensus_probe_timeout: config.consensus_probe_timeout(),
            network,
            discovery,
            own_consensus_keypair,
            own_peer_id,
            own_consensus_key,
            metrics,
            mailbox,
            inflight_probe_limit: Arc::new(Semaphore::new(config.concurrency())),
            tasks: FuturesUnordered::new(),
            groups: HashMap::new(),
            epoch_state: None,
        }
    }

    async fn start(mut self) {
        info!(
            good_interval_secs = self.good_interval.as_secs(),
            failed_interval_secs = self.failed_interval.as_secs(),
            "starting discovery address prober",
        );

        // A single resettable timer fires when the next group is due (see `next_deadline`).
        let mut timer = Box::pin(tokio::time::sleep_until(Instant::now()));
        loop {
            tokio::select! {
                _ = &mut timer => self.scan().await,
                Some((key, outcomes)) = self.tasks.next() => self.handle_probe_result(key, outcomes),
                maybe_message = self.mailbox.recv() => match maybe_message {
                    // Once all `Handle`s have been dropped this yields `None`, so we shut down.
                    Some(message) => self.handle_message(message),
                    None => break,
                },
            }
            timer.as_mut().reset(self.next_deadline());
        }

        info!("discovery address prober ended");
    }

    fn handle_message(&mut self, message: ProberMessage) {
        match message {
            ProberMessage::UpdateEpoch {
                epoch,
                consensus_committee,
                consensus_manager,
            } => {
                self.epoch_state = Some(Arc::new(ProberEpochContext {
                    epoch,
                    consensus_committee,
                    consensus_manager,
                }));
            }
            ProberMessage::LeaveCommittee => {
                self.epoch_state = None;
                // We no longer probe anyone; drop all tracked groups and their metric series so a
                // demoted node doesn't keep exporting stale per-peer metrics.
                for key in self.groups.keys().cloned().collect::<Vec<_>>() {
                    self.remove_group_metrics(&key);
                }
                self.groups.clear();
            }
            ProberMessage::GetReport { reply } => {
                let _ = reply.send(self.build_report());
            }
        }
    }

    /// When to next scan for due groups: the earliest per-group due time, capped by `failed_interval`
    /// so newly-advertised peers/addresses are still discovered promptly even when everything known
    /// is healthy. Groups with an in-flight probe are excluded — they reschedule when they complete.
    fn next_deadline(&self) -> Instant {
        let now = Instant::now();
        let cap = now + self.failed_interval;
        if self.epoch_state.is_none() {
            return cap;
        }
        self.groups
            .values()
            .filter(|group| !group.probing)
            .map(|group| group.next_due(self.good_interval, self.failed_interval, now))
            .min()
            .map_or(cap, |deadline| deadline.min(cap))
            .max(now)
    }

    /// Rebuild the current candidate set from discovery + the committee, merge it into `self.groups`,
    /// then spawn a probe for every group that is now due and not already being probed. The discovery
    /// snapshot fetch is the only await; the probes run off-loop.
    async fn scan(&mut self) {
        let Some(context) = self.epoch_state.clone() else {
            return;
        };
        let epoch = context.epoch;
        let trusted_p2p_addresses = self.discovery.trusted_peer_p2p_addresses().await;
        let candidates = build_groups(
            trusted_p2p_addresses,
            &context.consensus_manager,
            &context.consensus_committee,
            &self.own_peer_id,
            &self.own_consensus_key,
        );
        self.refresh_groups(candidates);

        let now = Instant::now();
        let due: Vec<GroupKey> = self
            .groups
            .iter()
            .filter(|(_, group)| {
                !group.probing
                    && group.next_due(self.good_interval, self.failed_interval, now) <= now
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in due {
            self.spawn_probe(epoch, &key);
        }
    }

    /// Merge a freshly-built candidate set into `self.groups`: refresh addresses/identity for known
    /// groups, start tracking new ones, and drop groups no longer advertised — unless a probe is
    /// still in flight for them, in which case they survive until that probe is recorded.
    fn refresh_groups(&mut self, candidates: Vec<ProbeGroup>) {
        let current: HashSet<GroupKey> = candidates.iter().map(group_key).collect();
        // Drop groups no longer advertised (unless a probe is still in flight for them) and clear
        // their metric series, so a peer removed from the trusted set stops being exported.
        let removed: Vec<GroupKey> = self
            .groups
            .iter()
            .filter(|(key, group)| !current.contains(*key) && !group.probing)
            .map(|(key, _)| key.clone())
            .collect();
        for key in removed {
            self.remove_group_metrics(&key);
            self.groups.remove(&key);
        }
        for candidate in candidates {
            let key = group_key(&candidate);
            match self.groups.get_mut(&key) {
                Some(group) => {
                    group.targets = candidate.addresses;
                    group.authority = candidate.authority;
                }
                None => {
                    self.groups.insert(key, Group::new(candidate));
                }
            }
        }
    }

    /// Drop the Prometheus series for a group that is no longer tracked, so churned-out peers don't
    /// linger in the exported metrics. Removing a nonexistent series is a no-op (ignored error).
    fn remove_group_metrics(&self, key: &GroupKey) {
        let (peer, endpoint_type, source) = (key.0.as_str(), key.1, key.2);
        let labels = [peer, endpoint_type, source];
        let _ = self.metrics.connectable.remove_label_values(&labels);
        let _ = self
            .metrics
            .last_success_timestamp_seconds
            .remove_label_values(&labels);
        // `attempts_total` also carries the `result` label, so one series exists per outcome.
        for result in ProbeResult::ALL {
            let _ = self.metrics.attempts_total.remove_label_values(&[
                peer,
                endpoint_type,
                source,
                result.as_str(),
            ]);
        }
    }

    /// Mark a group in-flight and spawn its probe onto `self.tasks`.
    fn spawn_probe(&mut self, epoch: u64, key: &GroupKey) {
        let targets = {
            let group = self
                .groups
                .get_mut(key)
                .expect("a due group is present in the map");
            group.probing = true;
            group.targets.clone()
        };
        let key = key.clone();
        let network = self.network.clone();
        let own_consensus_keypair = self.own_consensus_keypair.clone();
        let consensus_probe_timeout = self.consensus_probe_timeout;
        let inflight_probe_limit = self.inflight_probe_limit.clone();
        self.tasks.push(
            async move {
                let outcomes = join_all(targets.into_iter().map(|target| {
                    let network = network.clone();
                    let own_consensus_keypair = own_consensus_keypair.clone();
                    let inflight_probe_limit = inflight_probe_limit.clone();
                    async move {
                        let _permit = inflight_probe_limit
                            .acquire_owned()
                            .await
                            .expect("prober semaphore is never closed");
                        let address = target.display();
                        let result = probe_one(
                            &target,
                            &network,
                            &own_consensus_keypair,
                            epoch,
                            consensus_probe_timeout,
                        )
                        .await;
                        AddressOutcome { address, result }
                    }
                }))
                .await;
                (key, outcomes)
            }
            .boxed(),
        );
    }

    fn handle_probe_result(&mut self, key: GroupKey, outcomes: Vec<AddressOutcome>) {
        // The group may have been dropped from the map if it churned out while probing; if so, the
        // result is stale — discard it.
        let Some(group) = self.groups.get_mut(&key) else {
            return;
        };
        group.probing = false;

        let now = Instant::now();
        let connectable = outcomes.iter().any(|outcome| outcome.result.is_reachable());
        let peer_label = group.peer_label.clone();
        let endpoint_type = group.endpoint_type.as_str();
        let source = address_source_str(group.source);
        let labels = [peer_label.as_str(), endpoint_type, source];

        for outcome in &outcomes {
            self.metrics
                .attempts_total
                .with_label_values(&[
                    peer_label.as_str(),
                    endpoint_type,
                    source,
                    outcome.result.as_str(),
                ])
                .inc();
            debug!(
                peer = %peer_label,
                endpoint_type,
                source,
                address = %outcome.address,
                result = outcome.result.as_str(),
                "probed address"
            );
        }

        // Update the smoothed connectability gauge: set on any reachable probe, cleared only after
        // `failure_threshold` consecutive failures so transient blips don't flap the gauge.
        if connectable {
            self.metrics.connectable.with_label_values(&labels).set(1);
            let timestamp = now_unix_seconds();
            self.metrics
                .last_success_timestamp_seconds
                .with_label_values(&labels)
                .set(timestamp);
            group.connectable = true;
            group.last_success_unix_secs = Some(timestamp);
            group.consecutive_failures = 0;
        } else {
            group.consecutive_failures += 1;
            if group.consecutive_failures >= self.failure_threshold {
                self.metrics.connectable.with_label_values(&labels).set(0);
                group.connectable = false;
            }
        }
        group.last_probed = Some(now);
        group.outcomes = outcomes;
    }

    /// Snapshot the latest probe results for manual inspection.
    fn build_report(&self) -> ProbeReport {
        let now = Instant::now();
        let mut groups: Vec<ProbeGroupReport> = self
            .groups
            .values()
            .filter_map(|group| {
                let last_probed = group.last_probed?;
                Some(ProbeGroupReport {
                    peer: group.peer_label.clone(),
                    authority_name: group
                        .authority
                        .as_ref()
                        .map(|authority| authority.authority_name.clone()),
                    hostname: group
                        .authority
                        .as_ref()
                        .map(|authority| authority.hostname.clone()),
                    endpoint_type: group.endpoint_type.as_str().to_string(),
                    address_source: address_source_str(group.source).to_string(),
                    connectable: group.connectable,
                    consecutive_failures: group.consecutive_failures,
                    seconds_since_last_probe: now.duration_since(last_probed).as_secs(),
                    last_success_unix_seconds: group.last_success_unix_secs,
                    addresses: group
                        .outcomes
                        .iter()
                        .map(|outcome| ProbeAddressReport {
                            address: outcome.address.clone(),
                            result: outcome.result.as_str().to_string(),
                        })
                        .collect(),
                })
            })
            .collect();

        // Surface problems first: most-failed groups on top, then a stable label ordering.
        groups.sort_by(|a, b| {
            b.consecutive_failures
                .cmp(&a.consecutive_failures)
                .then_with(|| a.endpoint_type.cmp(&b.endpoint_type))
                .then_with(|| a.peer.cmp(&b.peer))
                .then_with(|| a.address_source.cmp(&b.address_source))
        });

        ProbeReport {
            epoch: self.epoch_state.as_ref().map(|context| context.epoch),
            groups,
        }
    }
}

/// Build the set of probe groups for this cycle: discovery's per-source P2P addresses, the
/// consensus override addresses, and the consensus on-chain (`Chain`) baseline from the committee.
fn build_groups(
    trusted_p2p_addresses: TrustedPeerP2pAddresses,
    consensus_manager: &ConsensusManager,
    committee: &ConsensusCommittee,
    own_peer_id: &PeerId,
    own_consensus_key: &ConsensusNetworkPublicKey,
) -> Vec<ProbeGroup> {
    let mut groups = Vec::new();

    // Index committee validators by their 32-byte network key so each group can be tagged with the
    // validator's name + hostname. The anemo `PeerId` is these same bytes (both are the validator's
    // `narwhal_network_pubkey`), so this resolves P2P peers as well as consensus ones; trusted
    // non-validator peers (seeds / fullnodes) simply don't match and stay unnamed.
    let authority_by_network_key: HashMap<[u8; 32], &ConsensusAuthority> = committee
        .authorities()
        .map(|(_, authority)| (authority.network_key.to_bytes(), authority))
        .collect();

    // (a) P2P: every source for every trusted peer.
    for (peer_id, sources) in trusted_p2p_addresses {
        if &peer_id == own_peer_id {
            continue;
        }
        let authority = authority_by_network_key
            .get(&peer_id.0)
            .copied()
            .map(authority_info);
        for (source, addresses) in sources {
            if addresses.is_empty() {
                continue;
            }
            groups.push(ProbeGroup {
                peer_label: peer_id.to_string(),
                endpoint_type: EndpointType::P2p,
                source,
                authority: authority.clone(),
                addresses: addresses
                    .into_iter()
                    .map(|address| AddressTarget::P2p { peer_id, address })
                    .collect(),
            });
        }
    }

    // (b) Consensus overrides (Discovery/Admin), per source.
    for (network_key, sources) in consensus_manager.address_overrides_snapshot() {
        if &network_key == own_consensus_key {
            continue;
        }
        let authority = authority_by_network_key
            .get(&network_key.to_bytes())
            .copied()
            .map(authority_info);
        for (source, addresses) in sources {
            if addresses.is_empty() {
                continue;
            }
            groups.push(ProbeGroup {
                peer_label: consensus_peer_label(&network_key),
                endpoint_type: EndpointType::Consensus,
                source,
                authority: authority.clone(),
                addresses: addresses
                    .into_iter()
                    .map(|address| AddressTarget::Consensus {
                        target_key: network_key.clone(),
                        address,
                    })
                    .collect(),
            });
        }
    }

    // (c) Consensus on-chain baseline (the committee address), labeled `Chain`.
    for (_index, authority) in committee.authorities() {
        if &authority.network_key == own_consensus_key {
            continue;
        }
        groups.push(ProbeGroup {
            peer_label: consensus_peer_label(&authority.network_key),
            endpoint_type: EndpointType::Consensus,
            source: AddressSource::Chain,
            authority: Some(authority_info(authority)),
            addresses: vec![AddressTarget::Consensus {
                target_key: authority.network_key.clone(),
                address: authority.address.clone(),
            }],
        });
    }

    groups
}

/// Operator-facing identity (name + hostname) for a committee validator.
fn authority_info(authority: &ConsensusAuthority) -> AuthorityInfo {
    AuthorityInfo {
        authority_name: Hex::encode(authority.authority_name.to_bytes()),
        hostname: authority.hostname.clone(),
    }
}

async fn probe_one(
    target: &AddressTarget,
    network: &Network,
    own_consensus_keypair: &ConsensusNetworkKeyPair,
    epoch: u64,
    consensus_probe_timeout: Duration,
) -> ProbeResult {
    match target {
        AddressTarget::P2p { peer_id, address } => network
            .probe_address(address.clone(), *peer_id)
            .await
            .into(),
        AddressTarget::Consensus {
            target_key,
            address,
        } => {
            probe_consensus_address(
                own_consensus_keypair,
                target_key,
                epoch,
                address,
                consensus_probe_timeout,
            )
            .await
        }
    }
}

/// Replicates the real consensus client's mutual-TLS setup (`tonic_network::get_channel`) in a
/// throwaway endpoint — expected peer network key, `consensus_epoch_{epoch}` server name, and our
/// own network key as the client cert — then eagerly connects with a bounded timeout and drops the
/// connection. Does not use the shared channel pool (that caches one channel per peer and would
/// defeat per-source probing).
async fn probe_consensus_address(
    own_consensus_keypair: &ConsensusNetworkKeyPair,
    target_key: &ConsensusNetworkPublicKey,
    epoch: u64,
    address: &Multiaddr,
    timeout: Duration,
) -> ProbeResult {
    let Some(host_port) = consensus_host_port(address) else {
        return ProbeResult::BadAddress;
    };
    let uri = format!("https://{host_port}");

    // Matches `consensus/core/src/network/tonic_tls.rs::certificate_server_name`.
    let server_name = format!("consensus_epoch_{epoch}");
    let client_tls_config = sui_tls::create_rustls_client_config(
        target_key.clone().into_inner(),
        server_name,
        Some(own_consensus_keypair.clone().private_key().into_inner()),
    );

    let endpoint = match tonic_rustls::Channel::from_shared(uri) {
        Ok(endpoint) => endpoint.connect_timeout(timeout),
        Err(_) => return ProbeResult::BadAddress,
    };
    let endpoint = match endpoint.tls_config(client_tls_config) {
        Ok(endpoint) => endpoint,
        Err(_) => return ProbeResult::BadAddress,
    };

    match tokio::time::timeout(timeout, endpoint.connect()).await {
        Ok(Ok(_channel)) => ProbeResult::Reachable,
        // A failed connect covers both unreachable endpoints and TLS failures (e.g. wrong key or
        // wrong epoch); tonic does not let us cleanly distinguish them here.
        Ok(Err(_)) => ProbeResult::Unreachable,
        Err(_) => ProbeResult::Timeout,
    }
}

/// host:port for the tonic URI, bracketing IPv6 literals. Mirrors
/// `consensus/core/src/network/mod.rs::to_host_port_str` for `/ip{4,6}|dns/.../udp/{port}`.
fn consensus_host_port(addr: &Multiaddr) -> Option<String> {
    let host = addr.hostname()?;
    let port = addr.port()?;
    if host.contains(':') {
        Some(format!("[{host}]:{port}"))
    } else {
        Some(format!("{host}:{port}"))
    }
}

/// Key identifying a group in the per-group scheduling/smoothing state.
fn group_key(group: &ProbeGroup) -> GroupKey {
    (
        group.peer_label.clone(),
        group.endpoint_type.as_str(),
        address_source_str(group.source),
    )
}

/// Metric `peer_id` label for a consensus endpoint: the hex-encoded network public key. (Consensus
/// peers are keyed by network key, not by anemo `PeerId`.)
fn consensus_peer_label(key: &ConsensusNetworkPublicKey) -> String {
    Hex::encode(key.to_bytes())
}

/// Test helper: compute the consensus `peer_id` metric label from raw network public key bytes,
/// matching [`consensus_peer_label`].
#[cfg(any(test, msim))]
pub fn consensus_peer_label_for_testing(network_key_bytes: [u8; 32]) -> String {
    Hex::encode(network_key_bytes)
}

fn address_source_str(source: AddressSource) -> &'static str {
    match source {
        AddressSource::Admin => "admin",
        AddressSource::Config => "config",
        AddressSource::Discovery => "discovery",
        AddressSource::Seed => "seed",
        AddressSource::Chain => "chain",
    }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
