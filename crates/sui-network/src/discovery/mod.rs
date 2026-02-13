// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::types::PeerAffinity;
use anemo::types::PeerInfo;
use anemo::{Network, Peer, PeerId, Request, Response, types::PeerEvent};
use fastcrypto::ed25519::{Ed25519PublicKey, Ed25519Signature};
use futures::StreamExt;
use mysten_common::debug_fatal;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentScope;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::endpoint_manager::{AddressSource, EndpointId, EndpointManager};
use sui_config::p2p::{AccessType, DiscoveryConfig, P2pConfig};
use sui_types::crypto::{NetworkKeyPair, NetworkPublicKey, Signer, ToFromBytes, VerifyingKey};
use sui_types::digests::Digest;
use sui_types::message_envelope::{Envelope, Message, VerifiedEnvelope};
use sui_types::multiaddr::Multiaddr;
use tap::{Pipe, TapFallible};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio::{
    sync::oneshot,
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, trace};

const TIMEOUT: Duration = Duration::from_secs(1);
const ONE_DAY_MILLISECONDS: u64 = 24 * 60 * 60 * 1_000;
const MAX_ADDRESS_LENGTH: usize = 300;
const MAX_PEERS_TO_SEND: usize = 200;
const MAX_ADDRESSES_PER_PEER: usize = 2;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.Discovery.rs"));
}
mod builder;
mod metrics;
mod server;
#[cfg(test)]
mod tests;

pub use builder::{Builder, UnstartedDiscovery};
pub use generated::{
    discovery_client::DiscoveryClient,
    discovery_server::{Discovery, DiscoveryServer},
};
pub use server::{GetKnownPeersRequestV3, GetKnownPeersResponseV2, GetKnownPeersResponseV3};

/// Message types for the discovery system mailbox.
#[derive(Debug)]
pub enum DiscoveryMessage {
    PeerAddressChange {
        peer_id: PeerId,
        source: AddressSource,
        addresses: Vec<anemo::types::Address>,
    },
    ReceivedNodeInfo {
        peer_info: Box<SignedVersionedNodeInfo>,
    },
}

/// A Handle to the Discovery subsystem. The Discovery system will be shut down once all Handles
/// have been dropped.
#[derive(Clone, Debug)]
pub struct Handle {
    pub(super) _shutdown_handle: Arc<oneshot::Sender<()>>,
    pub(super) sender: Sender,
}

impl Handle {
    pub fn sender(&self) -> Sender {
        self.sender.clone()
    }
}

/// A lightweight handle for sending messages to the discovery event loop
/// without holding a shutdown reference.
#[derive(Clone, Debug)]
pub struct Sender {
    pub(super) sender: mpsc::Sender<DiscoveryMessage>,
}

impl Sender {
    pub fn peer_address_change(
        &self,
        peer_id: PeerId,
        source: AddressSource,
        addresses: Vec<anemo::types::Address>,
    ) {
        self.sender
            .try_send(DiscoveryMessage::PeerAddressChange {
                peer_id,
                source,
                addresses,
            })
            .expect("Discovery mailbox should not overflow or be closed")
    }
}

use self::metrics::Metrics;

/// The internal discovery state shared between the main event loop and the request handler
struct State {
    our_info: Option<SignedNodeInfo>,
    our_info_v2: Option<SignedVersionedNodeInfo>,
    connected_peers: HashMap<PeerId, ()>,
    known_peers: HashMap<PeerId, VerifiedSignedNodeInfo>,
    known_peers_v2: HashMap<PeerId, VerifiedSignedVersionedNodeInfo>,
    peer_address_overrides: HashMap<PeerId, BTreeMap<AddressSource, Vec<anemo::types::Address>>>,
}

/// The information necessary to dial another peer.
///
/// `NodeInfo` contains all the information that is shared with other nodes via the discovery
/// service to advertise how a node can be reached.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeInfo {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,

    /// Creation time.
    ///
    /// This is used to determine which of two NodeInfo's from the same PeerId should be retained.
    pub timestamp_ms: u64,

    /// See docstring for `AccessType`.
    pub access_type: AccessType,
}

impl NodeInfo {
    fn sign(self, keypair: &NetworkKeyPair) -> SignedNodeInfo {
        let msg = bcs::to_bytes(&self).expect("BCS serialization should not fail");
        let sig = keypair.sign(&msg);
        SignedNodeInfo::new_from_data_and_sig(self, sig)
    }
}

