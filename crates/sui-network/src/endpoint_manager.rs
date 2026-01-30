// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

    /// Updates the address(es) for the given endpoint from the specified source.
    ///
    /// Multiple sources can provide addresses for the same peer. The highest-priority
    /// source's addresses are used. Empty `addresses` clears a source.
    pub fn update_endpoint(
        &self,
        endpoint: EndpointId,
        source: AddressSource,
        addresses: Vec<Multiaddr>,
    ) {
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
                self.discovery_handle
                    .peer_address_change(peer_id, source, anemo_addresses);
            }
        }
    }
}

pub enum EndpointId {
    P2p(anemo::PeerId),
    // TODO: Implement support for updating consensus addresses via EndpointManager.
    // Consensus(NetworkPublicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// NOTE: AddressSources are prioritized in order of the enum variants below.
pub enum AddressSource {
    Admin,
    Committee,
}
