// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::types::PeerInfo;
use anemo::{types::PeerEvent, Network, Peer, PeerId, Request, Response};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_config::p2p::{AccessType, DiscoveryConfig, P2pConfig, SeedPeer};
use sui_types::multiaddr::Multiaddr;
use tap::{Pipe, TapFallible};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::{
    sync::oneshot,
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, trace};

const TIMEOUT: Duration = Duration::from_secs(1);
const ONE_DAY_MILLISECONDS: u64 = 24 * 60 * 60 * 1_000;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.Discovery.rs"));
}
mod builder;
mod server;
#[cfg(test)]
mod tests;

pub use builder::{Builder, Handle, UnstartedDiscovery};
pub use generated::{
    discovery_client::DiscoveryClient,
    discovery_server::{Discovery, DiscoveryServer},
};
pub use server::GetKnownPeersResponse;

/// The internal discovery state shared between the main event loop and the request handler
struct State {
    our_info: Option<NodeInfo>,
    connected_peers: HashMap<PeerId, ()>,
    known_peers: HashMap<PeerId, NodeInfo>,
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

#[derive(Clone, Debug, Default)]
pub struct TrustedPeerChangeEvent {
    pub new_peers: Vec<PeerInfo>,
}

struct DiscoveryEventLoop {
    config: P2pConfig,
    discovery_config: Arc<DiscoveryConfig>,
    allowlisted_peers: Arc<HashMap<PeerId, Option<Multiaddr>>>,
    network: Network,
    tasks: JoinSet<()>,
    pending_dials: HashMap<PeerId, AbortHandle>,
    dial_seed_peers_task: Option<AbortHandle>,
    shutdown_handle: oneshot::Receiver<()>,
    state: Arc<RwLock<State>>,
    trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>,
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
                Ok(()) = self.trusted_peer_change_rx.changed() => {
                    let event: TrustedPeerChangeEvent = self.trusted_peer_change_rx.borrow_and_update().clone();
                    self.handle_trusted_peer_change_event(event);
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

    fn construct_our_info(&mut self) {
        if self.state.read().unwrap().our_info.is_some() {
            return;
        }

        let address = self
            .config
            .external_address
            .clone()
            .and_then(|addr| addr.to_anemo_address().ok().map(|_| addr))
            .into_iter()
            .collect();
        let our_info = NodeInfo {
            peer_id: self.network.peer_id(),
            addresses: address,
            timestamp_ms: now_unix(),
            access_type: self.discovery_config.access_type(),
        };

        self.state.write().unwrap().our_info = Some(our_info);
    }

    fn configure_preferred_peers(&mut self) {
        for (peer_id, address) in self
            .discovery_config
            .allowlisted_peers
            .iter()
            .map(|sp| (sp.peer_id, sp.address.clone()))
            .chain(self.config.seed_peers.iter().filter_map(|ap| {
                ap.peer_id
                    .map(|peer_id| (peer_id, Some(ap.address.clone())))
            }))
        {
            let anemo_address = if let Some(address) = address {
                let Ok(address) = address.to_anemo_address() else {
                    debug!(p2p_address=?address, "Can't convert p2p address to anemo address");
                    continue;
                };
                Some(address)
            } else {
                None
            };

            // TODO: once we have `PeerAffinity::Allowlisted` we should update allowlisted peers'
            // affinity.
            let peer_info = anemo::types::PeerInfo {
                peer_id,
                affinity: anemo::types::PeerAffinity::High,
                address: anemo_address.into_iter().collect(),
            };
            self.network.known_peers().insert(peer_info);
        }
    }

    fn update_our_info_timestamp(&mut self, now_unix: u64) {
        if let Some(our_info) = &mut self.state.write().unwrap().our_info {
            our_info.timestamp_ms = now_unix;
        }
    }

    // TODO: we don't boot out old committee member yets, however we may want to do this
    // in the future along with other network management work.
    fn handle_trusted_peer_change_event(
        &mut self,
        trusted_peer_change_event: TrustedPeerChangeEvent,
    ) {
        for peer_info in trusted_peer_change_event.new_peers {
            debug!(?peer_info, "Add committee member as preferred peer.");
            self.network.known_peers().insert(peer_info);
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
                        self.state.clone(),
                        self.allowlisted_peers.clone(),
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
                self.allowlisted_peers.clone(),
            ));

        // Cull old peers older than a day
        self.state
            .write()
            .unwrap()
            .known_peers
            .retain(|_k, v| now_unix.saturating_sub(v.timestamp_ms) < ONE_DAY_MILLISECONDS);

        // Clean out the pending_dials
        self.pending_dials.retain(|_k, v| !v.is_finished());
        if let Some(abort_handle) = &self.dial_seed_peers_task {
            if abort_handle.is_finished() {
                self.dial_seed_peers_task = None;
            }
        }

        // Spawn some dials
        let state = self.state.read().unwrap();
        let eligible = state
            .known_peers
            .clone()
            .into_iter()
            .filter(|(peer_id, info)| {
                peer_id != &self.network.peer_id() &&
                !info.addresses.is_empty() // Peer has addresses we can dial
                && !state.connected_peers.contains_key(peer_id) // We're not already connected
                && !self.pending_dials.contains_key(peer_id) // There is no pending dial to this node
            })
            .collect::<Vec<_>>();

        // No need to connect to any more peers if we're already connected to a bunch
        let number_of_connections = state.connected_peers.len();
        let number_to_dial = std::cmp::min(
            eligible.len(),
            self.discovery_config
                .target_concurrent_connections()
                .saturating_sub(number_of_connections),
        );

        // randomize the order
        for (peer_id, info) in rand::seq::SliceRandom::choose_multiple(
            eligible.as_slice(),
            &mut rand::thread_rng(),
            number_to_dial,
        ) {
            let abort_handle = self.tasks.spawn(try_to_connect_to_peer(
                self.network.clone(),
                info.to_owned(),
            ));
            self.pending_dials.insert(*peer_id, abort_handle);
        }

        // If we aren't connected to anything and we aren't presently trying to connect to anyone
        // we need to try the seed peers
        if self.dial_seed_peers_task.is_none()
            && state.connected_peers.is_empty()
            && self.pending_dials.is_empty()
            && !self.config.seed_peers.is_empty()
        {
            let abort_handle = self.tasks.spawn(try_to_connect_to_seed_peers(
                self.network.clone(),
                self.discovery_config.clone(),
                self.config.seed_peers.clone(),
            ));

            self.dial_seed_peers_task = Some(abort_handle);
        }
    }
}

async fn try_to_connect_to_peer(network: Network, info: NodeInfo) {
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
    seed_peers: Vec<SeedPeer>,
) {
    let network = &network;

    futures::stream::iter(seed_peers.into_iter().filter_map(|seed| {
        seed.address
            .to_anemo_address()
            .ok()
            .map(|address| (seed, address))
    }))
    .for_each_concurrent(
        config.target_concurrent_connections(),
        |(seed, address)| async move {
            // Ignore the result and just log the error  if there is one
            let _ = if let Some(peer_id) = seed.peer_id {
                network.connect_with_peer_id(address, peer_id).await
            } else {
                network.connect(address).await
            }
            .tap_err(|e| debug!("error dialing multiaddr '{}': {e}", seed.address));
        },
    )
    .await;
}

async fn query_peer_for_their_known_peers(
    peer: Peer,
    state: Arc<RwLock<State>>,
    allowlisted_peers: Arc<HashMap<PeerId, Option<Multiaddr>>>,
) {
    let mut client = DiscoveryClient::new(peer);

    let request = Request::new(()).with_timeout(TIMEOUT);
    if let Some(found_peers) = client
        .get_known_peers(request)
        .await
        .ok()
        .map(Response::into_inner)
        .map(
            |GetKnownPeersResponse {
                 own_info,
                 mut known_peers,
             }| {
                if !own_info.addresses.is_empty() {
                    known_peers.push(own_info)
                }
                known_peers
            },
        )
    {
        update_known_peers(state, found_peers, allowlisted_peers);
    }
}

async fn query_connected_peers_for_their_known_peers(
    network: Network,
    config: Arc<DiscoveryConfig>,
    state: Arc<RwLock<State>>,
    allowlisted_peers: Arc<HashMap<PeerId, Option<Multiaddr>>>,
) {
    use rand::seq::IteratorRandom;

    let peers_to_query = network
        .peers()
        .into_iter()
        .flat_map(|id| network.peer(id))
        .choose_multiple(&mut rand::thread_rng(), config.peers_to_query());

    let found_peers = peers_to_query
        .into_iter()
        .map(DiscoveryClient::new)
        .map(|mut client| async move {
            let request = Request::new(()).with_timeout(TIMEOUT);
            client
                .get_known_peers(request)
                .await
                .ok()
                .map(Response::into_inner)
                .map(
                    |GetKnownPeersResponse {
                         own_info,
                         mut known_peers,
                     }| {
                        known_peers.push(own_info);
                        known_peers
                    },
                )
        })
        .pipe(futures::stream::iter)
        .buffer_unordered(config.peers_to_query())
        .filter_map(std::future::ready)
        .flat_map(futures::stream::iter)
        .collect::<Vec<_>>()
        .await;

    update_known_peers(state, found_peers, allowlisted_peers);
}

fn update_known_peers(
    state: Arc<RwLock<State>>,
    found_peers: Vec<NodeInfo>,
    allowlisted_peers: Arc<HashMap<PeerId, Option<Multiaddr>>>,
) {
    use std::collections::hash_map::Entry;

    let now_unix = now_unix();
    let our_peer_id = state.read().unwrap().our_info.clone().unwrap().peer_id;
    let known_peers = &mut state.write().unwrap().known_peers;
    for peer in found_peers {
        // Skip peers whose timestamp is too far in the future from our clock
        // or that are too old
        if peer.timestamp_ms > now_unix.saturating_add(30 * 1_000) // 30 seconds
            || now_unix.saturating_sub(peer.timestamp_ms) > ONE_DAY_MILLISECONDS
        {
            continue;
        }

        if peer.peer_id == our_peer_id {
            continue;
        }

        // If Peer is Private, and not in our allowlist, skip it.
        if peer.access_type == AccessType::Private && !allowlisted_peers.contains_key(&peer.peer_id)
        {
            continue;
        }

        match known_peers.entry(peer.peer_id) {
            Entry::Occupied(mut o) => {
                if peer.timestamp_ms > o.get().timestamp_ms {
                    o.insert(peer);
                }
            }
            Entry::Vacant(v) => {
                v.insert(peer);
            }
        }
    }
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