pub type SignedNodeInfo = Envelope<NodeInfo, Ed25519Signature>;

pub type VerifiedSignedNodeInfo = VerifiedEnvelope<NodeInfo, Ed25519Signature>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NodeInfoDigest(Digest);

impl NodeInfoDigest {
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }
}

impl Message for NodeInfo {
    type DigestType = NodeInfoDigest;
    const SCOPE: IntentScope = IntentScope::DiscoveryPeers;

    fn digest(&self) -> Self::DigestType {
        unreachable!("NodeInfoDigest is not used today")
    }
}

/// NodeInfoV2 supports multiple address types keyed by EndpointId.
// TODO: Remove support for V1 once V2 is available in all production networks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfoV2 {
    pub addresses: BTreeMap<EndpointId, Vec<Multiaddr>>,
    pub timestamp_ms: u64,
    pub access_type: AccessType,
}

impl NodeInfoV2 {
    /// Derive the P2P PeerId from the addresses map.
    /// Returns None if no P2P endpoint is present.
    pub fn peer_id(&self) -> Option<PeerId> {
        self.addresses.keys().find_map(|k| match k {
            EndpointId::P2p(peer_id) => Some(*peer_id),
            EndpointId::Consensus(_) => None,
        })
    }

    pub fn p2p_addresses(&self) -> &[Multiaddr] {
        self.addresses
            .iter()
            .find_map(|(k, v)| match k {
                EndpointId::P2p(_) => Some(v.as_slice()),
                EndpointId::Consensus(_) => None,
            })
            .unwrap_or(&[])
    }
}

/// Versioned wrapper for NodeInfo types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionedNodeInfo {
    V1(NodeInfo),
    V2(NodeInfoV2),
}

impl VersionedNodeInfo {
    pub fn peer_id(&self) -> Option<PeerId> {
        match self {
            VersionedNodeInfo::V1(info) => Some(info.peer_id),
            VersionedNodeInfo::V2(info) => info.peer_id(),
        }
    }

    pub fn timestamp_ms(&self) -> u64 {
        match self {
            VersionedNodeInfo::V1(info) => info.timestamp_ms,
            VersionedNodeInfo::V2(info) => info.timestamp_ms,
        }
    }

    pub fn access_type(&self) -> AccessType {
        match self {
            VersionedNodeInfo::V1(info) => info.access_type,
            VersionedNodeInfo::V2(info) => info.access_type,
        }
    }

    pub fn p2p_addresses(&self) -> &[Multiaddr] {
        match self {
            VersionedNodeInfo::V1(info) => &info.addresses,
            VersionedNodeInfo::V2(info) => info.p2p_addresses(),
        }
    }

    pub fn sign(self, keypair: &NetworkKeyPair) -> SignedVersionedNodeInfo {
        let msg = bcs::to_bytes(&self).expect("BCS serialization should not fail");
        let sig = keypair.sign(&msg);
        SignedVersionedNodeInfo::new_from_data_and_sig(self, sig)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VersionedNodeInfoDigest(Digest);

impl Message for VersionedNodeInfo {
    type DigestType = VersionedNodeInfoDigest;
    const SCOPE: IntentScope = IntentScope::DiscoveryPeers;

    fn digest(&self) -> Self::DigestType {
        unreachable!("VersionedNodeInfoDigest is not used today")
    }
}

pub type SignedVersionedNodeInfo = Envelope<VersionedNodeInfo, Ed25519Signature>;
pub type VerifiedSignedVersionedNodeInfo = VerifiedEnvelope<VersionedNodeInfo, Ed25519Signature>;

struct DiscoveryEventLoop {
    config: P2pConfig,
    discovery_config: Arc<DiscoveryConfig>,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
    unidentified_seed_peers: Vec<anemo::types::Address>,
    network: Network,
    keypair: NetworkKeyPair,
    tasks: JoinSet<()>,
    pending_dials: HashMap<PeerId, AbortHandle>,
    dial_seed_peers_task: Option<AbortHandle>,
    shutdown_handle: oneshot::Receiver<()>,
    state: Arc<RwLock<State>>,
    mailbox: mpsc::Receiver<DiscoveryMessage>,
    metrics: Metrics,
    consensus_external_address: Option<Multiaddr>,
    endpoint_manager: EndpointManager,
}

impl DiscoveryEventLoop {
    pub async fn start(mut self) {
        info!("Discovery started");

        self.construct_our_info();
        self.configure_preferred_peers();

        let mut interval = tokio::time::interval(self.discovery_config.interval_period());
        let mut peer_events = {
            let (subscriber, _peers) = self.network.subscribe().unwrap();
            subscriber
        };

        loop {
            tokio::select! {
                now = interval.tick() => {
                    let now_unix = now_unix();
                    self.handle_tick(now.into_std(), now_unix);
                }
                peer_event = peer_events.recv() => {
                    self.handle_peer_event(peer_event);
                },
                Some(message) = self.mailbox.recv() => {
                    self.handle_message(message);
                }
                Some(task_result) = self.tasks.join_next() => {
                    match task_result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                                // avoid crashing on ungraceful shutdown
                            } else if e.is_panic() {
                                // propagate panics.
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("task failed: {e}");
                            }
                        },
                    };
                },
                // Once the shutdown notification resolves we can terminate the event loop
                _ = &mut self.shutdown_handle => {
                    break;
                }
            }
        }

