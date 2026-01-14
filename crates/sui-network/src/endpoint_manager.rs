// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::types::{PeerAffinity, PeerInfo};
use mysten_network::Multiaddr;
use tap::TapFallible;
use tracing::warn;

use crate::discovery;

/// EndpointManager can be used to dynamically update the addresses of
/// other nodes in the network.
#[derive(Clone, Debug)]
pub struct EndpointManager {
    discovery_handle: discovery::Handle,
}

impl EndpointManager {
    pub fn new(discovery_handle: discovery::Handle) -> Self {
        Self { discovery_handle }
    }

    /// Updates the address(es) for the given endpoint.
    ///
    /// If the addresses have changed, forcibly reconnects to the peer.
    pub fn update_endpoint(&self, endpoint: EndpointId, addresses: Vec<Multiaddr>) {
        match endpoint {
            EndpointId::P2p(peer_id) => {
                let anemo_addresses: Vec<_> = addresses
                    .into_iter()
                    .filter_map(|addr| {
                        addr.to_anemo_address()
                            .tap_err(|_| {
                                warn!(
                                    ?addr,
                                    "Skipping peer address: can't convert to anemo address"
                                )
                            })
                            .ok()
                    })
                    .collect();
                if anemo_addresses.is_empty() {
                    warn!(?peer_id, "No valid addresses for peer after conversion");
                }
                self.discovery_handle.peer_address_change(PeerInfo {
                    peer_id,
                    affinity: PeerAffinity::High,
                    address: anemo_addresses,
                });
            }
        }
    }
}

pub enum EndpointId {
    P2p(anemo::PeerId),
    // TODO: Implement support for updating consensus addresses via EndpointManager.
    // Consensus(NetworkPublicKey),
}