        info!("Discovery ended");
    }

    fn handle_message(&mut self, message: DiscoveryMessage) {
        match message {
            DiscoveryMessage::PeerAddressChange {
                peer_id,
                source,
                addresses,
            } => {
                self.handle_peer_address_change(peer_id, source, addresses);
            }
            DiscoveryMessage::ReceivedNodeInfo { peer_info } => {
                update_known_peers_versioned(
                    self.state.clone(),
                    self.metrics.clone(),
                    vec![*peer_info],
                    self.configured_peers.clone(),
                    &self.endpoint_manager,
                );
            }
        }
    }

    fn construct_our_info(&mut self) {
        if self.state.read().unwrap().our_info.is_some() {
            return;
        }

        let peer_id = self.network.peer_id();
        let timestamp_ms = now_unix();
        let access_type = self.discovery_config.access_type();

        let addresses: Vec<Multiaddr> = self
            .config
            .external_address
            .clone()
            .and_then(|addr| addr.to_anemo_address().ok().map(|_| addr))
            .into_iter()
            .collect();

        let our_info = NodeInfo {
            peer_id,
            addresses: addresses.clone(),
            timestamp_ms,
            access_type,
        }
        .sign(&self.keypair);

        let mut addresses_map = BTreeMap::new();
        addresses_map.insert(EndpointId::P2p(peer_id), addresses);
        if let Some(consensus_addr) = &self.consensus_external_address {
            // Populates `Consensus` EndpointId from our P2P (anemo) `PeerId`.
            // This is safe because both the P2P and consensus networks use the same
            // ed25519 network keypair. Both originate from `NodeConfig::network_key_pair()`.
            let network_pubkey =
                NetworkPublicKey::from_bytes(&peer_id.0).expect("PeerId is a valid public key");
            addresses_map.insert(
                EndpointId::Consensus(network_pubkey),
                vec![consensus_addr.clone()],
            );
        }
        let our_info_v2 = VersionedNodeInfo::V2(NodeInfoV2 {
            addresses: addresses_map,
            timestamp_ms,
            access_type,
        })
        .sign(&self.keypair);

        let mut state = self.state.write().unwrap();
        state.our_info = Some(our_info);
        state.our_info_v2 = Some(our_info_v2);
    }

    fn configure_preferred_peers(&mut self) {
        for peer_info in self.configured_peers.values() {
            debug!(?peer_info, "Add configured preferred peer");
            self.network.known_peers().insert(peer_info.clone());
        }
    }

    fn update_our_info_timestamp(&mut self, now_unix: u64) {
        let state = &mut self.state.write().unwrap();

        if let Some(our_info) = &state.our_info {
            let mut data = our_info.data().clone();
            data.timestamp_ms = now_unix;
            state.our_info = Some(data.sign(&self.keypair));
        }

        if let Some(our_info_v2) = &state.our_info_v2 {
            let mut data = our_info_v2.data().clone();
            match &mut data {
                VersionedNodeInfo::V1(info) => info.timestamp_ms = now_unix,
                VersionedNodeInfo::V2(info) => info.timestamp_ms = now_unix,
            }
            state.our_info_v2 = Some(data.sign(&self.keypair));
        }
    }

    fn handle_peer_address_change(
        &mut self,
        peer_id: PeerId,
        source: AddressSource,
        addresses: Vec<anemo::types::Address>,
    ) {
        debug!(
            ?peer_id,
            ?source,
            ?addresses,
            "Received peer address change"
        );

        // Update stored addresses.
        {
            let mut state = self.state.write().unwrap();
            let source_map = state.peer_address_overrides.entry(peer_id).or_default();

            if addresses.is_empty() {
                source_map.remove(&source);
                if source_map.is_empty() {
                    state.peer_address_overrides.remove(&peer_id);
                }
            } else {
                source_map.insert(source, addresses);
            }
        }

        // Reconfigure network if priority addresses changed.
        let priority_addresses = self
            .state
            .read()
            .unwrap()
            .peer_address_overrides
            .get(&peer_id)
            .and_then(|sources| sources.first_key_value().map(|(_, addrs)| addrs.clone()))
            .unwrap_or_default();
        let current_addresses = self
            .network
            .known_peers()
            .get(&peer_id)
            .map(|info| info.address.clone())
            .unwrap_or_default();
        if priority_addresses != current_addresses {
            let new_peer_info = PeerInfo {
                peer_id,
                affinity: PeerAffinity::High,
                address: priority_addresses.clone(),
            };

            self.network.known_peers().insert(new_peer_info);
            let _ = self.network.disconnect(peer_id);

            if let Some(address) = priority_addresses.first().cloned() {
                let network = self.network.clone();
                self.tasks.spawn(async move {
                    // If this fails, ConnectionManager will retry.
                    let _ = network.connect_with_peer_id(address, peer_id).await;
                });
            }
        }
    }

    fn handle_peer_event(&mut self, peer_event: Result<PeerEvent, RecvError>) {
        match peer_event {
            Ok(PeerEvent::NewPeer(peer_id)) => {
                if let Some(peer) = self.network.peer(peer_id) {
                    self.state
                        .write()
                        .unwrap()
                        .connected_peers
                        .insert(peer_id, ());

                    // Query the new node for any peers
                    self.tasks.spawn(query_peer_for_their_known_peers(
                        peer,
                        self.discovery_config.clone(),
                        self.state.clone(),
                        self.metrics.clone(),
                        self.configured_peers.clone(),
                        self.endpoint_manager.clone(),
                    ));
                }
            }
            Ok(PeerEvent::LostPeer(peer_id, _)) => {
                self.state.write().unwrap().connected_peers.remove(&peer_id);
            }

            Err(RecvError::Closed) => {
                panic!("PeerEvent channel shouldn't be able to be closed");
            }

            Err(RecvError::Lagged(_)) => {
                trace!("State-Sync fell behind processing PeerEvents");
            }
        }
    }

    fn handle_tick(&mut self, _now: std::time::Instant, now_unix: u64) {
        self.update_our_info_timestamp(now_unix);

        self.tasks
            .spawn(query_connected_peers_for_their_known_peers(
                self.network.clone(),
                self.discovery_config.clone(),
                self.state.clone(),
                self.metrics.clone(),
                self.configured_peers.clone(),
                self.endpoint_manager.clone(),
            ));

        // Cull old peers older than a day
        {
            let mut state = self.state.write().unwrap();
            state
                .known_peers
                .retain(|_k, v| now_unix.saturating_sub(v.timestamp_ms) < ONE_DAY_MILLISECONDS);
            state
                .known_peers_v2
                .retain(|_k, v| now_unix.saturating_sub(v.timestamp_ms()) < ONE_DAY_MILLISECONDS);
        }

        // Clean out the pending_dials
        self.pending_dials.retain(|_k, v| !v.is_finished());
        if let Some(abort_handle) = &self.dial_seed_peers_task
            && abort_handle.is_finished()
        {
            self.dial_seed_peers_task = None;
        }

        // Spawn some dials
        let state = self.state.read().unwrap();
        let our_peer_id = self.network.peer_id();

        // Collect eligible peers from both known_peers (V2) and known_peers_v2 (V3),
        // preferring fresher timestamps when a peer appears in both maps.
        let mut eligible: HashMap<PeerId, NodeInfo> = HashMap::new();

        for (peer_id, info) in state.known_peers.iter() {
            if *peer_id != our_peer_id
                && !info.addresses.is_empty()
                && !state.connected_peers.contains_key(peer_id)
                && !self.pending_dials.contains_key(peer_id)
            {
                eligible.insert(*peer_id, info.data().clone());
            }
        }
        for (peer_id, info) in state.known_peers_v2.iter() {
            let p2p_addresses = info.p2p_addresses();
            if *peer_id != our_peer_id
                && !p2p_addresses.is_empty()
                && !state.connected_peers.contains_key(peer_id)
                && !self.pending_dials.contains_key(peer_id)
                && eligible
                    .get(peer_id)
                    .is_none_or(|existing| info.timestamp_ms() > existing.timestamp_ms)
            {
                eligible.insert(
                    *peer_id,
                    NodeInfo {
                        peer_id: *peer_id,
                        addresses: p2p_addresses.to_vec(),
                        timestamp_ms: info.timestamp_ms(),
                        access_type: info.access_type(),
                    },
                );
            }
        }

        // No need to connect to any more peers if we're already connected to a bunch
        let number_of_connections = state.connected_peers.len();
        let number_to_dial = std::cmp::min(
            eligible.len(),
            self.discovery_config
                .target_concurrent_connections()
                .saturating_sub(number_of_connections),
        );

        // randomize the order
        use rand::seq::IteratorRandom;
        for (peer_id, info) in eligible
            .into_iter()
            .choose_multiple(&mut rand::thread_rng(), number_to_dial)
        {
            let abort_handle = self
                .tasks
                .spawn(try_to_connect_to_peer(self.network.clone(), info));
            self.pending_dials.insert(peer_id, abort_handle);
        }

        // If we aren't connected to anything and we aren't presently trying to connect to anyone
        // we need to try the configured peers with High affinity (seed peers)
        let has_peers_to_dial = || {
            self.configured_peers
                .values()
                .any(|p| p.affinity == PeerAffinity::High)
                || !self.unidentified_seed_peers.is_empty()
        };
        if self.dial_seed_peers_task.is_none()
            && state.connected_peers.is_empty()
            && self.pending_dials.is_empty()
            && has_peers_to_dial()
        {
            let abort_handle = self.tasks.spawn(try_to_connect_to_seed_peers(
                self.network.clone(),
                self.discovery_config.clone(),
                self.configured_peers.clone(),
                self.unidentified_seed_peers.clone(),
            ));

            self.dial_seed_peers_task = Some(abort_handle);
        }
    }
}

async fn try_to_connect_to_peer(network: Network, info: NodeInfo) {
    debug!("Connecting to peer {info:?}");
    for multiaddr in &info.addresses {
        if let Ok(address) = multiaddr.to_anemo_address() {
            // Ignore the result and just log the error if there is one
            if network
                .connect_with_peer_id(address, info.peer_id)
                .await
                .tap_err(|e| {
                    debug!(
                        "error dialing {} at address '{}': {e}",
                        info.peer_id.short_display(4),
                        multiaddr
                    )
                })
                .is_ok()
            {
                return;
            }
        }
    }
}

async fn try_to_connect_to_seed_peers(
    network: Network,
    config: Arc<DiscoveryConfig>,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
    unidentified_seed_peers: Vec<anemo::types::Address>,
) {
    let high_affinity_peers: Vec<_> = configured_peers
        .values()
        .filter(|p| p.affinity == PeerAffinity::High)
        .cloned()
        .collect();
    debug!(
        ?high_affinity_peers,
        ?unidentified_seed_peers,
        "Connecting to seed peers"
    );
    let network = &network;

    // Attempt connection to all high-affinity and seed peers.
    let with_peer_id = high_affinity_peers.into_iter().flat_map(|peer_info| {
        peer_info
            .address
            .into_iter()
            .map(move |addr| (Some(peer_info.peer_id), addr))
    });
    let without_peer_id = unidentified_seed_peers.into_iter().map(|addr| (None, addr));
    futures::stream::iter(with_peer_id.chain(without_peer_id))
        .for_each_concurrent(
            config.target_concurrent_connections(),
            |(peer_id, address)| async move {
                // Ignore the result and just log the error if there is one
                let _ = if let Some(peer_id) = peer_id {
                    network
                        .connect_with_peer_id(address.clone(), peer_id)
                        .await
                        .tap_err(|e| {
                            debug!(
                                "error dialing peer {} at '{}': {e}",
                                peer_id.short_display(4),
                                address
                            )
                        })
                } else {
                    network
                        .connect(address.clone())
                        .await
                        .tap_err(|e| debug!("error dialing address '{}': {e}", address))
                };
            },
        )
        .await;
}

async fn query_peer_for_known_peers_v2(peer: Peer) -> Option<Vec<SignedNodeInfo>> {
    let mut client = DiscoveryClient::new(peer);
    let request = Request::new(()).with_timeout(TIMEOUT);
    client
        .get_known_peers_v2(request)
        .await
        .ok()
        .map(Response::into_inner)
        .map(
            |GetKnownPeersResponseV2 {
                 own_info,
                 mut known_peers,
             }| {
                if !own_info.addresses.is_empty() {
                    known_peers.push(own_info)
                }
                known_peers
            },
        )
}

async fn query_peer_for_their_known_peers(
    peer: Peer,
    discovery_config: Arc<DiscoveryConfig>,
    state: Arc<RwLock<State>>,
    metrics: Metrics,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
    endpoint_manager: EndpointManager,
) {
    // Query V3 concurrently with V2 when enabled
    if discovery_config.use_get_known_peers_v3() {
        let our_info_v2 = state.read().unwrap().our_info_v2.clone();
        if let Some(own_info) = our_info_v2 {
            let peer_for_v3 = peer.clone();
            let v3_query = async move {
                let mut client = DiscoveryClient::new(peer_for_v3);
                let request =
                    Request::new(GetKnownPeersRequestV3 { own_info }).with_timeout(TIMEOUT);
                client
                    .get_known_peers_v3(request)
                    .await
                    .ok()
                    .map(Response::into_inner)
                    .map(
                        |GetKnownPeersResponseV3 {
                             own_info,
                             mut known_peers,
                         }| {
                            if !own_info.p2p_addresses().is_empty() {
                                known_peers.push(own_info)
                            }
                            known_peers
                        },
                    )
            };

            let (found_peers_v2, found_peers_v3) =
                tokio::join!(query_peer_for_known_peers_v2(peer), v3_query);

            if let Some(found_peers) = found_peers_v2 {
                update_known_peers(
                    state.clone(),
                    metrics.clone(),
                    found_peers,
                    configured_peers.clone(),
                );
            }
            if let Some(found_peers) = found_peers_v3 {
                update_known_peers_versioned(
                    state,
                    metrics,
                    found_peers,
                    configured_peers,
                    &endpoint_manager,
                );
            }
            return;
        }
    }

    // V3 not enabled or our_info_v2 not available - just run V2
    if let Some(found_peers) = query_peer_for_known_peers_v2(peer).await {
        update_known_peers(state, metrics, found_peers, configured_peers);
    }
}

async fn query_connected_peers_for_their_known_peers(
    network: Network,
    config: Arc<DiscoveryConfig>,
    state: Arc<RwLock<State>>,
    metrics: Metrics,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
    endpoint_manager: EndpointManager,
) {
    use rand::seq::IteratorRandom;

    let peers_to_query: Vec<_> = network
        .peers()
        .into_iter()
        .flat_map(|id| network.peer(id))
        .choose_multiple(&mut rand::thread_rng(), config.peers_to_query());

    // Query V2 to keep known_peers populated for V2 clients
    let v2_query = {
        let peers = peers_to_query.clone();
        let peers_to_query_count = config.peers_to_query();
        async move {
            peers
                .into_iter()
                .map(DiscoveryClient::new)
                .map(|mut client| async move {
                    let request = Request::new(()).with_timeout(TIMEOUT);
                    client
                        .get_known_peers_v2(request)
                        .await
                        .ok()
                        .map(Response::into_inner)
                        .map(
                            |GetKnownPeersResponseV2 {
                                 own_info,
                                 mut known_peers,
                             }| {
                                if !own_info.addresses.is_empty() {
                                    known_peers.push(own_info)
                                }
                                known_peers
                            },
                        )
                })
                .pipe(futures::stream::iter)
                .buffer_unordered(peers_to_query_count)
                .filter_map(std::future::ready)
                .flat_map(futures::stream::iter)
                .collect::<Vec<_>>()
                .await
        }
    };

    // Additionally query V3 when enabled, concurrently with V2
    if config.use_get_known_peers_v3() {
        let our_info_v2 = state.read().unwrap().our_info_v2.clone();
        if let Some(own_info) = our_info_v2 {
            let v3_query = {
                let peers_to_query_count = config.peers_to_query();
                async move {
                    peers_to_query
                        .into_iter()
                        .map(DiscoveryClient::new)
                        .map(|mut client| {
                            let own_info = own_info.clone();
                            async move {
                                let request = Request::new(GetKnownPeersRequestV3 { own_info })
                                    .with_timeout(TIMEOUT);
                                client
                                    .get_known_peers_v3(request)
                                    .await
                                    .ok()
                                    .map(Response::into_inner)
                                    .map(
                                        |GetKnownPeersResponseV3 {
                                             own_info,
                                             mut known_peers,
                                         }| {
                                            if !own_info.p2p_addresses().is_empty() {
                                                known_peers.push(own_info)
                                            }
                                            known_peers
                                        },
                                    )
                            }
                        })
                        .pipe(futures::stream::iter)
                        .buffer_unordered(peers_to_query_count)
                        .filter_map(std::future::ready)
                        .flat_map(futures::stream::iter)
                        .collect::<Vec<_>>()
                        .await
                }
            };

            let (found_peers_v2, found_peers_v3) = tokio::join!(v2_query, v3_query);

            update_known_peers(
                state.clone(),
                metrics.clone(),
                found_peers_v2,
                configured_peers.clone(),
            );
            update_known_peers_versioned(
                state,
                metrics,
                found_peers_v3,
                configured_peers,
                &endpoint_manager,
            );
            return;
        }
    }

    // V3 not enabled or our_info_v2 not available - just run V2
    let found_peers_v2 = v2_query.await;
    update_known_peers(state, metrics, found_peers_v2, configured_peers);
}

fn update_known_peers(
    state: Arc<RwLock<State>>,
    metrics: Metrics,
    found_peers: Vec<SignedNodeInfo>,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
) {
    use std::collections::hash_map::Entry;

    let now_unix = now_unix();
    let our_peer_id = state.read().unwrap().our_info.clone().unwrap().peer_id;
    let known_peers = &mut state.write().unwrap().known_peers;
    // only take the first MAX_PEERS_TO_SEND peers
    for peer_info in found_peers.into_iter().take(MAX_PEERS_TO_SEND + 1) {
        // +1 to account for the "own_info" of the serving peer
        // Skip peers whose timestamp is too far in the future from our clock
        // or that are too old
        if peer_info.timestamp_ms > now_unix.saturating_add(30 * 1_000) // 30 seconds
            || now_unix.saturating_sub(peer_info.timestamp_ms) > ONE_DAY_MILLISECONDS
        {
            continue;
        }

        if peer_info.peer_id == our_peer_id {
            continue;
        }

        // If Peer is Private or Trusted, and not in our configured peers, skip it.
        let is_restricted = match peer_info.access_type {
            AccessType::Public => false,
            AccessType::Private | AccessType::Trusted => true,
        };
        if is_restricted && !configured_peers.contains_key(&peer_info.peer_id) {
            continue;
        }

        // Skip entries that have too many addresses as a means to cap the size of a node's info
        if peer_info.addresses.len() > MAX_ADDRESSES_PER_PEER {
            continue;
        }

        // verify that all addresses provided are valid anemo addresses
        if !peer_info
            .addresses
            .iter()
            .all(|addr| addr.len() < MAX_ADDRESS_LENGTH && addr.to_anemo_address().is_ok())
        {
            continue;
        }
        let Ok(public_key) = Ed25519PublicKey::from_bytes(&peer_info.peer_id.0) else {
            debug_fatal!(
                // This should never happen.
                "Failed to convert anemo PeerId {:?} to Ed25519PublicKey",
                peer_info.peer_id
            );
            continue;
        };
        let msg = bcs::to_bytes(peer_info.data()).expect("BCS serialization should not fail");
        if let Err(e) = public_key.verify(&msg, peer_info.auth_sig()) {
            info!(
                "Discovery failed to verify signature for NodeInfo for peer {:?}: {e:?}",
                peer_info.peer_id
            );
            // TODO: consider denylisting the source of bad NodeInfo from future requests.
            continue;
        }
        let peer = VerifiedSignedNodeInfo::new_from_verified(peer_info);

        match known_peers.entry(peer.peer_id) {
            Entry::Occupied(mut o) => {
                if peer.timestamp_ms > o.get().timestamp_ms {
                    if o.get().addresses.is_empty() && !peer.addresses.is_empty() {
                        metrics.inc_num_peers_with_external_address();
                    }
                    if !o.get().addresses.is_empty() && peer.addresses.is_empty() {
                        metrics.dec_num_peers_with_external_address();
                    }
                    o.insert(peer);
                }
            }
            Entry::Vacant(v) => {
                if !peer.addresses.is_empty() {
                    metrics.inc_num_peers_with_external_address();
                }
                v.insert(peer);
            }
        }
    }
}

fn update_known_peers_versioned(
    state: Arc<RwLock<State>>,
    metrics: Metrics,
    found_peers: Vec<SignedVersionedNodeInfo>,
    configured_peers: Arc<HashMap<PeerId, PeerInfo>>,
    endpoint_manager: &EndpointManager,
) {
    use std::collections::hash_map::Entry;

    let now_unix = now_unix();
    let our_peer_id = state
        .read()
        .unwrap()
        .our_info_v2
        .as_ref()
        .and_then(|info| info.peer_id());
    let known_peers_v2 = &mut state.write().unwrap().known_peers_v2;

    for peer_info in found_peers.into_iter().take(MAX_PEERS_TO_SEND + 1) {
        let timestamp_ms = peer_info.timestamp_ms();

        if timestamp_ms > now_unix.saturating_add(30 * 1_000)
            || now_unix.saturating_sub(timestamp_ms) > ONE_DAY_MILLISECONDS
        {
            continue;
        }

        let Some(peer_id) = peer_info.peer_id() else {
            continue;
        };

        if Some(peer_id) == our_peer_id {
            continue;
        }

        let is_restricted = match peer_info.access_type() {
            AccessType::Public => false,
            AccessType::Private | AccessType::Trusted => true,
        };
        if is_restricted && !configured_peers.contains_key(&peer_id) {
            continue;
        }

        let p2p_addresses = peer_info.p2p_addresses();
        if p2p_addresses.len() > MAX_ADDRESSES_PER_PEER {
            continue;
        }

        if !p2p_addresses
            .iter()
            .all(|addr| addr.len() < MAX_ADDRESS_LENGTH && addr.to_anemo_address().is_ok())
        {
            continue;
        }

        let Ok(public_key) = Ed25519PublicKey::from_bytes(&peer_id.0) else {
            debug_fatal!(
                "Failed to convert anemo PeerId {:?} to Ed25519PublicKey",
                peer_id
            );
            continue;
        };

        let msg = bcs::to_bytes(peer_info.data()).expect("BCS serialization should not fail");
        if let Err(e) = public_key.verify(&msg, peer_info.auth_sig()) {
            info!(
                "Discovery failed to verify signature for VersionedNodeInfo for peer {:?}: {e:?}",
                peer_id
            );
            continue;
        }

        // Forward non-P2P addresses from configured peers to EndpointManager.
        if configured_peers.contains_key(&peer_id)
            && let VersionedNodeInfo::V2(info_v2) = peer_info.data()
        {
            for (endpoint_id, addrs) in &info_v2.addresses {
                if !matches!(endpoint_id, EndpointId::P2p(_)) && !addrs.is_empty() {
                    let _ = endpoint_manager.update_endpoint(
                        endpoint_id.clone(),
                        AddressSource::Discovery,
                        addrs.clone(),
                    );
                }
            }
        }

        let peer = VerifiedSignedVersionedNodeInfo::new_from_verified(peer_info);
        let peer_p2p_addresses = peer.p2p_addresses();

        match known_peers_v2.entry(peer_id) {
            Entry::Occupied(mut o) => {
                if peer.timestamp_ms() > o.get().timestamp_ms() {
                    let old_addresses = o.get().p2p_addresses();
                    if old_addresses.is_empty() && !peer_p2p_addresses.is_empty() {
                        metrics.inc_num_peers_with_external_address();
                    }
                    if !old_addresses.is_empty() && peer_p2p_addresses.is_empty() {
                        metrics.dec_num_peers_with_external_address();
                    }
                    o.insert(peer);
                }
            }
            Entry::Vacant(v) => {
                if !peer_p2p_addresses.is_empty() {
                    metrics.inc_num_peers_with_external_address();
                }
                v.insert(peer);
            }
        }
    }
}

pub(super) fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
